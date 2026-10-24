//! `PresentationSession.links`: every PR, GitHub issue and Linear ticket a session is linked to,
//! with what was last read about each, and the PR and issue URLs offered under Suggested. Pure
//! cache reads, so the presentation projection can call it per row.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "yes" to one session linking several PRs and several GitHub issues, "and several linear tickets too". This replaces the work-mode rule "Linear issues are a list, the other kinds one each" for what a session shows; the Link to menu's own single PR and issue (`workLinks`) stay as they are and join this list as the session's branch links.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "everywhere": every session gets its links, not only sessions of Work mode projects. How a sidebar row shows them is the "Session links in the sidebar" setting (CDXC:SessionLinks 2026-10-24 in packages/gx-core/src/sidebar_view/links.rs), which supersedes keeping the row's line Work-mode-only.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User picked "two-line" for each entry: the number, title and state on the first line, then checks, review and where the link came from (`sourceLabel`) on the second.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use crate::session_git_status::effective_session_git_cwd;
use crate::work_mode::{
    cached_linear_issue, cached_work_github_issue, cached_work_github_issue_url,
    cached_work_pull_request, cached_work_repo, manual_work_links, project_linear_key,
    work_targets, ManualPullRequest,
};

use super::{
    parse_github_url, stored_session_links, SessionLinkKind, SessionLinkRef, SessionLinkSource,
};

/// Suggestions a session shows at most.
const MAX_SUGGESTED: usize = 5;
/// Removed links the "Removed (N)" group lists at most.
const MAX_REMOVED: usize = 30;

/// One link of a session, before it is read from the caches.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionLinkEntry {
    pub(crate) link: SessionLinkRef,
    pub(crate) source: SessionLinkSource,
    /// Whether it comes from the branch the session is on right now (else an earlier one).
    pub(crate) current_branch: bool,
    /// The work-mode PR Linear attached to a linked ticket.
    pub(crate) from_ticket: bool,
    pub(crate) via: Option<String>,
    pub(crate) url: Option<String>,
}

/// The session repo's `owner/repo`, lowercase, as the background pass last read it.
pub(crate) fn session_repo(project: &Value, session: &Value) -> Option<String> {
    effective_session_git_cwd(session, Some(project))
        .as_deref()
        .and_then(cached_work_repo)
}

/// What a session is linked to: its branch's links first, then the stored ones, without the ones
/// a person removed.
pub(crate) fn session_link_entries(project: &Value, session: &Value) -> Vec<SessionLinkEntry> {
    let stored = stored_session_links(session);
    let mut entries: Vec<SessionLinkEntry> = Vec::new();
    let removed = stored.removed.clone();
    let push = |entry: SessionLinkEntry, entries: &mut Vec<SessionLinkEntry>| {
        let key = entry.link.key();
        if removed.contains(&key) || entries.iter().any(|known| known.link.key() == key) {
            return;
        }
        entries.push(entry);
    };

    let targets = work_targets(project, session);
    let repo = targets.cwd.as_deref().and_then(cached_work_repo);
    let manual = manual_work_links(session);
    let live = |link: SessionLinkRef, by_hand: bool| SessionLinkEntry {
        link,
        source: if by_hand {
            SessionLinkSource::User
        } else {
            SessionLinkSource::Branch
        },
        current_branch: !by_hand,
        from_ticket: false,
        via: None,
        url: None,
    };
    if let Some(selector) = targets.pull_request.as_deref() {
        let link = parse_github_url(selector).or_else(|| {
            Some(SessionLinkRef::Github {
                kind: SessionLinkKind::PullRequest,
                repo: repo.clone()?,
                number: selector.parse().ok()?,
            })
        });
        if let Some(link) = link {
            let by_hand = matches!(manual.pull_request, Some(Some(_)));
            let from_ticket = !by_hand
                && targets
                    .branch_pull_request
                    .is_none_or(|number| Some(number.to_string()).as_deref() != Some(selector));
            let mut entry = live(link, by_hand);
            entry.from_ticket = from_ticket;
            if let Some(Some(ManualPullRequest::Url(url))) = &manual.pull_request {
                entry.url = Some(url.clone());
            }
            push(entry, &mut entries);
        }
    }
    for number in &targets.github_issues {
        if let Some(repo) = &repo {
            let link = SessionLinkRef::Github {
                kind: SessionLinkKind::Issue,
                repo: repo.clone(),
                number: *number,
            };
            push(live(link, manual.github_issues.is_some()), &mut entries);
        }
    }
    for identifier in &targets.linear_issues {
        let link = SessionLinkRef::Linear {
            identifier: identifier.clone(),
        };
        push(live(link, manual.linear_issues.is_some()), &mut entries);
    }
    for item in stored.items {
        push(
            SessionLinkEntry {
                link: item.link,
                source: item.source,
                current_branch: false,
                from_ticket: false,
                via: item.via,
                url: item.url,
            },
            &mut entries,
        );
    }
    entries
}

/// The suggestions the background pass found in each session's transcript.
fn suggestions() -> &'static Mutex<HashMap<(String, String), Vec<SessionLinkRef>>> {
    static SUGGESTIONS: OnceLock<Mutex<HashMap<(String, String), Vec<SessionLinkRef>>>> =
        OnceLock::new();
    SUGGESTIONS.get_or_init(Default::default)
}

/// Remembers the GitHub URLs a session's chat mentioned, newest last.
pub(crate) fn remember_session_link_mentions(
    project_id: &str,
    session_id: &str,
    mentions: Vec<SessionLinkRef>,
) {
    if let Ok(mut suggestions) = suggestions().lock() {
        let key = (project_id.to_string(), session_id.to_string());
        if mentions.is_empty() {
            suggestions.remove(&key);
        } else {
            suggestions.insert(key, mentions);
        }
    }
}

pub(crate) fn forget_session_link_mentions_except(open: &HashSet<(String, String)>) {
    if let Ok(mut suggestions) = suggestions().lock() {
        suggestions.retain(|key, _| open.contains(key));
    }
}

/// The GitHub URLs a session's chat mentioned, newest last (the background pass reads their titles).
pub(crate) fn session_link_mentions(project_id: &str, session_id: &str) -> Vec<SessionLinkRef> {
    mentioned_links(project_id, session_id)
}

fn mentioned_links(project_id: &str, session_id: &str) -> Vec<SessionLinkRef> {
    suggestions()
        .lock()
        .ok()
        .and_then(|suggestions| {
            suggestions
                .get(&(project_id.to_string(), session_id.to_string()))
                .cloned()
        })
        .unwrap_or_default()
}

/// What was last read about a PR: by its URL first, then by number in the session's checkout (how
/// work mode asks about a hand-set number).
fn pull_request_info(
    link: &SessionLinkRef,
    cwd: Option<&str>,
) -> Option<crate::work_mode::WorkPullRequestInfo> {
    let url = link.github_url()?;
    cached_work_pull_request("", &url).or_else(|| {
        let SessionLinkRef::Github { number, .. } = link else {
            return None;
        };
        cached_work_pull_request(cwd?, &number.to_string())
    })
}

fn issue_info(
    link: &SessionLinkRef,
    cwd: Option<&str>,
) -> Option<crate::work_mode::WorkGithubIssueInfo> {
    let url = link.github_url()?;
    cached_work_github_issue_url(&url).or_else(|| {
        let SessionLinkRef::Github { number, .. } = link else {
            return None;
        };
        cached_work_github_issue(cwd?, *number)
    })
}

fn source_label(entry: &SessionLinkEntry) -> String {
    match entry.source {
        SessionLinkSource::Branch if entry.from_ticket => "From its ticket".to_string(),
        SessionLinkSource::Branch if entry.current_branch => "From branch".to_string(),
        SessionLinkSource::Branch => match &entry.via {
            Some(branch) => format!("Earlier branch {branch}"),
            None => "Earlier branch".to_string(),
        },
        SessionLinkSource::AgentOpened => "Agent opened".to_string(),
        SessionLinkSource::AgentReported => "Agent added".to_string(),
        SessionLinkSource::Closes => match &entry.via {
            Some(pull_request) => format!("Closed by {pull_request}"),
            None => "Closed by a linked PR".to_string(),
        },
        SessionLinkSource::Mentioned => "Mentioned in chat".to_string(),
        SessionLinkSource::User => "You added".to_string(),
    }
}

fn insert_text(output: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        output.insert(key.to_string(), json!(value));
    }
}

/// One entry as clients receive it.
fn entry_value(
    entry: &SessionLinkEntry,
    cwd: Option<&str>,
    repo: Option<&str>,
    linear_key: Option<u64>,
) -> Value {
    let link = &entry.link;
    let mut output = Map::new();
    output.insert("key".to_string(), json!(link.key()));
    output.insert("kind".to_string(), json!(link.kind().as_wire()));
    output.insert("label".to_string(), json!(link.label(repo)));
    output.insert("source".to_string(), json!(entry.source.as_wire()));
    output.insert("sourceLabel".to_string(), json!(source_label(entry)));
    let mut finished = false;
    match link {
        SessionLinkRef::Github {
            kind: SessionLinkKind::PullRequest,
            number,
            ..
        } => {
            output.insert("number".to_string(), json!(number));
            let info = pull_request_info(link, cwd);
            if let Some(info) = &info {
                output.insert("state".to_string(), json!(info.state.as_wire()));
                insert_text(&mut output, "checks", info.checks);
                insert_text(&mut output, "review", info.review);
                insert_text(&mut output, "title", info.title.as_deref());
                finished = info.state.disposition()
                    == crate::session_git_status::PullRequestDisposition::Finished;
            }
            let url = info
                .and_then(|info| info.url)
                .or_else(|| entry.url.clone())
                .or_else(|| link.github_url());
            insert_text(&mut output, "url", url.as_deref());
        }
        SessionLinkRef::Github { number, .. } => {
            output.insert("number".to_string(), json!(number));
            let info = issue_info(link, cwd);
            if let Some(info) = &info {
                insert_text(&mut output, "state", info.state.as_deref());
                insert_text(&mut output, "title", info.title.as_deref());
                finished = info.state.as_deref() == Some("closed");
            }
            let url = info.and_then(|info| info.url).or_else(|| link.github_url());
            insert_text(&mut output, "url", url.as_deref());
        }
        SessionLinkRef::Linear { identifier } => {
            output.insert("identifier".to_string(), json!(identifier));
            let info = linear_key.and_then(|key| cached_linear_issue(key, identifier));
            if let Some(info) = &info {
                insert_text(&mut output, "state", info.state_type.as_deref());
                insert_text(&mut output, "stateName", info.state_name.as_deref());
                insert_text(&mut output, "title", info.title.as_deref());
                finished = matches!(
                    info.state_type.as_deref(),
                    Some("completed") | Some("canceled")
                );
            }
            let url = info.and_then(|info| info.url).or_else(|| entry.url.clone());
            insert_text(&mut output, "url", url.as_deref());
        }
    }
    if finished {
        output.insert("finished".to_string(), json!(true));
    }
    Value::Object(output)
}

/// `PresentationSession.links` (packages/gx-protocol/src/presentation.rs); `None` when the session
/// has no link and nothing to suggest.
///
/// CDXC:SessionLinks 2026-10-24 WHY:
/// The list arrives sorted so every client shows it the same way: open work first (a failing PR is never hidden behind a merged one), PRs before issues before Linear tickets, then in the order they joined.
pub(crate) fn presentation_session_links(project: &Value, session: &Value) -> Option<Value> {
    let entries = session_link_entries(project, session);
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mentioned = mentioned_links(project_id, session_id);
    let stored = stored_session_links(session);
    if entries.is_empty() && mentioned.is_empty() && stored.removed.is_empty() {
        return None;
    }
    let cwd = effective_session_git_cwd(session, Some(project));
    let repo = cwd.as_deref().and_then(cached_work_repo);
    let linear_key = project_linear_key(project_id);
    let mut items: Vec<(usize, Value)> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            (
                index,
                entry_value(entry, cwd.as_deref(), repo.as_deref(), linear_key),
            )
        })
        .collect();
    items.sort_by_key(|(index, item)| {
        let finished = item.get("finished").and_then(Value::as_bool) == Some(true);
        let kind = match item.get("kind").and_then(Value::as_str) {
            Some("pullRequest") => 0,
            Some("issue") => 1,
            _ => 2,
        };
        (finished, kind, *index)
    });
    let linked: HashSet<String> = entries.iter().map(|entry| entry.link.key()).collect();
    let suggested: Vec<Value> = mentioned
        .iter()
        .rev()
        .filter(|link| {
            let key = link.key();
            !linked.contains(&key) && !stored.is_removed(&key)
        })
        .take(MAX_SUGGESTED)
        .map(|link| {
            let mut output = Map::new();
            output.insert("key".to_string(), json!(link.key()));
            output.insert("kind".to_string(), json!(link.kind().as_wire()));
            output.insert("label".to_string(), json!(link.label(repo.as_deref())));
            insert_text(&mut output, "url", link.github_url().as_deref());
            let title = match link.kind() {
                SessionLinkKind::PullRequest => {
                    pull_request_info(link, cwd.as_deref()).and_then(|info| info.title)
                }
                _ => issue_info(link, cwd.as_deref()).and_then(|info| info.title),
            };
            insert_text(&mut output, "title", title.as_deref());
            Value::Object(output)
        })
        .collect();
    // The links a person removed, newest first, for the lists' "Removed (N)" group and its Restore.
    let removed: Vec<Value> = stored
        .removed
        .iter()
        .rev()
        .take(MAX_REMOVED)
        .filter_map(|key| SessionLinkRef::from_key(key))
        .map(|link| {
            let mut output = Map::new();
            output.insert("key".to_string(), json!(link.key()));
            output.insert("kind".to_string(), json!(link.kind().as_wire()));
            output.insert("label".to_string(), json!(link.label(repo.as_deref())));
            let (title, url) = match &link {
                SessionLinkRef::Linear { identifier } => {
                    let info = linear_key.and_then(|key| cached_linear_issue(key, identifier));
                    let stored_url = stored
                        .removed_items
                        .iter()
                        .find(|item| item.link == link)
                        .and_then(|item| item.url.clone());
                    (
                        info.as_ref().and_then(|info| info.title.clone()),
                        info.and_then(|info| info.url).or(stored_url),
                    )
                }
                SessionLinkRef::Github {
                    kind: SessionLinkKind::PullRequest,
                    ..
                } => (
                    pull_request_info(&link, cwd.as_deref()).and_then(|info| info.title),
                    link.github_url(),
                ),
                SessionLinkRef::Github { .. } => (
                    issue_info(&link, cwd.as_deref()).and_then(|info| info.title),
                    link.github_url(),
                ),
            };
            insert_text(&mut output, "url", url.as_deref());
            insert_text(&mut output, "title", title.as_deref());
            Value::Object(output)
        })
        .collect();
    if items.is_empty() && suggested.is_empty() && removed.is_empty() {
        return None;
    }
    let mut output = Map::new();
    output.insert(
        "items".to_string(),
        Value::Array(items.into_iter().map(|(_, item)| item).collect()),
    );
    if !suggested.is_empty() {
        output.insert("suggested".to_string(), Value::Array(suggested));
    }
    if !removed.is_empty() {
        output.insert("removed".to_string(), Value::Array(removed));
    }
    Some(Value::Object(output))
}
