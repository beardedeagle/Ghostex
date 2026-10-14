//! Whether the session a work area pane shows is in attention, read from the store the sidebar's
//! blue dot reads.

use ghostex_gx_core::SessionKey;

use crate::GhostexGpuiApp;
use crate::TerminalSessionId;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-10-09 DECISION:
    /// User: "top border here should have 1px blue line when the session here has \"attention\" status". A pane draws its attention outline, whose top line is the work area's top edge, exactly while the sidebar row of the session it shows has its blue dot, and clears it by the same acknowledgement that clears the dot.
    ///
    /// CDXC:Workarea 2026-10-09 WHY:
    /// The pane used to read the workspace model's copy of the activity, which `focus_pane` and `select_tab` set to idle at once while the store waits for its own acknowledgement (gx-core `attention.rs`), and an unchanged focus snapshot never reconciles the copy back. So the dot stayed blue while the pane showed nothing. Reading the store keeps one answer for both.
    pub(crate) fn gx_store_shell_session_has_attention(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| {
                (*mapped == session_id)
                    .then(|| SessionKey::local(key.project_id.clone(), key.session_id.clone()))
            })
            .or_else(|| {
                self.remote_attach_sessions
                    .iter()
                    .find_map(|(key, mapped)| {
                        (*mapped == session_id).then(|| {
                            SessionKey::remote(
                                key.remote_machine_id.clone(),
                                key.project_id.clone(),
                                key.session_id.clone(),
                            )
                        })
                    })
            });
        key.and_then(|key| self.gx_store.core.presentation().session(&key))
            .is_some_and(|session| session.activity.as_str() == "attention")
    }
}
