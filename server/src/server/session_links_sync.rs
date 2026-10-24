//! Running session links (crate::session_links): the background pass that finds the links that
//! join each open session by themselves, keeps what is known about every link current, and
//! publishes the rows whose links changed.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: a change in a linked PR tells you through the chips only ("dot-only"): failing checks, a review asked for and a merge show on the chips and in the list, and no notification is sent.
//! CDXC:SessionLinks 2026-10-24 WHY:
//! Merged and closed PRs, closed issues and done or canceled Linear tickets are final, so once read they are never read again; the `gh` budget goes to open work. Only sessions that are not closed are refreshed, like work mode's pass (CDXC:WorkMode 2026-10-23).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use super::{schedule_presentation_session_delta, AppState};
use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::session_git_status::{
    cached_session_git_status, effective_session_git_cwd, gh_cli_is_available,
    PullRequestDisposition,
};
use crate::session_links::{
    forget_session_link_mentions_except, needs_full_scan, parse_github_url,
    presentation_session_links, remember_session_link_mentions, scan_transcript,
    session_link_entries, stored_session_links, write_session_links, SessionLinkKind,
    SessionLinkRef, SessionLinkSource, StoredSessionLink,
};
use crate::storage::open_gxserver_database;
use crate::work_mode::{
    cached_linear_team_keys, cached_work_github_issue_url, cached_work_pull_request,
    cached_work_repo, linear_api_key, linear_key_fingerprint, refresh_linear_issues,
    refresh_linear_team_keys, refresh_work_github_issue_selector, refresh_work_pull_request,
    refresh_work_repo, remember_project_linear_key, work_branch,
};

/// `gh` calls one pass may make (work mode's pass has its own).
const MAX_GH_CALLS_PER_PASS: usize = 12;
/// Requests one pass may send to Linear.
const MAX_LINEAR_REQUESTS_PER_PASS: usize = 3;
/// Transcripts one pass may read in full (a first read); later reads only add what is new.
const MAX_FULL_SCANS_PER_PASS: usize = 4;

/// What each open session last published as `links`, so a pass only sends the rows that changed.
fn published_links() -> &'static Mutex<HashMap<(String, String), String>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ids(session: &Value) -> Option<(String, String)> {
    Some((
        session.get("projectId")?.as_str()?.to_string(),
        session.get("sessionId")?.as_str()?.to_string(),
    ))
}

/// One pass. Blocking; runs on the 60s background worker after work mode's pass, which fills the
/// caches both read.
pub(crate) fn run_session_links_refresh_once(
    state: &Arc<AppState>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<String, &Value> = projects
        .iter()
        .filter_map(|project| Some((project.get("projectId")?.as_str()?.to_string(), project)))
        .collect();
    let mut sessions = Vec::new();
    for project_id in projects_by_id.keys() {
        sessions.extend(repository.list_sessions_excluding_stopped(Some(project_id))?);
    }

    // Each project's Linear key, remembered for the projection like work mode does for its own.
    let mut linear_keys: HashMap<u64, String> = HashMap::new();
    for (project_id, project) in &projects_by_id {
        let workspace_id = crate::workspaces::stored_project_workspace_id(project);
        let key = linear_api_key(&state.paths, Some(project_id), workspace_id);
        let fingerprint = key.as_deref().map(linear_key_fingerprint);
        remember_project_linear_key(project_id, fingerprint);
        if let (Some(fingerprint), Some(key)) = (fingerprint, key) {
            linear_keys.insert(fingerprint, key);
        }
    }
    let mut linear_budget = MAX_LINEAR_REQUESTS_PER_PASS;
    for key in linear_keys.values() {
        if linear_budget > 0 && refresh_linear_team_keys(key) {
            linear_budget -= 1;
        }
    }

    // How many open sessions share each checkout: an earlier branch's PR is only kept for a
    // session that has its checkout to itself (a worktree), or every session in the project's
    // folder would collect it.
    let mut sessions_per_cwd: HashMap<String, usize> = HashMap::new();
    for session in &sessions {
        let project = session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|id| projects_by_id.get(id));
        if let Some(cwd) = effective_session_git_cwd(session, project.copied()) {
            *sessions_per_cwd.entry(cwd).or_default() += 1;
        }
    }

    // The sessions used most recently are read first, so after a restart the ones in use get
    // their links within a pass or two while the rest catch up a few per pass.
    sessions.sort_by(|a, b| {
        let at = |session: &Value| {
            session
                .get("lastActiveAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        at(b).cmp(&at(a))
    });
    let mut full_scans = MAX_FULL_SCANS_PER_PASS;
    let mut open_sessions: HashSet<(String, String)> = HashSet::new();
    for session in &sessions {
        let Some((project_id, session_id)) = ids(session) else {
            continue;
        };
        open_sessions.insert((project_id.clone(), session_id.clone()));
        let Some(project) = projects_by_id.get(&project_id) else {
            continue;
        };
        let cwd = effective_session_git_cwd(session, Some(project));
        if let Some(cwd) = cwd.as_deref() {
            refresh_work_repo(cwd);
        }
        let mut stored = stored_session_links(session);
        let mut changed = false;
        let found = |link: SessionLinkRef, source: SessionLinkSource, via: Option<String>| {
            StoredSessionLink {
                link,
                source,
                via,
                url: None,
                added_at: Some(crate::session_links::now_iso()),
            }
        };

        // The branch's PR, kept so it stays listed after the session moves to another branch.
        if let Some(cwd) = cwd.as_deref() {
            if sessions_per_cwd.get(cwd) == Some(&1) {
                if let Some(git) = cached_session_git_status(cwd) {
                    let link = git
                        .pull_request
                        .as_ref()
                        .and_then(|pull_request| pull_request.url.as_deref())
                        .and_then(parse_github_url);
                    if let (Some(link), Some(branch)) = (link, work_branch(git.branch.as_deref())) {
                        changed |= stored.add_found(found(
                            link,
                            SessionLinkSource::Branch,
                            Some(branch.to_string()),
                        ));
                    }
                }
            }
        }

        // What the transcript says.
        if let Some((agent, path)) = session_transcript(state, &project_id, &session_id) {
            let first_read = needs_full_scan(&path);
            if !first_read || full_scans > 0 {
                if first_read {
                    full_scans -= 1;
                }
                if let Some(links) = scan_transcript(agent, &path, &project_id, &session_id) {
                    for link in links.opened {
                        changed |=
                            stored.add_found(found(link, SessionLinkSource::AgentOpened, None));
                    }
                    let team_keys = crate::work_mode::project_linear_key(&project_id)
                        .and_then(cached_linear_team_keys)
                        .unwrap_or_default();
                    for identifier in links.linear_mentions {
                        let team = identifier.split('-').next().unwrap_or_default();
                        if team_keys.iter().any(|key| key.eq_ignore_ascii_case(team)) {
                            changed |= stored.add_found(found(
                                SessionLinkRef::Linear { identifier },
                                SessionLinkSource::Mentioned,
                                None,
                            ));
                        }
                    }
                    remember_session_link_mentions(&project_id, &session_id, links.github_mentions);
                }
            }
        }

        // The issues a linked PR closes.
        let repo = cwd.as_deref().and_then(cached_work_repo);
        for entry in session_link_entries(project, session) {
            if entry.link.kind() != SessionLinkKind::PullRequest {
                continue;
            }
            let Some(url) = entry.link.github_url() else {
                continue;
            };
            let Some(info) = cached_work_pull_request("", &url) else {
                continue;
            };
            let via = entry.link.label(repo.as_deref());
            for issue in info
                .closing_issues
                .iter()
                .filter_map(|url| parse_github_url(url))
            {
                changed |=
                    stored.add_found(found(issue, SessionLinkSource::Closes, Some(via.clone())));
            }
        }

        if changed {
            write_session_links(&db, &project_id, &session_id, &stored)?;
        }
    }
    forget_session_link_mentions_except(&open_sessions);

    // Re-read: the stored links may have changed above.
    let mut sessions = Vec::new();
    for project_id in projects_by_id.keys() {
        sessions.extend(repository.list_sessions_excluding_stopped(Some(project_id))?);
    }
    refresh_link_caches(&projects_by_id, &sessions, &linear_keys, linear_budget);

    let mut changed = Vec::new();
    if let Ok(mut published) = published_links().lock() {
        published.retain(|key, _| open_sessions.contains(key));
        for session in &sessions {
            let Some(key) = ids(session) else {
                continue;
            };
            let Some(project) = projects_by_id.get(&key.0) else {
                continue;
            };
            let fingerprint = presentation_session_links(project, session)
                .map(|links| links.to_string())
                .unwrap_or_default();
            if published.get(&key) != Some(&fingerprint) {
                published.insert(key.clone(), fingerprint);
                changed.push(key);
            }
        }
    }
    for (project_id, session_id) in changed {
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
    }
    Ok(())
}

/// The session's transcript, when its agent writes one Ghostex can read.
fn session_transcript(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<(
    crate::session_chat::SessionChatTranscriptAgent,
    std::path::PathBuf,
)> {
    let resolution = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    )
    .ok()?;
    let (family, path) = resolution.transcript();
    Some((
        crate::session_chat::resolve_session_chat_transcript_agent(family)?,
        path?.to_path_buf(),
    ))
}

/// Reads what is stale among every open session's links, within the pass's budgets: open PRs and
/// issues and unfinished Linear tickets; a finished one is read once.
fn refresh_link_caches(
    projects_by_id: &HashMap<String, &Value>,
    sessions: &[Value],
    linear_keys: &HashMap<u64, String>,
    mut linear_budget: usize,
) {
    let mut pull_requests: Vec<String> = Vec::new();
    let mut issues: Vec<String> = Vec::new();
    let mut linear: HashMap<u64, Vec<String>> = HashMap::new();
    for session in sessions {
        let Some(project) = session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|id| projects_by_id.get(id))
        else {
            continue;
        };
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for entry in session_link_entries(project, session) {
            match &entry.link {
                SessionLinkRef::Github {
                    kind: SessionLinkKind::PullRequest,
                    ..
                } => {
                    let Some(url) = entry.link.github_url() else {
                        continue;
                    };
                    let finished = cached_work_pull_request("", &url).is_some_and(|info| {
                        info.state.disposition() == PullRequestDisposition::Finished
                    });
                    if !finished && !pull_requests.contains(&url) {
                        pull_requests.push(url);
                    }
                }
                SessionLinkRef::Github { .. } => {
                    let Some(url) = entry.link.github_url() else {
                        continue;
                    };
                    let finished = cached_work_github_issue_url(&url)
                        .is_some_and(|info| info.state.as_deref() == Some("closed"));
                    if !finished && !issues.contains(&url) {
                        issues.push(url);
                    }
                }
                SessionLinkRef::Linear { identifier } => {
                    if let Some(key) = crate::work_mode::project_linear_key(project_id) {
                        let finished = crate::work_mode::cached_linear_issue(key, identifier)
                            .is_some_and(|info| {
                                matches!(
                                    info.state_type.as_deref(),
                                    Some("completed") | Some("canceled")
                                )
                            });
                        let list = linear.entry(key).or_default();
                        if !finished && !list.contains(identifier) {
                            list.push(identifier.clone());
                        }
                    }
                }
            }
        }
    }
    for (fingerprint, identifiers) in &linear {
        if linear_budget == 0 {
            break;
        }
        if let Some(key) = linear_keys.get(fingerprint) {
            linear_budget = linear_budget.saturating_sub(refresh_linear_issues(
                key,
                identifiers,
                &[],
                linear_budget,
            ));
        }
    }
    if (pull_requests.is_empty() && issues.is_empty()) || !gh_cli_is_available() {
        return;
    }
    let mut gh_budget = MAX_GH_CALLS_PER_PASS;
    for url in &pull_requests {
        if gh_budget == 0 {
            return;
        }
        if refresh_work_pull_request("", url) {
            gh_budget -= 1;
        }
    }
    for url in &issues {
        if gh_budget == 0 {
            return;
        }
        if refresh_work_github_issue_selector("", url) {
            gh_budget -= 1;
        }
    }
}

/// Runs a pass right away (off the request path), so a link someone just added shows its state
/// without waiting for the next minute.
pub(crate) fn spawn_session_links_refresh(state: &Arc<AppState>) {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = run_session_links_refresh_once(&state) {
            log_session_links_refresh_failure(&state, &error.message);
        }
    });
}

pub(crate) fn log_session_links_refresh_failure(state: &AppState, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionLinksRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
