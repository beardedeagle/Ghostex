//! A chat's Run in terminal and Paste in terminal: the command a reply showed, in a new plain
//! terminal, run (Enter pressed) or only typed.
//!
//! CDXC:SessionChat 2026-10-24 WHY: the command never goes to the chat's own terminal, where the agent would read it as a prompt; it goes to a new terminal of the same project, in the session's own folder (its worktree when it has one), shown so the user can answer a login prompt or a menu. A remote session's terminal opens on that computer.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/run_command.rs decides which commands qualify and what runs; apps/desktop/src/app/gx_store/create/command_terminal.rs makes the terminal; `ghostex run-in-terminal` (server/src/ghostex_cli/run_in_terminal.rs) is the same for the phone.

use ghostex_gx_core::{ProjectKey, SessionKey};

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn run_session_chat_command_in_terminal(
        &mut self,
        session_id: TerminalSessionId,
        command: &str,
        submit: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let session = match self.workspace_terminal_key_for_shell_session(session_id) {
            Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                SessionKey::local(key.project_id, key.session_id)
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                SessionKey::remote(key.remote_machine_id, key.project_id, key.session_id)
            }
            None => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not run the command",
                    "This chat's session is not open.",
                    cx,
                );
                return;
            }
        };
        let cwd = self
            .gx_store
            .core
            .presentation()
            .session(&session)
            .and_then(|row| row.cwd.clone());
        self.gx_store_create_command_terminal(
            ProjectKey {
                machine: session.machine,
                project_id: session.project_id,
            },
            cwd,
            command.to_string(),
            submit,
            cx,
        );
    }
}
