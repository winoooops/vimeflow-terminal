use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};

use super::tabs::TabBarView;
use super::text::{display_width, display_width_u16, truncate_end};
use super::widgets::panel_contrast_fg;
use crate::app::state::{AppState, IslandAnim, IslandSpring};
use crate::config::{
    IslandCapsConfig, IslandDisplayConfig, IslandMotionConfig, IslandPositionConfig,
};

const LEFT_CAP: &str = "\u{e0b6}";
const RIGHT_CAP: &str = "\u{e0b4}";
const ROUND_DOT: &str = "⬤";
const CAPSULE_PADDING_BUDGET: usize = 1;
const MARKER_GAP: usize = 1;
const MAX_PAGE_SIZE: usize = 10;
const ACTIVE_TITLE_MAX_WIDTH: usize = 10;
const LABEL_MAX_WIDTH: usize = 16;
const VELOCITY_BRIGHTNESS_SCALE: f32 = 0.000_3;
const MAX_VELOCITY_BRIGHTNESS: f32 = 0.06;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ParticipantWidths {
    outgoing: f32,
    incoming: f32,
}

impl ParticipantWidths {
    const fn new(outgoing: f32, incoming: f32) -> Self {
        Self { outgoing, incoming }
    }

    fn total(self) -> f32 {
        self.outgoing + self.incoming
    }
}

fn normalized_progress(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * normalized_progress(progress)
}

/// Resolve any concrete palette color to RGB channels so the crossfade never
/// degrades to an end-of-animation pop for named-ANSI or indexed themes.
/// Named and indexed slots resolve through the host-reported palette (OSC 4)
/// first so motion frames stay on the user's actual theme colors; slots the
/// host never reported fall back to the stock ghostty palette. `Reset` (the
/// "reset"/"default" theme alias) resolves through the host-reported default
/// foreground (OSC 10) and stays unresolvable only when the host never
/// answered that query.
fn color_channels(
    color: Color,
    host_theme: &crate::terminal_theme::TerminalTheme,
) -> Option<(u8, u8, u8)> {
    let indexed = |idx: u8| {
        Some(
            host_theme.palette[usize::from(idx)]
                .map(|rgb| (rgb.r, rgb.g, rgb.b))
                .unwrap_or_else(|| stock_palette_channels(idx)),
        )
    };
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Reset => host_theme.foreground.map(|rgb| (rgb.r, rgb.g, rgb.b)),
        Color::Indexed(n) => indexed(n),
        Color::Black => indexed(0),
        Color::Red => indexed(1),
        Color::Green => indexed(2),
        Color::Yellow => indexed(3),
        Color::Blue => indexed(4),
        Color::Magenta => indexed(5),
        Color::Cyan => indexed(6),
        Color::Gray => indexed(7),
        Color::DarkGray => indexed(8),
        Color::LightRed => indexed(9),
        Color::LightGreen => indexed(10),
        Color::LightYellow => indexed(11),
        Color::LightBlue => indexed(12),
        Color::LightMagenta => indexed(13),
        Color::LightCyan => indexed(14),
        Color::White => indexed(15),
    }
}

fn stock_palette_channels(idx: u8) -> (u8, u8, u8) {
    static STOCK: std::sync::OnceLock<[crate::ghostty::RgbColor; 256]> = std::sync::OnceLock::new();
    let rgb = STOCK.get_or_init(crate::ghostty::default_palette)[usize::from(idx)];
    (rgb.r, rgb.g, rgb.b)
}

fn rgb_to_hsl(
    color: Color,
    host_theme: &crate::terminal_theme::TerminalTheme,
) -> Option<(f32, f32, f32)> {
    let (r, g, b) = color_channels(color, host_theme)?;
    let [r, g, b] = [r, g, b].map(|channel| f32::from(channel) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = (max + min) / 2.0;
    if delta <= f32::EPSILON {
        return Some((0.0, 0.0, lightness));
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    Some((hue, saturation, lightness))
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hue.rem_euclid(360.0) {
        hue if hue < 60.0 => (chroma, x, 0.0),
        hue if hue < 120.0 => (x, chroma, 0.0),
        hue if hue < 180.0 => (0.0, chroma, x),
        hue if hue < 240.0 => (0.0, x, chroma),
        hue if hue < 300.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = lightness - chroma / 2.0;
    let channel = |value: f32| ((value + offset) * 255.0).round() as u8;
    Color::Rgb(channel(r), channel(g), channel(b))
}

fn lerp_hsl(
    from: Color,
    to: Color,
    progress: f32,
    host_theme: &crate::terminal_theme::TerminalTheme,
) -> Color {
    let progress = normalized_progress(progress);
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    let (Some((from_h, from_s, from_l)), Some((to_h, to_s, to_l))) =
        (rgb_to_hsl(from, host_theme), rgb_to_hsl(to, host_theme))
    else {
        return from;
    };
    let hue_delta = (to_h - from_h + 180.0).rem_euclid(360.0) - 180.0;
    hsl_to_rgb(
        from_h + hue_delta * progress,
        lerp(from_s, to_s, progress),
        lerp(from_l, to_l, progress),
    )
}

fn brighten(
    color: Color,
    velocity: f32,
    host_theme: &crate::terminal_theme::TerminalTheme,
) -> Color {
    let amount = (velocity.abs() * VELOCITY_BRIGHTNESS_SCALE).min(MAX_VELOCITY_BRIGHTNESS);
    if amount <= f32::EPSILON {
        return color;
    }
    let Some((hue, saturation, lightness)) = rgb_to_hsl(color, host_theme) else {
        return color;
    };
    hsl_to_rgb(hue, saturation, (lightness + amount).min(1.0))
}

fn animated_content_visible(progress: f32) -> bool {
    progress > 0.6
}

fn quantized_participant_widths(
    display: IslandDisplayConfig,
    caps: IslandCapsConfig,
    widths: ParticipantWidths,
    renders_titles: bool,
) -> Option<ParticipantWidths> {
    let minimum = if caps == IslandCapsConfig::Round {
        2.0
    } else {
        1.0
    };
    match display {
        IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers if !renders_titles => {
            let total = widths.total().round();
            // Rapid onward retargets can catch both participants at their
            // inactive width (e.g. two next-tab presses inside one tick), so
            // the total cannot carry both participants' caps; report that so
            // the caller cuts to a settled frame instead of clamping into a
            // reversed range and panicking.
            let maximum = total - minimum;
            if maximum < minimum {
                return None;
            }
            let outgoing = widths.outgoing.round().clamp(minimum, maximum);
            Some(ParticipantWidths::new(outgoing, total - outgoing))
        }
        IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers | IslandDisplayConfig::Labels => {
            Some(ParticipantWidths::new(
                widths.outgoing.round().max(minimum),
                widths.incoming.round().max(minimum),
            ))
        }
    }
}

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
    display: IslandDisplayConfig,
    active_title_budget: usize,
    capsule: Rect,
    indicator: Option<(Rect, String)>,
    markers: Vec<MarkerLayout>,
}

struct AnimatedIslandLayout {
    from: IslandLayout,
    to: IslandLayout,
    display: IslandDisplayConfig,
    widths: ParticipantWidths,
    capsule_total: f32,
    fixed_width: f32,
    outgoing_activation: f32,
    incoming_activation: f32,
    outgoing_velocity: f32,
    incoming_velocity: f32,
    at_from: bool,
    at_to: bool,
}

struct AnimationEndpoints {
    display: IslandDisplayConfig,
    renders_titles: bool,
    from: IslandLayout,
    to: IslandLayout,
    settled_from: ParticipantWidths,
    settled_to: ParticipantWidths,
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

fn capsule_padding(caps: IslandCapsConfig, adjacent_marker_cap: bool) -> usize {
    match (caps, adjacent_marker_cap) {
        (IslandCapsConfig::Round, true) => 0,
        _ => 1,
    }
}

fn marker_budget(
    display: IslandDisplayConfig,
    tab_count: usize,
    caps: IslandCapsConfig,
    active_title: bool,
) -> usize {
    let active_width = (match display {
        IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers if active_title => {
            ACTIVE_TITLE_MAX_WIDTH
        }
        IslandDisplayConfig::Dots => 3,
        IslandDisplayConfig::Numbers => digits(tab_count) + 2,
        IslandDisplayConfig::Labels => LABEL_MAX_WIDTH,
    }) + caps_width(caps);
    let inactive_width = match (display, caps) {
        (IslandDisplayConfig::Dots, _) => 1,
        (IslandDisplayConfig::Numbers, IslandCapsConfig::Round) => digits(tab_count) + 2,
        (IslandDisplayConfig::Numbers, IslandCapsConfig::Square) => digits(tab_count),
        (IslandDisplayConfig::Labels, caps) => LABEL_MAX_WIDTH + caps_width(caps),
    };
    active_width.max(inactive_width)
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
    active_title: bool,
) -> PagePlan {
    let marker_width = marker_budget(display, tab_count, caps, active_title);
    let fixed_width = 2 * CAPSULE_PADDING_BUDGET + caps_width(caps);
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

#[cfg(test)]
fn marker_text(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    display: IslandDisplayConfig,
    caps: IslandCapsConfig,
) -> String {
    marker_text_for_active(ws, tab_idx, ws.active_tab, display, caps, 0)
}

#[cfg(test)]
fn active_title_marker_text(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    display: IslandDisplayConfig,
    active_title: bool,
) -> String {
    marker_text_for_active(
        ws,
        tab_idx,
        ws.active_tab,
        display,
        IslandCapsConfig::Round,
        if active_title {
            ACTIVE_TITLE_MAX_WIDTH
        } else {
            0
        },
    )
}

fn marker_text_for_active(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    active_tab: usize,
    display: IslandDisplayConfig,
    caps: IslandCapsConfig,
    active_title_budget: usize,
) -> String {
    let active = tab_idx == active_tab;
    if !active && caps == IslandCapsConfig::Round && display == IslandDisplayConfig::Dots {
        return ROUND_DOT.to_string();
    }
    // A real name supersedes the numbers-mode index entirely — upstream's
    // index is only the default text an unnamed tab falls back to — while
    // the dots circle stays as the mode's shape mark beside the title.
    let titled = |mark: Option<&str>, untitled: String| {
        if active_title_budget == 0 {
            return untitled;
        }
        let Some(title) = ws
            .tabs
            .get(tab_idx)
            .and_then(|tab| tab.custom_name.as_deref())
        else {
            return untitled;
        };
        let fixed_width = mark.map_or(0, |mark| display_width(mark) + 1) + 2;
        let untitled_width = display_width(&untitled);
        if fixed_width >= active_title_budget || untitled_width > active_title_budget {
            return untitled;
        }
        let title = truncate_end(title, active_title_budget - fixed_width);
        if title.is_empty() {
            return untitled;
        }
        let padding = untitled_width.saturating_sub(fixed_width + display_width(&title));
        match mark {
            Some(mark) => format!(" {mark} {title}{} ", " ".repeat(padding)),
            None => format!(" {title}{} ", " ".repeat(padding)),
        }
    };
    match display {
        IslandDisplayConfig::Dots => {
            if active {
                titled(Some(ROUND_DOT), "   ".to_string())
            } else {
                "●".to_string()
            }
        }
        IslandDisplayConfig::Numbers => {
            let number = (tab_idx + 1).to_string();
            if active {
                titled(None, format!(" {number} "))
            } else if caps == IslandCapsConfig::Round {
                format!("{LEFT_CAP}{number}{RIGHT_CAP}")
            } else {
                number
            }
        }
        IslandDisplayConfig::Labels => {
            let name = ws
                .tab_display_name(tab_idx)
                .unwrap_or_else(|| (tab_idx + 1).to_string());
            if !active {
                let label = truncate_end(&name, LABEL_MAX_WIDTH - 2);
                return if caps == IslandCapsConfig::Round {
                    format!("{LEFT_CAP}{label}{RIGHT_CAP}")
                } else {
                    label
                };
            }
            let width = (display_width(&name) + 2).clamp(3, LABEL_MAX_WIDTH);
            let label = truncate_end(&name, width - 2);
            let padding = width - 2 - display_width(&label);
            format!(" {label}{} ", " ".repeat(padding))
        }
    }
}

fn layout_for_display(
    app: &AppState,
    area: Rect,
    display: IslandDisplayConfig,
    active_title: bool,
) -> Option<IslandLayout> {
    let active_tab = app
        .active
        .and_then(|idx| app.workspaces.get(idx))?
        .active_tab;
    layout_for_display_active(app, area, display, active_tab, active_title)
}

fn layout_for_display_active(
    app: &AppState,
    area: Rect,
    display: IslandDisplayConfig,
    active_tab: usize,
    active_title: bool,
) -> Option<IslandLayout> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let ws = app.active.and_then(|idx| app.workspaces.get(idx))?;
    if ws.tabs.is_empty() {
        return None;
    }

    let page = page_plan(
        ws.tabs.len(),
        active_tab,
        usize::from(area.width),
        display,
        app.island.caps,
        active_title,
    );
    let page_end = (page.start + page.page_size).min(ws.tabs.len());
    let candidate_layout = |candidate_active, active_title_budget| {
        let marker_texts = (page.start..page_end)
            .map(|tab_idx| {
                (
                    tab_idx,
                    marker_text_for_active(
                        ws,
                        tab_idx,
                        candidate_active,
                        display,
                        app.island.caps,
                        active_title_budget,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let marker_width = marker_texts
            .iter()
            .map(|(_, text)| display_width(text))
            .sum::<usize>()
            + marker_texts.len().saturating_sub(1) * MARKER_GAP
            + caps_width(app.island.caps);
        let first_marker_has_cap = marker_texts.first().is_some_and(|(tab_idx, _)| {
            *tab_idx == candidate_active
                || (app.island.caps == IslandCapsConfig::Round
                    && matches!(
                        display,
                        IslandDisplayConfig::Numbers | IslandDisplayConfig::Labels
                    ))
        });
        let last_marker_has_cap = marker_texts.last().is_some_and(|(tab_idx, _)| {
            *tab_idx == candidate_active
                || (app.island.caps == IslandCapsConfig::Round
                    && matches!(
                        display,
                        IslandDisplayConfig::Numbers | IslandDisplayConfig::Labels
                    ))
        });
        let left_padding = capsule_padding(
            app.island.caps,
            page.indicator_width == 0 && first_marker_has_cap,
        );
        let right_padding = capsule_padding(app.island.caps, last_marker_has_cap);
        let capsule_width = caps_width(app.island.caps)
            + left_padding
            + right_padding
            + page.indicator_width
            + marker_width;
        (marker_texts, left_padding, right_padding, capsule_width)
    };
    let maximum_title_budget = if active_title
        && matches!(
            display,
            IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers
        ) {
        ACTIVE_TITLE_MAX_WIDTH
    } else {
        0
    };
    let minimum_title_budget = match display {
        IslandDisplayConfig::Dots => 5,
        IslandDisplayConfig::Numbers => 3,
        IslandDisplayConfig::Labels => 1,
    };
    let (
        active_title_budget,
        (marker_texts, left_padding, _right_padding, content_width),
        capsule_width,
    ) = (minimum_title_budget..=maximum_title_budget)
        .rev()
        .chain(std::iter::once(0))
        .find_map(|active_title_budget| {
            let current = candidate_layout(active_tab, active_title_budget);
            let capsule_width = (page.start..page_end)
                .filter(|candidate_active| *candidate_active != active_tab)
                .map(|candidate_active| candidate_layout(candidate_active, active_title_budget).3)
                .fold(current.3, usize::max);
            (capsule_width <= usize::from(area.width)).then_some((
                active_title_budget,
                current,
                capsule_width,
            ))
        })?;

    let content_offset = (capsule_width - content_width) / 2;
    let capsule_width = capsule_width as u16;
    let capsule_x = match app.island.position {
        IslandPositionConfig::Center => area.x + area.width.saturating_sub(capsule_width) / 2,
        IslandPositionConfig::Left => area.x,
    };
    let capsule = Rect::new(capsule_x, area.y, capsule_width, 1);
    let round_caps = app.island.caps == IslandCapsConfig::Round;
    let mut x =
        capsule.x + content_offset as u16 + left_padding as u16 + if round_caps { 1 } else { 0 };
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
        let active = tab_idx == active_tab;
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

    Some(IslandLayout {
        display,
        active_title_budget,
        capsule,
        indicator,
        markers,
    })
}

fn layout(app: &AppState, area: Rect) -> Option<IslandLayout> {
    layout_for_display(app, area, app.island.display, app.island.active_title).or_else(|| match app
        .island
        .display
    {
        IslandDisplayConfig::Labels => {
            layout_for_display(app, area, IslandDisplayConfig::Dots, false)
        }
        _ => None,
    })
}

fn marker_visual_width(
    layout: &IslandLayout,
    tab_idx: usize,
    active_tab: usize,
    caps: IslandCapsConfig,
) -> Option<f32> {
    let marker = layout
        .markers
        .iter()
        .find(|marker| marker.tab_idx == tab_idx)?;
    let cap_width = if caps == IslandCapsConfig::Round && tab_idx == active_tab {
        2.0
    } else {
        0.0
    };
    Some(f32::from(marker.rect.width) + cap_width)
}

fn animation_endpoints(
    app: &AppState,
    area: Rect,
    from_tab: usize,
    to_tab: usize,
) -> Option<AnimationEndpoints> {
    let endpoint_layouts = |display: IslandDisplayConfig, active_title| {
        let from = layout_for_display_active(app, area, display, from_tab, active_title)?;
        let to = layout_for_display_active(app, area, display, to_tab, active_title)?;
        from.markers
            .iter()
            .map(|marker| marker.tab_idx)
            .eq(to.markers.iter().map(|marker| marker.tab_idx))
            .then_some((from, to))
    };
    let (display, (from, to)) = endpoint_layouts(app.island.display, app.island.active_title)
        .map(|layouts| (app.island.display, layouts))
        .or_else(|| match app.island.display {
            IslandDisplayConfig::Labels => {
                endpoint_layouts(IslandDisplayConfig::Dots, false).map(|layouts| {
                    tracing::debug!(
                        "island labels animation fell back to dots geometry \
                         (mismatched page membership between endpoints)"
                    );
                    (IslandDisplayConfig::Dots, layouts)
                })
            }
            _ => None,
        })?;
    let settled_from = ParticipantWidths::new(
        marker_visual_width(&from, from_tab, from_tab, app.island.caps)?,
        marker_visual_width(&from, to_tab, from_tab, app.island.caps)?,
    );
    let settled_to = ParticipantWidths::new(
        marker_visual_width(&to, from_tab, to_tab, app.island.caps)?,
        marker_visual_width(&to, to_tab, to_tab, app.island.caps)?,
    );
    let renders_titles = matches!(
        display,
        IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers
    ) && app
        .active
        .and_then(|idx| app.workspaces.get(idx))
        .is_some_and(|ws| {
            [(&from, from_tab), (&to, to_tab)]
                .into_iter()
                .any(|(layout, tab_idx)| {
                    let untitled =
                        marker_text_for_active(ws, tab_idx, tab_idx, display, app.island.caps, 0);
                    layout
                        .markers
                        .iter()
                        .find(|marker| marker.tab_idx == tab_idx)
                        .is_some_and(|marker| marker.text != untitled)
                })
        });
    Some(AnimationEndpoints {
        display,
        renders_titles,
        from,
        to,
        settled_from,
        settled_to,
    })
}

pub(crate) fn island_animation_for_tab_change(
    app: &AppState,
    area: Rect,
    from_tab: usize,
    to_tab: usize,
) -> Option<IslandAnim> {
    let endpoints = animation_endpoints(app, area, from_tab, to_tab)?;
    let (mut outgoing_width, mut incoming_width, mut capsule_total) =
        if let Some(current) = app.island_anim.filter(|anim| anim.to_tab == from_tab) {
            // Springs carry per-tab visual state: the old incoming spring is
            // the new outgoing tab's own geometry. The old outgoing spring
            // belongs to the tab we were leaving, so it only carries over when
            // reversing back to that tab; a third tab's marker has been
            // rendered settled all along and starts from its actual geometry.
            let incoming_width = if current.from_tab == to_tab {
                current.outgoing_width
            } else {
                IslandSpring::new(
                    endpoints.settled_from.incoming,
                    endpoints.settled_to.incoming,
                )
            };
            (
                current.incoming_width,
                incoming_width,
                current.capsule_total,
            )
        } else {
            (
                IslandSpring::new(
                    endpoints.settled_from.outgoing,
                    endpoints.settled_to.outgoing,
                ),
                IslandSpring::new(
                    endpoints.settled_from.incoming,
                    endpoints.settled_to.incoming,
                ),
                IslandSpring::new(endpoints.settled_from.total(), endpoints.settled_to.total()),
            )
        };
    outgoing_width.retarget(endpoints.settled_to.outgoing);
    incoming_width.retarget(endpoints.settled_to.incoming);
    capsule_total.retarget(endpoints.settled_to.total());
    quantized_participant_widths(
        endpoints.display,
        app.island.caps,
        ParticipantWidths::new(outgoing_width.position, incoming_width.position),
        endpoints.renders_titles,
    )?;
    Some(IslandAnim {
        from_tab,
        to_tab,
        display: endpoints.display,
        page_start: endpoints
            .from
            .markers
            .first()
            .map_or(0, |marker| marker.tab_idx),
        page_len: endpoints.from.markers.len(),
        outgoing_width,
        incoming_width,
        capsule_total,
    })
}

fn activation(position: f32, inactive: f32, active: f32) -> f32 {
    if (active - inactive).abs() <= f32::EPSILON {
        return 1.0;
    }
    normalized_progress((position - inactive) / (active - inactive))
}

fn layout_animated(app: &AppState, area: Rect) -> Option<AnimatedIslandLayout> {
    let anim = app.island_anim?;
    let endpoints = animation_endpoints(app, area, anim.from_tab, anim.to_tab)?;
    // A mid-flight geometry change (e.g. a resize paginating labels apart so
    // the endpoints fall back to dots) must not flip display modes on screen:
    // cut the animated overlay and render settled; the springs settle quietly.
    if endpoints.display != anim.display {
        return None;
    }
    // Same for a resize that changes the page plan (markers leaving the page,
    // the indicator appearing): the springs were tuned to the starting page's
    // geometry, so render settled instead of jumping the capsule.
    if endpoints
        .from
        .markers
        .first()
        .map_or(0, |marker| marker.tab_idx)
        != anim.page_start
        || endpoints.from.markers.len() != anim.page_len
    {
        return None;
    }
    // Non-participant width (other markers, indicator, padding, and reserved
    // slack) can differ between endpoints. Keep its interpolation on the
    // invisible capsule-total spring for continuous internal accounting; the
    // reserved capsule rect itself remains fixed.
    let fixed_from = f32::from(endpoints.from.capsule.width) - endpoints.settled_from.total();
    let fixed_to = f32::from(endpoints.to.capsule.width) - endpoints.settled_to.total();
    // Dots/numbers moves between an edge and an interior tab conserve the
    // participant total while the conditional endpoint padding still differs,
    // so the capsule spring's range can be zero-length and its activation
    // degenerates to 1.0. Fall back to a participant spring's travel so the
    // fixed width keeps interpolating instead of jumping a cell.
    let progress_sources = [
        (
            anim.capsule_total.position,
            endpoints.settled_from.total(),
            endpoints.settled_to.total(),
        ),
        (
            anim.incoming_width.position,
            endpoints.settled_from.incoming,
            endpoints.settled_to.incoming,
        ),
        (
            anim.outgoing_width.position,
            endpoints.settled_from.outgoing,
            endpoints.settled_to.outgoing,
        ),
    ];
    let fixed_progress = progress_sources
        .into_iter()
        .find(|(_, from, to)| (to - from).abs() > f32::EPSILON)
        .map_or(1.0, |(position, from, to)| activation(position, from, to));
    let fixed_width = lerp(fixed_from, fixed_to, fixed_progress);
    let widths = ParticipantWidths::new(anim.outgoing_width.position, anim.incoming_width.position);
    let quantized_widths = quantized_participant_widths(
        endpoints.display,
        app.island.caps,
        widths,
        endpoints.renders_titles,
    )?;
    let outgoing_activation = activation(
        widths.outgoing,
        endpoints.settled_to.outgoing,
        endpoints.settled_from.outgoing,
    );
    let incoming_activation = activation(
        widths.incoming,
        endpoints.settled_from.incoming,
        endpoints.settled_to.incoming,
    );
    let matches = |actual: f32, expected: f32| (actual - expected).abs() <= f32::EPSILON;

    Some(AnimatedIslandLayout {
        from: endpoints.from,
        to: endpoints.to,
        display: endpoints.display,
        widths: quantized_widths,
        capsule_total: anim.capsule_total.position,
        fixed_width,
        outgoing_activation,
        incoming_activation,
        outgoing_velocity: anim.outgoing_width.velocity,
        incoming_velocity: anim.incoming_width.velocity,
        at_from: matches(widths.outgoing, endpoints.settled_from.outgoing)
            && matches(widths.incoming, endpoints.settled_from.incoming)
            && matches(anim.capsule_total.position, endpoints.settled_from.total()),
        at_to: matches(widths.outgoing, endpoints.settled_to.outgoing)
            && matches(widths.incoming, endpoints.settled_to.incoming)
            && matches(anim.capsule_total.position, endpoints.settled_to.total()),
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
        ..TabBarView::default()
    }
}

fn positional_fg(app: &AppState, tab_idx: usize, active_tab: usize) -> Color {
    if tab_idx < active_tab {
        app.palette.overlay1
    } else {
        app.palette.overlay0
    }
}

fn render_settled_layout(
    app: &AppState,
    frame: &mut Frame,
    layout: &IslandLayout,
    active_tab: usize,
) {
    let p = &app.palette;
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

    if let Some((rect, text)) = &layout.indicator {
        frame.render_widget(
            Paragraph::new(text.as_str()).style(Style::default().fg(p.overlay0).bg(p.surface0)),
            *rect,
        );
    }

    for marker in &layout.markers {
        let active = active_tab == marker.tab_idx;
        let inactive_number_stadium = !active
            && round_caps
            && matches!(
                layout.display,
                IslandDisplayConfig::Numbers | IslandDisplayConfig::Labels
            );
        let style = if active {
            Style::default().fg(panel_contrast_fg(p)).bg(p.accent)
        } else {
            Style::default()
                .fg(positional_fg(app, marker.tab_idx, active_tab))
                .bg(if inactive_number_stadium {
                    p.surface1
                } else {
                    p.surface0
                })
        };
        frame.render_widget(
            Paragraph::new(marker.text.as_str()).style(style),
            marker.rect,
        );
        if inactive_number_stadium {
            let cap_style = Style::default().fg(p.surface1).bg(p.surface0);
            frame.buffer_mut()[(marker.rect.x, marker.rect.y)]
                .set_symbol(LEFT_CAP)
                .set_style(cap_style);
            frame.buffer_mut()[(marker.rect.right() - 1, marker.rect.y)]
                .set_symbol(RIGHT_CAP)
                .set_style(cap_style);
        } else if active && round_caps {
            let cap_style = Style::default().fg(p.accent).bg(p.surface0);
            frame.buffer_mut()[(marker.rect.x - 1, marker.rect.y)]
                .set_symbol(LEFT_CAP)
                .set_style(cap_style);
            frame.buffer_mut()[(marker.rect.right(), marker.rect.y)]
                .set_symbol(RIGHT_CAP)
                .set_style(cap_style);
        }
    }
}

fn render_capsule(
    frame: &mut Frame,
    rect: Rect,
    caps: IslandCapsConfig,
    fill: Color,
    under: Color,
) {
    if rect.width == 0 {
        return;
    }
    if caps == IslandCapsConfig::Round {
        if rect.width < 2 {
            return;
        }
        frame.buffer_mut()[(rect.x, rect.y)]
            .set_symbol(LEFT_CAP)
            .set_style(Style::default().fg(fill).bg(under));
        frame.buffer_mut()[(rect.right() - 1, rect.y)]
            .set_symbol(RIGHT_CAP)
            .set_style(Style::default().fg(fill).bg(under));
        for x in rect.x + 1..rect.right() - 1 {
            frame.buffer_mut()[(x, rect.y)]
                .set_symbol(" ")
                .set_style(Style::default().bg(fill));
        }
    } else {
        for x in rect.x..rect.right() {
            frame.buffer_mut()[(x, rect.y)]
                .set_symbol(" ")
                .set_style(Style::default().bg(fill));
        }
    }
}

fn marker_visual_start(marker: &MarkerLayout, active_tab: usize, caps: IslandCapsConfig) -> f32 {
    f32::from(marker.rect.x)
        - if caps == IslandCapsConfig::Round && marker.tab_idx == active_tab {
            1.0
        } else {
            0.0
        }
}

fn animated_participant_rect(
    app: &AppState,
    animated: &AnimatedIslandLayout,
    tab_idx: usize,
    width: f32,
) -> Option<Rect> {
    let anim = app.island_anim?;
    let from = animated
        .from
        .markers
        .iter()
        .find(|marker| marker.tab_idx == tab_idx)?;
    let to = animated
        .to
        .markers
        .iter()
        .find(|marker| marker.tab_idx == tab_idx)?;
    let x = lerp(
        marker_visual_start(from, anim.from_tab, app.island.caps),
        marker_visual_start(to, anim.to_tab, app.island.caps),
        if tab_idx == anim.from_tab {
            1.0 - animated.outgoing_activation
        } else {
            animated.incoming_activation
        },
    );
    let x = x.round() as u16;
    Some(Rect::new(x, animated.from.capsule.y, width as u16, 1))
}

fn render_animated_content(
    app: &AppState,
    frame: &mut Frame,
    display: IslandDisplayConfig,
    active_title_budget: usize,
    tab_idx: usize,
    rect: Rect,
    fill: Color,
) {
    let Some(ws) = app.active.and_then(|idx| app.workspaces.get(idx)) else {
        return;
    };
    let text = marker_text_for_active(
        ws,
        tab_idx,
        tab_idx,
        display,
        app.island.caps,
        active_title_budget,
    );
    if display == IslandDisplayConfig::Dots && text == "   " {
        return;
    }
    let text_width = display_width_u16(&text);
    let cap_width = u16::from(app.island.caps == IslandCapsConfig::Round);
    let left = rect.x + cap_width;
    let right = rect.right().saturating_sub(cap_width);
    let available = right.saturating_sub(left);
    if available == 0 {
        return;
    }
    let (text, x) = if available < text_width {
        (truncate_end(&text, usize::from(available)), left)
    } else {
        (text, left + (available - text_width) / 2)
    };
    let text_width = display_width_u16(&text);
    frame.render_widget(
        Paragraph::new(text).style(
            Style::default()
                .fg(panel_contrast_fg(&app.palette))
                .bg(fill),
        ),
        Rect::new(x, rect.y, text_width, 1),
    );
}

fn render_animated_layout(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    animated: &AnimatedIslandLayout,
) {
    let Some(anim) = app.island_anim else {
        return;
    };
    let p = &app.palette;
    let crossfade = app.island.motion == IslandMotionConfig::Smooth;
    // The lifecycle/fixed-width chain remains live, but never paints the rect.
    debug_assert!((animated.fixed_width + animated.capsule_total).is_finite());
    debug_assert_eq!(animated.from.capsule, animated.to.capsule);
    debug_assert_eq!(
        animated.from.active_title_budget,
        animated.to.active_title_budget
    );
    render_capsule(
        frame,
        animated.to.capsule,
        app.island.caps,
        p.surface0,
        p.panel_bg,
    );

    if let (Some((from_rect, _)), Some((to_rect, text))) =
        (&animated.from.indicator, &animated.to.indicator)
    {
        let x = lerp(
            f32::from(from_rect.x),
            f32::from(to_rect.x),
            animated.incoming_activation,
        )
        .round() as u16;
        frame.render_widget(
            Paragraph::new(text.as_str()).style(Style::default().fg(p.overlay0).bg(p.surface0)),
            Rect::new(x, area.y, to_rect.width, 1),
        );
    }

    for (from, to) in animated.from.markers.iter().zip(&animated.to.markers) {
        if from.tab_idx == anim.from_tab || from.tab_idx == anim.to_tab {
            continue;
        }
        let x = lerp(
            f32::from(from.rect.x),
            f32::from(to.rect.x),
            animated.incoming_activation,
        )
        .round() as u16;
        let inactive_number_stadium = app.island.caps == IslandCapsConfig::Round
            && matches!(
                animated.display,
                IslandDisplayConfig::Numbers | IslandDisplayConfig::Labels
            );
        let bg = if inactive_number_stadium {
            p.surface1
        } else {
            p.surface0
        };
        let rect = Rect::new(x, area.y, to.rect.width, 1);
        frame.render_widget(
            Paragraph::new(to.text.as_str()).style(
                Style::default()
                    .fg(positional_fg(app, to.tab_idx, anim.to_tab))
                    .bg(bg),
            ),
            rect,
        );
        if inactive_number_stadium {
            let cap_style = Style::default().fg(p.surface1).bg(p.surface0);
            frame.buffer_mut()[(rect.x, rect.y)]
                .set_symbol(LEFT_CAP)
                .set_style(cap_style);
            frame.buffer_mut()[(rect.right() - 1, rect.y)]
                .set_symbol(RIGHT_CAP)
                .set_style(cap_style);
        }
    }

    let Some(outgoing_rect) =
        animated_participant_rect(app, animated, anim.from_tab, animated.widths.outgoing)
    else {
        return;
    };
    let Some(incoming_rect) =
        animated_participant_rect(app, animated, anim.to_tab, animated.widths.incoming)
    else {
        return;
    };
    let outgoing_tone = positional_fg(app, anim.from_tab, anim.to_tab);
    let incoming_tone = positional_fg(app, anim.to_tab, anim.from_tab);
    let host_theme = &app.host_terminal_theme;
    // "reset"-aliased tones have no channels when the host never reported its
    // default foreground (Windows skips the OSC 10/4 query entirely), which
    // would hold the from-color for the whole crossfade and pop at the end.
    // Approximate with the theme's own text token for the blend — and when
    // that is itself "reset", with the appearance-matched stock foreground —
    // so the chain always ends concrete. Settled frames still paint the
    // genuine Reset.
    let resolve_reset = |color: Color| {
        if color != Color::Reset || host_theme.foreground.is_some() {
            return color;
        }
        if p.text != Color::Reset {
            return p.text;
        }
        let (r, g, b) = stock_palette_channels(match app.host_terminal_appearance {
            Some(crate::terminal_theme::HostAppearance::Light) => 0,
            _ => 7,
        });
        Color::Rgb(r, g, b)
    };
    // The accent fills the pill body's background, so a reset accent means the
    // terminal's default background — resolve it with background semantics
    // (host-reported background, then the theme's panel token, then the
    // appearance-matched stock background) rather than the foreground chain.
    let resolve_reset_accent = |color: Color| {
        if color != Color::Reset {
            return color;
        }
        if let Some(rgb) = host_theme.background {
            return Color::Rgb(rgb.r, rgb.g, rgb.b);
        }
        if p.panel_bg != Color::Reset {
            return p.panel_bg;
        }
        let (r, g, b) = stock_palette_channels(match app.host_terminal_appearance {
            Some(crate::terminal_theme::HostAppearance::Light) => 15,
            _ => 0,
        });
        Color::Rgb(r, g, b)
    };
    let (outgoing_fill, incoming_fill) = if crossfade {
        (
            brighten(
                lerp_hsl(
                    resolve_reset(outgoing_tone),
                    resolve_reset_accent(p.accent),
                    animated.outgoing_activation,
                    host_theme,
                ),
                animated.outgoing_velocity,
                host_theme,
            ),
            brighten(
                lerp_hsl(
                    resolve_reset(incoming_tone),
                    resolve_reset_accent(p.accent),
                    animated.incoming_activation,
                    host_theme,
                ),
                animated.incoming_velocity,
                host_theme,
            ),
        )
    } else if animated_content_visible(animated.incoming_activation) {
        (outgoing_tone, p.accent)
    } else {
        (p.accent, incoming_tone)
    };
    render_capsule(
        frame,
        outgoing_rect,
        app.island.caps,
        outgoing_fill,
        p.surface0,
    );
    render_capsule(
        frame,
        incoming_rect,
        app.island.caps,
        incoming_fill,
        p.surface0,
    );
    if animated_content_visible(animated.outgoing_activation) {
        render_animated_content(
            app,
            frame,
            animated.display,
            animated.to.active_title_budget,
            anim.from_tab,
            outgoing_rect,
            outgoing_fill,
        );
    }
    if animated_content_visible(animated.incoming_activation) {
        render_animated_content(
            app,
            frame,
            animated.display,
            animated.to.active_title_budget,
            anim.to_tab,
            incoming_rect,
            incoming_fill,
        );
    }
}

pub(super) fn render_tab_bar(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize))
            .style(Style::default().bg(app.palette.panel_bg)),
        area,
    );

    let animated = if app.island_anim.is_some() && app.island.motion != IslandMotionConfig::Off {
        layout_animated(app, area)
    } else {
        None
    };
    if let Some(animated) = animated {
        let Some(anim) = app.island_anim else {
            return;
        };
        if animated.at_from {
            render_settled_layout(app, frame, &animated.from, anim.from_tab);
        } else if animated.at_to {
            render_settled_layout(app, frame, &animated.to, anim.to_tab);
        } else {
            render_animated_layout(app, frame, area, &animated);
        }
        return;
    }

    let Some(layout) = layout(app, area) else {
        return;
    };
    let Some(active_tab) = app
        .active
        .and_then(|idx| app.workspaces.get(idx))
        .map(|ws| ws.active_tab)
    else {
        return;
    };
    render_settled_layout(app, frame, &layout, active_tab);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::{AppState, IslandAnim};
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn app_with_tabs(tab_count: usize, active_tab: usize) -> AppState {
        let mut app = AppState::test_new();
        app.island.active_title = false;
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

    fn rendered_buffer(app: &AppState, area: Rect) -> Buffer {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(app, frame, area))
            .expect("draw island");
        terminal.backend().buffer().clone()
    }

    fn rendered_animated_content_buffer(
        app: &AppState,
        display: IslandDisplayConfig,
        active_title_budget: usize,
        tab_idx: usize,
        rect: Rect,
    ) -> Buffer {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                render_capsule(
                    frame,
                    rect,
                    app.island.caps,
                    app.palette.accent,
                    app.palette.surface0,
                );
                render_animated_content(
                    app,
                    frame,
                    display,
                    active_title_budget,
                    tab_idx,
                    rect,
                    app.palette.accent,
                );
            })
            .expect("draw animated content");
        terminal.backend().buffer().clone()
    }

    fn animated_app(
        display: IslandDisplayConfig,
        motion: IslandMotionConfig,
        area: Rect,
    ) -> AppState {
        let mut app = app_with_tabs(2, 1);
        app.island.display = display;
        app.island.motion = motion;
        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        app
    }

    fn set_spring_frame(
        app: &mut AppState,
        outgoing: f32,
        incoming: f32,
        capsule_total: f32,
        outgoing_velocity: f32,
        incoming_velocity: f32,
    ) {
        let anim = app.island_anim.as_mut().expect("animation state");
        anim.outgoing_width.position = outgoing;
        anim.outgoing_width.velocity = outgoing_velocity;
        anim.incoming_width.position = incoming;
        anim.incoming_width.velocity = incoming_velocity;
        anim.capsule_total.position = capsule_total;
    }

    #[test]
    fn hsl_lerp_preserves_endpoints_and_crosses_the_short_hue_arc() {
        let from = Color::Rgb(255, 0, 0);
        let to = Color::Rgb(0, 255, 0);
        let host = crate::terminal_theme::TerminalTheme::default();

        assert_eq!(lerp_hsl(from, to, 0.0, &host), from);
        assert_eq!(lerp_hsl(from, to, 0.5, &host), Color::Rgb(255, 255, 0));
        assert_eq!(lerp_hsl(from, to, 1.0, &host), to);
        assert_eq!(brighten(from, 0.0, &host), from);
        assert_eq!(
            brighten(Color::Rgb(100, 100, 100), 100.0, &host),
            Color::Rgb(108, 108, 108)
        );
    }

    #[test]
    fn capsule_total_progress_still_interpolates_the_invisible_fixed_width() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(2, 1);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.active_title = true;
        app.island.motion = IslandMotionConfig::Smooth;
        app.workspaces[0].tabs[0].set_custom_name("a".to_string());
        app.workspaces[0].tabs[1].set_custom_name("really really really long label".to_string());
        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        assert!(app.island_anim.is_some(), "titled move animates");

        let endpoints = animation_endpoints(&app, area, 0, 1).expect("endpoints");
        let settled_from = endpoints.settled_from;
        let settled_to = endpoints.settled_to;
        assert!(
            (settled_from.total() - settled_to.total()).abs() > f32::EPSILON,
            "fixture must change the participant total (got {} -> {})",
            settled_from.total(),
            settled_to.total()
        );
        assert_eq!(endpoints.from.capsule, endpoints.to.capsule);
        let fixed_from = f32::from(endpoints.from.capsule.width) - settled_from.total();
        let fixed_to = f32::from(endpoints.to.capsule.width) - settled_to.total();
        assert!(
            (fixed_from - fixed_to).abs() > f32::EPSILON,
            "fixture must exercise redistributed slack ({fixed_from} vs {fixed_to})"
        );

        set_spring_frame(
            &mut app,
            (settled_from.outgoing + settled_to.outgoing) / 2.0,
            (settled_from.incoming + settled_to.incoming) / 2.0,
            (settled_from.total() + settled_to.total()) / 2.0,
            0.0,
            0.0,
        );
        let animated = layout_animated(&app, area).expect("animated layout");
        let expected = (fixed_from + fixed_to) / 2.0;
        assert!(
            (animated.fixed_width - expected).abs() <= 0.01,
            "invisible fixed width must interpolate on capsule-total travel: got {}, want {expected}",
            animated.fixed_width
        );
    }

    #[test]
    fn rapid_retarget_reuses_springs_only_for_the_tab_they_represent() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(3, 1);
        app.island.display = IslandDisplayConfig::Labels;
        app.island.motion = IslandMotionConfig::Smooth;
        for (idx, label) in ["aa", "beeeeee", "c"].iter().enumerate() {
            app.workspaces[0].tabs[idx].set_custom_name((*label).into());
        }

        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        let mid = {
            let anim = app.island_anim.as_mut().expect("first animation");
            anim.outgoing_width.position += 0.4;
            anim.outgoing_width.velocity = -3.0;
            anim.incoming_width.position += 0.3;
            anim.incoming_width.velocity = 2.5;
            *anim
        };

        // Onward to a third tab: the shared pill spring carries, but the
        // third tab starts from its own settled geometry, not the departed
        // tab's mid-flight spring.
        app.workspaces[0].switch_tab(2);
        let onward = island_animation_for_tab_change(&app, area, 1, 2).expect("onward animation");
        let onward_endpoints = animation_endpoints(&app, area, 1, 2).expect("onward endpoints");
        assert_eq!(
            (
                onward.outgoing_width.position,
                onward.outgoing_width.velocity
            ),
            (mid.incoming_width.position, mid.incoming_width.velocity),
            "the outgoing tab keeps its own in-flight spring"
        );
        assert_eq!(
            (
                onward.incoming_width.position,
                onward.incoming_width.velocity
            ),
            (onward_endpoints.settled_from.incoming, 0.0),
            "a third tab starts from its actual settled geometry"
        );

        // Reversing back reuses the departed tab's own spring.
        app.workspaces[0].switch_tab(0);
        app.island_anim = Some(mid);
        let reversed =
            island_animation_for_tab_change(&app, area, 1, 0).expect("reversed animation");
        assert_eq!(
            (
                reversed.incoming_width.position,
                reversed.incoming_width.velocity
            ),
            (mid.outgoing_width.position, mid.outgoing_width.velocity),
            "reversal continues the original tab's spring"
        );
    }

    #[test]
    fn two_immediate_switches_cut_an_under_width_animation() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(3, 0);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.caps = IslandCapsConfig::Round;
        app.island.motion = IslandMotionConfig::Smooth;

        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        app.workspaces[0].switch_tab(1);
        app.island_anim = island_animation_for_tab_change(&app, area, 1, 2);
        app.workspaces[0].switch_tab(2);

        assert!(app.island_anim.is_none());
        let mut settled = app_with_tabs(3, 2);
        settled.island = app.island;
        assert_eq!(rendered_buffer(&app, area), rendered_buffer(&settled, area));
    }

    #[test]
    fn mid_flight_display_fallback_cuts_the_animated_overlay() {
        let wide = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(3, 1);
        app.island.display = IslandDisplayConfig::Labels;
        app.island.motion = IslandMotionConfig::Smooth;
        for (idx, label) in ["alpha-one-long", "beta-two-long", "gamma-three-long"]
            .iter()
            .enumerate()
        {
            app.workspaces[0].tabs[idx].set_custom_name((*label).into());
        }
        app.island_anim = island_animation_for_tab_change(&app, wide, 0, 1);
        let anim = app.island_anim.expect("labels animation");
        assert_eq!(anim.display, IslandDisplayConfig::Labels);
        assert!(
            layout_animated(&app, wide).is_some(),
            "unchanged geometry keeps animating"
        );

        // Narrowing paginates the labels apart; the endpoints fall back to
        // dots geometry, which must cut the overlay instead of flipping
        // display modes on screen mid-flight.
        let narrow = Rect::new(0, 0, 30, 1);
        let narrow_endpoints = animation_endpoints(&app, narrow, 0, 1).expect("fallback endpoints");
        assert_eq!(
            narrow_endpoints.display,
            IslandDisplayConfig::Dots,
            "fixture must trigger the labels-to-dots fallback"
        );
        assert!(layout_animated(&app, narrow).is_none());
    }

    #[test]
    fn rapid_double_retarget_before_first_tick_cuts_instead_of_panicking() {
        // Two next-tab presses inside one tick leave both participants at the
        // inactive width, so round caps cannot fit on either side.
        assert_eq!(
            quantized_participant_widths(
                IslandDisplayConfig::Dots,
                IslandCapsConfig::Round,
                ParticipantWidths::new(1.0, 1.0),
                false,
            ),
            None
        );

        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(3, 1);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.caps = IslandCapsConfig::Round;
        app.island.motion = IslandMotionConfig::Smooth;
        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        assert!(app.island_anim.is_some(), "first switch animates");
        app.workspaces[0].switch_tab(2);
        app.island_anim = island_animation_for_tab_change(&app, area, 1, 2);
        assert!(
            app.island_anim.is_none(),
            "an under-width onward retarget is refused at creation"
        );
        // Rendering after the refused retarget must not panic.
        let _ = rendered_buffer(&app, area);
    }

    #[test]
    fn page_plan_change_on_resize_cuts_the_animated_overlay() {
        let wide = Rect::new(0, 0, 60, 1);
        let mut app = app_with_tabs(8, 1);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.motion = IslandMotionConfig::Smooth;
        app.island_anim = island_animation_for_tab_change(&app, wide, 0, 1);
        let anim = app.island_anim.expect("animation");
        assert!(
            layout_animated(&app, wide).is_some(),
            "unchanged geometry keeps animating"
        );

        // Narrowing shrinks the page: markers leave, the indicator appears,
        // but both endpoints stay in the same block and the display stays
        // dots — the page-plan guard must still cut the overlay.
        let narrow = Rect::new(0, 0, 20, 1);
        let narrow_endpoints = animation_endpoints(&app, narrow, 0, 1).expect("narrow endpoints");
        assert_eq!(narrow_endpoints.display, IslandDisplayConfig::Dots);
        assert!(
            narrow_endpoints.from.markers.len() != anim.page_len
                || narrow_endpoints
                    .from
                    .markers
                    .first()
                    .map_or(0, |marker| marker.tab_idx)
                    != anim.page_start,
            "fixture must change the page plan"
        );
        assert!(layout_animated(&app, narrow).is_none());
    }

    #[test]
    fn reset_tones_crossfade_via_theme_text_when_host_fg_is_unreported() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = animated_app(IslandDisplayConfig::Dots, IslandMotionConfig::Smooth, area);
        app.palette.overlay0 = Color::Reset;
        app.palette.overlay1 = Color::Reset;
        app.palette.text = Color::Rgb(210, 200, 190);
        assert!(app.host_terminal_theme.foreground.is_none());
        set_spring_frame(&mut app, 3.4, 2.6, 6.0, -20.0, 20.0);
        let animated = layout_animated(&app, area).expect("animated layout");
        let anim = app.island_anim.expect("animation state");
        let incoming =
            animated_participant_rect(&app, &animated, anim.to_tab, animated.widths.incoming)
                .expect("incoming participant");
        let buffer = rendered_buffer(&app, area);
        let host_theme = &app.host_terminal_theme;
        let expected = brighten(
            lerp_hsl(
                app.palette.text,
                app.palette.accent,
                animated.incoming_activation,
                host_theme,
            ),
            animated.incoming_velocity,
            host_theme,
        );
        assert!(
            matches!(expected, Color::Rgb(..)),
            "fixture must produce a concrete blend"
        );
        assert_eq!(
            buffer[(incoming.x, incoming.y)].style().fg,
            Some(expected),
            "reset tones blend through the theme text token instead of popping"
        );
    }

    #[test]
    fn reset_tones_blend_via_stock_foreground_when_theme_text_is_also_reset() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = animated_app(IslandDisplayConfig::Dots, IslandMotionConfig::Smooth, area);
        app.palette.overlay0 = Color::Reset;
        app.palette.overlay1 = Color::Reset;
        app.palette.text = Color::Reset;
        assert!(app.host_terminal_theme.foreground.is_none());
        assert!(app.host_terminal_appearance.is_none());
        set_spring_frame(&mut app, 3.4, 2.6, 6.0, -20.0, 20.0);
        let animated = layout_animated(&app, area).expect("animated layout");
        let anim = app.island_anim.expect("animation state");
        let incoming =
            animated_participant_rect(&app, &animated, anim.to_tab, animated.widths.incoming)
                .expect("incoming participant");
        let buffer = rendered_buffer(&app, area);
        let host_theme = &app.host_terminal_theme;
        let (r, g, b) = stock_palette_channels(7);
        let expected = brighten(
            lerp_hsl(
                Color::Rgb(r, g, b),
                app.palette.accent,
                animated.incoming_activation,
                host_theme,
            ),
            animated.incoming_velocity,
            host_theme,
        );
        assert!(matches!(expected, Color::Rgb(..)));
        assert_eq!(
            buffer[(incoming.x, incoming.y)].style().fg,
            Some(expected),
            "an all-reset theme still blends via the stock foreground"
        );
    }

    #[test]
    fn reset_accent_blends_with_background_semantics() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = animated_app(IslandDisplayConfig::Dots, IslandMotionConfig::Smooth, area);
        app.palette.accent = Color::Reset;
        let host_bg = crate::terminal_theme::RgbColor {
            r: 10,
            g: 10,
            b: 40,
        };
        app.host_terminal_theme.background = Some(host_bg);
        // A reported foreground must NOT hijack the accent's resolution.
        app.host_terminal_theme.foreground = Some(crate::terminal_theme::RgbColor {
            r: 240,
            g: 240,
            b: 240,
        });
        set_spring_frame(&mut app, 3.4, 2.6, 6.0, -20.0, 20.0);
        let animated = layout_animated(&app, area).expect("animated layout");
        let anim = app.island_anim.expect("animation state");
        let incoming =
            animated_participant_rect(&app, &animated, anim.to_tab, animated.widths.incoming)
                .expect("incoming participant");
        let buffer = rendered_buffer(&app, area);
        let host_theme = &app.host_terminal_theme;
        let expected = brighten(
            lerp_hsl(
                positional_fg(&app, anim.to_tab, anim.from_tab),
                Color::Rgb(host_bg.r, host_bg.g, host_bg.b),
                animated.incoming_activation,
                host_theme,
            ),
            animated.incoming_velocity,
            host_theme,
        );
        assert!(matches!(expected, Color::Rgb(..)));
        assert_eq!(
            buffer[(incoming.x, incoming.y)].style().fg,
            Some(expected),
            "a reset accent blends toward the host default background, not the foreground"
        );
    }

    #[test]
    fn crossfade_resolves_named_colors_through_the_host_palette() {
        let host_red = crate::terminal_theme::RgbColor {
            r: 250,
            g: 100,
            b: 50,
        };
        let host_fg = crate::terminal_theme::RgbColor {
            r: 200,
            g: 200,
            b: 190,
        };
        let mut host = crate::terminal_theme::TerminalTheme::default();
        host.palette[1] = Some(host_red);
        host.foreground = Some(host_fg);
        let unreported = crate::terminal_theme::TerminalTheme::default();

        assert_eq!(
            color_channels(Color::Red, &host),
            Some((host_red.r, host_red.g, host_red.b)),
            "reported palette slots must override the stock palette"
        );
        assert_eq!(
            color_channels(Color::Red, &host),
            color_channels(Color::Indexed(1), &host),
            "named ANSI and its index must resolve identically"
        );
        assert_eq!(
            color_channels(Color::Red, &unreported),
            Some(stock_palette_channels(1)),
            "unreported slots fall back to the stock ghostty palette"
        );
        assert_eq!(
            color_channels(Color::Reset, &host),
            Some((host_fg.r, host_fg.g, host_fg.b)),
            "the reset alias resolves through the host default foreground"
        );
        assert_eq!(
            color_channels(Color::Reset, &unreported),
            None,
            "reset stays unresolvable when the host never reported a foreground"
        );
        // Endpoint continuity: the first blend step away from the settled frame
        // must depart from the host's actual color, not a stock approximation.
        assert_eq!(
            lerp_hsl(Color::Red, Color::Rgb(0, 0, 255), 0.0, &host),
            Color::Red
        );
        let near_start = lerp_hsl(Color::Red, Color::Rgb(0, 0, 255), 0.001, &host);
        let Color::Rgb(r, g, b) = near_start else {
            panic!("blend should produce concrete rgb: {near_start:?}");
        };
        assert!(
            i16::from(r).abs_diff(i16::from(host_red.r)) <= 2
                && i16::from(g).abs_diff(i16::from(host_red.g)) <= 2
                && i16::from(b).abs_diff(i16::from(host_red.b)) <= 2,
            "near-endpoint blend {near_start:?} must stay continuous with the host color {host_red:?}"
        );
    }

    #[test]
    fn animated_content_appears_only_over_sixty_percent() {
        assert!(!animated_content_visible(0.0));
        assert!(!animated_content_visible(0.6));
        assert!(animated_content_visible(0.600_001));
        assert!(animated_content_visible(1.0));
    }

    #[test]
    fn active_title_dots_content_renders_above_animation_gate() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(2, 1);
        app.island.active_title = true;
        app.island.display = IslandDisplayConfig::Dots;
        app.island.motion = IslandMotionConfig::Steps;
        app.workspaces[0].tabs[0].set_custom_name("a".into());
        app.workspaces[0].tabs[1].set_custom_name("work".into());
        let endpoints = animation_endpoints(&app, area, 0, 1).expect("titled endpoints");
        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        let progress = 0.95;
        set_spring_frame(
            &mut app,
            lerp(
                endpoints.settled_from.outgoing,
                endpoints.settled_to.outgoing,
                progress,
            ),
            lerp(
                endpoints.settled_from.incoming,
                endpoints.settled_to.incoming,
                progress,
            ),
            lerp(
                endpoints.settled_from.total(),
                endpoints.settled_to.total(),
                progress,
            ),
            0.0,
            0.0,
        );

        let row = rect_text(&rendered_buffer(&app, area), area);
        assert!(row.contains("⬤ work"), "row: {row:?}");
    }

    #[test]
    fn animated_content_clips_long_titles_and_preserves_short_frames() {
        let mut app = app_with_tabs(2, 1);
        app.island.active_title = true;
        app.island.display = IslandDisplayConfig::Numbers;
        app.workspaces[0].tabs[1].set_custom_name("really really really long label".to_string());
        let full_text = marker_text_for_active(
            &app.workspaces[0],
            1,
            1,
            IslandDisplayConfig::Numbers,
            app.island.caps,
            ACTIVE_TITLE_MAX_WIDTH,
        );
        assert_eq!(display_width(&full_text), ACTIVE_TITLE_MAX_WIDTH);

        let clipped_rect = Rect::new(2, 0, 8, 1);
        let clipped = rendered_animated_content_buffer(
            &app,
            IslandDisplayConfig::Numbers,
            ACTIVE_TITLE_MAX_WIDTH,
            1,
            clipped_rect,
        );
        let clipped_text = truncate_end(&full_text, 6);
        assert!(!clipped_text.trim().is_empty());
        assert_eq!(
            rect_text(&clipped, clipped_rect),
            format!("{LEFT_CAP}{clipped_text}{RIGHT_CAP}")
        );

        let full_rect = Rect::new(2, 0, 12, 1);
        let full = rendered_animated_content_buffer(
            &app,
            IslandDisplayConfig::Numbers,
            ACTIVE_TITLE_MAX_WIDTH,
            1,
            full_rect,
        );
        assert_eq!(
            rect_text(&full, full_rect),
            format!("{LEFT_CAP}{full_text}{RIGHT_CAP}")
        );

        app.workspaces[0].tabs[1].set_custom_name("docs".to_string());
        for (width, expected) in [
            (8, format!("{LEFT_CAP} docs {RIGHT_CAP}")),
            (10, format!("{LEFT_CAP}  docs  {RIGHT_CAP}")),
        ] {
            let rect = Rect::new(2, 0, width, 1);
            let buffer = rendered_animated_content_buffer(
                &app,
                IslandDisplayConfig::Numbers,
                ACTIVE_TITLE_MAX_WIDTH,
                1,
                rect,
            );
            assert_eq!(rect_text(&buffer, rect), expected);
        }
    }

    #[test]
    fn long_title_clips_during_incoming_and_outgoing_motion() {
        let area = Rect::new(0, 0, 80, 1);
        for long_tab in [0, 1] {
            let mut app = app_with_tabs(2, 1);
            app.island.active_title = true;
            app.island.display = IslandDisplayConfig::Numbers;
            app.island.motion = IslandMotionConfig::Smooth;
            for tab in &mut app.workspaces[0].tabs {
                tab.set_custom_name("docs".to_string());
            }
            app.workspaces[0].tabs[long_tab]
                .set_custom_name("really really really long label".to_string());
            let endpoints = animation_endpoints(&app, area, 0, 1).expect("titled endpoints");
            app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
            let (outgoing, incoming) = if long_tab == 0 {
                (9.0, endpoints.settled_from.incoming)
            } else {
                (endpoints.settled_to.outgoing, 9.0)
            };
            set_spring_frame(&mut app, outgoing, incoming, outgoing + incoming, 0.0, 0.0);

            let animated = layout_animated(&app, area).expect("animated titled layout");
            let activation = if long_tab == 0 {
                animated.outgoing_activation
            } else {
                animated.incoming_activation
            };
            assert!(animated_content_visible(activation));
            let width = if long_tab == 0 {
                animated.widths.outgoing
            } else {
                animated.widths.incoming
            };
            let rect = animated_participant_rect(&app, &animated, long_tab, width)
                .expect("long-title participant");
            let full_text = marker_text_for_active(
                &app.workspaces[0],
                long_tab,
                long_tab,
                IslandDisplayConfig::Numbers,
                app.island.caps,
                animated.to.active_title_budget,
            );
            let interior = Rect::new(rect.x + 1, rect.y, rect.width - 2, 1);
            let buffer = rendered_buffer(&app, area);
            assert_eq!(
                rect_text(&buffer, interior),
                truncate_end(&full_text, usize::from(interior.width))
            );
        }
    }

    #[test]
    fn labels_animated_path_renders_named_inactive_stadium() {
        let area = Rect::new(0, 0, 100, 1);
        let mut app = app_with_tabs(3, 1);
        app.island.display = IslandDisplayConfig::Labels;
        app.island.motion = IslandMotionConfig::Smooth;
        app.workspaces[0].tabs[2].set_custom_name("later".to_string());
        let endpoints = animation_endpoints(&app, area, 0, 1).expect("labels endpoints");
        app.island_anim = island_animation_for_tab_change(&app, area, 0, 1);
        let midpoint = |from: f32, to: f32| lerp(from, to, 0.5);
        set_spring_frame(
            &mut app,
            midpoint(
                endpoints.settled_from.outgoing,
                endpoints.settled_to.outgoing,
            ),
            midpoint(
                endpoints.settled_from.incoming,
                endpoints.settled_to.incoming,
            ),
            midpoint(endpoints.settled_from.total(), endpoints.settled_to.total()),
            0.0,
            0.0,
        );

        let animated = layout_animated(&app, area).expect("animated labels layout");
        assert!(!animated.at_from && !animated.at_to);
        let from = animated
            .from
            .markers
            .iter()
            .find(|marker| marker.tab_idx == 2)
            .expect("from inactive label");
        let to = animated
            .to
            .markers
            .iter()
            .find(|marker| marker.tab_idx == 2)
            .expect("to inactive label");
        let x = lerp(
            f32::from(from.rect.x),
            f32::from(to.rect.x),
            animated.incoming_activation,
        )
        .round() as u16;
        let rect = Rect::new(x, area.y, to.rect.width, 1);
        let buffer = rendered_buffer(&app, area);

        assert_eq!(
            rect_text(&buffer, rect),
            format!("{LEFT_CAP}later{RIGHT_CAP}")
        );
        for x in [rect.x, rect.right() - 1] {
            let style = buffer[(x, rect.y)].style();
            assert_eq!(style.fg, Some(app.palette.surface1));
            assert_eq!(style.bg, Some(app.palette.surface0));
        }
        assert_eq!(
            buffer[(rect.x + 1, rect.y)].style().bg,
            Some(app.palette.surface1)
        );
    }

    #[test]
    fn island_animation_state_defaults_unset() {
        let app = AppState::test_new();
        assert!(app.island_anim.is_none());

        let anim = IslandAnim {
            from_tab: 1,
            to_tab: 2,
            display: IslandDisplayConfig::Dots,
            page_start: 0,
            page_len: 3,
            outgoing_width: IslandSpring::new(5.0, 1.0),
            incoming_width: IslandSpring::new(1.0, 5.0),
            capsule_total: IslandSpring::new(6.0, 6.0),
        };
        assert_eq!((anim.from_tab, anim.to_tab), (1, 2));
    }

    #[test]
    fn animated_endpoints_are_cell_exact_settled_frames() {
        let area = Rect::new(0, 0, 80, 1);
        for display in [
            IslandDisplayConfig::Dots,
            IslandDisplayConfig::Numbers,
            IslandDisplayConfig::Labels,
        ] {
            let mut settled_from = app_with_tabs(2, 0);
            settled_from.island.display = display;
            let mut settled_to = app_with_tabs(2, 1);
            settled_to.island.display = display;
            let animated_from = animated_app(display, IslandMotionConfig::Smooth, area);
            let mut animated_to = animated_app(display, IslandMotionConfig::Smooth, area);
            let anim = animated_to.island_anim.as_mut().expect("animation state");
            anim.outgoing_width.position = anim.outgoing_width.target;
            anim.incoming_width.position = anim.incoming_width.target;
            anim.capsule_total.position = anim.capsule_total.target;

            assert_eq!(
                rendered_buffer(&animated_from, area),
                rendered_buffer(&settled_from, area),
                "initial springs must be the settled-from frame for {display:?}"
            );
            assert_eq!(
                rendered_buffer(&animated_to, area),
                rendered_buffer(&settled_to, area),
                "target springs must be the settled-to frame for {display:?}"
            );
        }
    }

    #[test]
    fn smooth_spring_frames_keep_caps_and_apply_hsl_velocity_color() {
        let area = Rect::new(0, 0, 80, 1);
        for (display, outgoing, incoming, total, expected_widths) in [
            (IslandDisplayConfig::Dots, 3.4, 2.6, 6.0, (3, 3)),
            (IslandDisplayConfig::Numbers, 4.4, 3.6, 8.0, (4, 4)),
            (IslandDisplayConfig::Labels, 3.4, 4.6, 8.0, (3, 5)),
        ] {
            let mut app = animated_app(display, IslandMotionConfig::Smooth, area);
            set_spring_frame(&mut app, outgoing, incoming, total, -20.0, 20.0);
            let animated = layout_animated(&app, area).expect("animated layout");
            let anim = app.island_anim.expect("animation state");
            let outgoing =
                animated_participant_rect(&app, &animated, anim.from_tab, animated.widths.outgoing)
                    .expect("outgoing participant");
            let incoming =
                animated_participant_rect(&app, &animated, anim.to_tab, animated.widths.incoming)
                    .expect("incoming participant");
            assert_eq!((outgoing.width, incoming.width), expected_widths);

            let buffer = rendered_buffer(&app, area);
            let host_theme = &app.host_terminal_theme;
            let outgoing_fill = brighten(
                lerp_hsl(
                    positional_fg(&app, anim.from_tab, anim.to_tab),
                    app.palette.accent,
                    animated.outgoing_activation,
                    host_theme,
                ),
                animated.outgoing_velocity,
                host_theme,
            );
            let incoming_fill = brighten(
                lerp_hsl(
                    positional_fg(&app, anim.to_tab, anim.from_tab),
                    app.palette.accent,
                    animated.incoming_activation,
                    host_theme,
                ),
                animated.incoming_velocity,
                host_theme,
            );
            for (rect, fill) in [(outgoing, outgoing_fill), (incoming, incoming_fill)] {
                assert_eq!(buffer[(rect.x, rect.y)].symbol(), LEFT_CAP, "{display:?}");
                assert_eq!(
                    buffer[(rect.right() - 1, rect.y)].symbol(),
                    RIGHT_CAP,
                    "{display:?}"
                );
                for x in [rect.x, rect.right() - 1] {
                    let style = buffer[(x, rect.y)].style();
                    assert_eq!(style.fg, Some(fill), "{display:?}");
                    assert_eq!(style.bg, Some(app.palette.surface0), "{display:?}");
                }
                if rect.width > 2 {
                    assert_eq!(buffer[(rect.x + 1, rect.y)].style().bg, Some(fill));
                }
            }
        }
    }

    #[test]
    fn animated_frame_sweep_never_renders_bare_participant_rects() {
        let area = Rect::new(0, 0, 80, 1);
        for display in [
            IslandDisplayConfig::Dots,
            IslandDisplayConfig::Numbers,
            IslandDisplayConfig::Labels,
        ] {
            for motion in [IslandMotionConfig::Smooth, IslandMotionConfig::Steps] {
                for sample in 1..100 {
                    let mut app = animated_app(display, motion, area);
                    let phase = sample as f32 / 100.0;
                    let anim = app.island_anim.as_mut().expect("animation state");
                    anim.outgoing_width.position = lerp(
                        anim.outgoing_width.position,
                        anim.outgoing_width.target,
                        phase,
                    );
                    anim.incoming_width.position = lerp(
                        anim.incoming_width.position,
                        anim.incoming_width.target,
                        phase,
                    );
                    anim.capsule_total.position = lerp(
                        anim.capsule_total.position,
                        anim.capsule_total.target,
                        phase,
                    );

                    let animated = layout_animated(&app, area).expect("animated layout");
                    let anim = app.island_anim.expect("animation state");
                    let outgoing = animated_participant_rect(
                        &app,
                        &animated,
                        anim.from_tab,
                        animated.widths.outgoing,
                    )
                    .expect("outgoing participant");
                    let incoming = animated_participant_rect(
                        &app,
                        &animated,
                        anim.to_tab,
                        animated.widths.incoming,
                    )
                    .expect("incoming participant");
                    let buffer = rendered_buffer(&app, area);
                    let reserved = layout(&app, area).expect("reserved layout").capsule;
                    assert_eq!(animated.from.capsule, reserved);
                    assert_eq!(animated.to.capsule, reserved);
                    assert_eq!(buffer[(reserved.x, reserved.y)].symbol(), LEFT_CAP);
                    assert_eq!(
                        buffer[(reserved.right() - 1, reserved.y)].symbol(),
                        RIGHT_CAP
                    );
                    if reserved.x > area.x {
                        assert_eq!(
                            buffer[(reserved.x - 1, reserved.y)].style().bg,
                            Some(app.palette.panel_bg)
                        );
                    }
                    if reserved.right() < area.right() {
                        assert_eq!(
                            buffer[(reserved.right(), reserved.y)].style().bg,
                            Some(app.palette.panel_bg)
                        );
                    }

                    for rect in [outgoing, incoming] {
                        assert!(rect.width >= 2, "{display:?} {motion:?} sample {sample}");
                        assert_eq!(
                            buffer[(rect.x, rect.y)].symbol(),
                            LEFT_CAP,
                            "{display:?} {motion:?} sample {sample}"
                        );
                        assert_eq!(
                            buffer[(rect.right() - 1, rect.y)].symbol(),
                            RIGHT_CAP,
                            "{display:?} {motion:?} sample {sample}"
                        );
                    }

                    let mut x = area.x;
                    let mut accent_runs = 0;
                    while x < area.right() {
                        let style = buffer[(x, area.y)].style();
                        if style.fg != Some(app.palette.accent)
                            && style.bg != Some(app.palette.accent)
                        {
                            x += 1;
                            continue;
                        }
                        let start = x;
                        while x < area.right() {
                            let style = buffer[(x, area.y)].style();
                            if style.fg != Some(app.palette.accent)
                                && style.bg != Some(app.palette.accent)
                            {
                                break;
                            }
                            x += 1;
                        }
                        assert_eq!(buffer[(start, area.y)].symbol(), LEFT_CAP);
                        assert_eq!(buffer[(x - 1, area.y)].symbol(), RIGHT_CAP);
                        accent_runs += 1;
                    }
                    if motion == IslandMotionConfig::Steps {
                        assert_eq!(accent_runs, 1);
                    }
                }
            }
        }
    }

    #[test]
    fn off_motion_is_byte_identical_even_with_stale_animation_state() {
        let area = Rect::new(0, 0, 40, 1);
        for display in [
            IslandDisplayConfig::Dots,
            IslandDisplayConfig::Numbers,
            IslandDisplayConfig::Labels,
        ] {
            let mut settled = app_with_tabs(2, 1);
            settled.island.display = display;
            settled.island.motion = IslandMotionConfig::Off;
            let animated = animated_app(display, IslandMotionConfig::Off, area);

            assert_eq!(
                rendered_buffer(&animated, area),
                rendered_buffer(&settled, area)
            );
        }
    }

    #[test]
    fn steps_motion_uses_whole_capsules_without_crossfade() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = animated_app(IslandDisplayConfig::Dots, IslandMotionConfig::Steps, area);
        set_spring_frame(&mut app, 3.4, 2.6, 6.0, -20.0, 20.0);
        let animated = layout_animated(&app, area).expect("animated layout");
        let anim = app.island_anim.expect("animation state");
        let outgoing =
            animated_participant_rect(&app, &animated, anim.from_tab, animated.widths.outgoing)
                .expect("outgoing participant");
        let incoming =
            animated_participant_rect(&app, &animated, anim.to_tab, animated.widths.incoming)
                .expect("incoming participant");
        assert_eq!((outgoing.width, incoming.width), (3, 3));

        let buffer = rendered_buffer(&app, area);
        for (rect, fill) in [
            (outgoing, app.palette.accent),
            (incoming, app.palette.overlay0),
        ] {
            assert_eq!(buffer[(rect.x, rect.y)].symbol(), LEFT_CAP);
            assert_eq!(buffer[(rect.right() - 1, rect.y)].symbol(), RIGHT_CAP);
            for x in [rect.x, rect.right() - 1] {
                assert_eq!(buffer[(x, rect.y)].style().fg, Some(fill));
            }
        }
    }

    #[test]
    fn animation_never_changes_settled_hit_areas() {
        let area = Rect::new(0, 0, 40, 1);
        let mut settled = app_with_tabs(2, 1);
        settled.island.display = IslandDisplayConfig::Labels;
        let animated = animated_app(
            IslandDisplayConfig::Labels,
            IslandMotionConfig::Smooth,
            area,
        );

        assert_eq!(
            compute_tab_bar_view(&animated, area).island_marker_hit_areas,
            compute_tab_bar_view(&settled, area).island_marker_hit_areas
        );
    }

    #[test]
    fn renders_all_display_modes_cell_exact() {
        let area = Rect::new(0, 0, 80, 1);
        for (display, expected) in [
            (
                IslandDisplayConfig::Dots,
                "\u{e0b6} ⬤ \u{e0b6}   \u{e0b4} ⬤ ⬤ \u{e0b4}",
            ),
            (
                IslandDisplayConfig::Numbers,
                "\u{e0b6}\u{e0b6}1\u{e0b4} \u{e0b6} 2 \u{e0b4} \u{e0b6}3\u{e0b4} \u{e0b6}4\u{e0b4}\u{e0b4}",
            ),
            (
                IslandDisplayConfig::Labels,
                "\u{e0b6}\u{e0b6}1\u{e0b4} \u{e0b6} work \u{e0b4} \u{e0b6}3\u{e0b4} \u{e0b6}4\u{e0b4}\u{e0b4}",
            ),
        ] {
            let mut app = app_with_tabs(4, 1);
            app.island.display = display;
            let layout = layout(&app, area).expect("island should fit");
            if display == IslandDisplayConfig::Numbers {
                assert_eq!(
                    compute_tab_bar_view(&app, area)
                        .island_marker_hit_areas
                        .iter()
                        .map(|rect| rect.width)
                        .collect::<Vec<_>>(),
                    vec![3, 3, 3, 3]
                );
            }
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
            " ●     ● ● "
        );
        assert_eq!(
            rect_text(terminal.backend().buffer(), layout.markers[0].rect),
            "●"
        );
    }

    #[test]
    fn renders_positional_colors_and_inactive_marker_clearance() {
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
        assert_eq!(layout.markers[0].rect.x, layout.capsule.x + 2);
        assert_eq!(
            layout.markers.last().expect("last marker").rect.right(),
            layout.capsule.right() - 2
        );
        for (marker_idx, fg) in [(0, app.palette.overlay1), (2, app.palette.overlay0)] {
            let cell = &buffer[(layout.markers[marker_idx].rect.x, area.y)];
            assert_eq!(cell.symbol(), ROUND_DOT);
            assert_eq!(cell.style().fg, Some(fg));
            assert_eq!(cell.style().bg, Some(app.palette.surface0));
        }
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
            assert_eq!(buffer[(x, area.y)].symbol(), " ");
            assert_eq!(style.bg, Some(app.palette.accent));
        }
        assert_eq!(
            buffer[(area.x, area.y)].style().bg,
            Some(app.palette.panel_bg)
        );
    }

    #[test]
    fn renders_inactive_stadium_palette() {
        let mut app = app_with_tabs(3, 1);
        app.workspaces[0].tabs[2].set_custom_name("later".to_string());
        let area = Rect::new(0, 0, 80, 1);

        for (display, labels) in [
            (IslandDisplayConfig::Numbers, ["1", "3"]),
            (IslandDisplayConfig::Labels, ["1", "later"]),
        ] {
            app.island.display = display;
            let layout = layout(&app, area).expect("stadium island");
            let buffer = rendered_buffer(&app, area);

            for ((marker_idx, fg), label) in [(0, app.palette.overlay1), (2, app.palette.overlay0)]
                .into_iter()
                .zip(labels)
            {
                let rect = layout.markers[marker_idx].rect;
                assert_eq!(
                    rect_text(&buffer, rect),
                    format!("{LEFT_CAP}{label}{RIGHT_CAP}")
                );
                for x in [rect.x, rect.right() - 1] {
                    let style = buffer[(x, rect.y)].style();
                    assert_eq!(style.fg, Some(app.palette.surface1));
                    assert_eq!(style.bg, Some(app.palette.surface0));
                }
                let body = &buffer[(rect.x + 1, rect.y)];
                assert_eq!(body.style().fg, Some(fg));
                assert_eq!(body.style().bg, Some(app.palette.surface1));
            }
        }
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
    fn capsule_width_is_stable_for_every_active_tab_on_the_page() {
        let area = Rect::new(0, 0, 200, 1);
        let mut app = app_with_tabs(4, 0);
        app.island.active_title = true;
        for (tab, name) in app.workspaces[0].tabs.iter_mut().zip([
            "a",
            "docs",
            "really really really long label",
            "test",
        ]) {
            tab.set_custom_name(name.to_string());
        }

        for display in [
            IslandDisplayConfig::Dots,
            IslandDisplayConfig::Numbers,
            IslandDisplayConfig::Labels,
        ] {
            let capsules = (0..app.workspaces[0].tabs.len())
                .map(|active_tab| {
                    layout_for_display_active(
                        &app,
                        area,
                        display,
                        active_tab,
                        app.island.active_title,
                    )
                    .expect("candidate layout")
                    .capsule
                })
                .collect::<Vec<_>>();
            assert!(
                capsules.windows(2).all(|pair| pair[0] == pair[1]),
                "capsule moved in {display:?}: {capsules:?}"
            );
        }
    }

    #[test]
    fn equal_candidate_page_reserves_exact_content_width() {
        let area = Rect::new(0, 0, 80, 1);
        let mut app = app_with_tabs(2, 0);
        app.island.display = IslandDisplayConfig::Numbers;

        for active_tab in 0..2 {
            let layout = layout_for_display_active(
                &app,
                area,
                IslandDisplayConfig::Numbers,
                active_tab,
                false,
            )
            .expect("equal-candidate layout");
            let first = layout.markers.first().expect("first marker");
            let last = layout.markers.last().expect("last marker");
            let first_x = first.rect.x - u16::from(first.tab_idx == active_tab);
            let last_x = last.rect.right() + u16::from(last.tab_idx == active_tab);

            assert_eq!(layout.capsule.width, 11);
            assert_eq!(first_x, layout.capsule.x + 1);
            assert_eq!(last_x, layout.capsule.right() - 1);
        }
    }

    #[test]
    fn narrow_active_titles_walk_down_while_the_untitled_page_fits() {
        for display in [IslandDisplayConfig::Dots, IslandDisplayConfig::Numbers] {
            let mut app = app_with_tabs(4, 0);
            app.island.display = display;
            app.island.active_title = true;
            app.workspaces[0].tabs[0].set_custom_name("xxxxxxxxxxxxxxxxxxxxxxxx".to_string());
            let mut samples = Vec::new();

            for width in 13..=20 {
                let area = Rect::new(0, 0, width, 1);
                assert!(layout_for_display_active(&app, area, display, 0, false).is_some());
                let layout = layout_for_display_active(&app, area, display, 0, true)
                    .expect("titled island should degrade while its untitled page fits");
                assert_eq!(
                    layout
                        .markers
                        .iter()
                        .map(|marker| marker.tab_idx)
                        .collect::<Vec<_>>(),
                    vec![0]
                );
                let marker = layout.markers.first().expect("active marker");
                samples.push((layout.active_title_budget, marker.text.matches('x').count()));
            }

            assert!(samples.windows(2).all(|pair| pair[0].0 <= pair[1].0));
            assert!(samples.windows(2).all(|pair| pair[0].1 <= pair[1].1));
            assert_eq!(
                samples.last().map(|sample| sample.0),
                Some(ACTIVE_TITLE_MAX_WIDTH)
            );
        }
    }

    #[test]
    fn reduced_title_budget_matches_settled_and_animated_content() {
        let area = Rect::new(0, 0, 17, 1);
        let mut app = app_with_tabs(4, 0);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.active_title = true;
        app.workspaces[0].tabs[0].set_custom_name("xxxxxxxxxxxxxxxxxxxxxxxx".to_string());
        let layout = layout(&app, area).expect("reduced-title layout");
        assert!(layout.active_title_budget > 0);
        assert!(layout.active_title_budget < ACTIVE_TITLE_MAX_WIDTH);
        let marker = layout.markers.first().expect("active marker");
        let participant = Rect::new(marker.rect.x - 1, marker.rect.y, marker.rect.width + 2, 1);
        let settled = rendered_buffer(&app, area);
        let animated = rendered_animated_content_buffer(
            &app,
            layout.display,
            layout.active_title_budget,
            marker.tab_idx,
            participant,
        );

        for x in participant.x..participant.right() {
            assert_eq!(settled[(x, participant.y)], animated[(x, participant.y)]);
        }
    }

    #[test]
    fn narrow_labels_fallback_matches_untitled_dots() {
        let area = Rect::new(0, 0, 15, 1);
        let mut labels = app_with_tabs(2, 1);
        labels.island.display = IslandDisplayConfig::Labels;
        labels.island.active_title = true;
        let mut dots = app_with_tabs(2, 1);
        dots.island.display = IslandDisplayConfig::Dots;
        dots.island.active_title = false;

        let fallback = layout(&labels, area).expect("labels fallback");
        let dots_layout = layout(&dots, area).expect("dots layout");
        assert_eq!(fallback.display, IslandDisplayConfig::Dots);
        assert_eq!(fallback.active_title_budget, 0);
        assert_eq!(fallback, dots_layout);
        assert_eq!(rendered_buffer(&labels, area), rendered_buffer(&dots, area));
    }

    #[test]
    fn reserved_slack_centers_content_and_hit_areas_in_both_positions() {
        let area = Rect::new(5, 2, 80, 1);
        let mut app = app_with_tabs(2, 0);
        app.island.display = IslandDisplayConfig::Dots;
        app.island.caps = IslandCapsConfig::Square;
        app.island.active_title = true;
        app.workspaces[0].tabs[0].set_custom_name("a".to_string());
        app.workspaces[0].tabs[1].set_custom_name("really really really long label".to_string());

        for position in [IslandPositionConfig::Center, IslandPositionConfig::Left] {
            app.island.position = position;
            let layout = layout(&app, area).expect("reserved layout");
            let marker_width = layout
                .markers
                .iter()
                .map(|marker| marker.rect.width)
                .sum::<u16>();
            let content_width = marker_width
                + layout.markers.len().saturating_sub(1) as u16 * MARKER_GAP as u16
                + 2;
            let slack = layout.capsule.width - content_width;
            assert!(slack > 0);
            let content_left = layout.markers.first().expect("first marker").rect.x - 1;
            let content_right = layout.markers.last().expect("last marker").rect.right() + 1;
            assert_eq!(content_left - layout.capsule.x, slack / 2);
            assert_eq!(layout.capsule.right() - content_right, slack - slack / 2);

            let hit_areas = compute_tab_bar_view(&app, area).island_marker_hit_areas;
            for marker in &layout.markers {
                assert_eq!(hit_areas[marker.tab_idx], marker.rect);
            }
            assert_eq!(
                layout.capsule.x,
                match position {
                    IslandPositionConfig::Center => {
                        area.x + (area.width - layout.capsule.width) / 2
                    }
                    IslandPositionConfig::Left => area.x,
                }
            );
        }
    }

    #[test]
    fn round_padding_follows_adjacent_rendered_elements() {
        let area = Rect::new(0, 0, 60, 1);
        let pill_first = layout(&app_with_tabs(2, 0), area).expect("two-tab island");
        assert_eq!(pill_first.markers[0].rect.x - 1, pill_first.capsule.x + 1);
        assert_eq!(
            pill_first.markers[1].rect.right() + 1,
            pill_first.capsule.right() - 1
        );

        for display in [IslandDisplayConfig::Numbers, IslandDisplayConfig::Labels] {
            let mut mini_last_app = app_with_tabs(2, 0);
            mini_last_app.island.display = display;
            let mini_last = layout(&mini_last_app, area).expect("mini-stadium last");
            assert_eq!(
                mini_last.markers[1].rect.right(),
                mini_last.capsule.right() - 1
            );

            let mut mini_first_app = app_with_tabs(2, 1);
            mini_first_app.island.display = display;
            let mini_first = layout(&mini_first_app, area).expect("mini-stadium first");
            assert_eq!(mini_first.markers[0].rect.x, mini_first.capsule.x + 1);
        }

        let pill_only = layout(&app_with_tabs(1, 0), area).expect("single-tab island");
        assert_eq!(pill_only.markers[0].rect.x - 1, pill_only.capsule.x + 1);
        assert_eq!(
            pill_only.markers[0].rect.right() + 1,
            pill_only.capsule.right() - 1
        );

        let mut paged_app = app_with_tabs(11, 10);
        paged_app.island.position = IslandPositionConfig::Left;
        let paged = layout(&paged_app, area).expect("paged island");
        assert_eq!(
            paged.indicator.expect("page indicator").0.x,
            paged.capsule.x + 2
        );
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
            false,
        );
        let same_page = page_plan(
            11,
            5,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
            false,
        );
        assert_eq!(first.page_size, 8);
        assert_eq!(first.start, 0);
        assert_eq!(same_page.start, first.start);

        app.workspaces[0].switch_tab(10);
        let next = page_plan(
            11,
            10,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
            false,
        );
        assert_eq!(next.start, 8);
        assert_eq!(next.total_pages, 2);
        let layout = layout(&app, area).expect("second page island");
        assert_eq!(
            layout
                .markers
                .iter()
                .map(|marker| marker.tab_idx)
                .collect::<Vec<_>>(),
            vec![8, 9, 10]
        );

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw island");
        assert_eq!(
            rect_text(terminal.backend().buffer(), layout.capsule),
            "\u{e0b6} ‹2/2›  ⬤ ⬤ \u{e0b6}   \u{e0b4} \u{e0b4}"
        );
    }

    #[test]
    fn marker_budgets_are_active_state_independent() {
        assert_eq!(capsule_padding(IslandCapsConfig::Round, true), 0);
        assert_eq!(capsule_padding(IslandCapsConfig::Round, false), 1);
        assert_eq!(capsule_padding(IslandCapsConfig::Square, true), 1);
        assert_eq!(capsule_padding(IslandCapsConfig::Square, false), 1);
        let untitled_budget = |display, caps| marker_budget(display, 11, caps, false);
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Dots, IslandCapsConfig::Round),
            5
        );
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Numbers, IslandCapsConfig::Round),
            6
        );
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Numbers, IslandCapsConfig::Square),
            4
        );
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Labels, IslandCapsConfig::Round),
            18
        );
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Labels, IslandCapsConfig::Square),
            LABEL_MAX_WIDTH
        );
        assert_eq!(
            untitled_budget(IslandDisplayConfig::Dots, IslandCapsConfig::Square),
            3
        );
        for display in [IslandDisplayConfig::Dots, IslandDisplayConfig::Numbers] {
            assert_eq!(
                marker_budget(display, 11, IslandCapsConfig::Round, true),
                ACTIVE_TITLE_MAX_WIDTH + 2
            );
            assert_eq!(
                marker_budget(display, 11, IslandCapsConfig::Square, true),
                ACTIVE_TITLE_MAX_WIDTH
            );
        }
    }

    #[test]
    fn active_title_composes_markers_and_preserves_untitled_forms() {
        let mut app = app_with_tabs(2, 1);
        let ws = &mut app.workspaces[0];

        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Dots, true),
            " ⬤ work "
        );
        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Numbers, true),
            " work "
        );

        ws.tabs[1].custom_name = None;
        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Dots, true),
            "   "
        );
        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Numbers, true),
            " 2 "
        );

        ws.tabs[1].set_custom_name("work".to_string());
        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Dots, false),
            "   "
        );
        assert_eq!(
            active_title_marker_text(ws, 1, IslandDisplayConfig::Numbers, false),
            " 2 "
        );
    }

    #[test]
    fn active_title_clamps_to_ten_cells() {
        let mut app = app_with_tabs(2, 1);
        app.workspaces[0].tabs[1].set_custom_name("abcdefghijklmnop".to_string());

        for (display, expected) in [
            (IslandDisplayConfig::Dots, " ⬤ abcde… "),
            (IslandDisplayConfig::Numbers, " abcdefg… "),
        ] {
            let text = active_title_marker_text(&app.workspaces[0], 1, display, true);
            assert_eq!(text, expected);
            assert_eq!(display_width(&text), ACTIVE_TITLE_MAX_WIDTH);
        }
    }

    #[test]
    fn active_label_clamps_to_three_through_sixteen_cells() {
        let mut app = app_with_tabs(2, 1);
        app.workspaces[0].tabs[1].set_custom_name("x".to_string());
        let short = marker_text(
            &app.workspaces[0],
            1,
            IslandDisplayConfig::Labels,
            IslandCapsConfig::Round,
        );
        assert_eq!(short, " x ");

        app.workspaces[0].tabs[1].set_custom_name("a very long island label".to_string());
        let long = marker_text(
            &app.workspaces[0],
            1,
            IslandDisplayConfig::Labels,
            IslandCapsConfig::Round,
        );

        assert_eq!(display_width(&long), LABEL_MAX_WIDTH);
        assert!(long.contains('…'));
    }

    #[test]
    fn active_label_never_narrows_or_reveals_less_of_the_name() {
        let mut app = app_with_tabs(2, 0);

        for name_len in 1..=LABEL_MAX_WIDTH * 2 {
            app.workspaces[0].tabs[0].set_custom_name("x".repeat(name_len));
            for caps in [IslandCapsConfig::Round, IslandCapsConfig::Square] {
                let active = marker_text_for_active(
                    &app.workspaces[0],
                    0,
                    0,
                    IslandDisplayConfig::Labels,
                    caps,
                    0,
                );
                let inactive = marker_text_for_active(
                    &app.workspaces[0],
                    0,
                    1,
                    IslandDisplayConfig::Labels,
                    caps,
                    0,
                );

                assert!(display_width(&active) + caps_width(caps) >= display_width(&inactive));
                assert!(active.matches('x').count() >= inactive.matches('x').count());
            }
        }
    }

    #[test]
    fn oversized_label_island_falls_back_to_a_clickable_dot() {
        let area = Rect::new(0, 0, 27, 1);
        let mut app = app_with_tabs(11, 10);
        app.island.display = IslandDisplayConfig::Labels;
        app.workspaces[0].tabs[10].set_custom_name("a very long island label".to_string());

        let hit_area = compute_tab_bar_view(&app, area).island_marker_hit_areas[10];
        assert!(hit_area.width > 0);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw fallback island");
        assert_eq!(
            terminal.backend().buffer()[(hit_area.x, hit_area.y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn inactive_markers_follow_the_display_and_caps_style() {
        let mut app = app_with_tabs(3, 2);
        assert_eq!(
            marker_text(
                &app.workspaces[0],
                0,
                IslandDisplayConfig::Dots,
                IslandCapsConfig::Round,
            ),
            ROUND_DOT
        );
        assert_eq!(
            marker_text(
                &app.workspaces[0],
                0,
                IslandDisplayConfig::Dots,
                IslandCapsConfig::Square,
            ),
            "●"
        );
        for (tab_idx, label) in [(0, "1"), (1, "work")] {
            assert_eq!(
                marker_text(
                    &app.workspaces[0],
                    tab_idx,
                    IslandDisplayConfig::Labels,
                    IslandCapsConfig::Round,
                ),
                format!("{LEFT_CAP}{label}{RIGHT_CAP}")
            );
            assert_eq!(
                marker_text(
                    &app.workspaces[0],
                    tab_idx,
                    IslandDisplayConfig::Labels,
                    IslandCapsConfig::Square,
                ),
                label
            );
        }
        assert_eq!(
            marker_text(
                &app.workspaces[0],
                0,
                IslandDisplayConfig::Numbers,
                IslandCapsConfig::Round,
            ),
            format!("{LEFT_CAP}1{RIGHT_CAP}")
        );
        assert_eq!(
            marker_text(
                &app.workspaces[0],
                0,
                IslandDisplayConfig::Numbers,
                IslandCapsConfig::Square,
            ),
            "1"
        );

        app.workspaces[0].tabs[0].set_custom_name("a very long inactive island label".to_string());
        let round = marker_text(
            &app.workspaces[0],
            0,
            IslandDisplayConfig::Labels,
            IslandCapsConfig::Round,
        );
        let square = marker_text(
            &app.workspaces[0],
            0,
            IslandDisplayConfig::Labels,
            IslandCapsConfig::Square,
        );
        assert_eq!(display_width(&round), LABEL_MAX_WIDTH);
        assert_eq!(display_width(&square), LABEL_MAX_WIDTH - 2);
        assert!(round.contains('…'));
        assert!(square.contains('…'));
    }
}
