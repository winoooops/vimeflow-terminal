//! Trace navigation on the focused agent card.
//!
//! Selectability, dedup order, hit-test geometry, panel layout and the
//! formatters all come from `herdr_agent_watcher::sidebar`, so herdr and the
//! watcher can never disagree about what a trace row is. What lives here is
//! only herdr's own interaction state, which the runtime/client boundary keeps
//! out of the server API.
//!
//! `Mode::Agents` has two zones, mirroring the watcher's own model. The card
//! zone moves a transient cursor between cards; the trace zone walks the
//! selected card's ring. The cursor deliberately does not move pane focus —
//! `focus_agent_entry` switches workspace and tab, so driving it from `j`/`k`
//! would throw the main view around on every keystroke. Enter commits.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use herdr_agent_watcher::daemon::store::PaneTelemetry;
use herdr_agent_watcher::sidebar::dialog::{Panel, Row};
use herdr_agent_watcher::sidebar::format;
use herdr_agent_watcher::sidebar::view::{call_id, newest_selectable_id, selectable_call};
use serde_json::Value;

use crate::app::state::{AppState, Mode};
use crate::app::App;
use crate::layout::PaneId;

impl AppState {
    fn agent_cards_visible(&self) -> bool {
        let body = crate::ui::agent_panel_items_rect(self, self.agent_panel_rect(), false);
        self.agents_view == crate::config::AgentsViewConfig::Cards
            && body.width > 0
            && body.height > 0
    }

    /// The card traces navigate: the keyboard cursor while `Mode::Agents` is
    /// active, otherwise the focused pane's card, which is the only expanded
    /// one. `None` when it is collapsed or there is no agent card.
    pub(super) fn trace_anchor_pane(&self) -> Option<PaneId> {
        if let Some(cursor) = self.agent_card_cursor {
            return Some(cursor);
        }
        crate::ui::agent_panel_entries(self)
            .iter()
            .find(|entry| {
                self.is_active_pane(entry.ws_idx, entry.tab_idx, entry.pane_id)
                    && self.agent_card_collapsed_for != Some(entry.pane_id)
            })
            .map(|entry| entry.pane_id)
    }

    fn trace_telemetry(&self, pane: PaneId) -> Option<&PaneTelemetry> {
        let ws_idx = crate::ui::agent_panel_entries(self)
            .iter()
            .find(|entry| entry.pane_id == pane)
            .map(|entry| entry.ws_idx)?;
        let workspace = self.workspaces.get(ws_idx)?;
        let number = workspace.public_pane_number(pane)?;
        let id = crate::workspace::public_pane_id_for_number(&workspace.id, number);
        self.agent_telemetry.get(&id)
    }

    /// Selectable ids newest-first, under the same newest-shadows-older dedup
    /// the card renders with, so index arithmetic here matches the rows on
    /// screen.
    fn trace_selectable_ids(&self, pane: PaneId) -> Vec<String> {
        let Some(telemetry) = self.trace_telemetry(pane) else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        telemetry
            .tool_calls
            .iter()
            .rev()
            .filter_map(|call| {
                let id = call_id(call)?;
                if !seen.insert(id) {
                    return None;
                }
                selectable_call(call).then(|| id.to_string())
            })
            .collect()
    }

    /// Enters the card zone, anchored on whichever card is already expanded so
    /// the mode starts where the eye is.
    pub(crate) fn enter_agents_mode(&mut self) {
        self.exit_agents_mode();
        if !self.agent_cards_visible() {
            return;
        }
        let entries = crate::ui::agent_panel_entries(self);
        if entries.is_empty() {
            return;
        }
        let start = entries
            .iter()
            .position(|entry| self.is_active_pane(entry.ws_idx, entry.tab_idx, entry.pane_id))
            .unwrap_or(0);
        let pane = entries[start].pane_id;
        drop(entries);
        self.agent_card_cursor = Some(pane);
        self.agent_trace_focus = None;
        self.mode = Mode::Agents;
        self.ensure_agent_panel_entry_visible(start);
    }

    pub(super) fn exit_agents_mode(&mut self) {
        self.agent_trace_panel = None;
        self.agent_trace_focus = None;
        self.agent_card_cursor = None;
        if self.mode == Mode::Agents {
            self.mode = Mode::Terminal;
        }
    }

    fn move_card_cursor(&mut self, delta: isize) {
        let entries = crate::ui::agent_panel_entries(self);
        if entries.is_empty() {
            return;
        }
        let current = self
            .agent_card_cursor
            .and_then(|pane| entries.iter().position(|entry| entry.pane_id == pane))
            .unwrap_or(0);
        // Clamped, no wrap — same as the trace zone and the card list.
        let next = current
            .saturating_add_signed(delta)
            .min(entries.len().saturating_sub(1));
        let pane = entries[next].pane_id;
        drop(entries);
        self.agent_card_cursor = Some(pane);
        self.ensure_agent_panel_entry_visible(next);
    }

    /// Enter on a card commits: this is the only path in the mode that moves
    /// pane focus, and it leaves the mode so keys go back to the agent.
    fn commit_card_cursor(&mut self) {
        let Some(cursor) = self.agent_card_cursor else {
            return;
        };
        let target = crate::ui::agent_panel_entries(self)
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.pane_id == cursor)
            .map(|(index, entry)| (index, entry.ws_idx));
        self.exit_agents_mode();
        let Some((index, ws_idx)) = target else {
            return;
        };
        if self.focus_pane_in_workspace(ws_idx, cursor) {
            self.ensure_agent_panel_entry_visible(index);
        }
    }

    /// `l` descends into the cursor card's ring, landing on the newest
    /// selectable row wherever it sits — resolved against the canonical ring,
    /// not the rendered window, which may not contain it.
    fn descend_to_traces(&mut self) {
        let Some(pane) = self.agent_card_cursor else {
            return;
        };
        let Some(id) = self
            .trace_telemetry(pane)
            .and_then(|telemetry| newest_selectable_id(&telemetry.tool_calls))
        else {
            return;
        };
        self.agent_trace_focus = Some((pane, id));
        self.anchor_card_to_top(pane);
    }

    pub(super) fn move_trace_selection(&mut self, delta: isize) {
        let Some((pane, current)) = self.agent_trace_focus.clone() else {
            return;
        };
        let ids = self.trace_selectable_ids(pane);
        let Some(index) = ids.iter().position(|id| *id == current) else {
            return;
        };
        // Clamped at both ends, no wrap — the card list behaves the same way.
        let next = index
            .saturating_add_signed(delta)
            .min(ids.len().saturating_sub(1));
        if let Some(id) = ids.get(next) {
            self.agent_trace_focus = Some((pane, id.clone()));
        }
    }

    /// Selects `id` on `pane` outright. The mouse deep-selects in one click,
    /// including across cards, so this also retargets the anchor.
    pub(super) fn select_trace(&mut self, pane: PaneId, id: &str) {
        self.agent_trace_focus = Some((pane, id.to_string()));
        self.anchor_card_to_top(pane);
    }

    #[cfg(test)]
    pub(crate) fn select_trace_for_test(&mut self, pane: PaneId, id: &str) {
        self.select_trace(pane, id);
    }

    /// Scrolls the anchor card to the top of the panel.
    ///
    /// Descending makes a card grow from three lines to its whole ring, and the
    /// panel renders a card only when it fits in the rows still left below it —
    /// so a card near the bottom becomes too tall to draw and vanishes exactly
    /// as the reader descends into it. Starting the list at the anchor gives it
    /// the full panel height, which is the budget `build_card` already clips
    /// to. Any index is a legal start, so this is never clamped away.
    fn anchor_card_to_top(&mut self, pane: PaneId) {
        if let Some(index) = crate::ui::agent_panel_entries(self)
            .iter()
            .position(|entry| entry.pane_id == pane)
        {
            self.agent_panel_scroll = index;
        }
    }

    /// `(pane, toolUseId)` for the trace row under the cursor. Callers must
    /// resolve this before the card-body hit: a trace row sits inside a card's
    /// span, so the more specific hit has to win.
    pub(super) fn trace_target_at(&self, col: u16, row: u16) -> Option<(PaneId, String)> {
        if !self.agent_cards_visible() {
            return None;
        }
        let detail_area = self.agent_panel_rect();
        let metrics = crate::ui::agent_panel_scroll_metrics(self, detail_area);
        let body = crate::ui::agent_panel_items_rect(
            self,
            detail_area,
            crate::ui::should_show_scrollbar(metrics),
        );
        if body.height == 0
            || col < body.x
            || col >= body.x + body.width
            || row < body.y
            || row >= body.y + body.height
        {
            return None;
        }

        let mut row_y = body.y;
        let body_bottom = body.y + body.height;
        let entries = crate::ui::agent_panel_entries(self);
        let scroll = self.agent_panel_scroll.min(metrics.max_offset_from_bottom);
        for (index, entry) in entries.iter().enumerate().skip(scroll) {
            let height =
                crate::ui::agent_entry_height_in_body(self, entry, body.width, body.height);
            if row_y.saturating_add(height) > body_bottom {
                break;
            }
            if row >= row_y && row < row_y.saturating_add(height) {
                let spans = crate::ui::agent_card_trace_spans(self, entry, body.width, body.height);
                let line = usize::from(row - row_y);
                return herdr_agent_watcher::sidebar::layout::trace_at(&spans, line)
                    .map(|(_, id)| (entry.pane_id, id.to_string()));
            }
            row_y = row_y
                .saturating_add(height)
                .saturating_add(crate::ui::agent_entry_gap(self, index, entries.len()))
                .min(body_bottom);
        }
        None
    }

    pub(super) fn open_trace_detail(&mut self) {
        let Some((pane, id)) = self.agent_trace_focus.clone() else {
            return;
        };
        let area = self.view.terminal_area;
        // Same refusal the other panels make: a panel too small to read would
        // still capture every key.
        if crate::ui::trace_panel_width(area) < 5 || area.height < 7 {
            return;
        }
        // Snapshot at open: a call falling off the ring must not rewrite text
        // under a reading user.
        let Some(call) = self.trace_telemetry(pane).and_then(|telemetry| {
            telemetry
                .tool_calls
                .iter()
                .rev()
                .find(|call| call_id(call) == Some(id.as_str()))
                .cloned()
        }) else {
            return;
        };
        self.agent_trace_panel = Some(trace_panel(&call, now_unix_ms()));
        self.mode = Mode::Agents;
    }

    pub(super) fn handle_trace_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.agent_trace_panel.is_none() {
            return false;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_trace_detail(-1),
            MouseEventKind::ScrollDown => self.scroll_trace_detail(1),
            _ => {}
        }
        true
    }

    fn scroll_trace_detail(&mut self, delta: isize) {
        let width = crate::ui::trace_panel_width(self.view.terminal_area);
        let Some(panel) = &mut self.agent_trace_panel else {
            return;
        };
        // Bounded by the *rendered* line count at the drawn width: bounding by
        // logical rows would strand the tail of a wrapped args preview.
        let rendered = herdr_agent_watcher::sidebar::dialog::line_count(panel, width);
        panel.offset = panel
            .offset
            .saturating_add_signed(delta)
            .min(rendered.saturating_sub(1));
    }

    /// Geometry can be computed for background clients too; only the
    /// foreground view may dismiss navigation surfaces that no longer fit.
    pub(crate) fn reconcile_trace_visibility(&mut self) {
        let area = self.view.terminal_area;
        if !self.agent_cards_visible()
            || (self.agent_trace_panel.is_some()
                && (crate::ui::trace_panel_width(area) < 5 || area.height < 7))
        {
            self.exit_agents_mode();
        }
    }

    /// Drops navigation-owned state after any mode exit, and reconciles row
    /// identities before each draw independently of client geometry.
    pub(crate) fn reconcile_trace_focus(&mut self) {
        // Mouse-only trace selection has no card cursor or detail panel and
        // remains supported outside Agents mode.
        if (self.mode != Mode::Agents
            && (self.agent_card_cursor.is_some() || self.agent_trace_panel.is_some()))
            || self.sidebar_collapsed
            || self.agents_view != crate::config::AgentsViewConfig::Cards
        {
            self.exit_agents_mode();
            return;
        }
        // A cursor whose card left the list (pane closed, `z` hid it) takes the
        // whole mode with it rather than silently retargeting.
        if let Some(cursor) = self.agent_card_cursor {
            let present = crate::ui::agent_panel_entries(self)
                .iter()
                .any(|entry| entry.pane_id == cursor);
            if !present {
                self.exit_agents_mode();
                return;
            }
        }

        let Some((pane, id)) = self.agent_trace_focus.clone() else {
            return;
        };
        if self.trace_anchor_pane() != Some(pane) {
            self.exit_agents_mode();
            return;
        }
        let ids = self.trace_selectable_ids(pane);
        if ids.contains(&id) {
            return;
        }
        // The id was evicted: snap to the newest surviving row, or fall back to
        // the card zone when the ring emptied.
        match ids.first() {
            Some(newest) => self.agent_trace_focus = Some((pane, newest.clone())),
            None => self.agent_trace_focus = None,
        }
    }
}

impl App {
    pub(crate) fn handle_trace_key(&mut self, key: crate::input::TerminalKey) {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return;
        }
        if self.state.is_prefix_key(&key) {
            self.state.exit_agents_mode();
            self.state.mode = Mode::Prefix;
            return;
        }
        let event: KeyEvent = key.as_key_event();
        // Three zones, innermost first. `h` always goes up exactly one level;
        // `esc`/`q` leave the mode outright, except while reading, where the
        // panel is what closes.
        let zone = if self.state.agent_trace_panel.is_some() {
            Zone::Reading
        } else if self.state.agent_trace_focus.is_some() {
            Zone::Trace
        } else {
            Zone::Card
        };

        match (zone, event.code) {
            (Zone::Reading, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h')) => {
                // Closing keeps the zone and the selection, so close-and-reopen
                // lands where you left.
                self.state.agent_trace_panel = None;
            }
            (Zone::Reading, KeyCode::Char('j') | KeyCode::Down) => {
                self.state.scroll_trace_detail(1)
            }
            (Zone::Reading, KeyCode::Char('k') | KeyCode::Up) => self.state.scroll_trace_detail(-1),

            (Zone::Trace, KeyCode::Char('h') | KeyCode::Left) => {
                self.state.agent_trace_focus = None;
            }
            (Zone::Trace, KeyCode::Esc | KeyCode::Char('q')) => self.state.exit_agents_mode(),
            (Zone::Trace, KeyCode::Char('j') | KeyCode::Down) => self.state.move_trace_selection(1),
            (Zone::Trace, KeyCode::Char('k') | KeyCode::Up) => self.state.move_trace_selection(-1),
            (Zone::Trace, KeyCode::Char('o') | KeyCode::Enter) => self.state.open_trace_detail(),

            (Zone::Card, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h')) => {
                self.state.exit_agents_mode()
            }
            (Zone::Card, KeyCode::Char('j') | KeyCode::Down) => self.state.move_card_cursor(1),
            (Zone::Card, KeyCode::Char('k') | KeyCode::Up) => self.state.move_card_cursor(-1),
            (Zone::Card, KeyCode::Char('l') | KeyCode::Right) => self.state.descend_to_traces(),
            (Zone::Card, KeyCode::Enter) => self.state.commit_card_cursor(),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    Card,
    Trace,
    Reading,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// Mirrors the watcher's own `trace_snapshot`, which is private to its shell.
/// Every field convention it relies on — `timestamp` being an ISO-8601 string,
/// the `?`/`—` fallbacks, the settled-only status — comes from the shared
/// formatters rather than a second parser.
fn trace_panel(call: &Value, now_unix_ms: u64) -> Panel {
    let tool = format::sanitise(call.get("tool").and_then(Value::as_str).unwrap_or("?"));
    let tool = if tool.is_empty() { "?" } else { &tool };

    let failed = call.get("status").and_then(Value::as_str) == Some("failed");
    let mut status = if failed {
        "✕ failed".to_string()
    } else {
        "✓ done".to_string()
    };
    if let Some(duration) = call.get("durationMs").and_then(Value::as_u64) {
        status.push_str(&format!(" · {}", format::duration_ms(duration)));
    }

    let when = call
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|stamp| {
            format::parse_iso8601_ms(stamp).map(|then| {
                format!(
                    "{} · {}",
                    format::sanitise(stamp),
                    format::age(then, now_unix_ms)
                )
            })
        })
        .unwrap_or_else(|| "—".into());

    let mut rows = vec![
        Row::Entry {
            label: "status".into(),
            value: status,
            enabled: false,
        },
        Row::Entry {
            label: "when".into(),
            value: when,
            enabled: false,
        },
        Row::Rule,
    ];

    let args = call.get("args").and_then(Value::as_str).unwrap_or("");
    if args.is_empty() {
        rows.push(Row::Text("(no arguments retained)".into()));
    } else if let Ok(value) = serde_json::from_str::<Value>(args) {
        let pretty =
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| format::sanitise(args));
        rows.extend(pretty.split('\n').map(|line| Row::Text(line.into())));
    } else {
        rows.push(Row::Text(format::sanitise(args)));
    }

    Panel {
        title: format!("Trace — {tool}"),
        rows,
        footer: "j/k scroll · esc close".into(),
        cursor: None,
        offset: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    const NOW: u64 = 1_756_000_000_000;

    fn state_with_agent() -> AppState {
        let mut state = AppState::test_new();
        state.workspaces = vec![crate::workspace::Workspace::test_new("agent")];
        state.active = Some(0);
        state.mode = Mode::Terminal;
        state.ensure_test_terminals();
        let terminal = state.terminals.values_mut().next().unwrap();
        terminal.detected_agent = Some(crate::detect::Agent::Claude);
        terminal.state = crate::detect::AgentState::Working;
        state.sidebar_collapsed = false;
        state.agents_view = crate::config::AgentsViewConfig::Cards;
        crate::ui::compute_view(&mut state, Rect::new(0, 0, 100, 20));
        state
    }

    #[test]
    fn agents_entry_and_reconciliation_require_visible_cards() {
        use crate::config::AgentsViewConfig::{Cards, Legacy};

        for (collapsed, view) in [(true, Cards), (false, Legacy)] {
            let mut state = state_with_agent();
            assert!(!crate::ui::agent_panel_entries(&state).is_empty());
            state.sidebar_collapsed = collapsed;
            state.agents_view = view;
            state.enter_agents_mode();
            assert_ne!(state.mode, Mode::Agents);
            assert!(state.agent_card_cursor.is_none());

            state.sidebar_collapsed = false;
            state.agents_view = Cards;
            state.enter_agents_mode();
            assert_eq!(state.mode, Mode::Agents);
            state.agent_trace_panel = Some(trace_panel(&serde_json::json!({}), NOW));
            state.sidebar_collapsed = collapsed;
            state.agents_view = view;
            state.reconcile_trace_focus();
            assert_eq!(state.mode, Mode::Terminal);
            assert!(state.agent_card_cursor.is_none());
            assert!(state.agent_trace_panel.is_none());
        }
    }

    #[test]
    fn fresh_agents_entry_clears_the_old_detail_panel() {
        let mut state = state_with_agent();
        state.enter_agents_mode();
        state.agent_trace_focus = state.agent_card_cursor.map(|pane| (pane, "old".into()));
        state.agent_trace_panel = Some(trace_panel(&serde_json::json!({}), NOW));

        state.enter_agents_mode();

        assert_eq!(state.mode, Mode::Agents);
        assert!(state.agent_card_cursor.is_some());
        assert!(state.agent_trace_focus.is_none());
        assert!(state.agent_trace_panel.is_none());
    }

    #[test]
    fn agents_entry_requires_sidebar_body_geometry() {
        for area in [Rect::new(0, 0, 44, 20), Rect::new(0, 0, 100, 3)] {
            let mut state = state_with_agent();
            crate::ui::compute_view(&mut state, area);
            state.enter_agents_mode();
            assert_ne!(state.mode, Mode::Agents);
            assert!(state.agent_card_cursor.is_none());
        }
    }

    #[tokio::test]
    async fn only_foreground_geometry_dismisses_agents_navigation_and_detail() {
        for (area, detail) in [
            (Rect::new(0, 0, 44, 20), false),
            (Rect::new(0, 0, 44, 20), true),
            (Rect::new(0, 0, 100, 7), true),
        ] {
            let mut app = super::super::app_for_mouse_test();
            app.state = state_with_agent();
            app.state.enter_agents_mode();
            if detail {
                app.state.agent_trace_panel = Some(trace_panel(&serde_json::json!({}), NOW));
            }

            // Background rendering computes a temporary view without owning
            // the foreground's input capture state.
            crate::server::render_stream::render_virtual_with_runtime_registry(
                &mut app.state,
                &app.terminal_runtimes,
                area,
                false,
                crate::kitty_graphics::HostCellSize::default(),
            );
            assert_eq!(app.state.mode, Mode::Agents);
            assert_eq!(app.state.agent_trace_panel.is_some(), detail);

            crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 100, 20));
            app.reconcile_panels_from_foreground_view();
            assert_eq!(app.state.mode, Mode::Agents);
            assert_eq!(app.state.agent_trace_panel.is_some(), detail);

            crate::ui::compute_view(&mut app.state, area);
            app.reconcile_panels_from_foreground_view();
            assert_eq!(app.state.mode, Mode::Terminal);
            assert!(app.state.agent_card_cursor.is_none());
            assert!(app.state.agent_trace_panel.is_none());
        }
    }

    #[tokio::test]
    async fn terminal_click_clears_navigation_cursor_on_reconciliation() {
        let mut app = super::super::app_for_mouse_test();
        app.state = state_with_agent();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("other"));
        app.state.ensure_test_terminals();
        for terminal in app.state.terminals.values_mut() {
            terminal.detected_agent = Some(crate::detect::Agent::Claude);
            terminal.state = crate::detect::AgentState::Working;
        }
        app.state.enter_agents_mode();
        let focused = app.state.agent_card_cursor;
        app.state.move_card_cursor(1);
        assert_ne!(app.state.agent_card_cursor, focused);

        let pane = app.state.view.pane_infos[0].inner_rect;
        app.handle_mouse(super::super::mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            pane.x,
            pane.y,
        ));
        assert_eq!(app.state.mode, Mode::Terminal);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 100, 20));
        assert!(app.state.agent_card_cursor.is_none());
        assert_eq!(app.state.trace_anchor_pane(), focused);
    }

    #[test]
    fn reconciliation_preserves_mouse_only_trace_selection() {
        let mut state = state_with_agent();
        let pane = state.trace_anchor_pane().unwrap();
        let workspace = &state.workspaces[0];
        let id = crate::workspace::public_pane_id_for_number(
            &workspace.id,
            workspace.public_pane_number(pane).unwrap(),
        );
        let mut telemetry = PaneTelemetry::with_agent("claude");
        telemetry.tool_calls.push_back(serde_json::json!({
            "toolUseId": "trace", "status": "done", "tool": "Bash"
        }));
        state.agent_telemetry.insert(id, telemetry);
        state.select_trace(pane, "trace");

        crate::ui::compute_view(&mut state, Rect::new(0, 0, 100, 20));
        state.reconcile_trace_visibility();
        assert_eq!(state.mode, Mode::Terminal);
        assert!(state.agent_card_cursor.is_none());
        assert_eq!(state.agent_trace_focus, Some((pane, "trace".into())));
        state.open_trace_detail();
        assert!(state.agent_trace_panel.is_some());
    }

    #[tokio::test]
    async fn prefix_cancellation_clears_agent_navigation() {
        let mut app = super::super::app_for_mouse_test();
        app.state = state_with_agent();
        app.state.enter_agents_mode();
        app.state.agent_trace_focus = app.state.agent_card_cursor.map(|pane| (pane, "old".into()));
        app.state.agent_trace_panel = Some(trace_panel(&serde_json::json!({}), NOW));

        app.handle_trace_key(crate::input::TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ));
        assert_eq!(app.state.mode, Mode::Prefix);
        app.handle_key(crate::input::TerminalKey::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;
        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(app.state.agent_card_cursor.is_none());
        assert!(app.state.agent_trace_focus.is_none());
        assert!(app.state.agent_trace_panel.is_none());
    }

    #[tokio::test]
    async fn trace_detail_consumes_mouse_and_scrolls_its_body() {
        use crossterm::event::{MouseButton, MouseEventKind};

        let mut app = super::super::app_for_mouse_test();
        app.state = state_with_agent();
        app.state.enter_agents_mode();
        app.state.view.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);
        app.state.agent_trace_panel = Some(trace_panel(&serde_json::json!({"args": "tail"}), NOW));
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Right),
        ] {
            app.handle_mouse(super::super::mouse(kind, 60, 10));
            assert_eq!(app.state.mode, Mode::Agents);
            assert!(app.state.agent_trace_panel.is_some());
            assert!(app.state.context_menu.is_none());
        }
        for (kind, expected_offset) in [
            (MouseEventKind::ScrollDown, 1),
            (MouseEventKind::ScrollUp, 0),
            (MouseEventKind::ScrollUp, 0),
        ] {
            app.handle_mouse(super::super::mouse(kind, 60, 10));
            assert_eq!(
                app.state.agent_trace_panel.as_ref().unwrap().offset,
                expected_offset
            );
        }
        app.state.handle_pane_mouse_only(
            &app.terminal_runtimes,
            super::super::mouse(MouseEventKind::ScrollDown, 60, 10),
        );
        assert_eq!(app.state.agent_trace_panel.as_ref().unwrap().offset, 1);
    }

    fn rows_text(panel: &Panel) -> Vec<String> {
        panel
            .rows
            .iter()
            .map(|row| match row {
                Row::Entry { label, value, .. } => format!("{label}: {value}"),
                Row::Note(text) | Row::Warn(text) | Row::Text(text) => text.clone(),
                Row::Rule => "---".into(),
            })
            .collect()
    }

    #[test]
    fn zones_are_derived_from_state_not_tracked_separately() {
        // The zone is a projection of (panel, focus), so it can never disagree
        // with what is rendered.
        let mut state = AppState::test_new();
        assert!(state.agent_card_cursor.is_none());

        state.agent_card_cursor = Some(PaneId::from_raw(7));
        assert_eq!(
            state.trace_anchor_pane(),
            Some(PaneId::from_raw(7)),
            "the cursor outranks the focused pane while the mode is up"
        );

        state.exit_agents_mode();
        assert!(state.agent_card_cursor.is_none());
        assert!(state.agent_trace_focus.is_none());
        assert!(state.agent_trace_panel.is_none());
    }

    #[test]
    fn reconcile_drops_a_cursor_whose_card_left_the_list() {
        let mut state = AppState::test_new();
        state.mode = Mode::Agents;
        // test_new() has no agent panel entries, so any cursor is already stale.
        state.agent_card_cursor = Some(PaneId::from_raw(42));

        state.reconcile_trace_focus();

        assert!(
            state.agent_card_cursor.is_none(),
            "a cursor pointing at an unrendered card takes the mode with it"
        );
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn moving_the_cursor_on_an_empty_list_is_inert() {
        let mut state = AppState::test_new();
        state.mode = Mode::Agents;

        state.move_card_cursor(1);
        state.descend_to_traces();
        state.commit_card_cursor();

        assert!(state.agent_card_cursor.is_none());
        assert!(state.agent_trace_focus.is_none());
    }

    #[test]
    fn status_line_carries_the_watcher_duration_format() {
        let panel = trace_panel(
            &serde_json::json!({
                "tool": "Bash", "status": "done", "durationMs": 1400, "args": ""
            }),
            NOW,
        );

        assert_eq!(panel.title, "Trace — Bash");
        // The exported `format::duration_ms`, not a second implementation.
        assert_eq!(rows_text(&panel)[0], "status: ✓ done · 1.4s");
        assert_eq!(panel.cursor, None, "the panel scrolls, it does not select");
    }

    #[test]
    fn failed_calls_and_missing_metadata_use_the_documented_fallbacks() {
        let panel = trace_panel(
            &serde_json::json!({"tool": "", "status": "failed", "args": ""}),
            NOW,
        );
        let rows = rows_text(&panel);

        assert_eq!(panel.title, "Trace — ?", "an empty tool renders ?");
        assert_eq!(rows[0], "status: ✕ failed", "no duration segment, no ·");
        assert_eq!(rows[1], "when: —", "a missing timestamp renders an em dash");
        assert_eq!(
            rows[3], "(no arguments retained)",
            "the panel is never blank"
        );
    }

    #[test]
    fn json_args_pretty_print_and_plain_args_stay_verbatim() {
        let pretty = trace_panel(
            &serde_json::json!({
                "tool": "Edit", "status": "done", "args": r#"{"path":"a.rs"}"#
            }),
            NOW,
        );
        // Pretty-printing splits into one Row::Text per hard line so the
        // panel's wrapping preserves the shape.
        assert!(rows_text(&pretty)
            .iter()
            .any(|row| row.contains("\"path\"")));
        assert!(pretty.rows.len() > 4, "one row per pretty-printed line");

        let verbatim = trace_panel(
            &serde_json::json!({
                "tool": "Bash", "status": "done", "args": "cargo test --all"
            }),
            NOW,
        );
        assert_eq!(rows_text(&verbatim)[3], "cargo test --all");
    }

    #[test]
    fn timestamp_is_parsed_as_iso8601_and_frozen_into_the_row() {
        let panel = trace_panel(
            &serde_json::json!({
                "tool": "Read", "status": "done", "args": "",
                "timestamp": "2026-09-01T10:00:00.000Z"
            }),
            NOW,
        );

        let when = &rows_text(&panel)[1];
        assert!(
            when.starts_with("when: 2026-09-01T10:00:00.000Z · "),
            "reading the ISO string as epoch millis would render an em dash: {when}"
        );
    }
}
