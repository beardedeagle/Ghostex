//! What the voice agent knows about sessions and projects: every open session with a spoken-style
//! state, and matching the names the user says ("the sidebar one") to sessions and projects.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::coordinators::{
    classify_thread_session, thread_needs_user_approval, thread_prompt, ThreadProgress, ThreadState,
};
use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// A session's state in the words the agent speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VoiceState {
    Working,
    WaitingOnQuestion,
    WaitingOnApproval,
    Finished,
    Sleeping,
}

impl VoiceState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::WaitingOnQuestion => "waiting on a question",
            Self::WaitingOnApproval => "waiting on an approval",
            Self::Finished => "idle (turn finished)",
            Self::Sleeping => "sleeping",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::WaitingOnApproval => 0,
            Self::WaitingOnQuestion => 1,
            Self::Finished => 2,
            Self::Working => 3,
            Self::Sleeping => 4,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SessionInfo {
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) title: String,
    pub(super) project: String,
    pub(super) agent: String,
    pub(super) state: VoiceState,
    /// What it waits on, when it waits ("It asked which branch to use.").
    pub(super) waiting: Option<String>,
    pub(super) last_active_at: String,
    /// The attention event of the turn the agent's Stop hook last finished (a new id per turn);
    /// `None` while the session is not in a finished-turn attention or its agent has no hooks.
    pub(super) turn_event: Option<String>,
    /// Its agent has a chat (a transcript Ghostex reads); without one it is only a terminal.
    pub(super) has_chat: bool,
}

impl SessionInfo {
    pub(super) fn reference(&self) -> String {
        format!("{}:{}", self.project_id, self.session_id)
    }

    /// Who it is, without its state: for a result that just changed that state.
    pub(super) fn brief(&self) -> Value {
        json!({"ref": self.reference(), "title": self.title, "project": self.project})
    }

    pub(super) fn to_tool_value(&self) -> Value {
        let mut value = json!({
            "ref": self.reference(),
            "title": self.title,
            "project": self.project,
            "agent": self.agent,
            "state": self.state.as_str(),
        });
        if let Some(waiting) = &self.waiting {
            value["waitingOn"] = json!(waiting);
        }
        value
    }
}

#[derive(Clone, Debug)]
pub(super) struct ProjectInfo {
    pub(super) project_id: String,
    pub(super) name: String,
    pub(super) path: String,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn agent_label(agent: &str) -> String {
    let lower = agent.to_ascii_lowercase();
    for (needle, label) in [
        ("claude", "Claude"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("cursor", "Cursor"),
        ("gemini", "Gemini"),
        ("grok", "Grok"),
        ("copilot", "Copilot"),
        ("hermes", "Hermes"),
        ("empryo", "Empryo"),
        ("antigravity", "Antigravity"),
        ("pi", "Pi"),
    ] {
        if lower == needle || lower.contains(needle) {
            return label.to_string();
        }
    }
    if agent.is_empty() {
        "terminal".to_string()
    } else {
        agent.to_string()
    }
}

/// Every open (running or sleeping) session, waiting ones first. Blocking: run it off the runtime.
pub(super) fn list_sessions(state: &AppState) -> Result<Vec<SessionInfo>, String> {
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects: Vec<ProjectInfo> = project_rows(&repository)?;
    let sessions = repository
        .list_sessions_excluding_stopped(None)
        .map_err(|error| error.message)?;
    let now = crate::presentation::now_iso();
    let mut rows: Vec<SessionInfo> = sessions
        .iter()
        .filter_map(|session| {
            let project_id = text(session, "projectId");
            let session_id = text(session, "sessionId");
            if project_id.is_empty() || session_id.is_empty() {
                return None;
            }
            let thread = classify_thread_session(
                Some(session),
                ThreadProgress {
                    resolved: false,
                    has_run: true,
                },
                &now,
                false,
            );
            let (state, waiting) = match thread {
                ThreadState::Working => (VoiceState::Working, None),
                // A finished turn that left questions in its chat waits on the user too
                // (server/src/session_chat_open_questions.rs).
                ThreadState::Finished
                    if crate::session_chat_open_questions::open_question_count(session) > 0 =>
                {
                    let count = crate::session_chat_open_questions::open_question_count(session);
                    (
                        VoiceState::WaitingOnQuestion,
                        Some(match count {
                            1 => "It has 1 open question in its chat.".to_string(),
                            count => format!("It has {count} open questions in its chat."),
                        }),
                    )
                }
                ThreadState::Finished => (VoiceState::Finished, None),
                ThreadState::Sleeping => (VoiceState::Sleeping, None),
                ThreadState::Waiting => {
                    let summary = thread_prompt(session).map(|prompt| prompt.summary);
                    if thread_needs_user_approval(session) {
                        (VoiceState::WaitingOnApproval, summary)
                    } else {
                        (VoiceState::WaitingOnQuestion, summary)
                    }
                }
                ThreadState::Closed | ThreadState::Done => return None,
            };
            let project = projects
                .iter()
                .find(|project| project.project_id == project_id)
                .map(|project| project.name.clone())
                .unwrap_or_default();
            let title = Some(text(session, "title"))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Untitled session".to_string());
            Some(SessionInfo {
                project_id,
                session_id,
                title,
                project,
                agent: agent_label(&text(session, "agentId")),
                state,
                waiting,
                last_active_at: text(session, "lastActiveAt"),
                turn_event: turn_event(session),
                has_chat: crate::session_chat_follower::session_chat_agent_for_session(session)
                    .is_some(),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a.state
            .order()
            .cmp(&b.state.order())
            .then_with(|| b.last_active_at.cmp(&a.last_active_at))
    });
    Ok(rows)
}

fn turn_event(session: &Value) -> Option<String> {
    let activity = session.pointer("/runtimeSettings/agentActivity")?;
    if activity.get("attentionSource").and_then(Value::as_str)
        != Some(crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return None;
    }
    activity
        .get("attentionEventId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn project_rows(repository: &DomainRepository<'_>) -> Result<Vec<ProjectInfo>, String> {
    Ok(repository
        .list_projects()
        .map_err(|error| error.message)?
        .iter()
        .filter_map(|project| {
            let project_id = text(project, "projectId");
            if project_id.is_empty() {
                return None;
            }
            let path = text(project, "path");
            let name = Some(text(project, "name"))
                .filter(|name| !name.is_empty())
                .or_else(|| {
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| project_id.clone());
            Some(ProjectInfo {
                project_id,
                name,
                path,
            })
        })
        .collect())
}

pub(super) fn list_projects(state: &AppState) -> Result<Vec<ProjectInfo>, String> {
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    project_rows(&repository)
}

pub(super) async fn list_sessions_async(state: &Arc<AppState>) -> Result<Vec<SessionInfo>, String> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || list_sessions(&state))
        .await
        .map_err(|error| error.to_string())?
}

pub(super) async fn list_projects_async(state: &Arc<AppState>) -> Result<Vec<ProjectInfo>, String> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || list_projects(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// Words that say nothing about which session is meant ("the sidebar one").
const FILLER: &[&str] = &[
    "the", "a", "an", "one", "session", "sessions", "agent", "chat", "thread", "that", "this",
    "my", "on", "in", "of", "for", "about", "with", "to", "and", "is", "it", "thing", "task",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !FILLER.contains(word))
        .map(str::to_string)
        .collect()
}

/// How well `query` names a target described by `name` (its title) and `context` (its project and
/// agent): 0 when unrelated.
fn score(query: &str, name: &str, context: &str) -> u32 {
    let query_lower = query.trim().to_lowercase();
    let name_lower = name.to_lowercase();
    if query_lower.is_empty() {
        return 0;
    }
    if name_lower == query_lower {
        return 1000;
    }
    let query_words = words(query);
    if query_words.is_empty() {
        return 0;
    }
    let name_words = words(name);
    let context_words = words(context);
    let mut points = 0u32;
    let mut matched = 0usize;
    for word in &query_words {
        let in_name = name_words.iter().any(|candidate| {
            candidate == word
                || (word.len() >= 3 && candidate.starts_with(word.as_str()))
                || (candidate.len() >= 3 && word.starts_with(candidate.as_str()))
        });
        let in_context = context_words.iter().any(|candidate| candidate == word);
        if in_name {
            points += 100;
            matched += 1;
        } else if in_context {
            points += 40;
            matched += 1;
        }
    }
    if matched == 0 {
        return 0;
    }
    if name_lower.contains(&query_lower) {
        points += 150;
    }
    // Every word the user said matched: much stronger than a partial match.
    if matched == query_words.len() {
        points += 120;
    }
    points
}

pub(super) enum Match<T> {
    One(T),
    Ambiguous(Vec<T>),
    None,
}

/// The session `query` names: a `project:session` ref, a session id, or words of its title,
/// project or agent.
pub(super) fn match_session(sessions: &[SessionInfo], query: &str) -> Match<SessionInfo> {
    let trimmed = query.trim();
    if let Some(found) = sessions.iter().find(|session| {
        session.reference() == trimmed
            || session.session_id == trimmed
            || trimmed.ends_with(&format!(":{}", session.reference()))
    }) {
        return Match::One(found.clone());
    }
    pick(
        sessions
            .iter()
            .map(|session| {
                let context = format!("{} {}", session.project, session.agent);
                (score(trimmed, &session.title, &context), session.clone())
            })
            .collect(),
    )
}

pub(super) fn match_project(projects: &[ProjectInfo], query: &str) -> Match<ProjectInfo> {
    let trimmed = query.trim();
    if let Some(found) = projects
        .iter()
        .find(|project| project.project_id == trimmed)
    {
        return Match::One(found.clone());
    }
    // A folder anywhere in the argument ("~/dev/_active/Ghostex", or the hint an ambiguity answer
    // gave) names the project exactly.
    let home = std::env::var("HOME").unwrap_or_default();
    let folder = trimmed
        .split(|c: char| c.is_whitespace() || c == '"' || c == '(' || c == ')')
        .find(|part| part.starts_with('/') || part.starts_with("~/"))
        .map(|part| match part.strip_prefix('~') {
            Some(rest) if !home.is_empty() => format!("{home}{rest}"),
            _ => part.to_string(),
        })
        .map(|part| part.trim_end_matches('/').to_string());
    if let Some(folder) = folder {
        if let Some(found) = projects
            .iter()
            .find(|project| project.path.trim_end_matches('/') == folder)
        {
            return Match::One(found.clone());
        }
    }
    pick(
        projects
            .iter()
            .map(|project| {
                (
                    score(trimmed, &project.name, &project.path),
                    project.clone(),
                )
            })
            .collect(),
    )
}

/// The clear winner, or the close candidates when two score alike.
fn pick<T: Clone>(mut scored: Vec<(u32, T)>) -> Match<T> {
    scored.retain(|(points, _)| *points > 0);
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    match scored.as_slice() {
        [] => Match::None,
        [(_, only)] => Match::One(only.clone()),
        [(best, first), (second, _), ..] => {
            // Two exact names (two projects called "Ghostex") are a question, not a pick.
            if (*best >= 1000 && *second < 1000) || (*best < 1000 && *best >= second + 60) {
                Match::One(first.clone())
            } else {
                Match::Ambiguous(
                    scored
                        .iter()
                        .take_while(|(points, _)| *points + 60 > *best || *points >= 1000)
                        .take(5)
                        .map(|(_, item)| item.clone())
                        .collect(),
                )
            }
        }
    }
}
