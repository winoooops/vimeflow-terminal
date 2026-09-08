//! Adapter between herdr's agent panel and the watcher's card renderer.
//!
//! The cards themselves belong to `herdr_agent_watcher::sidebar::view`: header,
//! task line, model, context/cache/cost, tools and traces are all its work, and
//! duplicating any of it here would mean two implementations free to drift.
//! What stays is the part the watcher cannot know — which pane a card is for,
//! the workspace it lives in, and how herdr's palette paints the result.

use herdr_agent_watcher::daemon::store::{CardState, PaneTelemetry};
use herdr_agent_watcher::sidebar::config::{AgentMark, ToolCallStyle};
use herdr_agent_watcher::sidebar::layout::LineSpan;
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
    let width = width.max(1);

    // The watcher only knows panes it has bound. For the rest herdr stands in
    // with an empty telemetry carrying its own detector state, so an unbound
    // pane still gets a card and still reads working/blocked rather than
    // defaulting to idle. Where the watcher *has* bound a pane its lifecycle
    // events are the better source, so its `card_state` is left alone — and
    // the real telemetry is borrowed, never cloned, because this runs for
    // every card on every frame.
    let stand_in;
    let telemetry = match input.telemetry {
        Some(telemetry) => telemetry,
        None => {
            let mut telemetry = PaneTelemetry::with_agent(input.name);
            telemetry.card_state = card_state(input.state, input.seen);
            if let Some(task) = input.task.filter(|task| !task.is_empty()) {
                telemetry.title = Some(serde_json::json!({ "title": task }));
            }
            stand_in = telemetry;
            &stand_in
        }
    };

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
    let (mut lines, spans) = if expanded {
        fit_expanded(telemetry, &mut ctx, budget)
    } else {
        (view::compact_card(telemetry, &ctx), Vec::new())
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
        let rendered = plain(&card(Some(&telemetry), None, false, 3));

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
        assert!(
            rendered.iter().any(|line| line.contains("working")),
            "detector state should reach the glyph, got {rendered:?}"
        );
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
