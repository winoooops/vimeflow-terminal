use std::collections::HashSet;

use herdr_agent_watcher::daemon::store::PaneTelemetry;
use herdr_agent_watcher::sidebar::layout::LineSpan;
use herdr_agent_watcher::sidebar::view::{
    call_id, selectable_call, Line, Role, Semantic, Span, Style as WatcherStyle,
};
use ratatui::style::{Color, Modifier, Style};
use serde_json::Value;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::state::Palette;
use crate::detect::AgentState;

/// Trace rows shown when the card is not the trace anchor. The anchor renders
/// its whole retained ring instead, so navigation is not limited to a window
/// the selection can fall outside of.
const UNFOCUSED_TRACE_ROWS: usize = 5;

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
    let mut lines = vec![
        header(&input, width, expanded),
        location(&input, width),
        summary(&input, width),
    ];
    if !expanded || input.telemetry.is_none() || body_height <= 3 {
        lines.truncate(body_height as usize);
        return BuiltCard {
            lines,
            trace_spans: Vec::new(),
        };
    }

    let Some(telemetry) = input.telemetry else {
        return BuiltCard {
            lines,
            trace_spans: Vec::new(),
        };
    };
    let (trace_lines, local_spans) = trace_rows(telemetry, input.trace_focus);
    let groups = [
        model_rows(telemetry),
        gauge_rows(telemetry, width),
        tool_rows(telemetry),
        trace_lines,
    ];
    let mut trace_spans = Vec::new();
    let mut remaining = body_height.saturating_sub(3) as usize;
    for (index, group) in groups.into_iter().enumerate() {
        if index == 3 {
            // The trace group is the only one allowed to render partially, so
            // it is also the only one whose spans need clipping to what fit.
            let base = lines.len();
            lines.extend(group.into_iter().take(remaining));
            trace_spans = local_spans
                .into_iter()
                .filter(|(_, span)| span.start < remaining)
                .map(|(id, span)| {
                    (
                        id,
                        LineSpan {
                            start: base + span.start,
                            height: span.height,
                        },
                    )
                })
                .collect();
            break;
        }
        if group.len() > remaining {
            break;
        }
        remaining -= group.len();
        lines.extend(group);
    }
    for line in &mut lines {
        *line = fit_line(std::mem::take(line), width as usize);
    }
    BuiltCard { lines, trace_spans }
}

fn lifecycle(state: AgentState, seen: bool) -> (&'static str, &'static str, Semantic) {
    match (state, seen) {
        (AgentState::Working, _) => ("●", "working", Semantic::Good),
        (AgentState::Blocked, _) => ("◐", "blocked", Semantic::Warn),
        (AgentState::Idle, false) => ("✓", "done", Semantic::Good),
        (AgentState::Idle, true) => ("○", "idle", Semantic::Accent),
        (AgentState::Unknown, _) => ("?", "unknown", Semantic::Bad),
    }
}

fn header(input: &CardInput<'_>, width: u16, expanded: bool) -> Line {
    let (glyph, label, semantic) = lifecycle(input.state, input.seen);
    let chevron = if expanded { "▾ " } else { "▸ " };
    let mut fixed = UnicodeWidthStr::width(chevron) + UnicodeWidthStr::width(glyph) + 1;
    let show_label = width >= 32;
    if show_label {
        fixed += UnicodeWidthStr::width(label) + 1;
    }
    let name = truncate(input.name, (width as usize).saturating_sub(fixed));
    let mut line = vec![
        Span::new(chevron, WatcherStyle::role(Role::Label)),
        Span::new(glyph, WatcherStyle::semantic(Role::Body, semantic)),
        Span::body(" "),
        Span::new(name, WatcherStyle::role(Role::Emphasis)),
    ];
    if show_label {
        line.push(Span::body(" "));
        line.push(Span::new(
            label,
            WatcherStyle::semantic(Role::Label, semantic),
        ));
    }
    fit_line(line, width as usize)
}

fn location(input: &CardInput<'_>, width: u16) -> Line {
    let cwd = input
        .telemetry
        .and_then(|telemetry| telemetry.cwd.as_deref())
        .and_then(|cwd| cwd.trim_end_matches('/').rsplit('/').next())
        .filter(|cwd| !cwd.is_empty());
    let telemetry_task = input
        .telemetry
        .and_then(|telemetry| telemetry.title.as_ref())
        .and_then(|title| title.get("title"))
        .and_then(Value::as_str);
    let mut text = input.workspace.to_string();
    if let Some(cwd) = cwd {
        text.push_str(" · ");
        text.push_str(cwd);
    }
    if let Some(task) = telemetry_task
        .or(input.task)
        .filter(|task| !task.is_empty())
    {
        text.push_str(" › ");
        text.push_str(task);
    }
    fit_line(
        vec![
            Span::body("  "),
            Span::new(text, WatcherStyle::role(Role::Label)),
        ],
        width as usize,
    )
}

fn summary(input: &CardInput<'_>, width: u16) -> Line {
    let Some(telemetry) = input.telemetry else {
        return fit_line(
            vec![Span::body("  "), Span::label("— no telemetry")],
            width as usize,
        );
    };
    let context = telemetry.status.as_ref().and_then(context_percent);
    let gauge_cells = width.saturating_sub(10).clamp(6, 14);
    let gauge = context.map_or_else(|| "—".to_string(), |pct| gauge(pct, gauge_cells as usize));
    let mut line = vec![
        Span::body("  "),
        Span::new(gauge, WatcherStyle::semantic(Role::Body, Semantic::Accent)),
    ];
    if width >= 25 {
        if let Some(pct) = context {
            line.push(Span::body(format!(" {:>3}%", pct.round() as u64)));
        }
        line.push(Span::label(format!(
            " · {} calls",
            telemetry.tool_call_total
        )));
    }
    fit_line(line, width as usize)
}

fn model_rows(telemetry: &PaneTelemetry) -> Vec<Line> {
    let model = telemetry
        .status
        .as_ref()
        .and_then(|status| status.get("modelDisplayName"))
        .and_then(Value::as_str)
        .unwrap_or("—");
    vec![labeled("MODEL", model, Semantic::Accent)]
}

fn gauge_rows(telemetry: &PaneTelemetry, width: u16) -> Vec<Line> {
    let status = telemetry.status.as_ref();
    let context = status.and_then(context_percent);
    let cache = status.and_then(|status| cache_percent(status, telemetry.agent.as_deref()));
    let cost = status
        .and_then(|status| status.get("cost"))
        .and_then(|cost| cost.get("totalCostUsd"))
        .and_then(Value::as_f64);
    let cells = width.saturating_sub(14).clamp(6, 14) as usize;
    vec![
        metric("CONTEXT", context, cells, Semantic::Accent),
        metric("CACHE", cache, cells, Semantic::Good),
        labeled(
            "COST",
            &cost.map_or_else(|| "—".to_string(), |cost| format!("${cost:.2}")),
            Semantic::Accent,
        ),
    ]
}

fn tool_rows(telemetry: &PaneTelemetry) -> Vec<Line> {
    let mut tools: Vec<_> = telemetry
        .tool_counts
        .iter()
        .filter(|(_, count)| **count > 0)
        .collect();
    tools.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
    let text = if tools.is_empty() {
        format!("{} calls", telemetry.tool_call_total)
    } else {
        tools
            .into_iter()
            .take(3)
            .map(|(name, count)| format!("{name} {count}"))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    vec![labeled("TOOLS", &text, Semantic::Accent)]
}

/// Newest first, at most one row per `toolUseId`. `call_id` and
/// `selectable_call` come from the watcher so rendering, hit-testing and the
/// key resolver can never disagree on what counts as a selectable row.
fn trace_rows(
    telemetry: &PaneTelemetry,
    focus: Option<&str>,
) -> (Vec<Line>, Vec<(String, LineSpan)>) {
    let limit = if focus.is_some() {
        telemetry.tool_calls.len()
    } else {
        UNFOCUSED_TRACE_ROWS
    };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    for call in telemetry.tool_calls.iter().rev() {
        if lines.len() >= limit {
            break;
        }
        let id = call_id(call);
        // A newer occurrence shadows older ones; id-less rows are never
        // deduplicated because they carry no identity to collide on.
        if let Some(id) = id {
            if !seen.insert(id) {
                continue;
            }
        }
        let selectable = selectable_call(call);
        let selected = selectable && id.is_some() && id == focus;
        if selectable {
            if let Some(id) = id {
                spans.push((
                    id.to_string(),
                    LineSpan {
                        start: lines.len(),
                        height: 1,
                    },
                ));
            }
        }
        lines.push(trace_row(call, selected));
    }
    (lines, spans)
}

fn trace_row(call: &Value, selected: bool) -> Line {
    let failed = call.get("status").and_then(Value::as_str) == Some("failed");
    let glyph = if failed { "✕" } else { "✓" };
    let semantic = if failed {
        Semantic::Bad
    } else {
        Semantic::Good
    };
    let tool = call.get("tool").and_then(Value::as_str).unwrap_or("?");
    let args = call.get("args").and_then(Value::as_str).unwrap_or("");
    let mark = |style: WatcherStyle| {
        let mut style = style;
        style.reverse = selected;
        style
    };
    vec![
        Span::new("  ", mark(WatcherStyle::role(Role::Body))),
        Span::new(glyph, mark(WatcherStyle::semantic(Role::Body, semantic))),
        Span::new(" ", mark(WatcherStyle::role(Role::Body))),
        Span::new(
            format!("{tool} {args}"),
            mark(WatcherStyle::role(Role::Label)),
        ),
    ]
}

fn labeled(label: &str, value: &str, semantic: Semantic) -> Line {
    vec![
        Span::new(format!("{label:<8}"), WatcherStyle::role(Role::Label)),
        Span::new(value, WatcherStyle::semantic(Role::Body, semantic)),
    ]
}

fn metric(label: &str, percent: Option<f64>, cells: usize, semantic: Semantic) -> Line {
    let value = percent.map_or_else(
        || "—".to_string(),
        |percent| format!("{} {:>3}%", gauge(percent, cells), percent.round() as u64),
    );
    labeled(label, &value, semantic)
}

fn context_percent(status: &Value) -> Option<f64> {
    status
        .get("contextWindow")?
        .get("usedPercentage")?
        .as_f64()
        .filter(|percent| percent.is_finite() && *percent >= 0.0)
}

fn cache_percent(status: &Value, agent: Option<&str>) -> Option<f64> {
    let usage = status.get("contextWindow")?.get("currentUsage")?;
    let input = usage.get("inputTokens")?.as_u64()?;
    let read = usage
        .get("cacheReadInputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let created = usage
        .get("cacheCreationInputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let denominator = match agent {
        Some("codex") => input,
        Some("claude" | "claude-code" | "kimi" | "opencode") => input + read + created,
        _ => return None,
    };
    (denominator > 0).then(|| read as f64 * 100.0 / denominator as f64)
}

fn gauge(percent: f64, cells: usize) -> String {
    let filled = ((percent.clamp(0.0, 100.0) * cells as f64 / 100.0).round() as usize).min(cells);
    format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled))
}

fn fit_line(line: Line, width: usize) -> Line {
    let mut remaining = width;
    let mut fitted = Vec::new();
    for span in line {
        if remaining == 0 {
            break;
        }
        let text = truncate(&span.text, remaining);
        remaining = remaining.saturating_sub(UnicodeWidthStr::width(text.as_str()));
        fitted.push(Span { text, ..span });
    }
    fitted
}

fn truncate(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut taken = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width - 1 {
            break;
        }
        taken.push(character);
        used += character_width;
    }
    taken.push('…');
    taken
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
    use std::collections::{BTreeMap, VecDeque};

    use super::*;

    fn telemetry() -> PaneTelemetry {
        let mut telemetry = PaneTelemetry::with_agent("claude");
        telemetry.cwd = Some("/work/vimeflow-terminal".into());
        telemetry.status = Some(serde_json::json!({
            "modelDisplayName": "Claude Sonnet",
            "contextWindow": {
                "usedPercentage": 50.0,
                "currentUsage": {
                    "inputTokens": 500,
                    "cacheReadInputTokens": 500,
                    "cacheCreationInputTokens": 0
                }
            },
            "cost": {"totalCostUsd": 1.25}
        }));
        telemetry.title = Some(serde_json::json!({"title": "ship cards"}));
        telemetry.tool_counts = BTreeMap::from([("Edit".into(), 4), ("Bash".into(), 2)]);
        telemetry.tool_call_total = 6;
        telemetry.tool_calls = VecDeque::from([
            serde_json::json!({"tool":"Edit","args":"sidebar.rs","status":"done"}),
            serde_json::json!({"tool":"Bash","args":"cargo test","status":"done"}),
            serde_json::json!({"tool":"Read","args":"spec.md","status":"done"}),
            serde_json::json!({"tool":"Edit","args":"config.rs","status":"done"}),
            serde_json::json!({"tool":"Bash","args":"cargo clippy","status":"done"}),
        ]);
        telemetry
    }

    fn plain(card: &BuiltCard) -> Vec<String> {
        card.lines
            .iter()
            .map(|line| line.iter().map(|span| span.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn adaptive_matrix_stays_bounded_and_collapsed_is_three_lines() {
        let telemetry = telemetry();
        for width in [16, 17, 24, 25, 34, 35] {
            for (state, seen) in [
                (AgentState::Idle, true),
                (AgentState::Working, true),
                (AgentState::Blocked, true),
                (AgentState::Idle, false),
            ] {
                for present in [false, true] {
                    for expanded in [false, true] {
                        for height in [3, 4, 7, 8, 13] {
                            let card = build_card(
                                CardInput {
                                    workspace: "workspace-six",
                                    name: "claude",
                                    task: Some("fallback task"),
                                    state,
                                    seen,
                                    telemetry: present.then_some(&telemetry),
                                    trace_focus: None,
                                },
                                width,
                                height,
                                expanded,
                            );
                            assert!(card.lines.len() <= height as usize);
                            assert!(card.lines.iter().all(|line| {
                                UnicodeWidthStr::width(
                                    line.iter()
                                        .map(|span| span.text.as_str())
                                        .collect::<String>()
                                        .as_str(),
                                ) <= width as usize
                            }));
                            if !expanded || !present {
                                assert_eq!(card.lines.len(), 3);
                            }
                            assert!(plain(&card)[1].contains("workspace"));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn expansion_drops_traces_then_tools_then_gauges_then_model() {
        let telemetry = telemetry();
        let render = |height| {
            plain(&build_card(
                CardInput {
                    workspace: "w6",
                    name: "claude",
                    task: None,
                    state: AgentState::Working,
                    seen: true,
                    telemetry: Some(&telemetry),
                    trace_focus: None,
                },
                35,
                height,
                true,
            ))
        };

        assert_eq!(render(3).len(), 3);
        assert!(render(4).iter().any(|line| line.starts_with("MODEL")));
        assert!(!render(6).iter().any(|line| line.starts_with("CONTEXT")));
        assert!(render(7).iter().any(|line| line.starts_with("COST")));
        assert!(render(8).iter().any(|line| line.starts_with("TOOLS")));
        assert_eq!(render(13).len(), 13);
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
    }

    /// Oldest first, the way the daemon pushes into the ring.
    fn traced(calls: &[(&str, &str, &str)]) -> PaneTelemetry {
        let mut telemetry = telemetry();
        telemetry.tool_calls = calls
            .iter()
            .map(|(id, tool, status)| {
                serde_json::json!({
                    "toolUseId": id, "tool": tool, "args": "x", "status": status
                })
            })
            .collect();
        telemetry
    }

    fn card_with(telemetry: &PaneTelemetry, focus: Option<&str>, height: u16) -> BuiltCard {
        build_card(
            CardInput {
                workspace: "ws",
                name: "claude",
                task: None,
                state: AgentState::Working,
                seen: true,
                telemetry: Some(telemetry),
                trace_focus: focus,
            },
            40,
            height,
            true,
        )
    }

    #[test]
    fn only_selectable_rows_export_spans() {
        // An id-less row and an unsettled row both render but must not be
        // reachable by a hit-test.
        let telemetry = traced(&[
            ("", "NoId", "done"),
            ("a", "Running", "running"),
            ("b", "Edit", "done"),
        ]);
        let card = card_with(&telemetry, None, 40);

        let ids: Vec<&str> = card.trace_spans.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["b"], "only the settled id-bearing row");
        assert!(
            plain(&card).iter().any(|line| line.contains("Running")),
            "display-only rows still render"
        );
    }

    #[test]
    fn duplicate_ids_render_once_newest_wins() {
        let telemetry = traced(&[("dup", "Old", "done"), ("dup", "New", "done")]);
        let card = card_with(&telemetry, None, 40);

        assert_eq!(card.trace_spans.len(), 1);
        let rendered = plain(&card);
        assert!(rendered.iter().any(|line| line.contains("New")));
        assert!(!rendered.iter().any(|line| line.contains("Old")));
    }

    #[test]
    fn focus_renders_the_whole_ring_and_reverses_the_selection() {
        let calls: Vec<(String, &str, &str)> = (0..9)
            .map(|index| (format!("id{index}"), "Edit", "done"))
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = calls
            .iter()
            .map(|(id, tool, status)| (id.as_str(), *tool, *status))
            .collect();
        let telemetry = traced(&borrowed);

        let unfocused = card_with(&telemetry, None, 40);
        assert_eq!(
            unfocused.trace_spans.len(),
            UNFOCUSED_TRACE_ROWS,
            "unfocused cards keep the window"
        );

        let focused = card_with(&telemetry, Some("id2"), 40);
        assert_eq!(focused.trace_spans.len(), 9, "focus shows the full ring");

        // The selected row, and only it, is reversed.
        let selected = focused
            .trace_spans
            .iter()
            .find(|(id, _)| id == "id2")
            .map(|(_, span)| span.start)
            .expect("id2 is rendered");
        assert!(focused.lines[selected]
            .iter()
            .all(|span| span.style.reverse));
        for (_, span) in focused.trace_spans.iter().filter(|(id, _)| id != "id2") {
            assert!(focused.lines[span.start]
                .iter()
                .all(|line| !line.style.reverse));
        }
    }

    #[test]
    fn spans_are_clipped_to_the_rows_that_fit() {
        let calls: Vec<(String, &str, &str)> = (0..9)
            .map(|index| (format!("id{index}"), "Edit", "done"))
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = calls
            .iter()
            .map(|(id, tool, status)| (id.as_str(), *tool, *status))
            .collect();
        let telemetry = traced(&borrowed);

        // A short card renders only some trace rows; a span past the fold
        // would hit-test to a row that is not on screen.
        let card = card_with(&telemetry, Some("id0"), 9);
        assert!(card
            .trace_spans
            .iter()
            .all(|(_, span)| span.start < card.lines.len()));
    }
}
