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
/// `Reset` alone stays unresolvable (it has no defined channels).
fn color_channels(color: Color) -> Option<(u8, u8, u8)> {
    let named = |r, g, b| Some((r, g, b));
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => named(0, 0, 0),
        Color::Red => named(205, 49, 49),
        Color::Green => named(13, 188, 121),
        Color::Yellow => named(229, 229, 16),
        Color::Blue => named(36, 114, 200),
        Color::Magenta => named(188, 63, 188),
        Color::Cyan => named(17, 168, 205),
        Color::Gray => named(229, 229, 229),
        Color::DarkGray => named(102, 102, 102),
        Color::LightRed => named(241, 76, 76),
        Color::LightGreen => named(35, 209, 139),
        Color::LightYellow => named(245, 245, 67),
        Color::LightBlue => named(59, 142, 234),
        Color::LightMagenta => named(214, 112, 214),
        Color::LightCyan => named(41, 184, 219),
        Color::White => named(255, 255, 255),
        Color::Indexed(n) => Some(match n {
            0..=15 => {
                let base = [
                    (0, 0, 0),
                    (205, 49, 49),
                    (13, 188, 121),
                    (229, 229, 16),
                    (36, 114, 200),
                    (188, 63, 188),
                    (17, 168, 205),
                    (229, 229, 229),
                    (102, 102, 102),
                    (241, 76, 76),
                    (35, 209, 139),
                    (245, 245, 67),
                    (59, 142, 234),
                    (214, 112, 214),
                    (41, 184, 219),
                    (255, 255, 255),
                ];
                base[usize::from(n)]
            }
            16..=231 => {
                let n = n - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                (level(n / 36), level((n % 36) / 6), level(n % 6))
            }
            232..=255 => {
                let v = 8 + 10 * (n - 232);
                (v, v, v)
            }
        }),
        Color::Reset => None,
    }
}

fn rgb_to_hsl(color: Color) -> Option<(f32, f32, f32)> {
    let (r, g, b) = color_channels(color)?;
    let color = Color::Rgb(r, g, b);
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
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

fn lerp_hsl(from: Color, to: Color, progress: f32) -> Color {
    let progress = normalized_progress(progress);
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    let (Some((from_h, from_s, from_l)), Some((to_h, to_s, to_l))) =
        (rgb_to_hsl(from), rgb_to_hsl(to))
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

fn brighten(color: Color, velocity: f32) -> Color {
    let amount = (velocity.abs() * VELOCITY_BRIGHTNESS_SCALE).min(MAX_VELOCITY_BRIGHTNESS);
    if amount <= f32::EPSILON {
        return color;
    }
    let Some((hue, saturation, lightness)) = rgb_to_hsl(color) else {
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
) -> ParticipantWidths {
    let minimum = if caps == IslandCapsConfig::Round {
        2.0
    } else {
        1.0
    };
    match display {
        IslandDisplayConfig::Dots | IslandDisplayConfig::Numbers => {
            let total = widths.total().round();
            let outgoing = widths.outgoing.round().clamp(minimum, total - minimum);
            ParticipantWidths::new(outgoing, total - outgoing)
        }
        IslandDisplayConfig::Labels => ParticipantWidths::new(
            widths.outgoing.round().max(minimum),
            widths.incoming.round().max(minimum),
        ),
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

fn marker_budget(display: IslandDisplayConfig, tab_count: usize, caps: IslandCapsConfig) -> usize {
    let active_width = (match display {
        IslandDisplayConfig::Dots => 3,
        IslandDisplayConfig::Numbers => digits(tab_count) + 2,
        IslandDisplayConfig::Labels => LABEL_MAX_WIDTH,
    }) + caps_width(caps);
    let inactive_width = match (display, caps) {
        (IslandDisplayConfig::Dots | IslandDisplayConfig::Labels, _) => 1,
        (IslandDisplayConfig::Numbers, IslandCapsConfig::Round) => digits(tab_count) + 2,
        (IslandDisplayConfig::Numbers, IslandCapsConfig::Square) => digits(tab_count),
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
) -> PagePlan {
    let marker_width = marker_budget(display, tab_count, caps);
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
    marker_text_for_active(ws, tab_idx, ws.active_tab, display, caps)
}

fn marker_text_for_active(
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    active_tab: usize,
    display: IslandDisplayConfig,
    caps: IslandCapsConfig,
) -> String {
    let active = tab_idx == active_tab;
    if !active
        && caps == IslandCapsConfig::Round
        && matches!(
            display,
            IslandDisplayConfig::Dots | IslandDisplayConfig::Labels
        )
    {
        return ROUND_DOT.to_string();
    }
    match display {
        IslandDisplayConfig::Dots => {
            if active {
                "   ".to_string()
            } else {
                "●".to_string()
            }
        }
        IslandDisplayConfig::Numbers => {
            let number = (tab_idx + 1).to_string();
            if active {
                format!(" {number} ")
            } else if caps == IslandCapsConfig::Round {
                format!("{LEFT_CAP}{number}{RIGHT_CAP}")
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

fn layout_for_display(
    app: &AppState,
    area: Rect,
    display: IslandDisplayConfig,
) -> Option<IslandLayout> {
    let active_tab = app
        .active
        .and_then(|idx| app.workspaces.get(idx))?
        .active_tab;
    layout_for_display_active(app, area, display, active_tab)
}

fn layout_for_display_active(
    app: &AppState,
    area: Rect,
    display: IslandDisplayConfig,
    active_tab: usize,
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
    );
    let page_end = (page.start + page.page_size).min(ws.tabs.len());
    let marker_texts = (page.start..page_end)
        .map(|tab_idx| {
            (
                tab_idx,
                marker_text_for_active(ws, tab_idx, active_tab, display, app.island.caps),
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
        *tab_idx == active_tab
            || (app.island.caps == IslandCapsConfig::Round
                && display == IslandDisplayConfig::Numbers)
    });
    let last_marker_has_cap = marker_texts.last().is_some_and(|(tab_idx, _)| {
        *tab_idx == active_tab
            || (app.island.caps == IslandCapsConfig::Round
                && display == IslandDisplayConfig::Numbers)
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
    let mut x = capsule.x + left_padding as u16 + if round_caps { 1 } else { 0 };
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
        capsule,
        indicator,
        markers,
    })
}

fn layout(app: &AppState, area: Rect) -> Option<IslandLayout> {
    layout_for_display(app, area, app.island.display).or_else(|| match app.island.display {
        IslandDisplayConfig::Labels => layout_for_display(app, area, IslandDisplayConfig::Dots),
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
    let endpoint_layouts = |display: IslandDisplayConfig| {
        let from = layout_for_display_active(app, area, display, from_tab)?;
        let to = layout_for_display_active(app, area, display, to_tab)?;
        from.markers
            .iter()
            .map(|marker| marker.tab_idx)
            .eq(to.markers.iter().map(|marker| marker.tab_idx))
            .then_some((from, to))
    };
    let (display, (from, to)) = endpoint_layouts(app.island.display)
        .map(|layouts| (app.island.display, layouts))
        .or_else(|| match app.island.display {
            IslandDisplayConfig::Labels => {
                endpoint_layouts(IslandDisplayConfig::Dots).map(|layouts| {
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
    Some(AnimationEndpoints {
        display,
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
            (
                current.incoming_width,
                current.outgoing_width,
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
    Some(IslandAnim {
        from_tab,
        to_tab,
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
    // The non-participant width (caps, conditional endpoint padding, other
    // markers, indicator) can differ between the from- and to-layouts —
    // e.g. the pill moving to an edge flips the flush/clearance padding.
    // Interpolate it on the capsule spring's own travel so the animated
    // width lands exactly on the settled-to capsule, never snapping.
    let fixed_from = f32::from(endpoints.from.capsule.width) - endpoints.settled_from.total();
    let fixed_to = f32::from(endpoints.to.capsule.width) - endpoints.settled_to.total();
    let fixed_width = lerp(
        fixed_from,
        fixed_to,
        activation(
            anim.capsule_total.position,
            endpoints.settled_from.total(),
            endpoints.settled_to.total(),
        ),
    );
    let widths = ParticipantWidths::new(anim.outgoing_width.position, anim.incoming_width.position);
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
        widths: quantized_participant_widths(endpoints.display, app.island.caps, widths),
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
    display: IslandDisplayConfig,
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
        let inactive_number_stadium =
            !active && round_caps && display == IslandDisplayConfig::Numbers;
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
    tab_idx: usize,
    rect: Rect,
    fill: Color,
) {
    if display == IslandDisplayConfig::Dots {
        return;
    }
    let Some(ws) = app.active.and_then(|idx| app.workspaces.get(idx)) else {
        return;
    };
    let text = marker_text_for_active(ws, tab_idx, tab_idx, display, app.island.caps);
    let text_width = display_width_u16(&text);
    let cap_width = u16::from(app.island.caps == IslandCapsConfig::Round);
    let left = rect.x + cap_width;
    let right = rect.right().saturating_sub(cap_width);
    if right.saturating_sub(left) < text_width {
        return;
    }
    let x = left + (right - left - text_width) / 2;
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
    let capsule_width = (animated.fixed_width + animated.capsule_total)
        .round()
        .clamp(1.0, f32::from(area.width)) as u16;
    let capsule_x = match app.island.position {
        IslandPositionConfig::Center => area.x + area.width.saturating_sub(capsule_width) / 2,
        IslandPositionConfig::Left => area.x,
    };
    render_capsule(
        frame,
        Rect::new(capsule_x, area.y, capsule_width, 1),
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
            && animated.display == IslandDisplayConfig::Numbers;
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
    let (outgoing_fill, incoming_fill) = if crossfade {
        (
            brighten(
                lerp_hsl(outgoing_tone, p.accent, animated.outgoing_activation),
                animated.outgoing_velocity,
            ),
            brighten(
                lerp_hsl(incoming_tone, p.accent, animated.incoming_activation),
                animated.incoming_velocity,
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
            render_settled_layout(app, frame, &animated.from, anim.from_tab, animated.display);
        } else if animated.at_to {
            render_settled_layout(app, frame, &animated.to, anim.to_tab, animated.display);
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
    render_settled_layout(app, frame, &layout, active_tab, app.island.display);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::{AppState, IslandAnim};
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

    fn rendered_buffer(app: &AppState, area: Rect) -> Buffer {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(app, frame, area))
            .expect("draw island");
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

        assert_eq!(lerp_hsl(from, to, 0.0), from);
        assert_eq!(lerp_hsl(from, to, 0.5), Color::Rgb(255, 255, 0));
        assert_eq!(lerp_hsl(from, to, 1.0), to);
        assert_eq!(brighten(from, 0.0), from);
        assert_eq!(
            brighten(Color::Rgb(100, 100, 100), 100.0),
            Color::Rgb(108, 108, 108)
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
    fn island_animation_state_defaults_unset() {
        let app = AppState::test_new();
        assert!(app.island_anim.is_none());

        let anim = IslandAnim {
            from_tab: 1,
            to_tab: 2,
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
            let outgoing_fill = brighten(
                lerp_hsl(
                    positional_fg(&app, anim.from_tab, anim.to_tab),
                    app.palette.accent,
                    animated.outgoing_activation,
                ),
                animated.outgoing_velocity,
            );
            let incoming_fill = brighten(
                lerp_hsl(
                    positional_fg(&app, anim.to_tab, anim.from_tab),
                    app.palette.accent,
                    animated.incoming_activation,
                ),
                animated.incoming_velocity,
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
                "\u{e0b6} ⬤ \u{e0b6} work \u{e0b4} ⬤ ⬤ \u{e0b4}",
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
    fn renders_inactive_number_stadium_palette() {
        let mut app = app_with_tabs(3, 1);
        app.island.display = IslandDisplayConfig::Numbers;
        let area = Rect::new(0, 0, 40, 1);
        let layout = layout(&app, area).expect("number island");
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_tab_bar(&app, frame, area))
            .expect("draw number island");
        let buffer = terminal.backend().buffer();

        for (marker_idx, digit, fg) in [
            (0, "1", app.palette.overlay1),
            (2, "3", app.palette.overlay0),
        ] {
            let rect = layout.markers[marker_idx].rect;
            assert_eq!(
                rect_text(buffer, rect),
                format!("{LEFT_CAP}{digit}{RIGHT_CAP}")
            );
            for x in [rect.x, rect.right() - 1] {
                let style = buffer[(x, rect.y)].style();
                assert_eq!(style.fg, Some(app.palette.surface1));
                assert_eq!(style.bg, Some(app.palette.surface0));
            }
            let digit = &buffer[(rect.x + 1, rect.y)];
            assert_eq!(digit.style().fg, Some(fg));
            assert_eq!(digit.style().bg, Some(app.palette.surface1));
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
    fn round_padding_follows_adjacent_rendered_elements() {
        let area = Rect::new(0, 0, 60, 1);
        let pill_first = layout(&app_with_tabs(2, 0), area).expect("two-tab island");
        assert_eq!(pill_first.markers[0].rect.x - 1, pill_first.capsule.x + 1);
        assert_eq!(
            pill_first.markers[1].rect.right() + 1,
            pill_first.capsule.right() - 1
        );

        let mut mini_last_app = app_with_tabs(2, 0);
        mini_last_app.island.display = IslandDisplayConfig::Numbers;
        let mini_last = layout(&mini_last_app, area).expect("mini-stadium last");
        assert_eq!(
            mini_last.markers[1].rect.right(),
            mini_last.capsule.right() - 1
        );

        let mut mini_first_app = app_with_tabs(2, 1);
        mini_first_app.island.display = IslandDisplayConfig::Numbers;
        let mini_first = layout(&mini_first_app, area).expect("mini-stadium first");
        assert_eq!(mini_first.markers[0].rect.x, mini_first.capsule.x + 1);

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
        );
        let same_page = page_plan(
            11,
            5,
            60,
            IslandDisplayConfig::Dots,
            IslandCapsConfig::Round,
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
            "\u{e0b6} ‹2/2›  ⬤ ⬤ \u{e0b6}   \u{e0b4}\u{e0b4}"
        );
    }

    #[test]
    fn marker_budgets_are_active_state_independent() {
        assert_eq!(capsule_padding(IslandCapsConfig::Round, true), 0);
        assert_eq!(capsule_padding(IslandCapsConfig::Round, false), 1);
        assert_eq!(capsule_padding(IslandCapsConfig::Square, true), 1);
        assert_eq!(capsule_padding(IslandCapsConfig::Square, false), 1);
        assert_eq!(
            marker_budget(IslandDisplayConfig::Dots, 11, IslandCapsConfig::Round),
            5
        );
        assert_eq!(
            marker_budget(IslandDisplayConfig::Numbers, 11, IslandCapsConfig::Round),
            6
        );
        assert_eq!(
            marker_budget(IslandDisplayConfig::Numbers, 11, IslandCapsConfig::Square,),
            4
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
    fn inactive_markers_follow_the_caps_style() {
        let app = app_with_tabs(2, 1);
        for display in [IslandDisplayConfig::Dots, IslandDisplayConfig::Labels] {
            assert_eq!(
                marker_text(&app.workspaces[0], 0, display, IslandCapsConfig::Round),
                ROUND_DOT
            );
            assert_eq!(
                marker_text(&app.workspaces[0], 0, display, IslandCapsConfig::Square),
                "●"
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
    }
}
