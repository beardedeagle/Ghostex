use super::*;

/// How often a pending user rename re-reads the agent's own session metadata.
const SESSION_RENAME_CONFIRM_POLL_MS: u64 = 250;
/// Every this many polls the full reconcile runs even when the cheap read found no match, which
/// also covers the providers whose rename is only found through the reconcile's own fallbacks.
const SESSION_RENAME_FULL_RECONCILE_EVERY: u32 = 4;

/// CDXC:SessionTitles 2026-10-12 WHY:
/// A user rename used to wait for the one trailing metadata check three seconds after the request (`schedule_agent_title_metadata_check`), so the confirmed name reached every client 3–4 seconds late even though Claude writes its `custom-title` record within a few hundred milliseconds. The request now polls the agent's metadata until the name is confirmed, publishing the delta the moment it is, and gives up after the 8-second window `pending_session_rename_title` shows the name for.
/// SEE-ALSO: server/src/presentation/session_attributes.rs `pending_session_rename_title`, server/src/agents/title_reconcile.rs `fail_pending_session_rename`.
pub(crate) fn schedule_session_rename_confirmation(
    state: AppState,
    project_id: String,
    session_id: String,
) {
    tokio::spawn(async move {
        let Ok(db) =
            open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
        else {
            return;
        };
        let lifecycle = crate::agents::LifecycleParams {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
        };
        let mut requested_at: Option<String> = None;
        let mut tick: u32 = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(SESSION_RENAME_CONFIRM_POLL_MS)).await;
            tick += 1;
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let Ok(Some(session)) = repository.get_session(&project_id, &session_id) else {
                return;
            };
            let current_requested_at =
                read_runtime_text(&session, "pendingAgentTitleRequestRequestedAt");
            let requested = match (&requested_at, current_requested_at) {
                (None, Some(current)) => requested_at.insert(current).clone(),
                (Some(known), Some(current)) if *known == current => current,
                // A newer rename has its own watcher.
                _ => return,
            };
            if read_runtime_text(&session, "pendingAgentTitleRequestStatus").as_deref()
                != Some("pending")
            {
                return;
            }
            let pending_title = read_runtime_text(&session, "pendingAgentTitleRequestTitle");
            let metadata_matches = pending_title.as_deref().is_some_and(|pending| {
                crate::agents::read_agent_metadata_title(&state.paths.home_dir, &session)
                    .is_some_and(|metadata| crate::agents::titles_match(pending, metadata.title()))
            });
            if metadata_matches || tick.is_multiple_of(SESSION_RENAME_FULL_RECONCILE_EVERY) {
                let changed = reconcile_agent_metadata_title_for_session(
                    &repository,
                    &project_id,
                    &session_id,
                    &state.paths.home_dir,
                    "pending",
                )
                .unwrap_or(false);
                let confirmed = repository
                    .get_session(&project_id, &session_id)
                    .ok()
                    .flatten()
                    .is_some_and(|session| {
                        read_runtime_text(&session, "pendingAgentTitleRequestStatus").as_deref()
                            == Some("confirmed")
                    });
                if changed || confirmed {
                    let _ = schedule_presentation_session_delta(
                        &state,
                        &db,
                        &repository,
                        &project_id,
                        &session_id,
                    );
                }
                if confirmed {
                    return;
                }
            }
            let deadline = crate::presentation::parse_iso_ms(&requested)
                .map(|requested| requested + crate::presentation::SESSION_RENAME_CONFIRM_WINDOW_MS)
                .unwrap_or(0);
            if crate::presentation::now_ms() >= deadline {
                if crate::agents::fail_pending_session_rename(&repository, &lifecycle, &requested)
                    .unwrap_or(false)
                {
                    let _ = schedule_presentation_session_delta(
                        &state,
                        &db,
                        &repository,
                        &project_id,
                        &session_id,
                    );
                }
                return;
            }
        }
    });
}
