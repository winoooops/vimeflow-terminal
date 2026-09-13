//! Adapter between herdr's agent panel and the watcher's card renderer.
//!
//! The cards themselves belong to `herdr_agent_watcher::sidebar::view`: header,
//! task line, model, context/cache/cost, tools and traces are all its work, and
//! duplicating any of it here would mean two implementations free to drift.
//! What stays is the part the watcher cannot know — which pane a card is for,
//! the workspace it lives in, and how herdr's palette paints the result.

use std::borrow::Cow;

use herdr_agent_watcher::daemon::store::{CardState, PaneTelemetry};
use herdr_agent_watcher::sidebar::config::{AgentMark, ToolCallStyle};
use herdr_agent_watcher::sidebar::layout::{self, LineSpan};
use herdr_agent_watcher::sidebar::style::AgentAppearances;
use herdr_agent_watcher::sidebar::view::{
    self, CardCtx, Line, Role, Semantic, Style as WatcherStyle,
};
use ratatui::style::{Color, Modifier, Style};

use crate::app::state::Palette;
use crate::detect::AgentState;

/// Trace rows an unfocused card shows. The anchor renders its whole ring so
/// navigation is not limited to a window the selection can fall outside of.
const UNFOCUSED_TRACE_ROWS: u8 = 5;

/// `view::compact_card` is header, task and metrics. Named here because it is
/// the unit herdr reserves panel space in.
const COMPACT_CARD_LINES: usize = 3;

/// Columns held back from the card so its right-justified content never lands
/// on the panel's last cell.
///
/// The watcher justifies the state glyph flush right and its lines measure
/// exactly the width they were given, so by `unicode_width` nothing overflows.
/// But the marks it uses — `●`, `◐`, `○` — are East Asian *Ambiguous* width:
/// `unicode_width` calls them one cell, and a terminal configured wide for CJK
/// draws them as two. The extra cell then falls off the edge and the glyph
/// renders as a sliver. Two columns cover a wide mark at each end, and read as
/// deliberate padding when the terminal draws them narrow.
const RIGHT_GUTTER: u16 = 2;

pub(crate) struct CardInput<'a> {
    pub workspace: &'a str,
    pub name: &'a str,
    pub task: Option<&'a str>,
    pub state: AgentState,
    pub seen: bool,
    pub telemetry: Option<&'a PaneTelemetry>,
    /// `toolUseId` selected on this card, set only for the trace anchor. Some
    /// also means "render the full ring".
    pub trace_focus: Option<&'a str>,
}

pub(crate) struct BuiltCard {
    pub lines: Vec<Line>,
    /// `(toolUseId, span)` for every selectable trace row actually rendered,
    /// in card-local line coordinates. Display-only rows export nothing, so a
    /// hit-test can never select one.
    pub trace_spans: Vec<(String, LineSpan)>,
}

pub(crate) fn build_card(
    input: CardInput<'_>,
    width: u16,
    body_height: u16,
    expanded: bool,
) -> BuiltCard {
    let width = width.saturating_sub(RIGHT_GUTTER).max(1);

    // The watcher only knows panes it has bound. For the rest herdr stands in
    // with an empty telemetry carrying its own detector state, so an unbound
    // pane still gets a card and still reads working/blocked rather than
    // defaulting to idle. Where the watcher *has* bound a pane its lifecycle
    // events are the better source, so its `card_state` is left alone — and
    // telemetry with a title stays borrowed because this runs for every card
    // on every frame.
    let mut telemetry = match input.telemetry {
        Some(telemetry) => Cow::Borrowed(telemetry),
        None => {
            let mut telemetry = PaneTelemetry::with_agent(input.name);
            telemetry.card_state = card_state(input.state, input.seen);
            Cow::Owned(telemetry)
        }
    };
    if telemetry
        .title
        .as_ref()
        .and_then(|title| title.get("title"))
        .and_then(serde_json::Value::as_str)
        .is_none()
    {
        if let Some(task) = input.task.filter(|task| !task.is_empty()) {
            telemetry.to_mut().title = Some(serde_json::json!({ "title": task }));
        }
    }
    let telemetry = telemetry.as_ref();

    // herdr's identity, injected through the hook the watcher provides for it:
    // the watcher's own task line is `cwd › task` and has no notion of a
    // workspace, which is the one thing a herdr user needs to tell 23 cards
    // apart.
    let cwd_label = cwd_label(input.workspace, telemetry);

    let appearances = AgentAppearances::new();
    let mut ctx = CardCtx {
        appearances: &appearances,
        mark: AgentMark::default(),
        tool_calls: ToolCallStyle::default(),
        trace_lines: UNFOCUSED_TRACE_ROWS,
        plan_usage: false,
        cwd_label: Some(&cwd_label),
        width,
        selected: false,
        trace_focus: input.trace_focus,
        now_unix_ms: now_unix_ms(),
    };

    // How much of the panel a card may occupy is herdr's call, not the
    // watcher's: it owns the list, and an expanded card that swallows the panel
    // would hide every other agent. Reserve one compact card so a neighbour
    // always survives.
    let reserve = if body_height as usize > COMPACT_CARD_LINES {
        COMPACT_CARD_LINES
    } else {
        0
    };
    let budget = (body_height as usize).saturating_sub(reserve);
    let (mut lines, mut spans) = match (expanded, input.trace_focus.is_some()) {
        // A focused card renders its whole ring, and the watcher does that
        // regardless of `trace_lines` — so shrinking that knob cannot make it
        // fit, and trying would fall through to the traceless compact card:
        // descending into traces would render no traces. The reserve yields
        // instead; this is the card the reader descended into.
        (true, true) => view::expanded_card_with_traces(telemetry, &ctx),
        (true, false) => fit_expanded(telemetry, &mut ctx, budget),
        (false, _) => (view::compact_card(telemetry, &ctx), Vec::new()),
    };

    // On short panels, keep compact identity and leave at least one trace row.
    if let Some(first) = spans
        .iter()
        .map(|(_, line)| *line)
        .min()
        .filter(|first| input.trace_focus.is_some() && *first >= body_height as usize)
    {
        let mut compact = view::compact_card(telemetry, &ctx);
        compact.truncate(body_height.saturating_sub(1) as usize);
        let removed = first - compact.len();
        lines.splice(..first, compact);
        for (_, line) in &mut spans {
            *line -= removed;
        }
    }

    // A focused ring is routinely taller than the panel, and herdr scrolls the
    // agents panel by whole entries, so without this the selection walks off
    // the bottom and cannot be scrolled to.
    let spans = if let Some(focus) = input.trace_focus {
        scroll_traces_into_view(&mut lines, spans, focus, body_height)
    } else {
        spans
    };

    lines.truncate(body_height as usize);
    // Spans past the fold would hit-test to rows that are not on screen.
    let trace_spans = spans
        .into_iter()
        .filter(|(_, line)| *line < lines.len())
        .map(|(id, line)| {
            (
                id,
                LineSpan {
                    start: line,
                    height: 1,
                },
            )
        })
        .collect();

    BuiltCard { lines, trace_spans }
}

/// Scrolls the trace section so the selected row is on screen, keeping the
/// card's preamble anchored.
///
/// The trace rows are the only scrollable part of a card: the header, task and
/// metric lines above them are what identify the card, so scrolling the whole
/// thing would push the reader's anchor off the top. Only rows between the
/// first trace and the selection are dropped, and the offset comes from the
/// watcher's own `ensure_visible` so this agrees with how it scrolls its list.
///
/// Returns the spans rebased onto the scrolled lines; rows scrolled off the top
/// are dropped, so they can neither be clicked nor mistaken for visible.
fn scroll_traces_into_view(
    lines: &mut Vec<Line>,
    spans: Vec<(String, usize)>,
    focus: &str,
    body_height: u16,
) -> Vec<(String, usize)> {
    let Some(first) = spans.iter().map(|(_, line)| *line).min() else {
        return spans;
    };
    let Some(selected) = spans
        .iter()
        .find(|(id, _)| id == focus)
        .map(|(_, line)| *line)
    else {
        return spans;
    };
    let viewport = (body_height as usize).saturating_sub(first);
    if viewport == 0 || lines.len() <= body_height as usize {
        return spans;
    }

    let offset = usize::from(layout::ensure_visible(
        0,
        LineSpan {
            start: u16::try_from(selected - first).unwrap_or(u16::MAX) as usize,
            height: 1,
        },
        u16::try_from(viewport).unwrap_or(u16::MAX),
        lines.len() - first,
    ));
    if offset == 0 {
        return spans;
    }

    lines.drain(first..(first + offset).min(lines.len()));
    spans
        .into_iter()
        .filter_map(|(id, line)| (line >= first + offset).then(|| (id, line - offset)))
        .collect()
}

/// Expands as far as `budget` allows, shrinking the trace window first.
///
/// herdr's agents panel is a split of the sidebar and is often shorter than the
/// watcher's full expanded card. Rather than refusing to expand — which would
/// put traces out of reach on short terminals — trade away trace rows through
/// `CardCtx::trace_lines`, the watcher's own knob for this, and only fall back
/// to the compact card when even a traceless expansion will not fit. Truncating
/// mid-card is not an option: it would strip a CONTEXT label off its own detail
/// row.
fn fit_expanded(
    telemetry: &PaneTelemetry,
    ctx: &mut CardCtx<'_>,
    budget: usize,
) -> (Vec<Line>, Vec<(String, usize)>) {
    for trace_lines in (0..=UNFOCUSED_TRACE_ROWS).rev() {
        ctx.trace_lines = trace_lines;
        let rendered = view::expanded_card_with_traces(telemetry, ctx);
        if rendered.0.len() <= budget {
            return rendered;
        }
    }
    (view::compact_card(telemetry, ctx), Vec::new())
}

/// herdr's screen detector to the watcher's card vocabulary. `Unknown` maps to
/// `Idle` rather than `Error`: not having decided yet is not a failure, and
/// `Error` would paint the card red on every freshly opened pane.
fn card_state(state: AgentState, seen: bool) -> CardState {
    match (state, seen) {
        (AgentState::Working, _) => CardState::Running,
        (AgentState::Blocked, _) => CardState::Attention,
        (AgentState::Idle, false) => CardState::Finished,
        (AgentState::Idle, true) => CardState::Idle,
        (AgentState::Unknown, _) => CardState::Idle,
    }
}

fn cwd_label(workspace: &str, telemetry: &PaneTelemetry) -> String {
    let cwd = telemetry
        .cwd
        .as_deref()
        .map(|cwd| cwd.trim_end_matches('/'))
        .and_then(|cwd| cwd.rsplit('/').next())
        .filter(|cwd| !cwd.is_empty());
    match cwd {
        Some(cwd) if cwd != workspace => format!("{workspace} · {cwd}"),
        _ => workspace.to_string(),
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

pub(crate) fn palette_style(style: WatcherStyle, palette: &Palette) -> Style {
    let mut foreground = match style.role {
        Role::Body | Role::Emphasis => palette.text,
        Role::Label => palette.overlay0,
        Role::Rule => palette.surface_dim,
    };
    if let Some(semantic) = style.semantic {
        foreground = match semantic {
            Semantic::Good => palette.green,
            Semantic::Warn => palette.yellow,
            Semantic::Bad => palette.red,
            Semantic::Accent => palette.accent,
        };
    }
    if let Some((red, green, blue)) = style.rgb {
        foreground = Color::Rgb(red, green, blue);
    }
    let mut output = Style::default().fg(foreground);
    if style.role == Role::Emphasis {
        output = output.add_modifier(Modifier::BOLD);
    }
    if matches!(style.role, Role::Label | Role::Rule) {
        output = output.add_modifier(Modifier::DIM);
    }
    if style.reverse {
        output = output.add_modifier(Modifier::REVERSED);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn telemetry() -> PaneTelemetry {
        let mut telemetry = PaneTelemetry::with_agent("claude");
        telemetry.cwd = Some("/work/vimeflow-terminal".into());
        telemetry.tool_calls = VecDeque::from([
            serde_json::json!({"toolUseId":"a","tool":"Edit","args":"x","status":"done"}),
            serde_json::json!({"toolUseId":"b","tool":"Bash","args":"y","status":"done"}),
        ]);
        telemetry
    }

    fn card(
        telemetry: Option<&PaneTelemetry>,
        focus: Option<&str>,
        expanded: bool,
        height: u16,
    ) -> BuiltCard {
        build_card(
            CardInput {
                workspace: "vimeflow",
                name: "claude",
                task: Some("ship cards"),
                state: AgentState::Working,
                seen: true,
                telemetry,
                trace_focus: focus,
            },
            40,
            height,
            expanded,
        )
    }

    fn card_at(
        telemetry: Option<&PaneTelemetry>,
        expanded: bool,
        width: u16,
        height: u16,
    ) -> BuiltCard {
        build_card(
            CardInput {
                workspace: "vimeflow",
                name: "claude",
                task: Some("ship cards"),
                state: AgentState::Working,
                seen: true,
                telemetry,
                trace_focus: None,
            },
            width,
            height,
            expanded,
        )
    }

    fn plain(card: &BuiltCard) -> Vec<String> {
        card.lines
            .iter()
            .map(|line| line.iter().map(|span| span.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn workspace_identity_survives_delegating_the_card_to_the_watcher() {
        // The watcher's own task line is `cwd › task` with no workspace; the
        // cwd_label hook is what keeps 23 cards tellable apart.
        let telemetry = telemetry();
        let rendered = plain(&card_at(Some(&telemetry), false, 80, 3));

        assert!(
            rendered
                .iter()
                .any(|line| line.contains("vimeflow") && line.contains("vimeflow-terminal")),
            "expected `workspace · cwd` in {rendered:?}"
        );
    }

    #[test]
    fn unbound_panes_still_get_a_card_carrying_herdr_detector_state() {
        // 20 of 23 panes have no watcher telemetry. Delegating blindly would
        // drop them; standing in keeps them and keeps their real state.
        let rendered = plain(&card(None, None, false, 3));

        assert_eq!(rendered.len(), 3, "a stand-in card is still a card");
        // The state *word* only renders at card widths >= 40, which the sidebar
        // never reaches, so the glyph is what carries the detector's verdict.
        assert!(
            rendered.iter().any(|line| line.contains('◐')),
            "detector state should reach the glyph, got {rendered:?}"
        );
    }

    #[test]
    fn pane_title_survives_titleless_telemetry_and_yields_to_a_telemetry_title() {
        let mut telemetry = telemetry();
        telemetry.cwd = None;
        for title in [
            None,
            Some(serde_json::json!({})),
            Some(serde_json::json!({"title": null})),
            Some(serde_json::json!({"title": 42})),
        ] {
            telemetry.title = title.clone();
            for expanded in [false, true] {
                let rendered = plain(&card(Some(&telemetry), None, expanded, 40));
                assert!(rendered.iter().any(|line| line.contains("ship cards")));
            }
            assert_eq!(telemetry.title, title);
        }

        telemetry.title = Some(serde_json::json!({"title": "watcher task"}));
        let rendered = plain(&card(Some(&telemetry), None, false, 40));
        assert!(rendered.iter().any(|line| line.contains("watcher task")));
        assert!(!rendered.iter().any(|line| line.contains("ship cards")));

        // An explicit clear must not bring back a stale pane title.
        telemetry.title = Some(serde_json::json!({"title": ""}));
        for expanded in [false, true] {
            let rendered = plain(&card(Some(&telemetry), None, expanded, 40));
            assert!(!rendered.iter().any(|line| line.contains("ship cards")));
        }
    }

    #[test]
    fn detector_state_maps_onto_the_watcher_vocabulary() {
        assert_eq!(card_state(AgentState::Working, true), CardState::Running);
        assert_eq!(card_state(AgentState::Blocked, true), CardState::Attention);
        assert_eq!(card_state(AgentState::Idle, false), CardState::Finished);
        assert_eq!(card_state(AgentState::Idle, true), CardState::Idle);
        // Not yet decided is not a failure.
        assert_eq!(card_state(AgentState::Unknown, true), CardState::Idle);
    }

    #[test]
    fn expanded_cards_export_trace_spans_within_the_rendered_lines() {
        let telemetry = telemetry();
        let built = card(Some(&telemetry), Some("a"), true, 40);

        assert!(
            !built.trace_spans.is_empty(),
            "expanded card should export selectable rows"
        );
        assert!(
            built
                .trace_spans
                .iter()
                .all(|(_, span)| span.start < built.lines.len()),
            "a span past the fold would hit-test to an off-screen row"
        );
    }

    #[test]
    fn short_cards_clip_both_lines_and_spans() {
        let telemetry = telemetry();
        for height in [1, 3, 6, 9] {
            let built = card(Some(&telemetry), Some("a"), true, height);
            assert!(built.lines.len() <= height as usize);
            assert!(built
                .trace_spans
                .iter()
                .all(|(_, span)| span.start < built.lines.len()));
        }
    }

    #[test]
    fn an_expanded_card_never_swallows_the_panel() {
        // The watcher decides what a card contains; herdr decides how much of
        // the list one card may cover. Without the reserve, the focused agent's
        // card fills a short panel and every other agent disappears.
        let telemetry = telemetry();
        let expanded_len = card(Some(&telemetry), None, true, 200).lines.len();
        assert!(expanded_len > 3, "fixture should produce a tall card");

        for height in 4..=(expanded_len as u16 + 2) {
            let built = card(Some(&telemetry), None, true, height);
            assert!(
                built.lines.len() <= height as usize,
                "card overflowed its panel at height {height}"
            );
            if (built.lines.len() as u16) == height {
                panic!("card took the whole panel at height {height}, hiding every neighbour");
            }
        }
    }

    #[test]
    fn a_short_panel_trades_trace_rows_rather_than_refusing_to_expand() {
        // The agents panel is a split of the sidebar and is routinely shorter
        // than a full expanded card. Refusing to expand there would put traces
        // out of reach on exactly the terminals that need the space most.
        let mut telemetry = telemetry();
        telemetry.tool_calls = (0..UNFOCUSED_TRACE_ROWS)
            .map(|i| {
                serde_json::json!({
                    "toolUseId": format!("id{i}"), "tool": "Edit",
                    "args": "x", "status": "done"
                })
            })
            .collect();

        let full = card(Some(&telemetry), None, true, 200).lines.len();
        // A panel exactly as tall as the card still owes the reserve, so this
        // only expands if trace rows are traded away.
        let tight = card(Some(&telemetry), None, true, full as u16);

        assert!(
            tight.lines.len() > COMPACT_CARD_LINES,
            "should still expand, just with fewer trace rows, got {}",
            tight.lines.len()
        );
        assert!(
            tight.lines.len() <= full - COMPACT_CARD_LINES,
            "should have shrunk to respect the reserve"
        );
    }

    #[test]
    fn cards_leave_the_panels_last_columns_free() {
        // The watcher fills exactly the width it is given, so without the
        // gutter its flush-right state glyph lands on the final cell — where a
        // terminal that draws ambiguous-width marks as two cells clips it.
        // Only meaningful at or above the watcher's own `MIN_WIDTH`: its card
        // layout is not defined below that and overflows by a cell, which is
        // why herdr's sidebar has to stay wide enough for it.
        let telemetry = telemetry();
        for width in [view::MIN_WIDTH + RIGHT_GUTTER, 40, 50] {
            for expanded in [false, true] {
                let built = card_at(Some(&telemetry), expanded, width, 200);
                for line in plain(&built) {
                    let measured = UnicodeWidthStr::width(line.as_str()) as u16;
                    assert!(
                        measured + RIGHT_GUTTER <= width,
                        "line {line:?} measured {measured} of {width}, leaving no gutter"
                    );
                }
            }
        }
    }

    #[test]
    fn descending_into_traces_always_renders_trace_rows() {
        // The watcher renders the whole ring whenever trace_focus is set and
        // ignores trace_lines doing it. Shrinking that knob to fit therefore
        // changes nothing, runs out of options, and falls back to the compact
        // card — so `l` would enter the trace zone and show no traces.
        let mut telemetry = telemetry();
        telemetry.tool_calls = (0..12)
            .map(|i| {
                serde_json::json!({
                    "toolUseId": format!("id{i}"), "tool": "Edit",
                    "args": "x", "status": "done"
                })
            })
            .collect();

        // Derive the full preamble, and cover the compact fallback below it.
        let roomy = card(Some(&telemetry), Some("id0"), true, 200);
        let first_trace = roomy
            .trace_spans
            .iter()
            .map(|(_, span)| span.start)
            .min()
            .expect("a focused card should render trace rows when given room");

        // Every height that can fit a trace row must actually show one. These
        // are the heights that used to collapse to a traceless compact card,
        // because the focused ring is far taller than the panel.
        for height in [1, 3, 6, 9, first_trace as u16 + 1, 20, 40] {
            let built = card(Some(&telemetry), Some("id0"), true, height);
            assert!(
                !built.trace_spans.is_empty(),
                "focused card rendered no selectable trace rows at height {height}"
            );
        }
    }

    #[test]
    fn the_selected_trace_stays_on_screen_however_deep_it_sits() {
        // A focused ring is routinely taller than the panel and herdr scrolls
        // the agents panel by whole entries, so a selection deep in the ring
        // has no way to be scrolled to. It must be brought into view instead.
        let mut telemetry = telemetry();
        telemetry.tool_calls = (0..40)
            .map(|i| {
                serde_json::json!({
                    "toolUseId": format!("id{i}"), "tool": "Edit",
                    "args": "x", "status": "done"
                })
            })
            .collect();

        let height = 16u16;
        // Rendered newest-first, so id0 is the OLDEST and sits deepest.
        for id in ["id39", "id20", "id0"] {
            let built = card(Some(&telemetry), Some(id), true, height);
            let found = built.trace_spans.iter().find(|(span_id, _)| span_id == id);
            let (_, span) = found.unwrap_or_else(|| {
                panic!(
                    "selection {id} not rendered; card has {} lines and {} spans",
                    built.lines.len(),
                    built.trace_spans.len()
                )
            });
            assert!(
                span.start < built.lines.len() && span.start < height as usize,
                "selection {id} landed at line {} of a {}-line card in a {height}-row panel",
                span.start,
                built.lines.len()
            );
        }
    }

    #[test]
    fn scrolling_to_a_deep_trace_keeps_the_cards_preamble() {
        // Only the trace rows scroll. The header and task lines are what tell
        // the reader which card they are in.
        let mut telemetry = telemetry();
        telemetry.tool_calls = (0..40)
            .map(|i| {
                serde_json::json!({
                    "toolUseId": format!("id{i}"), "tool": "Edit",
                    "args": "x", "status": "done"
                })
            })
            .collect();

        let deep = card(Some(&telemetry), Some("id0"), true, 16);
        let rendered = plain(&deep);
        assert!(
            rendered.iter().any(|line| line.contains("CLAUDE")),
            "header scrolled away: {rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("vimeflow")),
            "workspace identity scrolled away: {rendered:?}"
        );
    }

    #[test]
    fn default_24_row_terminal_keeps_compact_identity_and_selected_trace() {
        let mut state = crate::app::state::AppState::test_new();
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 100, 24));
        let (_, panel) = crate::ui::expanded_sidebar_sections(
            state.view.sidebar_rect,
            state.sidebar_section_split,
        );
        let body = crate::ui::agent_panel_items_rect(&state, panel, false);
        assert_eq!(body.height, 9);

        let mut telemetry = telemetry();
        telemetry.tool_calls = (0..40)
            .map(|i| {
                serde_json::json!({
                    "toolUseId": format!("id{i}"), "tool": "Edit",
                    "args": format!("file{i}.rs"), "status": "done"
                })
            })
            .collect();
        let compact = plain(&card(Some(&telemetry), None, false, body.height));
        for id in ["id39", "id20", "id0"] {
            let built = card(Some(&telemetry), Some(id), true, body.height);
            assert_eq!(&plain(&built)[..COMPACT_CARD_LINES], compact.as_slice());
            let (_, span) = built.trace_spans.iter().find(|(key, _)| key == id).unwrap();
            assert!(span.start >= COMPACT_CARD_LINES && span.start < body.height as usize);
            assert!(built.lines[span.start]
                .iter()
                .any(|span| span.style.reverse));
        }
    }

    #[test]
    fn collapsed_cards_export_no_trace_geometry() {
        let telemetry = telemetry();
        let built = card(Some(&telemetry), Some("a"), false, 3);

        assert!(
            built.trace_spans.is_empty(),
            "a collapsed card renders no trace rows to hit-test"
        );
    }

    #[test]
    fn role_and_semantic_styles_map_to_the_app_palette() {
        let palette = Palette::catppuccin();

        assert_eq!(
            palette_style(WatcherStyle::semantic(Role::Body, Semantic::Warn), &palette).fg,
            Some(palette.yellow)
        );
        assert!(palette_style(WatcherStyle::role(Role::Emphasis), &palette)
            .add_modifier
            .contains(Modifier::BOLD));
        let mut reversed = WatcherStyle::role(Role::Body);
        reversed.reverse = true;
        assert!(palette_style(reversed, &palette)
            .add_modifier
            .contains(Modifier::REVERSED));
    }
}
