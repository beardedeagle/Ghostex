//! Where the next prompt goes by default.

use crate::GhostexGpuiApp;
use crate::app::gx_store::CaptureTargetProject;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// A new session in a project (`workspace_project_id`).
    NewSession { project: String, title: String },
    /// A running session (its sidebar session id).
    Session {
        session: String,
        title: String,
        project_title: String,
    },
}

impl Target {
    pub(crate) fn label(&self) -> String {
        match self {
            Target::NewSession { title, .. } => format!("+ New session · {title}"),
            Target::Session {
                title,
                project_title,
                ..
            } => format!("{title} · {project_title}"),
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_ms(value: Option<&str>) -> i64 {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.timestamp_millis())
        .unwrap_or(0)
}

impl GhostexGpuiApp {
    pub(super) fn ghostex_capture_targets(&self) -> Vec<CaptureTargetProject> {
        self.gx_store_capture_targets(now_ms())
    }

    /// CDXC:GhostexCapture 2026-10-22 DECISION:
    /// User: "make it default to the currently active session in the actual window that i last interacted with (last session that became active in the app)" (Q380, "Last active session"). Supersedes the 2026-09-30 default of a new session in the project the last prompt went to. When the main window has no session focused, the most recently active session is the default.
    pub(super) fn ghostex_capture_default_target(&self) -> Option<Target> {
        let projects = self.ghostex_capture_targets();
        let session = |project: &CaptureTargetProject, index: usize| {
            let session = &project.sessions[index];
            Target::Session {
                session: session.sidebar_session_id.clone(),
                title: session.title.clone(),
                project_title: project.title.clone(),
            }
        };
        let sessions = || {
            projects
                .iter()
                .flat_map(|project| (0..project.sessions.len()).map(move |index| (project, index)))
        };
        sessions()
            .find(|(project, index)| project.sessions[*index].focused)
            .or_else(|| {
                sessions().max_by_key(|(project, index)| {
                    parse_ms(project.sessions[*index].last_interaction_at.as_deref())
                })
            })
            .map(|(project, index)| session(project, index))
            .or_else(|| {
                projects.first().map(|project| Target::NewSession {
                    project: project.workspace_project_id.clone(),
                    title: project.title.clone(),
                })
            })
    }
}
