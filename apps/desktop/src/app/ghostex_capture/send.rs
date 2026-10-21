//! Sending the prompt box: to a new session in a project, or to a running session, without
//! bringing Ghostex forward.
//!
//! A new session follows the Git workflows' prompt agent start (create, start the provider, queue
//! the prompt with `startupSend`). The session the prompt went to is shown in the main window only
//! when `ghostexCaptureSwitchToSession` is on, and Ghostex still stays behind the user's app.
//! An existing session gets a normal chat message. Screenshots travel as `[Image #N](path)`
//! references in the text, the chat composer's format; a remote machine gets its own copy of each
//! picture first, since it cannot read this computer's files.

use std::path::PathBuf;

use base64::Engine as _;
use ghostex_gx_core::{ProjectKey, SessionKey};
use gpui::Context;
use serde_json::{Value, json};

use super::targets::Target;
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::shared_settings;

type Remote = Option<GpuiRemoteGxserverRequestTarget>;

/// Replaces each `[Image #N]` the user left in the text with its link, and adds the pictures the
/// text never mentions at the end.
pub(super) fn compose(text: &str, images: &[(u32, String)]) -> String {
    let mut out = text.trim().to_string();
    let mut missing = Vec::new();
    for (number, path) in images {
        let token = format!("[Image #{number}]");
        let link = format!("[Image #{number}]({path})");
        match out.find(&token) {
            Some(at) if !out[at + token.len()..].starts_with('(') => {
                out.replace_range(at..at + token.len(), &link);
            }
            _ => missing.push(link),
        }
    }
    if !missing.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&missing.join(" "));
    }
    out
}

/// Gives a remote machine its own copy of each picture and answers the paths it saved them at.
pub(super) async fn upload_images(
    remote: &Remote,
    project_id: &str,
    session_id: &str,
    images: &[(u32, PathBuf)],
) -> Result<Vec<(u32, String)>, String> {
    let mut uploaded = Vec::new();
    for (number, path) in images {
        let local = || path.to_string_lossy().to_string();
        if remote.is_none() {
            uploaded.push((*number, local()));
            continue;
        }
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("ghostex-capture-{number}.png"));
        let saved = gx_rpc(
            remote.clone(),
            "/api/saveSessionChatImage",
            json!({
                "base64Data": base64::engine::general_purpose::STANDARD.encode(bytes),
                "projectId": project_id,
                "sessionId": session_id,
                "suggestedName": name,
            }),
        )
        .await
        .map_err(|error| error.message)?;
        let path = saved
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "The remote computer did not save the screenshot.".to_string())?;
        uploaded.push((*number, path.to_string()));
    }
    Ok(uploaded)
}

/// What a send needs, read from the app before the async work starts.
pub(super) enum Plan {
    NewSession {
        remote: Remote,
        key: ProjectKey,
        params: serde_json::Map<String, Value>,
        title: String,
    },
    Session {
        remote: Remote,
        key: SessionKey,
        title: String,
    },
}

impl GhostexGpuiApp {
    /// `first_message` names a new session after the prompt; a draft is named from its chat draft.
    pub(super) fn ghostex_capture_send_plan(
        &self,
        target: &Target,
        first_message: Option<&str>,
    ) -> Result<Plan, String> {
        let unreachable = "That computer is not connected right now.".to_string();
        match target {
            Target::NewSession { project, title } => {
                let key = ProjectKey::parse_workspace_project_id(project)
                    .ok_or_else(|| "That project is no longer available.".to_string())?;
                let remote = self
                    .git_scope_for_key(key.clone())
                    .map(|scope| scope.remote)
                    .map_err(|_| unreachable)?;
                let agent_id = self.git_default_prompt_agent_id(None);
                let agent = self.git_hud_agent(&agent_id);
                let mut params = serde_json::Map::new();
                params.insert("agentId".into(), json!(agent_id));
                params.insert("draft".into(), json!(true));
                params.insert("projectId".into(), json!(key.project_id));
                params.insert(
                    "runtimeSettings".into(),
                    self.git_first_prompt_title_runtime_settings(first_message, None),
                );
                params.insert("surface".into(), json!("workspace"));
                if remote.is_some() {
                    params.insert("requireLaunchCommand".into(), json!(true));
                } else {
                    let mut launch = serde_json::Map::new();
                    launch.insert(
                        "agentCommand".into(),
                        json!(
                            agent
                                .as_ref()
                                .and_then(|agent| agent.command.clone())
                                .unwrap_or_default()
                        ),
                    );
                    if let Some(icon) = agent.as_ref().and_then(|agent| agent.icon.clone()) {
                        launch.insert("icon".into(), json!(icon));
                    }
                    params.insert("launchSettings".into(), Value::Object(launch));
                }
                Ok(Plan::NewSession {
                    remote,
                    key,
                    params,
                    title: title.clone(),
                })
            }
            Target::Session { session, title, .. } => {
                let key = SessionKey::parse_sidebar_session_id(session)
                    .ok_or_else(|| "That session is no longer available.".to_string())?;
                let remote = self
                    .git_scope_for_key(ProjectKey {
                        machine: key.machine.clone(),
                        project_id: key.project_id.clone(),
                    })
                    .map(|scope| scope.remote)
                    .map_err(|_| unreachable)?;
                Ok(Plan::Session {
                    remote,
                    key,
                    title: title.clone(),
                })
            }
        }
    }

    pub(super) fn send_ghostex_capture_prompt(&mut self, cx: &mut Context<Self>) {
        let prompt = &self.ghostex_capture.prompt;
        if prompt.sending {
            return;
        }
        let text = prompt.draft.trim().to_string();
        if text.is_empty() && prompt.attachments.is_empty() {
            return;
        }
        let Some(target) = prompt.target.clone() else {
            self.ghostex_capture.prompt.picker_open = true;
            cx.notify();
            return;
        };
        let images: Vec<(u32, PathBuf)> = prompt
            .attachments
            .iter()
            .map(|attachment| (attachment.number, attachment.path.clone()))
            .collect();
        let plan = match self.ghostex_capture_send_plan(&target, Some(&text)) {
            Ok(plan) => plan,
            Err(message) => {
                self.dispatch_gpui_app_modal_toast("warning", "Could not send", &message, cx);
                return;
            }
        };
        let created = matches!(plan, Plan::NewSession { .. });
        self.ghostex_capture.prompt.sending = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result: Result<(SessionKey, String), String> = match plan {
                Plan::NewSession {
                    remote,
                    key,
                    params,
                    title,
                } => {
                    async {
                        let created = gx_rpc(
                            remote.clone(),
                            "/api/createAgentSession",
                            Value::Object(params),
                        )
                        .await
                        .map_err(|error| error.message)?;
                        let session_id = created
                            .pointer("/session/sessionId")
                            .and_then(Value::as_str)
                            .filter(|id| !id.trim().is_empty())
                            .ok_or_else(|| "Could not create the session.".to_string())?
                            .to_string();
                        let project_id = created
                            .pointer("/session/projectId")
                            .and_then(Value::as_str)
                            .filter(|id| !id.trim().is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| key.project_id.clone());
                        let uploaded =
                            upload_images(&remote, &project_id, &session_id, &images).await?;
                        let message = compose(&text, &uploaded);
                        gx_rpc(
                            remote.clone(),
                            "/api/startSessionProvider",
                            json!({ "projectId": project_id, "sessionId": session_id }),
                        )
                        .await
                        .map_err(|error| error.message)?;
                        let receipt = gx_rpc(
                            remote,
                            "/api/queueSessionChatPrompt",
                            json!({
                                "projectId": project_id,
                                "sessionId": session_id,
                                "startupSend": true,
                                "text": message,
                                "sendRequestId": uuid::Uuid::new_v4().to_string(),
                            }),
                        )
                        .await
                        .map_err(|error| error.message)?;
                        if receipt
                            .pointer("/prompt/id")
                            .and_then(Value::as_str)
                            .is_none_or(|id| id.trim().is_empty())
                        {
                            return Err("The new session did not accept the prompt.".to_string());
                        }
                        Ok((
                            SessionKey {
                                machine: key.machine,
                                project_id,
                                session_id,
                            },
                            format!("Sent to a new session in {title}"),
                        ))
                    }
                    .await
                }
                Plan::Session { remote, key, title } => {
                    async {
                        let uploaded =
                            upload_images(&remote, &key.project_id, &key.session_id, &images)
                                .await?;
                        let message = compose(&text, &uploaded);
                        gx_rpc(
                            remote,
                            "/api/sendSessionChatMessage",
                            json!({
                                "projectId": key.project_id,
                                "sessionId": key.session_id,
                                "text": message,
                                "sendRequestId": uuid::Uuid::new_v4().to_string(),
                            }),
                        )
                        .await
                        .map_err(|error| error.message)?;
                        Ok((key, format!("Sent to {title}")))
                    }
                    .await
                }
            };
            let _ = this.update(cx, |app, cx| {
                app.ghostex_capture.prompt.sending = false;
                match result {
                    Ok((session, note)) => {
                        app.clear_ghostex_capture_draft_session(cx);
                        let prompt = &mut app.ghostex_capture.prompt;
                        prompt.draft.clear();
                        prompt.attachments.clear();
                        prompt.next_number = 0;
                        prompt.target = None;
                        app.close_ghostex_capture_prompt(cx);
                        app.show_ghostex_capture_sent_session(&session, created, cx);
                        app.show_ghostex_capture_note(note, session, cx);
                    }
                    Err(message) => {
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Could not send",
                            &message,
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Selects the session a prompt went to in the Ghostex window, without bringing Ghostex in
    /// front of the app the user is in.
    fn show_ghostex_capture_sent_session(
        &mut self,
        session: &SessionKey,
        created: bool,
        cx: &mut Context<Self>,
    ) {
        if !shared_settings::shared_sidebar_settings_snapshot().ghostex_capture_switch_to_session()
        {
            return;
        }
        if !created {
            self.gx_store_focus_activated_session(&session.to_sidebar_session_id(), cx);
            return;
        }
        match session.machine.remote_id() {
            // The store holds the focus on a new session until its row arrives.
            None => self.gx_store_focus_created_session(
                &session.project_id,
                &session.session_id,
                false,
                None,
                cx,
            ),
            Some(_) => {
                let payload = ghostex_gx_core::open_remote_session_terminal(
                    &session.to_sidebar_session_id(),
                    false,
                    None,
                );
                self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
            }
        }
    }
}
