//! CDXC:AgentProviders 2026-10-11 WHY:
//! A sleeping session woke on its saved account even when that account's trusted reading already showed it at its limit: G9ow7 woke on slot 3 (weekly 100%) for a chat message, hit "You've hit your weekly limit" 9 seconds later, and only then did recovery move it to slot 2. Waking now moves such a session to the best account with room first, with the ranking and trusted-room rule recovery uses (`recovery::ranked`), so the resumed CLI never starts on a login that cannot answer.
//! A session on its own login keeps it (CDXC:AgentProviders 2026-10-09 in launch.rs), a session whose recovery the user stopped or whose provider rule waits at a limit keeps its account, and with no account that has room the session wakes on its saved account as before. Drafts and sessions without a conversation to resume keep their own launch rebuild path (drafts.rs).
use super::{drafts, endpoint, launch, model::*, recovery, store};
use crate::domain::{DomainRepository, DomainStateError};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

/// Returns the row the wake should launch: `session` itself, or the row rebound to an account with room.
pub(crate) fn rebind_exhausted(
    repository: &DomainRepository<'_>,
    project: &Value,
    session: Value,
) -> Result<Value, DomainStateError> {
    let Some((account, command)) = rebind_target(repository, project, &session)? else {
        return Ok(session);
    };
    let runtime = &session["runtimeSettings"];
    let current = runtime["accountId"].as_str().unwrap_or_default();
    let provider = account.provider;
    let mut runtime = runtime.as_object().cloned().unwrap_or_default();
    launch::assign(&mut runtime, &account, command)?;
    runtime.remove("accountRecovery");
    // The resumed CLI repaints the old login's limit from the transcript, which must not read as the new login's limit (see `endpoint::select`).
    let switched_at = Utc::now();
    runtime.remove("accountSuppressedUsageNotice");
    runtime.insert(
        "accountSuppressedUsageNoticeAt".into(),
        json!(switched_at.to_rfc3339()),
    );
    runtime.insert(
        "accountSwitch".into(),
        json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "provider": provider,
            "source": "automatic",
            "phase": "success",
            "accountReady": true,
            "fromAccountId": current,
            "toAccountId": account.id,
            "updatedAt": switched_at.to_rfc3339(),
        }),
    );
    let mut launch_settings = session["launchSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
        launch_settings.remove(key);
    }
    let updated = repository.update_session(
        json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "runtimeSettings": runtime,
            "launchSettings": launch_settings,
        })
        .as_object()
        .unwrap(),
    )?;
    crate::session_chat_notice::suppress_account_usage_notice(
        session["projectId"].as_str().unwrap_or_default(),
        session["sessionId"].as_str().unwrap_or_default(),
        switched_at,
    );
    Ok(updated)
}

/// CDXC:AgentProviders 2026-10-14 WHY:
/// A wake on a cswap account with no saved login on this computer cannot start the CLI at all (the session G7v5a woke on slot 5 of a loaner Mac, printed "Account-5 has no stored credentials" twice and dropped to the shell with the user's message held). Like a disabled login (CDXC:AgentProviders 2026-10-08 in disabled.rs), waiting never brings it back, so the wake moves such a session to the best account with room even when the provider's rule waits at a limit. Everything else follows the 2026-10-11 rule above: a session on its own login, one whose recovery the user stopped, or one with Continue automatically off keeps its account and shows the sign-in card (session_chat_notice/start_failure.rs) instead.
/// The account a wake of `session` should move to and its launch command, or `None` to keep the saved account.
pub(crate) fn rebind_target(
    repository: &DomainRepository<'_>,
    project: &Value,
    session: &Value,
) -> Result<Option<(SavedAccount, String)>, DomainStateError> {
    let Some(provider) = launch::provider(project, session) else {
        return Ok(None);
    };
    let runtime = &session["runtimeSettings"];
    let Some(current) = runtime["accountId"].as_str() else {
        return Ok(None);
    };
    if launch::session_uses_own_login(provider, session)
        || runtime["accountRecoverySuppressed"].as_bool() == Some(true)
        || drafts::needs_fresh_launch(session)
    {
        return Ok(None);
    }
    let registry = store::read(repository.db)?;
    let policy = launch::effective_policy(&registry, provider, session);
    let snapshot = super::runtime::current_snapshot();
    let reading = snapshot
        .accounts
        .iter()
        .find(|d| endpoint::account_id(d) == current);
    let no_login = reading.is_some_and(|d| d.status == "loginRequired");
    if !policy.enabled || (!no_login && policy.at_limit != LimitAction::Switch) {
        return Ok(None);
    }
    let model = runtime["accountBaseCommand"]
        .as_str()
        .or_else(|| runtime["agentCommand"].as_str())
        .and_then(|command| command_model(command, provider.id()))
        .unwrap_or_default();
    if !no_login && !reading.is_some_and(|d| at_limit(d, &model, Utc::now())) {
        return Ok(None);
    }
    let Some(account) = recovery::ranked(
        &registry,
        &snapshot,
        provider,
        Some(current),
        &model,
        policy.priority,
    )
    .into_iter()
    .next() else {
        return Ok(None);
    };
    // A login cswap cannot launch right now keeps the wake on the saved account, as before this rule.
    let Ok(command) = launch::home().and_then(|home| launch::command(&home, account)) else {
        return Ok(None);
    };
    Ok(Some((account.clone(), command)))
}

/// A reading cswap trusts for switching that shows a window this model uses at 100%, with its reset still ahead.
fn at_limit(account: &DiscoveredAccount, model: &str, now: DateTime<Utc>) -> bool {
    account.status == "ready"
        && account.usage_error.is_none()
        && account
            .usage
            .iter()
            .filter(|w| recovery::relevant(w, model))
            .any(|w| {
                w.used_percent >= 100.
                    && w.resets_at
                        .as_deref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .is_none_or(|t| t > now)
            })
}

/// The model the saved command launches with (`--model`, Codex's `-m`), which decides the usage windows that apply.
fn command_model(command: &str, agent: &str) -> Option<String> {
    let mut offset = 0;
    let mut take_value = false;
    let mut skip_value = false;
    while let Some((start, end, word)) = crate::agents::command_word(command, offset) {
        offset = end;
        if std::mem::take(&mut take_value) {
            return Some(word);
        }
        if std::mem::take(&mut skip_value)
            || !crate::agents::is_option_word(&command[start..end], &word)
        {
            continue;
        }
        if let Some(model) = word.strip_prefix("--model=") {
            return Some(model.to_string());
        }
        if word == "--model" || (agent == "codex" && word == "-m") {
            take_value = true;
            continue;
        }
        skip_value = !word.contains('=') && crate::agents::option_takes_value(agent, &word);
    }
    None
}
