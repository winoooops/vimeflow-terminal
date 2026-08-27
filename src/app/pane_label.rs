// Modified from herdr by the vimeflow project — see FORK.md
//
// Platform-neutral pane-label mutation shared by the pane-rename API and the
// unix-only title-sync engine. Lives outside the cfg(unix) title_sync module
// so Windows builds keep the API path (issue #5).

use super::App;

impl App {
    pub(crate) fn mutate_pane_label(
        &mut self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
        label: Option<String>,
    ) -> Option<crate::api::schema::PaneInfo> {
        let terminal_id = self
            .state
            .workspaces
            .get(ws_idx)?
            .terminal_id(pane_id)?
            .clone();
        let label = label.and_then(|label| {
            let label = label.trim().to_string();
            (!label.is_empty()).then_some(label)
        });
        let changed = {
            let terminal = self.state.terminals.get_mut(&terminal_id)?;
            if terminal.manual_label == label {
                false
            } else {
                match label {
                    Some(label) => terminal.set_manual_label(label),
                    None => terminal.clear_manual_label(),
                }
                true
            }
        };
        if changed {
            self.state.mark_session_dirty();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        self.pane_info(ws_idx, pane_id)
    }
}
