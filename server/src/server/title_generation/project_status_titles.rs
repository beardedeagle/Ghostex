use super::*;

pub(crate) fn schedule_agent_title_metadata_check(
    state: AppState,
    project_id: String,
    session_id: String,
) {
    /*
    CDXC:SessionTitles 2026-06-21-15:35:
    Agent CLI renames are accepted asynchronously after Ghostex submits `/rename`. Match TypeScript gxserver's three-second trailing metadata check so Rust promotes the agent's own session-metadata title (Codex `thread_name`, Claude `custom-title`) and broadcasts a presentation delta after the CLI writes it.
    */
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS,
        ))
        .await;
        let Ok(db) =
            open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
        else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(changed) = reconcile_agent_metadata_title_for_session(
            &repository,
            &project_id,
            &session_id,
            &state.paths.home_dir,
            "metadata-mismatch",
        ) else {
            return;
        };
        if changed {
            let _ = schedule_presentation_session_delta(
                &state,
                &db,
                &repository,
                &project_id,
                &session_id,
            );
        }
        if let Ok(Some(session)) = repository.get_session(&project_id, &session_id) {
            if let Some(target) = fork_initial_rename_retry_target(&session) {
                schedule_fork_initial_rename(state.clone(), target);
            }
        }
    });
}
