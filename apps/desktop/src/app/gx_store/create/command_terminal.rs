//! A chat's Run in terminal and Paste in terminal: a new plain terminal of the chat session's
//! project, in the session's own folder, with a command from a reply in it.
//!
//! The calls are the sidebar's terminal create (`terminal.rs`) on this computer or down a remote
//! machine's tunnel, then the shell started at once so the command reaches it whichever view opens
//! it, then the terminal opened. Run hands the command to gxserver as the terminal's startup text,
//! which types it and presses Enter; Paste waits for the shell's prompt and types it there, leaving
//! Enter to the user.
//!
//! SEE-ALSO: `ghostex run-in-terminal` (server/src/ghostex_cli/run_in_terminal.rs) makes the same
//! calls for the phone; packages/gx-chat-core/src/transcript/run_command.rs decides what runs.

use std::time::Duration;

use ghostex_gx_core::{
    ProjectKey, SessionKey, command_terminal_create_params, created_session,
    open_remote_session_terminal, start_provider_params,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

/// How long Paste waits for the new shell to draw its prompt before typing anyway.
const PROMPT_WAIT: Duration = Duration::from_secs(5);
const PROMPT_POLL: Duration = Duration::from_millis(150);

impl GhostexGpuiApp {
    /// `submit` presses Enter after the command (Run); without it the command is only typed
    /// (Paste).
    pub(crate) fn gx_store_create_command_terminal(
        &mut self,
        project: ProjectKey,
        cwd: Option<String>,
        command: String,
        submit: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.create.counters.terminal_creates += 1;
        let remote = match project.machine.remote_id() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.gx_store_create_toast(
                        "warning",
                        "Could not open a terminal",
                        Some("That computer is not connected."),
                        cx,
                    );
                    return;
                }
            },
            None => {
                if !self.gx_store_ensure_local_project_path_available(&project.project_id, cx) {
                    return;
                }
                None
            }
        };
        let params =
            command_terminal_create_params(&project.project_id, cwd.as_deref(), &command, submit);
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(remote.clone(), "/api/createSession", params).await;
            let Some((created_project, session_id)) = created
                .as_ref()
                .ok()
                .and_then(|response| created_session(response, Some(&project.project_id)))
            else {
                let message = created.err().map(|error| error.message);
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_create_toast(
                        "warning",
                        "Could not open a terminal",
                        message.as_deref(),
                        cx,
                    );
                });
                return;
            };
            let created_project = created_project.unwrap_or_else(|| project.project_id.clone());
            let ids = start_provider_params(&created_project, &session_id);
            // A failed start still opens the terminal, whose attach starts it again.
            let _ = gx_rpc(remote.clone(), "/api/startSessionProvider", ids.clone()).await;
            if !submit {
                // Text typed before the shell reads its line is echoed once above the prompt too,
                // so Paste waits for the prompt to show.
                let polls = PROMPT_WAIT.as_millis() / PROMPT_POLL.as_millis();
                for _ in 0..polls {
                    let shown = gx_rpc(remote.clone(), "/api/readSessionText", ids.clone())
                        .await
                        .ok()
                        .and_then(|read| read["text"].as_str().map(|text| !text.trim().is_empty()));
                    if shown == Some(true) {
                        break;
                    }
                    cx.background_executor().timer(PROMPT_POLL).await;
                }
                let mut typed = ids.clone();
                typed["text"] = json!(command);
                let _ = gx_rpc(remote.clone(), "/api/sendSessionText", typed).await;
            }
            let _ = this.update(cx, |this, cx| {
                match project.machine.remote_id() {
                    // `openRemoteSessionTerminal`, as a remote project's terminal create opens it.
                    Some(machine_id) => {
                        let session = SessionKey::remote(machine_id, created_project, session_id);
                        let payload: Value = open_remote_session_terminal(
                            &session.to_sidebar_session_id(),
                            false,
                            None,
                        );
                        this.receive_sidebar_native_project_path_action_payload(
                            &payload.to_string(),
                            cx,
                        );
                    }
                    None => {
                        this.gx_store_focus_created_session(
                            &created_project,
                            &session_id,
                            false,
                            Some("terminal"),
                            cx,
                        );
                        this.show_created_terminal_tab_while_attaching(
                            &created_project,
                            &session_id,
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }
}
