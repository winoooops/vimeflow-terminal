use ratatui::{layout::Rect, style::Style, widgets::Paragraph, Frame};

use super::tabs::TabBarView;
use super::text::{display_width, display_width_u16, truncate_end};
use super::widgets::panel_contrast_fg;
use crate::app::AppState;
use crate::config::{IslandCapsConfig, IslandDisplayConfig, IslandPositionConfig};

const CAPSULE_PADDING: usize = 1;
const LEFT_CAP: &str = "\u{e0b6}";
const RIGHT_CAP: &str = "\u{e0b4}";
const MARKER_GAP: usize = 1;
const MAX_PAGE_SIZE: usize = 10;
const LABEL_MAX_WIDTH: usize = 16;
const NEW_TAB_WIDTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PagePlan {
    start: usize,
    page_size: usize,
    total_pages: usize,
    indicator_width: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkerLayout {
    tab_idx: usize,
    rect: Rect,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IslandLayout {
    capsule: Rect,
    indicator: Option<(Rect, String)>,
    markers: Vec<MarkerLayout>,
    new_tab_hit_area: Rect,
}

fn digits(value: usize) -> usize {
    value.max(1).ilog10() as usize + 1
}

fn caps_width(caps: IslandCapsConfig) -> usize {
    match caps {
        IslandCapsConfig::Round => 2,
        IslandCapsConfig::Square => 0,
    }
}

fn marker_budget(display: IslandDisplayConfig, tab_count: usize, caps: IslandCapsConfig) -> usize {
    let content_width = match display {
        IslandDisplayConfig::Dots => 3,
        IslandDisplayConfig::Numbers => digits(tab_count) + 2,
        IslandDisplayConfig::Labels => LABEL_MAX_WIDTH,
    };
    content_width + caps_width(caps)
}

fn markers_width(count: usize, marker_width: usize) -> usize {
    count
        .saturating_mul(marker_width)
        .saturating_add(count.saturating_sub(1).saturating_mul(MARKER_GAP))
}

fn page_plan(
    tab_count: usize,
    active_tab: usize,
    area_width: usize,
    display: IslandDisplayConfig,
    caps: IslandCapsConfig,
    show_new_tab: bool,
) -> PagePlan {
    let marker_width = marker_budget(display, tab_count, caps);
    let fixed_width =
        2 * CAPSULE_PADDING + caps_width(caps) + if show_new_tab { NEW_TAB_WIDTH } else { 0 };
    if tab_count <= MAX_PAGE_SIZE
        && fixed_width.saturating_add(markers_width(tab_count, marker_width)) <= area_width
    {
        return PagePlan {
            start: 0,
            page_size: tab_count.max(1),
            total_pages: 1,
            indicator_width: 0,
        };
    }

    let indicator_width = 2 * digits(tab_count) + 3;
    let available = area_width.saturating_sub(fixed_width + indicator_width);
    let markers_that_fit = if available < marker_width {
        0
    } else {
        (available + MARKER_GAP) / (marker_width + MARKER_GAP)
    };
    let page_size = markers_that_fit
        .clamp(1, MAX_PAGE_SIZE)
        .min(tab_count.max(1));
    let active_tab = active_tab.min(tab_count.saturating_sub(1));
    let start = active_tab / page_size * page_size;

    PagePlan {
        start,
        page_size,
        total_pages: tab_count.div_ceil(page_size).max(1),
        indicator_width,
    }
}

fn marker_text(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    display: IslandDisplayConfig,
) -> String {
    let active = tab_idx == ws.active_tab;
    match display {
        IslandDisplayConfig::Dots => {
            if active {
                " ━ ".to_string()
            } else {
                "●".to_string()
            }
        }
        IslandDisplayConfig::Numbers => {
            let number = (tab_idx + 1).to_string();
            if active {
                format!(" {number} ")
            } else {
                number
            }
        }
        IslandDisplayConfig::Labels => {
            if !active {
                return "●".to_string();
            }
            let name = ws
                .tab_display_name(tab_idx)
                .unwrap_or_else(|| (tab_idx + 1).to_string());
            let width = (display_width(&name) + 2).clamp(3, LABEL_MAX_WIDTH);
            let label = truncate_end(&name, width - 2);
            let padding = width - 2 - display_width(&label);
            format!(" {label}{} ", " ".repeat(padding))
        }
    }
}

fn layout(app: &AppState, area: Rect) -> Option<IslandLayout> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let ws = app.active.and_then(|idx| app.workspaces.get(idx))?;
    if ws.tabs.is_empty() {
        return None;
    }

    let page = page_plan(
        ws.tabs.len(),
        ws.active_tab,
        usize::from(area.width),
        app.island.display,
        app.island.caps,
        app.mouse_capture,
    );
    let page_end = (page.start + page.page_size).min(ws.tabs.len());
    let marker_texts = (page.start..page_end)
        .map(|tab_idx| (tab_idx, marker_text(ws, tab_idx, app.island.display)))
        .collect::<Vec<_>>();
    let marker_width = marker_texts
        .iter()
        .map(|(_, text)| display_width(text))
        .sum::<usize>()
        + marker_texts.len().saturating_sub(1) * MARKER_GAP
        + caps_width(app.island.caps);
    let new_tab_width = if app.mouse_capture { NEW_TAB_WIDTH } else { 0 };
    let capsule_width = caps_width(app.island.caps)
        + 2 * CAPSULE_PADDING
        + page.indicator_width
        + marker_width
        + new_tab_width;
    if capsule_width > usize::from(area.width) {
        return None;
    }

    let capsule_width = capsule_width as u16;
    let capsule_x = match app.island.position {
        IslandPositionConfig::Center => area.x + area.width.saturating_sub(capsule_width) / 2,
        IslandPositionConfig::Left => area.x,
    };
    let capsule = Rect::new(capsule_x, area.y, capsule_width, 1);
    let round_caps = app.island.caps == IslandCapsConfig::Round;
    let mut x = capsule.x + CAPSULE_PADDING as u16 + if round_caps { 1 } else { 0 };
    let indicator = (page.indicator_width > 0).then(|| {
        let rect = Rect::new(x, area.y, page.indicator_width as u16, 1);
        let current_page = page.start / page.page_size + 1;
        x += rect.width;
        (rect, format!("‹{current_page}/{}›", page.total_pages))
    });

    let mut markers = Vec::with_capacity(marker_texts.len());
    for (offset, (tab_idx, text)) in marker_texts.into_iter().enumerate() {
        if offset > 0 {
            x += MARKER_GAP as u16;
        }
        let active = tab_idx == ws.active_tab;
        if active && round_caps {
            x += 1;
        }
        let rect = Rect::new(x, area.y, display_width_u16(&text), 1);
        x += rect.width;
        if active && round_caps {
            x += 1;
        }
        markers.push(MarkerLayout {
            tab_idx,
            rect,
            text,
        });
    }

    let new_tab_hit_area = if app.mouse_capture {
        Rect::new(x, area.y, NEW_TAB_WIDTH as u16, 1)
    } else {
        Rect::default()
    };

    Some(IslandLayout {
        capsule,
        indicator,
        markers,
        new_tab_hit_area,
    })
}

pub(super) fn compute_tab_bar_view(app: &AppState, area: Rect) -> TabBarView {
    let Some(layout) = layout(app, area) else {
        return TabBarView::default();
    };
    let tab_count = app
        .active
        .and_then(|idx| app.workspaces.get(idx))
        .map_or(0, |ws| ws.tabs.len());
    let mut marker_hit_areas = vec![Rect::default(); tab_count];
    for marker in &layout.markers {
        marker_hit_areas[marker.tab_idx] = marker.rect;
    }

    TabBarView {
        island_marker_hit_areas: marker_hit_areas,
        new_tab_hit_area: layout.new_tab_hit_area,
        ..TabBarView::default()
    }
}

pub(super) fn render_tab_bar(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = &app.palette;
    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize)).style(Style::default().bg(p.panel_bg)),
        area,
    );

    let Some(layout) = layout(app, area) else {
        return;
    };
    let round_caps = app.island.caps == IslandCapsConfig::Round;
    let capsule_body = if round_caps {
        Rect::new(
            layout.capsule.x + 1,
            layout.capsule.y,
            layout.capsule.width - 2,
            1,
        )
    } else {
        layout.capsule
    };
    frame.render_widget(
        Paragraph::new(" ".repeat(capsule_body.width as usize))
            .style(Style::default().bg(p.surface0)),
        capsule_body,
    );
    if round_caps {
        frame.buffer_mut()[(layout.capsule.x, layout.capsule.y)]
            .set_symbol(LEFT_CAP)
            .set_style(Style::default().fg(p.surface0).bg(p.panel_bg));
        frame.buffer_mut()[(layout.capsule.right() - 1, layout.capsule.y)]
            .set_symbol(RIGHT_CAP)
            .set_style(Style::default().fg(p.surface0).bg(p.panel_bg));
    }

    if let Some((rect, text)) = layout.indicator {
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(p.overlay0).bg(p.surface0)),
            rect,
        );
    }

    let active_tab = app
        .active
        .and_then(|idx| app.workspaces.get(idx))
        .map(|ws| ws.active_tab);
    for marker in layout.markers {
        let active = active_tab == Some(marker.tab_idx);
        let style = if active {
            Style::default().fg(panel_contrast_fg(p)).bg(p.accent)
        } else if active_tab.is_some_and(|active| marker.tab_idx < active) {
            Style::default().fg(p.overlay1).bg(p.surface0)
        } else {
            Style::default().fg(p.overlay0).bg(p.surface0)
        };
        frame.render_widget(Paragraph::new(marker.text).style(style), marker.rect);
        if active && round_caps {
            let cap_style = Style::default().fg(p.accent).bg(p.surface0);
            frame.buffer_mut()[(marker.rect.x - 1, marker.rect.y)]
                .set_symbol(LEFT_CAP)
                .set_style(cap_style);
            frame.buffer_mut()[(marker.rect.right(), marker.rect.y)]
                .set_symbol(RIGHT_CAP)
                .set_style(cap_style);
        }
    }

    if layout.new_tab_hit_area.width > 0 {
        frame.render_widget(
            Paragraph::new(" + ").style(Style::default().fg(p.overlay1).bg(p.surface0)),
            layout.new_tab_hit_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn app_with_tabs(tab_count: usize, active_tab: usize) -> AppState {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        for idx in 1..tab_count {
            ws.test_add_tab((idx == 1).then_some("work"));
        }
        ws.switch_tab(active_tab);
        app.workspaces = vec![ws];
        app.active = Some(0);
        app
    }

    fn rect_text(buffer: &Buffer, rect: Rect) -> String {
        (rect.x..rect.x + rect.width)
            .map(|x| buffer[(x, rect.y)].symbol())
            .collect()
    }

    #[test]
    fn renders_all_display_modes_cell_exact() {
        let area = Rect::new(0, 0, 40, 1);
        for (display, expected) in [
            (
                IslandDisplayConfig::Dots,
                "\u{e0b6} ● \u{e0b6} ━ \u{e0b4} ● ● +  \u{e0b4}",
            ),
            (
                IslandDisplayConfig::Numbers,
                "\u{e0b6} 1 \u{e0b6} 2 \u{e0b4} 3 4 +  \u{e0b4}",
            ),
            (
                IslandDisplayConfig::Labels,
                "\u{e0b6} ‹2/4›\u{e0b6} work \u{e0b4} +  \u{e0b4}",
            ),
        ] {
            let mut app = app_with_tabs(4, 1);
            app.island.display = display;
            let layout = layout(&app, area).expect("island should fit");
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render_tab_bar(&app, frame, area))
                .expect("draw island");

            assert_eq!(
                rect_text(terminal.backend().buffer(), layout.capsule),
                expected
            );
        }
    }

    #[test]
    fn square_caps_preserve_flat_rendering() {
        let area = Rect::new(0, 0, 40, 1);
        let mut app = app_with_tabs(4, 1);
        let round_width = layout(&app, area).expect("round island").capsule.width;
        app.island.caps = IslandCapsConfig::Square;
        let layout = layout(&app, area).expect("square island");
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw island");

        assert_eq!(round_width, layout.capsule.width + 4);
        assert_eq!(
            rect_text(terminal.backend().buffer(), layout.capsule),
            " ●  ━  ● ● +  "
        );
    }

    #[test]
    fn renders_positional_colors_and_surface_padding() {
        let app = app_with_tabs(4, 1);
        let area = Rect::new(0, 0, 40, 1);
        let layout = layout(&app, area).expect("island should fit");
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw island");
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(layout.capsule.x, area.y)].symbol(), LEFT_CAP);
        assert_eq!(
            buffer[(layout.capsule.x, area.y)].style().fg,
            Some(app.palette.surface0)
        );
        assert_eq!(
            buffer[(layout.capsule.x, area.y)].style().bg,
            Some(app.palette.panel_bg)
        );
        assert_eq!(
            buffer[(layout.capsule.right() - 1, area.y)].symbol(),
            RIGHT_CAP
        );
        assert_eq!(
            buffer[(layout.capsule.right() - 1, area.y)].style().fg,
            Some(app.palette.surface0)
        );
        assert_eq!(
            buffer[(layout.capsule.right() - 1, area.y)].style().bg,
            Some(app.palette.panel_bg)
        );
        assert_eq!(
            buffer[(layout.capsule.x + 1, area.y)].style().bg,
            Some(app.palette.surface0)
        );
        assert_eq!(
            buffer[(layout.capsule.right() - 2, area.y)].style().bg,
            Some(app.palette.surface0)
        );
        assert_eq!(
            buffer[(layout.markers[0].rect.x, area.y)].style().fg,
            Some(app.palette.overlay1)
        );
        for (x, symbol) in [
            (layout.markers[1].rect.x - 1, LEFT_CAP),
            (layout.markers[1].rect.right(), RIGHT_CAP),
        ] {
            let cell = &buffer[(x, area.y)];
            assert_eq!(cell.symbol(), symbol);
            assert_eq!(cell.style().fg, Some(app.palette.accent));
            assert_eq!(cell.style().bg, Some(app.palette.surface0));
        }
        for x in layout.markers[1].rect.x..layout.markers[1].rect.right() {
            let style = buffer[(x, area.y)].style();
            assert_eq!(style.fg, Some(panel_contrast_fg(&app.palette)));
            assert_eq!(style.bg, Some(app.palette.accent));
        }
        assert_eq!(
            buffer[(layout.markers[2].rect.x, area.y)].style().fg,
            Some(app.palette.overlay0)
        );
        assert_eq!(
            buffer[(area.x, area.y)].style().bg,
            Some(app.palette.panel_bg)
        );
    }

    #[test]
    fn positions_capsule_center_or_left() {
        let area = Rect::new(5, 2, 30, 1);
        let mut app = app_with_tabs(4, 1);
        let centered = layout(&app, area).expect("centered island");
        assert_eq!(
            centered.capsule.x,
            area.x + (area.width - centered.capsule.width) / 2
        );

        app.island.position = IslandPositionConfig::Left;
        let left = layout(&app, area).expect("left island");
        assert_eq!(left.capsule.x, area.x);
    }

    #[test]
    fn batches_in_stable_blocks_and_renders_the_active_page() {
        let area = Rect::new(0, 0, 60, 1);
        let mut app = app_with_tabs(11, 0);
        let first = page_plan(
            11,
            0,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
            true,
        );
        let same_page = page_plan(
            11,
            6,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
            true,
        );
        assert_eq!(first.page_size, 7);
        assert_eq!(first.start, 0);
        assert_eq!(same_page.start, first.start);

        app.workspaces[0].switch_tab(10);
        let next = page_plan(
            11,
            10,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
            true,
        );
        assert_eq!(next.start, 7);
        assert_eq!(next.total_pages, 2);
        let layout = layout(&app, area).expect("second page island");
        assert_eq!(
            layout
                .markers
                .iter()
                .map(|marker| marker.tab_idx)
                .collect::<Vec<_>>(),
            vec![7, 8, 9, 10]
        );

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw island");
        assert_eq!(
            rect_text(terminal.backend().buffer(), layout.capsule),
            "\u{e0b6} ‹2/2›  ● ● ● \u{e0b6} ━ \u{e0b4} +  \u{e0b4}"
        );
    }

    #[test]
    fn marker_budgets_are_active_state_independent() {
        assert_eq!(
            marker_budget(IslandDisplayConfig::Dots, 11, IslandCapsConfig::Round),
            5
        );
        assert_eq!(
            marker_budget(IslandDisplayConfig::Numbers, 11, IslandCapsConfig::Round),
            6
        );
        assert_eq!(
            marker_budget(IslandDisplayConfig::Labels, 11, IslandCapsConfig::Round),
            18
        );
        assert_eq!(
            marker_budget(IslandDisplayConfig::Dots, 11, IslandCapsConfig::Square),
            3
        );
    }

    #[test]
    fn active_label_is_truncated_to_sixteen_cells() {
        let mut app = app_with_tabs(2, 1);
        app.workspaces[0].tabs[1].set_custom_name("a very long island label".to_string());

        let text = marker_text(&app.workspaces[0], 1, IslandDisplayConfig::Labels);

        assert_eq!(display_width(&text), LABEL_MAX_WIDTH);
        assert!(text.contains('…'));
    }
}
