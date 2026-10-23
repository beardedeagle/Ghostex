//! Claude cloud sessions: the rules every client shares about a session whose agent runs on
//! claude.ai's computers (server/src/claude_cloud/ owns the sessions).
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User picked a separate "Claude (cloud)" entry in the agent list, and Claude cloud as one of the
//! places in the Run on row next to This computer and the Cloud Boxes, "each with a one-line what
//! it means"; asked how the two meet, they confirmed "Yes, both": picking Claude (cloud) sets Run
//! on to Claude cloud, and every other agent dims that place, because only Claude runs there.
//! The phone shows cloud rows too, tap opening the session's claude.ai page (confirmed, built
//! after the computer part), so these rules stay client-neutral.
//!
//! SEE-ALSO: server/src/claude_cloud/ (the sessions), apps/desktop/src/app/window/new_thread_picker_run_on.rs
//! (the picker's Run on row), apps/desktop/src/app/native_sidebar/claude_cloud.rs (the row).

use crate::core::Core;
use crate::keys::SessionKey;

/// The built-in agent that starts a Claude cloud session.
pub const CLAUDE_CLOUD_AGENT_ID: &str = "claude-cloud";

/// The `runLocation` of a Claude cloud session.
pub const CLAUDE_CLOUD_RUN_LOCATION: &str = "claude-cloud";

/// What the Run on row calls the place.
pub const CLAUDE_CLOUD_LABEL: &str = "Claude cloud";

/// The Run on row's one line about the place.
pub const CLAUDE_CLOUD_DESCRIPTION: &str =
    "Claude only. Runs on Claude's computers and keeps going when this computer sleeps.";

/// The Run on row's one line about this computer, shown beside Claude cloud.
pub const THIS_COMPUTER_DESCRIPTION: &str = "Any agent. Stops when this computer sleeps.";

/// Whether a launcher agent is the Claude (cloud) agent.
pub fn is_claude_cloud_agent(agent_id: &str) -> bool {
    agent_id.trim() == CLAUDE_CLOUD_AGENT_ID
}

/// Whether a `runLocation` names Claude cloud.
pub fn is_claude_cloud_run_location(run_location: Option<&str>) -> bool {
    run_location.map(str::trim) == Some(CLAUDE_CLOUD_RUN_LOCATION)
}

/// The claude.ai page a session's row opens: set once a Claude cloud session exists on claude.ai
/// and until it is brought to this computer. `None` for every other session, and for a cloud
/// session still waiting for its task or starting, whose terminal shows that.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User: clicking a cloud session's row opens "Claude's page for the session inside Ghostex", the claude.ai page in Ghostex's Browser, until Claude enables attaching to a cloud session from a terminal; then it opens Ghostex's own terminal and chat. Every client asks this one rule (row click, focus, notifications), so no entry point lands on the terminal by itself.
pub fn session_claude_cloud_page(core: &Core, session: &SessionKey) -> Option<String> {
    let row = core.presentation().session(session)?;
    let cloud = row.claude_cloud.as_ref()?;
    // Attached, the terminal shows the cloud session itself.
    (cloud.state == "running" && cloud.attach.as_deref() != Some("live"))
        .then(|| cloud.url.clone())
        .flatten()
        .filter(|url| url.starts_with("https://claude.ai/code/"))
}

/// Whether a session is a Claude cloud session (any state).
pub fn is_claude_cloud_session(core: &Core, session: &SessionKey) -> bool {
    core.presentation()
        .session(session)
        .is_some_and(|session| session.claude_cloud.is_some())
}

/// The cloud session a claude.ai page belongs to on this computer, and the saved Claude account it
/// runs on (`None` inside: the current CLI login). `None` for a page that is not one of them.
pub fn claude_cloud_page_account(core: &Core, url: &str) -> Option<Option<String>> {
    let page = claude_cloud_session_link(url)?;
    core.presentation()
        .loaded(&crate::MachineId::Local)?
        .server_sessions()
        .filter_map(|session| session.claude_cloud.as_ref())
        .find(|cloud| {
            cloud
                .url
                .as_deref()
                .and_then(claude_cloud_session_link)
                .is_some_and(|link| link == page)
        })
        .map(|cloud| cloud.account_id.clone())
}

/// `https://claude.ai/code/session_…` without its query or fragment, for any link into one.
pub fn claude_cloud_session_link(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://claude.ai/code/")?;
    let id: String = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    (id.starts_with("session_") || id.starts_with("cse_"))
        .then(|| format!("https://claude.ai/code/{id}"))
}

/// The account of the project's newest cloud session, if it had one: what a new cloud session's
/// account list highlights first.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User picked "the account of this project's last cloud session" for the account New session highlights first for a cloud session, falling back to the rule local sessions use when the project never had one (a repo is often set up for the cloud on one account only). gxserver applies the same order when a create names no account (server/src/claude_cloud/create.rs `pick_claude_account`).
pub fn project_last_cloud_account(core: &Core, project_id: &str) -> Option<String> {
    core.presentation()
        .loaded(&crate::MachineId::Local)?
        .server_sessions()
        .filter(|session| session.project_id == project_id)
        .filter_map(|session| {
            let account = session.claude_cloud.as_ref()?.account_id.clone()?;
            Some((session.created_at.clone(), account))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, account)| account)
}
