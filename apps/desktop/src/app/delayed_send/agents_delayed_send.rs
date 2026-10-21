//! Agents session delayed sends: scheduling, send-when-stopped polling, restore, tickers and firing.

use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn enrich_gpui_agents_delayed_send_open_message(
        &self,
        open_message: &mut serde_json::Value,
        session_id: TerminalSessionId,
    ) {
        if let Some(agent_icon) = self
            .agents_workspace
            .session(session_id)
            .and_then(|session| session.agent_icon)
        {
            open_message["agentIcon"] = serde_json::json!(agent_icon);
        }
        open_message["supportsSendWhenAgentStops"] = serde_json::json!(true);
        open_message["supportsCloseAfterDone"] = serde_json::json!(
            !self
                .agents_sidebar_session_for_terminal(session_id)
                .is_some_and(|session| session.is_orchestrator)
        );
        /*
        CDXC:DelayedSend 2026-09-21 WHY:
        The open message names the session by its shell id (`GW…`), which never matches the sidebar-state `sessionsById` keys the dialog would otherwise read, so an armed Close After Done opened as switched off and could not be turned off from the dialog. State it here from the same armed map the chat indicator draws from.
        */
        open_message["closeAfterDoneActive"] = serde_json::json!(
            self.session_chat_armed_actions(session_id)
                .as_array()
                .is_some_and(|actions| actions
                    .iter()
                    .any(|action| action["id"] == "closeAfterDone"))
        );
        let remote_key = self
            .remote_attach_sessions
            .iter()
            .find_map(|(key, mapped_session_id)| (*mapped_session_id == session_id).then_some(key));
        if let Some(key) = remote_key {
            // Native hotkeys start with a shell id; both the awake picker and the saved action need the hosting daemon's routed identity.
            open_message["sessionId"] = serde_json::json!(gpui_remote_scoped_session_id(
                &key.remote_machine_id,
                &key.project_id,
                &key.session_id,
            ));
        }
        let supports_project_scope = remote_key.is_some()
            || self
                .local_workspace_session_mappings
                .iter()
                .any(|(_, mapped_session_id)| *mapped_session_id == session_id);
        open_message["supportsSendWhenAllProjectSessionsStop"] =
            serde_json::json!(supports_project_scope);
        // The daemon's armed send, which the row's own open carries; a local watcher or timer
        // below restates it.
        self.gx_store_seed_daemon_delayed_send(open_message, session_id);
        /*
        CDXC:DelayedSend 2026-08-19:
        Armed Delayed Sends live on the daemon, and the sidebar row already
        carries that projected trigger state into the open message. Only a
        locally owned watcher/timer may restate it, so this enrichment must not
        blank the daemon-owned trigger back to "After a delay".
        */
        if let Some(watcher) = self
            .agents_send_when_stopped_watchers
            .get(&session_id)
            .cloned()
        {
            let is_project_scope =
                matches!(&watcher.scope, GpuiAgentsSendWhenStoppedScope::Project(_));
            open_message["sendWhenAllProjectSessionsStopActive"] =
                serde_json::json!(is_project_scope);
            open_message["sendWhenAgentStopsActive"] = serde_json::json!(!is_project_scope);
            let is_working = self
                .gpui_agents_send_when_stopped_scope_is_working(session_id, &watcher.scope)
                .unwrap_or(false);
            open_message["delayedSendRemainingLabel"] = serde_json::json!(
                gpui_agents_send_when_stopped_remaining_label(&watcher, is_working, Instant::now(),)
            );
            if let Some(object) = open_message.as_object_mut() {
                object.remove("delayedSendDeadlineAt");
            }
            return;
        }
        if let Some(timer) = self.agents_delayed_send_timers.get(&session_id).copied() {
            let remaining_ms = timer.remaining_ms(SystemTime::now());
            open_message["delayedSendDeadlineAt"] =
                serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
            open_message["delayedSendRemainingLabel"] =
                serde_json::json!(gpui_command_delayed_send_countdown_label(remaining_ms));
        }
    }

    /// Agents Delayed Send uses the same unconditional modal presentation as
    /// Rename. The later schedule command still requires the exact terminal
    /// body to be mounted before it arms a timer.
    pub(crate) fn open_gpui_delayed_send_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        self.open_gpui_delayed_send_modal_for_agents_session(session_id, cx)
    }

    /// Opens Delayed Actions for one Agents session, so a chat or terminal bar request manages its own session rather than whichever pane holds focus.
    pub(crate) fn open_gpui_delayed_send_modal_for_agents_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let title = self.agents_workspace_tab_display_title(session_id);
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "sessionId": gpui_agents_session_external_id(session_id),
            "title": title,
            "type": "open",
        });
        self.enrich_gpui_agents_delayed_send_open_message(&mut open_message, session_id);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn handle_gpui_schedule_agents_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command)
        else {
            return;
        };
        let Some(key) = self.local_workspace_key_for_shell_session(session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "The selected terminal is not attached to a gxserver session.",
                cx,
            );
            return;
        };
        let delay_ms = command.get("delayMs").and_then(serde_json::Value::as_u64);
        let send_when_agent_stops = command
            .get("sendWhenAgentStops")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let send_when_all_project_sessions_stop = command
            .get("sendWhenAllProjectSessionsStop")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let watched = command
            .get("sendWhenSpecificAgentFinishes")
            .filter(|value| !value.is_null());
        if usize::from(delay_ms.is_some())
            + usize::from(send_when_agent_stops)
            + usize::from(send_when_all_project_sessions_stop)
            + usize::from(watched.is_some())
            != 1
            || delay_ms.is_some_and(|delay_ms| {
                gpui_command_delayed_send_duration_from_millis(delay_ms).is_none()
            })
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "Choose exactly one valid Delayed Send trigger.",
                cx,
            );
            return;
        }
        let params = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "delayMs": delay_ms,
            "sendWhenAgentStops": send_when_agent_stops,
            "sendWhenSpecificAgentFinishes": watched,
            "sendWhenAllProjectSessionsStop": send_when_all_project_sessions_stop,
        });
        let description = if watched.is_some() {
            "Presses Enter after the selected agent has finished working for 10 seconds."
                .to_string()
        } else if send_when_agent_stops {
            "Presses Enter after the agent has finished working for 10 seconds.".to_string()
        } else if send_when_all_project_sessions_stop {
            "Presses Enter after all agents in the project have finished working for 10 seconds."
                .to_string()
        } else {
            let duration = Duration::from_millis(delay_ms.unwrap_or_default());
            format!(
                "Presses Enter in {}.",
                gpui_command_delayed_send_duration_label(duration)
            )
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_schedule_agents_delayed_send_with_current_gxserver_build(&params)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if result.is_ok() {
                    this.agents_delayed_send_timers.remove(&session_id);
                    this.agents_send_when_stopped_watchers.remove(&session_id);
                    this.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                    this.persist_shell_layout_state();
                    this.dispatch_gpui_app_modal_toast(
                        "info",
                        "Delayed Send scheduled",
                        &description,
                        cx,
                    );
                } else {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Delayed Send unavailable",
                        "gxserver could not persist this Delayed Send.",
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_cancel_agents_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command)
        else {
            return;
        };
        let Some(key) = self.local_workspace_key_for_shell_session(session_id) else {
            return;
        };
        let response = gpui_gxserver_rpc_result(
            "/api/cancelDelayedSend",
            &serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
            Duration::from_secs(5),
        );
        let removed_timer = self
            .agents_delayed_send_timers
            .remove(&session_id)
            .is_some();
        let removed_watcher = self
            .agents_send_when_stopped_watchers
            .remove(&session_id)
            .is_some();
        let changed = response
            .ok()
            .and_then(|result| result.get("changed").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);
        if changed || removed_timer || removed_watcher {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            self.dispatch_gpui_app_modal_toast("info", "Delayed Send canceled", "", cx);
            cx.notify();
        } else {
            self.dispatch_gpui_app_modal_toast("info", "No Delayed Send timer is active", "", cx);
        }
    }

    pub(crate) fn gpui_agents_delayed_send_session_id_from_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TerminalSessionId> {
        // Sidebar cards retain their combined gxserver identity while focused
        // pane actions use GW ids. Resolve both only at the command boundary.
        let external_session_id = command.get("sessionId")?.as_str()?;
        self.gpui_titlebar_resource_shell_session_id(external_session_id)
    }

    pub(crate) fn restore_gpui_agents_delayed_sends(
        &mut self,
        intents: Vec<GpuiAgentsDelayedSendRestoreIntent>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut restored = false;
        for intent in intents {
            if self.agents_workspace.session(intent.session_id).is_none() {
                continue;
            }
            self.agents_delayed_send_generation =
                self.agents_delayed_send_generation.wrapping_add(1);
            let generation = self.agents_delayed_send_generation;
            match intent.trigger {
                GpuiAgentsDelayedSendRestoreTrigger::Timer { remaining_ms } => {
                    let duration = gpui_command_delayed_send_restore_duration(remaining_ms);
                    let deadline_at = SystemTime::now()
                        .checked_add(duration)
                        .unwrap_or_else(SystemTime::now);
                    self.agents_delayed_send_timers.insert(
                        intent.session_id,
                        GpuiCommandDelayedSendTimer {
                            deadline_at,
                            generation,
                        },
                    );
                    self.spawn_gpui_agents_delayed_send_fire(
                        intent.session_id,
                        generation,
                        duration,
                        cx,
                    );
                }
                GpuiAgentsDelayedSendRestoreTrigger::WhenAgentFinishesWorking => {
                    let scope = GpuiAgentsSendWhenStoppedScope::Session;
                    let Some(is_working) = self
                        .gpui_agents_send_when_stopped_scope_is_working(intent.session_id, &scope)
                    else {
                        continue;
                    };
                    self.agents_send_when_stopped_watchers.insert(
                        intent.session_id,
                        GpuiAgentsSendWhenStoppedWatcher {
                            generation,
                            non_working_since: (!is_working).then(Instant::now),
                            scope,
                        },
                    );
                    self.spawn_gpui_agents_send_when_stopped_poll(
                        intent.session_id,
                        generation,
                        cx,
                    );
                }
                GpuiAgentsDelayedSendRestoreTrigger::WhenAllAgentsFinishWorking { project_id } => {
                    let scope = GpuiAgentsSendWhenStoppedScope::Project(project_id);
                    let Some(is_working) = self
                        .gpui_agents_send_when_stopped_scope_is_working(intent.session_id, &scope)
                    else {
                        continue;
                    };
                    self.agents_send_when_stopped_watchers.insert(
                        intent.session_id,
                        GpuiAgentsSendWhenStoppedWatcher {
                            generation,
                            non_working_since: (!is_working).then(Instant::now),
                            scope,
                        },
                    );
                    self.spawn_gpui_agents_send_when_stopped_poll(
                        intent.session_id,
                        generation,
                        cx,
                    );
                }
            }
            restored = true;
        }
        if restored {
            self.ensure_gpui_agents_delayed_send_countdown_ticker(cx);
            self.ensure_gpui_agents_delayed_send_persistence_ticker(cx);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            cx.notify();
        }
        restored
    }

    pub(crate) fn spawn_gpui_agents_delayed_send_fire(
        &self,
        session_id: TerminalSessionId,
        generation: u64,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                this.fire_gpui_agents_delayed_send(session_id, generation, cx);
            });
        })
        .detach();
    }

    pub(crate) fn spawn_gpui_agents_send_when_stopped_poll(
        &self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_AGENTS_SEND_WHEN_STOPPED_POLL_INTERVAL)
                    .await;
                let keep_watching = this
                    .update(cx, |this, cx| {
                        this.poll_gpui_agents_send_when_stopped(session_id, generation, cx)
                    })
                    .unwrap_or(false);
                if !keep_watching {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn gpui_agents_send_when_stopped_scope_is_working(
        &self,
        session_id: TerminalSessionId,
        scope: &GpuiAgentsSendWhenStoppedScope,
    ) -> Option<bool> {
        match scope {
            GpuiAgentsSendWhenStoppedScope::Session => {
                self.agents_workspace.session(session_id).map(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Running
                        && session.activity == AgentTerminalActivity::Working
                })
            }
            GpuiAgentsSendWhenStoppedScope::Project(project_id) => {
                let target_belongs_to_project =
                    self.local_workspace_session_mappings
                        .iter()
                        .any(|(key, mapped_session_id)| {
                            *mapped_session_id == session_id && key.project_id == *project_id
                        });
                if !target_belongs_to_project {
                    return None;
                }
                let mut found_project_session = false;
                for (key, mapped_session_id) in &self.local_workspace_session_mappings {
                    if key.project_id != *project_id {
                        continue;
                    }
                    let Some(session) = self.agents_workspace.session(*mapped_session_id) else {
                        continue;
                    };
                    found_project_session = true;
                    if session.presentation_state == TerminalSessionPresentationState::Running
                        && session.activity == AgentTerminalActivity::Working
                    {
                        return Some(true);
                    }
                }
                found_project_session.then_some(false)
            }
        }
    }

    pub(crate) fn ensure_gpui_agents_delayed_send_countdown_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_delayed_send_countdown_ticker_active
            || self.agents_delayed_send_timers.is_empty()
        {
            return;
        }
        self.agents_delayed_send_countdown_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_ticking = this
                    .update(cx, |this, cx| {
                        if this.agents_delayed_send_timers.is_empty() {
                            this.agents_delayed_send_countdown_ticker_active = false;
                            return false;
                        }
                        this.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn ensure_gpui_agents_delayed_send_persistence_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_delayed_send_persistence_ticker_active
            || (self.agents_delayed_send_timers.is_empty()
                && self.agents_send_when_stopped_watchers.is_empty())
        {
            return;
        }
        self.agents_delayed_send_persistence_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(COMMAND_PANE_DELAYED_SEND_PERSIST_INTERVAL)
                    .await;
                let keep_running = this
                    .update(cx, |this, _cx| {
                        if this.agents_delayed_send_timers.is_empty()
                            && this.agents_send_when_stopped_watchers.is_empty()
                        {
                            this.agents_delayed_send_persistence_ticker_active = false;
                            false
                        } else {
                            this.persist_shell_layout_state();
                            true
                        }
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn poll_gpui_agents_send_when_stopped(
        &mut self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(watcher) = self
            .agents_send_when_stopped_watchers
            .get(&session_id)
            .cloned()
        else {
            return false;
        };
        if watcher.generation != generation {
            return false;
        }
        let Some(presentation_state) = self
            .agents_workspace
            .session(session_id)
            .map(|session| session.presentation_state)
        else {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        };
        if presentation_state != TerminalSessionPresentationState::Running {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        }
        let Some(is_working) =
            self.gpui_agents_send_when_stopped_scope_is_working(session_id, &watcher.scope)
        else {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        };
        if is_working {
            if watcher.non_working_since.is_some() {
                if let Some(current) = self.agents_send_when_stopped_watchers.get_mut(&session_id) {
                    current.non_working_since = None;
                }
                self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                cx.notify();
            }
            return true;
        }

        let now = Instant::now();
        let Some(non_working_since) = watcher.non_working_since else {
            if let Some(current) = self.agents_send_when_stopped_watchers.get_mut(&session_id) {
                current.non_working_since = Some(now);
            }
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            cx.notify();
            return true;
        };
        if now.saturating_duration_since(non_working_since)
            < GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION
        {
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            return true;
        }

        self.agents_send_when_stopped_watchers.remove(&session_id);
        let sent = self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_some_and(|target| {
                self.send_return_key_to_gpui_agents_delayed_send_target(target, cx)
            });
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        if !sent {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send skipped",
                "The terminal was no longer available.",
                cx,
            );
        }
        false
    }

    pub(crate) fn fire_gpui_agents_delayed_send(
        &mut self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(timer) = self.agents_delayed_send_timers.get(&session_id).copied() else {
            return false;
        };
        if timer.generation != generation {
            return false;
        }
        self.agents_delayed_send_timers.remove(&session_id);
        let session_still_exists = self.agents_workspace.session(session_id).is_some();
        let sent = self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_some_and(|target| {
                self.send_return_key_to_gpui_agents_delayed_send_target(target, cx)
            });
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        if !sent && session_still_exists {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send skipped",
                "The terminal was no longer available.",
                cx,
            );
        }
        sent
    }

    pub(crate) fn gpui_agents_delayed_send_mount_target(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiAgentsDelayedSendTarget> {
        let session = self.agents_workspace.session(session_id)?;
        if session.presentation_state != TerminalSessionPresentationState::Running {
            return None;
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(session_id)?;

        if self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .is_some_and(|record| record.runtime_session_id == runtime_session_id)
        {
            return Some(GpuiAgentsDelayedSendTarget::GpuiEngine {
                session_id,
                runtime_session_id,
            });
        }

        None
    }

    pub(crate) fn send_return_key_to_gpui_agents_delayed_send_target(
        &mut self,
        target: GpuiAgentsDelayedSendTarget,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match target {
            GpuiAgentsDelayedSendTarget::GpuiEngine {
                session_id,
                runtime_session_id,
            } => {
                let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) else {
                    return false;
                };
                if record.runtime_session_id != runtime_session_id {
                    return false;
                }
                let view = record.view.clone();
                view.update(cx, |view, cx| view.send_return_key(cx));
                true
            }
        }
    }
}
