//! What Settings > Cloud Boxes asks the app to do: run one agentbox setup step in a command-pane
//! terminal, and start the "Set it up for me" agent session.

use std::time::Duration;

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

/// The arguments a `runAgentboxTerminalCommand` message may carry to gxserver. gxserver validates
/// each one again; anything else in the message is dropped.
const AGENTBOX_TERMINAL_COMMAND_ARGUMENTS: [&str; 5] =
    ["provider", "agent", "host", "alias", "ssh"];

impl GhostexGpuiApp {
    /// CDXC:AgentBox 2026-10-01 WHY:
    /// Settings > Cloud Boxes sends only a step name (`install`, `login`, `prepare`, `agentLogin`, `remoteDockerAdd`, ...) and its arguments. gxserver's `/api/agentbox` `terminalCommand` answers with the command text, so a page can never make the app run text of its choosing, the same rule as `run_managed_tool_terminal_command`. The step runs in a real terminal because agentbox's logins and wizards need one.
    /// SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/cloud_boxes.rs, docs/2026-10-01/agentbox/PLAN.md (wire contract 3).
    pub(crate) fn run_agentbox_terminal_command(
        &mut self,
        message: &serde_json::Map<String, serde_json::Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = message
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let mut params = serde_json::Map::new();
        params.insert("action".into(), "terminalCommand".into());
        params.insert("command".into(), command.clone().into());
        for key in AGENTBOX_TERMINAL_COMMAND_ARGUMENTS {
            if let Some(value) = message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                params.insert(key.into(), value.into());
            }
        }
        let action_suffix = AGENTBOX_TERMINAL_COMMAND_ARGUMENTS
            .iter()
            .filter_map(|key| params.get(*key).and_then(serde_json::Value::as_str))
            .fold(command.clone(), |id, part| format!("{id}.{part}"));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/agentbox",
                        &serde_json::Value::Object(params),
                        Duration::from_secs(30),
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let answer = match result {
                    Ok(answer) => answer,
                    Err(error) => {
                        this.dispatch_gpui_app_modal_toast(
                            "error",
                            "Couldn't start the agentbox step",
                            &error,
                            cx,
                        );
                        return;
                    }
                };
                let Some(command_text) = answer["command"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                else {
                    this.dispatch_gpui_app_modal_toast(
                        "error",
                        "Couldn't start the agentbox step",
                        "gxserver did not return a command for this step.",
                        cx,
                    );
                    return;
                };
                let title = answer["title"]
                    .as_str()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("agentbox")
                    .to_string();
                // The step changes what is ready; the next launcher or picker reads it again.
                crate::app::gx_store::invalidate_agentbox_status();
                this.open_gpui_command_action_terminal(
                    format!("ghostex.gpui.agentbox.{action_suffix}"),
                    title.clone(),
                    command_text.to_string(),
                    false,
                    false,
                    window,
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    "info",
                    &title,
                    "Finish it in the terminal tab. Settings > Cloud Boxes updates while it runs.",
                    cx,
                );
            });
        })
        .detach();
    }

    /// "Set it up for me" on Settings > Cloud Boxes: an agent session in the Ghostex folder
    /// project (the one Help chats use) that installs and configures agentbox.
    pub(crate) fn run_agentbox_setup_chat(&mut self, cx: &mut gpui::Context<Self>) {
        let project_path = shared_settings::ghostex_storage_paths()
            .config_dir
            .to_string_lossy()
            .to_string();
        let dispatched = self.dispatch_gpui_os_integration_command_message(
            serde_json::json!({
                "action": "createAgentboxSetupChat",
                "projectPath": project_path,
            }),
            cx,
        );
        if !dispatched {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Couldn't start the setup agent",
                "The sidebar is not ready to start a session yet.",
                cx,
            );
        }
    }
}
