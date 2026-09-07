// Modified from herdr by the vimeflow project — see FORK.md

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::text::display_width_u16;
use super::widgets::panel_contrast_fg;
use crate::{
    app::state::{CopyFeedback, Palette, ToastKind, ToastNotification},
    config::{TabBarPositionConfig, ToastClipboardPosition, ToastHerdrPosition},
    detect::AgentState,
};

pub(crate) fn copy_feedback_rect(
    area: Rect,
    feedback: &CopyFeedback,
    offset_rows: u16,
    position: ToastClipboardPosition,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }

    let content_width = feedback.message.len() as u16 + 4;
    let width = content_width.min(area.width);
    let height = 3u16.min(area.height);
    let x = match position {
        ToastClipboardPosition::TopLeft | ToastClipboardPosition::BottomLeft => area.x,
        ToastClipboardPosition::TopCenter | ToastClipboardPosition::BottomCenter => {
            area.x + area.width.saturating_sub(width) / 2
        }
        ToastClipboardPosition::TopRight | ToastClipboardPosition::BottomRight => {
            area.x + area.width.saturating_sub(width)
        }
    };
    let y = match position {
        ToastClipboardPosition::TopLeft
        | ToastClipboardPosition::TopCenter
        | ToastClipboardPosition::TopRight => area.y + offset_rows.min(area.height),
        ToastClipboardPosition::BottomLeft
        | ToastClipboardPosition::BottomCenter
        | ToastClipboardPosition::BottomRight => {
            area.y + area.height.saturating_sub(height + offset_rows)
        }
    };
    Rect::new(x, y, width, height)
}

pub(crate) fn toast_notification_rect(
    area: Rect,
    toast: &ToastNotification,
    offset_for_warning: bool,
    position: ToastHerdrPosition,
    capsule: Rect,
    tab_bar_position: TabBarPositionConfig,
) -> Rect {
    let content_width = display_width_u16(&toast.title)
        .max(display_width_u16(&toast.context))
        .saturating_add(4);
    let width = content_width.saturating_add(2).min(area.width);
    let content_height = if toast.context.is_empty() { 1 } else { 2 };
    let mut height = (content_height + 2).min(area.height);
    let x = match position {
        ToastHerdrPosition::Island => {
            let anchor = if capsule.is_empty() { area } else { capsule };
            let center_twice = u32::from(anchor.x) * 2 + u32::from(anchor.width);
            let desired_x = center_twice.saturating_sub(u32::from(width)) / 2;
            desired_x.clamp(u32::from(area.x), u32::from(area.right() - width)) as u16
        }
        ToastHerdrPosition::TopLeft | ToastHerdrPosition::BottomLeft => area.x,
        ToastHerdrPosition::TopRight | ToastHerdrPosition::BottomRight => {
            area.x + area.width.saturating_sub(width)
        }
    };
    let warning_offset = u16::from(offset_for_warning);
    let y = match position {
        ToastHerdrPosition::Island => {
            let (top, bottom) = if capsule.is_empty() {
                (area.y, area.bottom())
            } else {
                match tab_bar_position {
                    TabBarPositionConfig::Top => {
                        (capsule.bottom().clamp(area.y, area.bottom()), area.bottom())
                    }
                    TabBarPositionConfig::Bottom => {
                        (area.y, capsule.y.clamp(area.y, area.bottom()))
                    }
                }
            };
            height = height.min(bottom - top);
            let warning_offset = warning_offset.min(bottom - top - height);
            if !capsule.is_empty() && tab_bar_position == TabBarPositionConfig::Bottom {
                bottom - height - warning_offset
            } else {
                top + warning_offset
            }
        }
        ToastHerdrPosition::TopLeft | ToastHerdrPosition::TopRight => {
            area.y + warning_offset.min(area.height)
        }
        ToastHerdrPosition::BottomLeft | ToastHerdrPosition::BottomRight => {
            area.y + area.height.saturating_sub(height + warning_offset)
        }
    };
    Rect::new(x, y, width, height)
}

pub(super) fn render_toast_notification(
    frame: &mut Frame,
    toast_area: Rect,
    toast: &ToastNotification,
    p: &Palette,
) {
    let dot_color = match toast.kind {
        ToastKind::NeedsAttention => p.red,
        ToastKind::Finished => p.blue,
        ToastKind::UpdateInstalled => p.accent,
    };
    frame.render_widget(Clear, toast_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.overlay0))
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    if inner.height < 1 {
        return;
    }

    let [title_row, context_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

    let title = Line::from(vec![
        Span::styled("●", Style::default().fg(dot_color)),
        Span::raw(" "),
        Span::styled(
            &toast.title,
            Style::default().fg(p.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    let context = Line::from(vec![
        Span::styled("  ", Style::default().fg(p.overlay0)),
        Span::styled(&toast.context, Style::default().fg(p.overlay0)),
    ]);

    frame.render_widget(Paragraph::new(title), title_row);
    if !toast.context.is_empty() && inner.height >= 2 {
        frame.render_widget(Paragraph::new(context), context_row);
    }
}

pub(super) fn render_copy_feedback(
    frame: &mut Frame,
    area: Rect,
    feedback: &CopyFeedback,
    offset_rows: u16,
    position: ToastClipboardPosition,
    p: &Palette,
) {
    let feedback_area = copy_feedback_rect(area, feedback, offset_rows, position);
    if feedback_area.is_empty() {
        return;
    }

    frame.render_widget(Clear, feedback_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.green))
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(feedback_area);
    frame.render_widget(block, feedback_area);

    if inner.height == 0 {
        return;
    }

    let text = Line::from(vec![
        Span::styled("●", Style::default().fg(p.green).bg(p.panel_bg)),
        Span::raw(" "),
        Span::styled(
            &feedback.message,
            Style::default()
                .fg(p.text)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}

pub(super) fn render_config_diagnostic(frame: &mut Frame, area: Rect, message: &str, p: &Palette) {
    let style = Style::default()
        .fg(panel_contrast_fg(p))
        .bg(p.yellow)
        .add_modifier(Modifier::BOLD);

    for (row, line) in message
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(area.height as usize)
        .enumerate()
    {
        let text = format!(" {line} ");
        let width = (text.len() as u16).min(area.width);
        let notif_area = Rect::new(
            area.x + area.width.saturating_sub(width),
            area.y + row as u16,
            width,
            1,
        );

        frame.render_widget(Clear, notif_area);
        frame.render_widget(Paragraph::new(Span::styled(text, style)), notif_area);
    }
}

pub(super) fn state_dot(state: AgentState, seen: bool, p: &Palette) -> (&'static str, Style) {
    match (state, seen) {
        (AgentState::Blocked, _) => ("●", Style::default().fg(p.red)),
        (AgentState::Working, _) => ("●", Style::default().fg(p.yellow)),
        (AgentState::Idle, false) => ("●", Style::default().fg(p.teal)),
        (AgentState::Idle, true) => ("○", Style::default().fg(p.green)),
        (AgentState::Unknown, _) => ("·", Style::default().fg(p.overlay0)),
    }
}

pub(super) fn state_label(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Working, _) => "working",
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Unknown, _) => "idle",
    }
}

pub(super) fn state_label_color(state: AgentState, seen: bool, p: &Palette) -> Color {
    match (state, seen) {
        (AgentState::Blocked, _) => p.red,
        (AgentState::Working, _) => p.yellow,
        (AgentState::Idle, false) => p.teal,
        (AgentState::Idle, true) => p.green,
        (AgentState::Unknown, _) => p.overlay0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ToastClipboardPosition, ToastHerdrPosition};

    fn toast() -> ToastNotification {
        ToastNotification {
            kind: ToastKind::Finished,
            title: "done".to_string(),
            context: "workspace".to_string(),
            position: None,
            target: None,
            island_record_id: None,
        }
    }

    fn feedback() -> CopyFeedback {
        CopyFeedback {
            message: "copied to clipboard".to_string(),
        }
    }

    #[test]
    fn state_dots_use_aligned_static_workspace_marks() {
        let palette = Palette::catppuccin();
        for (state, seen, symbol, color) in [
            (AgentState::Blocked, true, "●", palette.red),
            (AgentState::Working, true, "●", palette.yellow),
            (AgentState::Idle, false, "●", palette.teal),
            (AgentState::Idle, true, "○", palette.green),
            (AgentState::Unknown, true, "·", palette.overlay0),
        ] {
            let (actual_symbol, style) = state_dot(state, seen, &palette);
            assert_eq!(actual_symbol, symbol);
            assert_eq!(style.fg, Some(color));
        }
    }

    #[test]
    fn toast_rect_uses_configured_corner() {
        let area = Rect::new(10, 20, 100, 40);
        let toast = toast();

        for (position, x, y, warning_y) in [
            (ToastHerdrPosition::TopLeft, 10, 20, 21),
            (ToastHerdrPosition::TopRight, 95, 20, 21),
            (ToastHerdrPosition::BottomLeft, 10, 56, 55),
            (ToastHerdrPosition::BottomRight, 95, 56, 55),
        ] {
            for capsule in [Rect::default(), Rect::new(60, 20, 20, 1)] {
                for warning in [false, true] {
                    assert_eq!(
                        toast_notification_rect(
                            area,
                            &toast,
                            warning,
                            position,
                            capsule,
                            TabBarPositionConfig::Top,
                        ),
                        Rect::new(x, if warning { warning_y } else { y }, 15, 4),
                    );
                }
            }
        }
    }

    #[test]
    fn island_toast_rect_centers_on_capsule_and_opens_toward_panes() {
        let area = Rect::new(10, 20, 100, 40);
        for (position, capsule_y, toast_y, warning_y) in [
            (TabBarPositionConfig::Top, 20, 21, 22),
            (TabBarPositionConfig::Bottom, 59, 55, 54),
        ] {
            for warning in [false, true] {
                assert_eq!(
                    toast_notification_rect(
                        area,
                        &toast(),
                        warning,
                        ToastHerdrPosition::Island,
                        Rect::new(66, capsule_y, 20, 1),
                        position,
                    ),
                    Rect::new(68, if warning { warning_y } else { toast_y }, 15, 4),
                );
            }
        }
    }

    #[test]
    fn island_toast_rect_clamps_at_both_edges_and_in_short_areas() {
        for width in 1..=40 {
            for height in 1..=8 {
                let area = Rect::new(10, 20, width, height);
                for position in [TabBarPositionConfig::Top, TabBarPositionConfig::Bottom] {
                    let capsule_y = if position == TabBarPositionConfig::Top {
                        area.y
                    } else {
                        area.bottom() - 1
                    };
                    let capsule_width = 3.min(width);
                    for left in [true, false] {
                        let capsule_x = if left {
                            area.x
                        } else {
                            area.right() - capsule_width
                        };
                        let capsule = Rect::new(capsule_x, capsule_y, capsule_width, 1);
                        for warning in [false, true] {
                            let rect = toast_notification_rect(
                                area,
                                &toast(),
                                warning,
                                ToastHerdrPosition::Island,
                                capsule,
                                position,
                            );
                            assert_eq!(rect.width, 15.min(width));
                            assert_eq!(rect.height, 4.min(height - 1));
                            assert_eq!(
                                rect.x,
                                if left {
                                    area.x
                                } else {
                                    area.right() - rect.width
                                }
                            );
                            assert!(rect.y >= area.y && rect.bottom() <= area.bottom());
                            assert!(if position == TabBarPositionConfig::Top {
                                rect.y >= capsule.bottom()
                            } else {
                                rect.bottom() <= capsule.y
                            });
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn island_toast_rect_without_capsule_falls_back_to_top_center() {
        let area = Rect::new(10, 20, 100, 40);
        for position in [TabBarPositionConfig::Top, TabBarPositionConfig::Bottom] {
            for warning in [false, true] {
                assert_eq!(
                    toast_notification_rect(
                        area,
                        &toast(),
                        warning,
                        ToastHerdrPosition::Island,
                        Rect::default(),
                        position,
                    ),
                    Rect::new(52, 20 + u16::from(warning), 15, 4),
                );
            }
        }
    }

    #[test]
    fn toast_rect_uses_display_width_for_cjk_labels() {
        let area = Rect::new(0, 0, 100, 20);
        let toast = ToastNotification {
            kind: ToastKind::NeedsAttention,
            title: "重构用户认证模块".to_string(),
            context: "提交 herdr 的反馈".to_string(),
            position: None,
            target: None,
            island_record_id: None,
        };

        let rect = toast_notification_rect(
            area,
            &toast,
            false,
            ToastHerdrPosition::TopRight,
            Rect::default(),
            TabBarPositionConfig::Top,
        );

        let expected_content_width =
            display_width_u16(&toast.title).max(display_width_u16(&toast.context)) + 6;
        assert_eq!(rect.width, expected_content_width);
        assert_eq!(rect.x + rect.width, area.x + area.width);
    }

    #[test]
    fn copy_feedback_rect_uses_configured_position() {
        let area = Rect::new(10, 20, 100, 40);
        let feedback = feedback();

        let top_center = copy_feedback_rect(area, &feedback, 0, ToastClipboardPosition::TopCenter);
        assert_eq!(top_center.y, area.y);
        assert_eq!(
            top_center.x,
            area.x + area.width.saturating_sub(top_center.width) / 2
        );

        let bottom_center =
            copy_feedback_rect(area, &feedback, 0, ToastClipboardPosition::BottomCenter);
        assert_eq!(bottom_center.y + bottom_center.height, area.y + area.height);
        assert_eq!(
            bottom_center.x,
            area.x + area.width.saturating_sub(bottom_center.width) / 2
        );
    }
}
