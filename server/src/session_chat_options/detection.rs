use super::*;

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Scans the tail window bottom-up; the bottom-most match wins. Returns `None`
/// when the window carries no statusline this parser understands. A detection
/// carries the agent's whole footer (statusline down to the bottom of the
/// screen) as `terminal_status_line`.
pub fn detect_session_chat_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    let scanned_lines = scan_window(text);
    let empryo_head = if agent == SessionChatOptionAgent::Empryo {
        crate::session_chat_composer::empryo_input_head(&scanned_lines)
    } else {
        None
    };
    let mut found = SessionChatDetectedSelection::default();
    // Index of the topmost line that supplied any value; the footer capture
    // starts there.
    let mut topmost_match: Option<usize> = None;
    for (index, scanned) in scanned_lines.iter().enumerate().rev() {
        match agent {
            SessionChatOptionAgent::Antigravity => {
                if let Some(selection) = match_antigravity_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Cursor => {
                if let Some(selection) = match_cursor_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Pi => {
                if let Some(selection) = match_pi_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Omp => {
                if let Some(selection) = match_omp_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            SessionChatOptionAgent::Empryo => {
                if let Some(selection) = match_empryo_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                // The border line is the whole reading; the rows below it are the input box.
                if empryo_head == Some(index) {
                    if let Some(selection) = match_empryo_input_head(scanned) {
                        found = selection;
                        break;
                    }
                }
                continue;
            }
            SessionChatOptionAgent::Hermes => {
                if let Some(selection) = match_hermes_statusline(scanned) {
                    found = selection;
                    topmost_match = Some(index);
                    break;
                }
                continue;
            }
            _ => {}
        }
        // Grok draws its statusline on the composer box's bottom border.
        let unboxed;
        let line = if agent == SessionChatOptionAgent::Grok {
            unboxed = strip_box_drawing(scanned);
            &unboxed
        } else {
            scanned
        };
        if is_divider_line(line) {
            continue;
        }
        let (line, codex_plan_mode) = if agent == SessionChatOptionAgent::Codex {
            strip_codex_plan_mode_marker(line)
        } else {
            (line.as_str(), false)
        };
        let segments = line_segments(line);
        let matched_before = (
            found.model.is_some(),
            found.effort.is_some(),
            found.mode.is_some(),
        );
        for segment in segments.iter() {
            match agent {
                SessionChatOptionAgent::Claude => {
                    if found.model.is_none() {
                        found.model = match_claude_model(segment);
                    }
                    if found.effort.is_none() {
                        found.effort = match_claude_effort(segment);
                    }
                    if found.mode.is_none() {
                        found.mode = match_claude_mode(segment);
                    }
                }
                SessionChatOptionAgent::Codex => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(mut selection) = match_codex_segment(segment) {
                            if codex_plan_mode {
                                selection.mode = Some(codex_plan_mode_choice());
                            }
                            found = selection;
                        }
                    }
                }
                SessionChatOptionAgent::Antigravity => {
                    unreachable!("Antigravity is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Cursor => {
                    unreachable!("Cursor is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Grok => {
                    if found.model.is_none() && found.effort.is_none() {
                        if let Some(selection) = match_grok_segment(segment) {
                            found = selection;
                        }
                    }
                }
                SessionChatOptionAgent::Hermes => {
                    unreachable!("Hermes is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Omp => {
                    unreachable!("Omp is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Empryo => {
                    unreachable!("Empryo is parsed as a complete statusline")
                }
                SessionChatOptionAgent::Pi => unreachable!("Pi is parsed as a complete statusline"),
                // ZCode has no segment grammar: it is an option agent only so
                // the notice classifier accepts it (see the enum's CDXC).
                SessionChatOptionAgent::Zcode => {}
            }
        }
        if (
            found.model.is_some(),
            found.effort.is_some(),
            found.mode.is_some(),
        ) != matched_before
        {
            topmost_match = Some(index);
        }
        if found.model.is_some() && found.effort.is_some() {
            break;
        }
    }
    if agent == SessionChatOptionAgent::Claude
        && claude_ultracode_on_lines(&scanned_lines) == Some(true)
    {
        found.effort = Some(SessionChatDetectedChoice {
            value: "ultracode".to_string(),
            label: "ultracode".to_string(),
            source: SessionChatOptionEvidence::Terminal,
        });
    }
    if found.model.is_none() && found.effort.is_none() && found.mode.is_none() {
        return None;
    }
    if let Some(top) = topmost_match {
        let footer = scanned_lines[top..]
            .iter()
            .map(|line| {
                // Grok draws its statusline on the composer's border chrome.
                if agent == SessionChatOptionAgent::Grok {
                    strip_box_drawing(line).trim().to_string()
                } else {
                    line.trim().to_string()
                }
            })
            .filter(|line| !line.is_empty() && !is_divider_line(line))
            .collect::<Vec<_>>()
            .join("\n");
        if !footer.is_empty() {
            found.terminal_status_line = Some(footer);
        }
    }
    Some(found)
}

/// CDXC:AgentProviders 2026-09-29 WHY:
/// Claude Code 2.1.284 made Ultracode a switch beside the effort level ("high · ultracode"), and
/// neither its statusline JSON nor a custom status line names it: the only live sign is the
/// "ultracode" label on the top border of Claude's input box. Chat keeps Ultracode as the level
/// after Max, so a session with the switch on reads as effort `ultracode`, and the model picker
/// uses the same reading to decide whether a pick is already applied.
/// SEE-ALSO: server/src/session_chat_claude_effort_slider.rs `drive_claude_effort_slider`.
pub(crate) fn claude_ultracode_on(screen: &str) -> Option<bool> {
    claude_ultracode_on_lines(&scan_window(screen))
}

/// `lines` oldest first, as `scan_window` returns them. `None` when no input box is on screen.
fn claude_ultracode_on_lines(lines: &[String]) -> Option<bool> {
    let bottom = lines.iter().rposition(|line| is_divider_line(line))?;
    let top = lines[..bottom]
        .iter()
        .rposition(|line| is_divider_line(line))?;
    if !lines[top + 1].trim_start().starts_with('\u{276f}') {
        return None;
    }
    Some(
        lines[top]
            .split(|ch: char| ch == '\u{2500}' || ch.is_whitespace())
            .any(|word| word == "ultracode"),
    )
}

/// Agents whose questions exist only on their screen: no hook announces them, so the prompt
/// detectors in `detect_session_chat_terminal_state` are the only reading of them.
pub(crate) fn session_chat_questions_only_on_screen(agent: Option<&str>) -> bool {
    matches!(
        agent.map(str::trim),
        Some("cursor" | "cursor-agent" | "freebuff" | "empryo")
    )
}

/// Full detection for one session: resolve structured transcript metadata,
/// then let any current terminal statusline value win per option. `None` means
/// neither agent-owned source proved a value.
///
/// CDXC:AgentScreenDetection 2026-08-19: the same capture is classified
/// for terminal-state notices, so both readings ride one process spawn.
pub fn detect_session_chat_terminal_state(
    repository: &DomainRepository<'_>,
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
) -> SessionChatTerminalDetection {
    if agent_id == Some("opencode") {
        return repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .as_ref()
            .map(crate::session_chat_opencode::detect)
            .unwrap_or_default();
    }
    /*
    CDXC:SessionChat 2026-08-26:
    Two independent reasons to spend a capture on this session now. The
    statusline grammar covers three agents; the composer signature table covers
    nine, so an agent with only the latter (cursor, copilot, opencode, gemini,
    omp) reaches the funnel through this second door and gets every reading the
    capture can support — which for it is composer readiness alone, since the
    notice, activity and fleet classifiers are all keyed on the option agent.
    */
    let agent = session_chat_option_agent(agent_id);
    if agent.is_none()
        && !crate::session_chat_composer::has_session_chat_composer_signature(agent_id)
    {
        return SessionChatTerminalDetection::default();
    }
    let (transcript, statusline, claude_session_id, claude_session_path) =
        read_session_chat_stored_selections(
            repository,
            hook_state_directory,
            project_id,
            session_id,
            agent,
        );
    // CDXC:SessionChat 2026-09-03: disk, not screen, so it is read
    // whether or not the capture below succeeds.
    let tasks = crate::session_chat_agent_tasks::read_session_chat_agent_tasks(
        claude_session_id.as_deref(),
        claude_session_path.as_deref(),
    );
    let mut diff_panel_screen = None;
    // CDXC:AgentScreenDetection 2026-09-26 WHY:
    // Claude paints headings and bold-only title lines in bold and drops their Markdown, so only the VT capture can tell a title from a one-line paragraph. Claude is read as VT once and every detector below gets the plain text it always had; the streamed message alone also reads the styling.
    let mut styled_screen = None;
    let capture = if agent == Some(SessionChatOptionAgent::Claude) {
        crate::zmx::read_zmx_session_history_capture_vt(repository, project_id, session_id).map(
            |mut capture| {
                let plain = crate::session_chat_screen_styles::plain_screen_text(&capture.text);
                styled_screen = Some(std::mem::replace(&mut capture.text, plain));
                capture
            },
        )
    } else {
        crate::zmx::read_zmx_session_history_capture(repository, project_id, session_id)
    }
    .ok()
    .map(|mut capture| {
        if !capture.truncated {
            crate::session_chat_app_command::refresh_local_command_output(
                project_id,
                session_id,
                &capture.text,
            );
        }
        // CDXC:SessionChatTerminalActivity 2026-09-06 WHY:
        // The close helper rechecks the diff header, so passing the stripped conversation made auto-close reject the pane we had just detected.
        if agent == Some(SessionChatOptionAgent::Claude)
            && !capture.truncated
            && crate::session_chat_diff_panel::claude_diff_panel_on_screen(&capture.text)
        {
            diff_panel_screen = Some(capture.text.clone());
        }
        // One cut for every detector below (see session_chat_screen_pane.rs).
        // CDXC:AgentScreenDetection 2026-09-05 WHY:
        // Agent dialogs align descriptions and setting values in columns; the diff-pane heuristic mistook those columns for a side pane and deleted them.
        if agent == Some(SessionChatOptionAgent::Claude)
            && crate::session_chat_claude_dialog::detect_claude_dialog(&capture.text).is_none()
        {
            capture.text = crate::session_chat_screen_pane::strip_side_pane(&capture.text);
        }
        capture
    });
    // A capped capture lost its tail, so the live screen is not in it.
    let screen = capture.as_ref().filter(|capture| !capture.truncated);
    let terminal = agent
        .zip(screen)
        .and_then(|(agent, capture)| detect_session_chat_selection(agent, &capture.text));
    if let Some(capture) = screen {
        if agent == Some(SessionChatOptionAgent::Codex) {
            crate::session_chat_codex_pager::close_codex_transcript_pager_if_unwatched(
                repository,
                project_id,
                session_id,
                &capture.text,
            );
        }
    }
    let mut notice = screen.and_then(|capture| {
        crate::session_chat_notice::classify_session_chat_terminal_notice(agent_id, &capture.text)
    });
    if notice.is_none() {
        notice = screen.and_then(|capture| {
            crate::session_chat_start_failure::detect_agent_start_failure(
                repository,
                project_id,
                session_id,
                &capture.text,
            )
        });
    }
    if notice.is_none() && agent == Some(SessionChatOptionAgent::Empryo) {
        notice = screen
            .zip(
                repository
                    .get_session(project_id, session_id)
                    .ok()
                    .flatten(),
            )
            .and_then(|(capture, session)| {
                crate::session_chat_empryo_blocking::empryo_rate_limit_notice_for_session(
                    repository,
                    &session,
                    &capture.text,
                )
            });
    }
    if let Some(notice) = notice.as_mut() {
        crate::session_chat_codex_lock::enrich_notice(repository, project_id, session_id, notice);
    }
    if let (Some(notice), Some(styled)) = (notice.as_mut(), styled_screen.as_deref()) {
        crate::session_chat_claude_panel::enrich_claude_panel_notice(
            repository, project_id, session_id, notice, styled,
        );
    }
    if let Some(notice) = notice.as_mut().filter(|notice| {
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_TRUST_PROMPT
    }) {
        let remembered = crate::session_chat_trust_memory::session_folders_remembered(
            repository, project_id, session_id,
        );
        let answerable = remembered
            && screen.is_some_and(|capture| {
                crate::session_chat_trust_memory::workspace_trust_accept_steps(
                    agent_id,
                    &capture.text,
                )
                .is_some()
            });
        crate::session_chat_trust_memory::decorate_trust_notice(
            notice, remembered, answerable, project_id, session_id,
        );
    }
    let launch = read_session_chat_launch_selection(repository, project_id, session_id, agent);
    let options = merge_session_chat_option_selections(launch, transcript, statusline, terminal)
        .map(|mut selection| {
            crate::session_chat_hermes_status::restore_hermes_model_id(&mut selection);
            selection
        })
        .map(SessionChatDetectedOptions::new);
    // A usage limit an account switch is hiding must not veto the composer either: after the switch the resumed CLI repaints the previous login's limit, and the continuation dot and the user's sends have to reach the new login.
    let composer = match screen {
        Some(capture) => crate::session_chat_composer::detect_session_chat_composer_readiness(
            agent_id,
            &capture.text,
            notice.as_ref().filter(|notice| {
                !crate::session_chat_notice::account_usage_notice_suppressed(
                    project_id, session_id, notice,
                )
            }),
        ),
        None => crate::session_chat_composer::SessionChatComposerReadiness::default(),
    };
    if let Some(screen_text) = diff_panel_screen.as_deref() {
        crate::session_chat_diff_panel::hide_claude_diff_panel_if_unwatched(
            repository,
            project_id,
            session_id,
            screen_text,
            composer.state == crate::session_chat_composer::SessionChatComposerState::Ready,
        );
    }
    let activity = screen
        .and_then(|capture| {
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity_styled(
                agent_id,
                &capture.text,
                styled_screen.as_deref(),
            )
        })
        .filter(|activity| {
            // Cursor leaves old working rows in scrollback, but its composer
            // also stays available during summarizing. The compaction detector
            // requires the live stop hint before bypassing this idle gate.
            agent != Some(SessionChatOptionAgent::Cursor)
                || composer.state != crate::session_chat_composer::SessionChatComposerState::Ready
                || crate::session_chat_terminal_activity::is_session_chat_compacting_activity(Some(
                    activity,
                ))
        });
    let mut activity = activity;
    // Disk, not screen: the footer says how many background tasks run, the transcript says which.
    if let Some(work) = activity
        .as_mut()
        .and_then(|activity| activity.background.as_mut())
        .filter(|_| agent == Some(SessionChatOptionAgent::Claude))
    {
        if let Some(session) = repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
        {
            work.tasks = crate::session_chat_claude_fleet::read_claude_background_tasks(
                &session, work.count,
            )
            .unwrap_or_default();
        }
    }
    let (fleet, fleet_observed) = if crate::session_chat_fleet_status::has_fleet_reader(agent) {
        match repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .and_then(|session| {
                crate::session_chat_fleet_status::read_fleet(
                    &session,
                    screen.map(|capture| capture.text.as_str()),
                )
                .ok()
            }) {
            Some(fleet) => (fleet, true),
            None => (None, false),
        }
    } else {
        (None, true)
    };
    let prompt = screen.and_then(|capture| {
        crate::session_chat::detect_cursor_question_prompt(agent_id, &capture.text)
            .or_else(|| {
                crate::session_chat_freebuff_question::detect_freebuff_question_prompt(
                    agent_id,
                    &capture.text,
                )
            })
            .or_else(|| {
                crate::session_chat_empryo_question::detect_empryo_question_prompt(
                    agent_id,
                    &capture.text,
                )
            })
    });
    /*
    CDXC:AgentScreenDetection (settled 2026-08-30): probed only counts once
    the answer is settled. A capture that failed IS settled — a sleeping or
    stopped session has nothing to read, and that stays true until it runs. A
    capture that succeeded settles only when some classifier recognized the
    agent on it; a blank screen behind a CLI that is still booting is "not read
    yet", so the model pill keeps its loading skeleton instead of flashing a
    bare "Model" label until the statusline draws.
    */
    let attempted = capture.is_none()
        || options.is_some()
        || notice.is_some()
        || activity.is_some()
        || fleet.is_some()
        || prompt.is_some()
        || composer.state == crate::session_chat_composer::SessionChatComposerState::Ready;
    // Added after `attempted`: the checkout is known before the agent draws anything.
    let options = with_checkout_status(repository, project_id, session_id, agent_id, options);
    SessionChatTerminalDetection {
        options,
        prompt,
        composer,
        notice,
        activity,
        fleet,
        fleet_observed,
        tasks,
        captured: screen.is_some(),
        attempted,
    }
}

/// Adds the checkout's repository and branch for the chat agents whose status line has nothing
/// of their own to show (see CDXC:AgentProviders in session_chat_cursor_status.rs).
fn with_checkout_status(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
    options: Option<SessionChatDetectedOptions>,
) -> Option<SessionChatDetectedOptions> {
    if matches!(
        agent_id.map(str::trim),
        Some("claude" | "codex" | "cursor" | "cursor-agent" | "hermes" | "hermes-agent") | None
    ) {
        return options;
    }
    let Some(status) =
        crate::session_chat_cursor_status::read_checkout_status(repository, project_id, session_id)
    else {
        return options;
    };
    let mut options = options.unwrap_or_else(|| {
        SessionChatDetectedOptions::new(SessionChatDetectedSelection::default())
    });
    options.selection.checkout_status = Some(status);
    Some(options)
}
