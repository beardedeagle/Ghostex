//! `/api/agentAccounts` operations behind `ghostex accounts` (`roster`) and
//! `ghostex agents switch-account` (`switchSession`).

use super::{endpoint, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User: "we need to have the ability for the orchestrator to switch the account of another thread and to full reload it in case it gets stuck for any reason (should see all available accounts and usages etc) through ghostex cli". `roster` is the Settings > Accounts `list` with each account's sessions, its default flag and whether a usage window is at its limit; `switchSession` finds an account by number, email or id and runs the same `select` as the chat's Switch Account (an in-place restart that resumes the same conversation); `ghostex agents reload` is `reload` in session_chat_agent_restart.rs. All of them work on any session, like `ghostex agents send`, not only an orchestrator's threads.
/// SEE-ALSO: server/src/ghostex_cli/accounts.rs, server/src/ghostex_cli/agents/lifecycle.rs, server/src/coordinators/role.md ("Threads that cannot start or are stuck").
pub(super) fn roster(
    repository: &DomainRepository<'_>,
    snapshot: &Snapshot,
    value: &mut Value,
) -> Result<(), DomainStateError> {
    let mut sessions: HashMap<String, Vec<Value>> = HashMap::new();
    for session in repository.list_sessions(None)? {
        let Some(id) = session
            .pointer("/runtimeSettings/accountId")
            .and_then(Value::as_str)
        else {
            continue;
        };
        sessions.entry(id.to_string()).or_default().push(json!({
            "globalRef": session["globalRef"],
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "title": session["title"],
            "lifecycleState": session["lifecycleState"],
        }));
    }
    let defaults = value["defaultAccounts"].clone();
    let now = Utc::now();
    let Some(rows) = value["accounts"].as_array_mut() else {
        return Ok(());
    };
    for row in rows.iter_mut() {
        let id = row["id"].as_str().unwrap_or_default().to_string();
        let found = snapshot
            .accounts
            .iter()
            .find(|account| endpoint::account_id(account) == id);
        let limited = found
            .map(|account| limited_windows(account, now))
            .unwrap_or_default();
        let status = row["status"].as_str().unwrap_or_default().to_string();
        row["number"] = row["selector"].clone();
        row["default"] = json!(defaults[row["provider"].as_str().unwrap_or_default()] == json!(id));
        row["state"] = json!(match status.as_str() {
            "ready" if !limited.is_empty() => "limitReached",
            "ready" => "ready",
            "loginRequired" | "identityChanged" | "unavailable" => "needsSignIn",
            other => other,
        });
        row["limitReached"] = json!(limited);
        row["sessions"] = json!(sessions.remove(&id).unwrap_or_default());
    }
    rows.sort_by_key(|row| {
        (
            row["provider"].as_str().unwrap_or_default().to_string(),
            row["number"]
                .as_str()
                .and_then(|number| number.parse::<u64>().ok())
                .unwrap_or(u64::MAX),
        )
    });
    Ok(())
}

/// The labels of the account-wide windows (five-hour, weekly, Codex's) at 100% with their reset
/// still ahead. Model-scoped windows (Fable weekly) and Extra usage only limit part of the
/// account, so they stay in `usage` without marking the whole account as limited.
fn limited_windows(account: &DiscoveredAccount, now: DateTime<Utc>) -> Vec<String> {
    account
        .usage
        .iter()
        .filter(|window| window.model.is_none() && window.id != "spend")
        .filter(|window| {
            window.used_percent >= 100.
                && window
                    .resets_at
                    .as_deref()
                    .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                    .is_none_or(|at| at > now)
        })
        .map(|window| window.label.clone())
        .collect()
}

/// How long `switchSession` waits for the in-place restart to report success or failure.
const SWITCH_WAIT: Duration = Duration::from_secs(60);

/// Moves a session to the account named by `account` (cswap/xswap number, email or id) through
/// `select`, then waits for the restart to finish so the caller learns whether it worked.
pub(super) fn switch_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let wanted = params
        .get("account")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() < 512)
        .ok_or_else(|| DomainStateError::bad_request("account is required."))?;
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let session = endpoint::get_session(&repository, params)?;
    let project = repository
        .get_project(session["projectId"].as_str().unwrap_or_default())?
        .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
    let provider = launch::provider(&project, &session).ok_or_else(|| {
        DomainStateError::bad_request(
            "This session is not a Claude or Codex session, so it has no account to switch.",
        )
    })?;
    let discovered = |account: &SavedAccount| {
        snapshot
            .accounts
            .iter()
            .find(|found| endpoint::account_id(found) == account.id)
    };
    let email = |account: &SavedAccount| {
        discovered(account)
            .map(|found| found.email.clone())
            .filter(|email| !email.is_empty())
            .unwrap_or_else(|| {
                account
                    .identity
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
    };
    let ready = |account: &SavedAccount| {
        snapshot.accounts.iter().any(|found| {
            found.provider == provider
                && found.selector == account.selector
                && found.identity == account.identity
                && found.status == "ready"
        })
    };
    let describe = |account: &SavedAccount| {
        let email = email(account);
        let mut text = format!("{} ({})", account.selector, email);
        if let Some(found) = discovered(account) {
            let limited = limited_windows(found, Utc::now());
            if !limited.is_empty() {
                text.push_str(&format!(", {} reached", limited.join(" and ")));
            }
        }
        text
    };
    let accounts: Vec<&SavedAccount> = registry
        .accounts
        .iter()
        .filter(|account| account.provider == provider)
        .collect();
    let working = || {
        let rows: Vec<String> = accounts
            .iter()
            .filter(|account| ready(account))
            .map(|account| describe(account))
            .collect();
        if rows.is_empty() {
            format!(
                "No saved {} account is signed in on this computer; the user has to sign in to one (Settings > Accounts).",
                provider.id()
            )
        } else {
            format!("Accounts that would work: {}.", rows.join("; "))
        }
    };
    let target = accounts
        .iter()
        .find(|account| account.id == wanted)
        .or_else(|| accounts.iter().find(|account| account.selector == wanted))
        .or_else(|| {
            accounts
                .iter()
                .find(|account| email(account).eq_ignore_ascii_case(wanted))
        })
        .copied()
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "No saved {} account matches \"{wanted}\". {}",
                provider.id(),
                working()
            ))
        })?;
    if !ready(target) {
        return Err(DomainStateError::bad_request(format!(
            "Account {} needs to be signed in again on this computer before a session can use it. {}",
            describe(target),
            working()
        )));
    }
    let previous = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let account = json!({
        "id": target.id,
        "provider": provider,
        "number": target.selector,
        "email": email(target),
        "name": target.name,
    });
    let failed_switch_to_target = session
        .pointer("/runtimeSettings/accountSwitch/phase")
        .and_then(Value::as_str)
        == Some("failed")
        && session
            .pointer("/runtimeSettings/accountSwitch/toAccountId")
            .and_then(Value::as_str)
            == Some(target.id.as_str());
    if previous.as_deref() == Some(target.id.as_str()) && !failed_switch_to_target {
        return Ok(
            json!({ "status": "unchanged", "account": account, "session": summary(&session) }),
        );
    }
    let mut select = params.clone();
    select.insert("operation".into(), json!("select"));
    select.insert("accountId".into(), json!(target.id));
    endpoint::dispatch(state, &select)?;
    // `select` restarts a live agent in place on its own thread; a sleeping session only changes
    // its row and starts on the new account at its next wake.
    let started = Instant::now();
    let row = loop {
        let row = endpoint::get_session(&repository, params)?;
        let phase = row
            .pointer("/runtimeSettings/accountSwitch/phase")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(phase, "switching" | "resuming")
            || row
                .pointer("/runtimeSettings/accountSwitchAttempt")
                .is_none()
            || started.elapsed() >= SWITCH_WAIT
        {
            break row;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let phase = row
        .pointer("/runtimeSettings/accountSwitch/phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let still_restarting = row
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .is_some();
    let status = if phase == "failed" {
        "failed"
    } else if still_restarting {
        "switching"
    } else if row["lifecycleState"].as_str() == Some("running") {
        "switched"
    } else {
        "switchedWhileAsleep"
    };
    let mut result = json!({
        "status": status,
        "account": account,
        "previousAccountId": previous,
        "session": summary(&row),
    });
    if phase == "failed" {
        result["error"] = row
            .pointer("/runtimeSettings/accountSwitch/reason")
            .cloned()
            .unwrap_or(Value::Null);
    }
    Ok(result)
}

fn summary(session: &Value) -> Value {
    json!({
        "globalRef": session["globalRef"],
        "title": session["title"],
        "lifecycleState": session["lifecycleState"],
        "accountId": session.pointer("/runtimeSettings/accountId"),
        "agentSessionId": session.pointer("/runtimeSettings/agentSessionId"),
    })
}
