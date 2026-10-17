//! The quiet fallback name for a Codex session that Codex did not name itself.

use super::*;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

/// How long Codex has to name the thread itself after a prompt before Ghostex names it.
const CODEX_TITLE_FALLBACK_DELAY: Duration = Duration::from_secs(60);

/// Sessions with a fallback waiting or done in this gxserver run, so every later hook does not
/// start another one.
static SCHEDULED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// CDXC:SessionTitles 2026-10-17 WHY:
/// The fallback is not the first-prompt job: that job claims a `running` status, which shows "Generating title…" and holds the composer, and it names the thread by typing `/rename` into the input line Codex shares with the user, clearing any draft there. This runs in the background a minute after a prompt and saves the name on the session only, so typing is never blocked and nothing reaches the terminal. Codex stays the first choice: a thread name Codex writes (before or after this) is adopted by the metadata sync, which replaces this name. The name is saved only while the session still has Ghostex's placeholder (or a generic or folder title) and no rename is in flight, checked again after the title CLI returns, so a manual rename, Generate Name or `/rename` is never overwritten. The title comes from `generate_first_prompt_session_title`, which runs on the session's account and passes the prompt on stdin.
/// SEE-ALSO: `first_prompt_auto_title_strategy` in first_prompt_decision.rs (CDXC:SessionTitles 2026-10-17 DECISION).
pub(crate) fn schedule_codex_title_fallback(
    state: &AppState,
    params: &Map<String, Value>,
    result: &Value,
) {
    if !crate::agents::is_explicit_user_prompt_submit_event(params)
        && !crate::agents::is_turn_complete_hook_event(params)
    {
        return;
    }
    let Some(session) = result.get("session") else {
        return;
    };
    if normalize_agent_name(first_prompt_agent_name(session).as_deref()).as_deref() != Some("codex")
        || !codex_title_fallback_eligible(session)
    {
        return;
    }
    let (Some(project_id), Some(session_id)) = (
        read_session_text(session, "projectId"),
        read_session_text(session, "sessionId"),
    ) else {
        return;
    };
    let key = format!("{project_id}\u{1f}{session_id}");
    if !SCHEDULED
        .lock()
        .map(|mut scheduled| scheduled.insert(key.clone()))
        .unwrap_or(false)
    {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(CODEX_TITLE_FALLBACK_DELAY).await;
        let retry = run_codex_title_fallback(&state, &project_id, &session_id).await;
        if retry {
            // Nothing to name it from yet: the next prompt schedules it again.
            if let Ok(mut scheduled) = SCHEDULED.lock() {
                scheduled.remove(&key);
            }
        }
    });
}

/// Whether the session still carries a name Ghostex may replace.
fn codex_title_fallback_eligible(session: &Value) -> bool {
    if read_runtime_text(session, "pendingAgentTitleRequestTitle").is_some()
        || read_runtime_text(session, "gxserverManualTitleGenerationRequestedAt").is_some()
        || read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus").as_deref()
            == Some("running")
    {
        return false;
    }
    read_runtime_text(session, "titleSource").as_deref() == Some("placeholder")
        || is_terminal_auto_working_directory_title(session)
        || is_generic_agent_session_title(
            Some("codex"),
            read_session_text(session, "title").as_deref(),
        )
}

/// Names the session if it still needs a name; answers whether to try again on a later prompt.
async fn run_codex_title_fallback(state: &AppState, project_id: &str, session_id: &str) -> bool {
    let (session, project_path) = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return false;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let (Ok(Some(session)), Ok(Some(project))) = (
            repository.get_session(project_id, session_id),
            repository.get_project(project_id),
        ) else {
            return false;
        };
        if !codex_title_fallback_eligible(&session)
            || crate::coordinators::keeps_its_given_title(&db, &session)
        {
            return false;
        }
        let project_path = read_session_text(&project, "path")
            .unwrap_or_else(|| state.paths.home_dir.to_string_lossy().to_string());
        (session, project_path)
    };
    let Some(source) = session_history_title_source(&session) else {
        return true;
    };
    let title = match generate_first_prompt_session_title(
        state,
        Some(&project_path),
        &source,
        GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MAX_LENGTH,
        &session,
    )
    .await
    {
        Ok(title) => title,
        Err(error) => {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "codexTitleFallbackFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error),
                details: Some(json!({ "projectId": project_id, "sessionId": session_id })),
            });
            return false;
        }
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(latest)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    if !codex_title_fallback_eligible(&latest)
        || crate::coordinators::keeps_its_given_title(&db, &latest)
    {
        return false;
    }
    let mut runtime_settings = latest
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert("titleSource".to_string(), json!("generated"));
    runtime_settings.insert(
        "gxserverCodexTitleFallbackAppliedAt".to_string(),
        json!(now_iso()),
    );
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("title".to_string(), json!(title));
    if repository.update_session(&update).is_ok() {
        schedule_delta_for_ids(state, project_id, session_id);
    }
    false
}
