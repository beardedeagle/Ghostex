use super::*;
use crate::*;

impl CommandPaneModel {
    pub(crate) fn select_or_create_action_session(
        &mut self,
        command_id: String,
        title: String,
    ) -> CommandPaneActionSessionSelection {
        /*
        CDXC:CommandPane 2026-06-24-23:36:
        GPUI sidebar/titlebar terminal Actions own one live command-pane tab per
        Action. Idle owners rerun in place; active owners are selected without a
        duplicate run. The command id and run id remain process-memory ownership
        only, while restored shell state can reclaim daemon identity separately.
        */
        if let Some((kind, group_id, session_id)) =
            self.find_reusable_action_session(&command_id, &title)
        {
            self.select_session_in_group(group_id, session_id);
            if let Some(session) = self.session_mut(session_id) {
                session.title = title;
                session.action_command_id = Some(command_id);
            }
            self.reveal_dock_for_group(group_id);
            return CommandPaneActionSessionSelection {
                kind,
                group_id,
                session_id,
            };
        }

        self.prune_stale_existing_action_sessions_before_new_run(&command_id);
        let session_id = self.allocate_session_id();
        self.terminal_sessions.push(
            CommandTerminalSession::placeholder(session_id, title)
                .with_action_command_id(command_id),
        );
        let tab = CommandPaneTab { session_id };
        let group_id = self.insert_created_action_tab_for_untargeted_creation(tab, session_id);
        CommandPaneActionSessionSelection {
            kind: CommandPaneActionSessionSelectionKind::Created,
            group_id,
            session_id,
        }
    }

    pub(crate) fn prune_stale_existing_action_sessions_before_new_run(
        &mut self,
        command_id: &str,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-27-06:10:
        Native `runNativeSidebarCommand` closes an existing mapped Action session before creating a replacement when that mapped terminal is no longer running. Match that only for exact same-command sleeping or orphaned GPUI command tabs; keep running non-idle tabs alive, let idle running tabs reuse earlier, and do not prune title-only restored candidates for other command ids.
        */
        let tab_groups = self
            .flat_tab_ids()
            .into_iter()
            .map(|(group_id, session_id)| (session_id, group_id))
            .collect::<HashMap<_, _>>();
        let stale_sessions = self
            .terminal_sessions
            .iter()
            .filter(|session| session.action_command_id.as_deref() == Some(command_id))
            .filter_map(|session| {
                let group_id = tab_groups.get(&session.id).copied();
                (session.is_sleeping || group_id.is_none()).then_some((group_id, session.id))
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for (group_id, session_id) in stale_sessions {
            if let Some(group_id) = group_id {
                changed |= self.close_session(group_id, session_id);
            } else {
                let before_len = self.terminal_sessions.len();
                self.terminal_sessions
                    .retain(|session| session.id != session_id);
                changed |= self.terminal_sessions.len() != before_len;
            }
        }
        if self.terminal_sessions.is_empty() {
            self.reset_empty_layout();
        }
        changed
    }

    pub(crate) fn find_reusable_action_session(
        &self,
        command_id: &str,
        title: &str,
    ) -> Option<(
        CommandPaneActionSessionSelectionKind,
        CommandPaneGroupId,
        CommandSessionId,
    )> {
        /*
        CDXC:CommandPane 2026-06-25-11:18:
        MacOS command Actions reuse one idle command-pane tab per normalized Action title after restore, even when the live command-id map is missing. Keep the exact command-id match first, then allow idle title-owned reuse regardless of stale/missing action id; duplicate Action titles are rejected at save time, and run-start rewrites the live mapping.

        CDXC:CommandPane 2026-08-08:
        Idle sleeping/restored Action tabs are reusable too. Run start wakes the
        selected session before mounted-surface detection, and an unmounted reuse
        goes through the exact existing gxserver attach slot with startup text.
        Exact command-id ownership wins even if the Action is active; that tab is
        selected without launching another run. Title-only restore matching stays
        idle-only so it cannot claim an unrelated working terminal.
        */
        let title_key = gpui_command_action_title_key(title);
        if title_key.is_empty() {
            return None;
        }
        let tabs = self.flat_tab_ids();
        if let Some((group_id, session_id)) =
            tabs.iter().copied().find(|(_group_id, session_id)| {
                self.session(*session_id)
                    .is_some_and(|session| session.action_command_id.as_deref() == Some(command_id))
            })
        {
            let kind = if self
                .session(session_id)
                .is_some_and(command_session_is_reusable_for_action)
            {
                CommandPaneActionSessionSelectionKind::Reused
            } else {
                CommandPaneActionSessionSelectionKind::ReusedActive
            };
            return Some((kind, group_id, session_id));
        }

        tabs.iter()
            .copied()
            .find(|(_group_id, session_id)| {
                self.session(*session_id).is_some_and(|session| {
                    command_session_is_reusable_for_action(session)
                        && gpui_command_action_title_key(&session.title) == title_key
                })
            })
            .map(|(group_id, session_id)| {
                (
                    CommandPaneActionSessionSelectionKind::Reused,
                    group_id,
                    session_id,
                )
            })
    }

    pub(crate) fn close_completed_action_run_tab(
        &mut self,
        completion: &CommandPaneActionRunCompletion,
    ) -> Option<CommandPaneActionRunCompletedTab> {
        if !gpui_command_pane_action_runtime_close_terminal_on_exit(
            completion.close_terminal_on_exit,
        ) {
            return None;
        }
        let completed_tab = completion.completed_tab?;
        let Some(session) = self.session(completed_tab.session_id) else {
            return None;
        };
        if session.action_command_id.as_deref() != Some(completion.command_id.as_str())
            || session.action_run_id.is_some()
            || session.action_status_file_path.is_some()
            || session.activity != CommandTerminalActivity::Idle
        {
            return None;
        }
        self.close_session(completed_tab.group_id, completed_tab.session_id)
            .then_some(completed_tab)
    }

    pub(crate) fn take_action_run_completion_for_exited_session(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> Option<CommandPaneActionRunCompletion> {
        /*
        CDXC:CommandPane 2026-06-25-11:11:
        If a mapped command-pane terminal exits before the status-file poller clears the run, GPUI must still finish sidebar Action feedback like macOS terminal-exit cleanup. Trust only the matching session-state file when it already reached idle; otherwise report the live run as error so button feedback cannot remain running after the command-pane surface is gone. Do not read terminal output, command text, cwd/env, project paths, logs, or shell-state JSON.
        */
        let session = self.session_mut(session_id)?;
        let command_id = session.action_command_id.clone()?;
        let run_id = session.action_run_id.clone()?;
        let close_terminal_on_exit = gpui_command_pane_action_runtime_close_terminal_on_exit(
            session.action_close_terminal_on_exit,
        );
        let play_completion_sound = session.action_play_completion_sound;
        let exit_code = session
            .action_status_file_path
            .as_ref()
            .and_then(|status_file_path| gpui_command_action_status_from_file(status_file_path))
            .filter(|status| {
                status.run_id == run_id && status.status == GpuiCommandActionRunFileStatus::Idle
            })
            .map(|status| status.exit_code)
            .unwrap_or(1);
        session.activity = CommandTerminalActivity::Idle;
        session.action_close_terminal_on_exit = false;
        session.action_run_id = None;
        session.action_status_file_path = None;
        Some(CommandPaneActionRunCompletion {
            close_terminal_on_exit,
            command_id,
            completed_tab: Some(CommandPaneActionRunCompletedTab {
                group_id,
                session_id,
            }),
            exit_code,
            play_completion_sound,
            run_id,
        })
    }

    pub(crate) fn mark_action_session_run_started(
        &mut self,
        session_id: CommandSessionId,
        command_id: String,
        title: String,
        run_id: String,
        status_file_path: PathBuf,
        play_completion_sound: bool,
        close_terminal_on_exit: bool,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-26-06:28:
        Native `setNativeSidebarCommandPaneTitle` run-start parity requires reused/restored Action tabs to carry the current live Action title and command id, clear stale Delayed Send chrome, and show Working without allocating a replacement tab.

        CDXC:CommandPane 2026-06-27-05:59:
        Native command-pane Action ownership has one current `commandId -> session` mapping. Starting a newer same-command run clears only `action_command_id` on older same-command sessions, preserving their run id, status-file path, activity, and title so status refresh can clear their local Working state without sidebar completion feedback.
        */
        let Some(target_index) = self
            .terminal_sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };
        for (index, session) in self.terminal_sessions.iter_mut().enumerate() {
            if index != target_index
                && session.action_command_id.as_deref() == Some(command_id.as_str())
            {
                session.action_command_id = None;
            }
        }
        let session = &mut self.terminal_sessions[target_index];
        session.title = title;
        session.activity = CommandTerminalActivity::Working;
        session.is_sleeping = false;
        session.delayed_send_active = false;
        session.delayed_send_timer_owned = false;
        session.action_command_id = Some(command_id);
        session.action_close_terminal_on_exit =
            gpui_command_pane_action_runtime_close_terminal_on_exit(close_terminal_on_exit);
        session.action_play_completion_sound = play_completion_sound;
        session.action_run_id = Some(run_id);
        session.action_status_file_path = Some(status_file_path);
        true
    }

    pub(crate) fn refresh_action_run_states_from_status_files(
        &mut self,
    ) -> CommandPaneActionRunRefresh {
        /*
        CDXC:CommandPane 2026-06-24-23:36:
        Command Action completion is observed only through the same session-state file env contract the hidden shell wrapper writes. Refreshing this state may change a live tab's safe activity enum and clear its run id. Shell state may retain only the validated bounded Action selector needed for restart reuse; command text, output, paths, env, tokens, run ids, and status-file paths remain runtime-only.

        CDXC:CommandPane 2026-06-24-23:49:
        The refresh result carries only command id, run id, exit code, and the saved per-action sound flag so the app can mirror macOS button success/error feedback and action completion sounds. Do not include command text, cwd, env, terminal output, status-file paths, project names, or renderer payloads in the completion record.

        CDXC:CommandPane 2026-06-26-04:59:
        Command-pane Action completions normalize legacy close-on-exit requests to false at the poller boundary. The status-file poller may still return safe command group/session ids for feedback plumbing, but it must leave the completed command tab alive for reuse and must not persist or report command text, status-file paths, cwd/env, terminal output, or project paths.

        CDXC:CommandPane 2026-06-27-05:07:
        A matching `status=working` file is live Action ownership evidence only: keep the tab Working, retain the in-memory command id/run id/status path for the poller, emit no completion, and ignore unrelated status-file keys. Only a matching idle stamp or exact exit cleanup may clear ownership and post completion feedback, and neither path may infer status from shell titles, output, paths, command text, env, logs, or persisted shell JSON.

        CDXC:CommandPane 2026-06-27-05:59:
        Superseded same-command Action sessions may keep a run id and status-file path after losing `action_command_id`. Their idle status refresh clears only local runtime state; without current command ownership it must not emit command-button completion feedback or expose private status-file fields.
        */
        let mut refresh = CommandPaneActionRunRefresh::default();
        let completed_tabs = self
            .flat_tab_ids()
            .into_iter()
            .map(|(group_id, session_id)| {
                (
                    session_id,
                    CommandPaneActionRunCompletedTab {
                        group_id,
                        session_id,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        for session in &mut self.terminal_sessions {
            let Some(run_id) = session.action_run_id.as_deref() else {
                continue;
            };
            let Some(status_file_path) = session.action_status_file_path.as_ref() else {
                continue;
            };
            let Some(status) = gpui_command_action_status_from_file(status_file_path) else {
                continue;
            };
            if status.run_id != run_id {
                continue;
            }
            match status.status {
                GpuiCommandActionRunFileStatus::Working => {
                    if session.activity != CommandTerminalActivity::Working {
                        session.activity = CommandTerminalActivity::Working;
                        refresh.changed = true;
                    }
                }
                GpuiCommandActionRunFileStatus::Idle => {
                    if session.activity != CommandTerminalActivity::Idle {
                        session.activity = CommandTerminalActivity::Idle;
                        refresh.changed = true;
                    }
                    if let Some(command_id) = session.action_command_id.clone() {
                        refresh.completions.push(CommandPaneActionRunCompletion {
                            close_terminal_on_exit:
                                gpui_command_pane_action_runtime_close_terminal_on_exit(
                                    session.action_close_terminal_on_exit,
                                ),
                            command_id,
                            completed_tab: completed_tabs.get(&session.id).copied(),
                            exit_code: status.exit_code,
                            play_completion_sound: session.action_play_completion_sound,
                            run_id: status.run_id.clone(),
                        });
                    }
                    session.action_close_terminal_on_exit = false;
                    session.action_run_id = None;
                    session.action_status_file_path = None;
                }
            }
        }
        refresh
    }

    pub(crate) fn has_active_action_runs(&self) -> bool {
        self.terminal_sessions
            .iter()
            .any(|session| session.action_run_id.is_some())
    }
}
