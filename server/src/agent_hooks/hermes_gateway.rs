//! Hermes hooks fired inside Hermes's shared gateway, routed to the Ghostex terminal whose Hermes
//! client shows their session.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::event_mapping::env_string;

/// What a Hermes hook should do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HermesHookRoute {
    /// Fired inside the terminal's own `hermes` (Hermes 0.21 and older): its environment names the
    /// session, and the existing rules apply.
    InTerminal,
    /// Fired in the gateway and routed to the pane whose Hermes client shows the conversation.
    /// `subagent` marks a delegate_task child (or a child of one) of that conversation.
    Attached { subagent: bool },
    /// No Ghostex terminal shows this conversation (a one-shot or nested run, a messaging chat, a
    /// session whose terminal closed): it is not any session's agent.
    Unattached,
}

/*
CDXC:SessionIdentity 2026-10-17 WHY:
Hermes 0.22 runs every turn in one shared gateway (`hermes gateway run --idle-exit`), started by whichever terminal opened Hermes first and kept alive for all of them, and fires the shell hooks from there. The gateway kept the environment of that first terminal, so every Hermes session's hooks named a session that no longer existed (GHOSTEX_GLOBAL_SESSION_REF of S90:P70np:G1mao on 2026-10-17), and the no-terminal rule meant for nested runs dropped them all anyway: chat stayed empty, no session learned its Hermes id, and wake started a new conversation. The hook payload carries the conversation's `local-<hex>` session id, and each terminal client records the conversation it shows in a per-terminal breadcrumb (`<HERMES_HOME>/terminal-sessions/tty-dev-ttys014`, the file `hermes -c` reads). So the hook finds the breadcrumb naming its session (or the session its lineage continues: a compaction that rotated the id, a subagent), the live Hermes client on that terminal, and the `zmx run` pane above it, and reports to that pane's session. A hook nothing maps reports nothing. Hermes 0.21 fires the hooks inside the terminal's own process and keeps its environment rules.
SEE-ALSO: server/src/agent_hooks/notify_runtime.rs (applies the route), server/src/agent_hooks/claude_daemon.rs (the same routing for Claude's daemon), server/src/agent_hooks/plugin_sources.rs (the notify script lets a Hermes hook through without routing variables).
*/
pub(crate) fn route_hermes_hook(session_id: Option<&str>, payload: &Value) -> HermesHookRoute {
    if hook_process_has_terminal() {
        return HermesHookRoute::InTerminal;
    }
    let Some(session_id) =
        session_id.filter(|id| crate::session_chat_hermes::is_safe_hermes_session_id(id))
    else {
        return HermesHookRoute::Unattached;
    };
    for home in hermes_homes(payload) {
        let crumbs = read_breadcrumbs(&home.join("terminal-sessions"));
        if crumbs.is_empty() {
            continue;
        }
        let Some((terminal_id, subagent)) = lineage_terminal(&home, session_id, &crumbs) else {
            continue;
        };
        let routed = pane_on_terminal(&terminal_id)
            .is_some_and(|zmx_name| super::ancestor_session::adopt_zmx_session_routing(&zmx_name));
        if routed {
            super::ancestor_session::adopt_local_gxserver_address();
            return HermesHookRoute::Attached { subagent };
        }
    }
    HermesHookRoute::Unattached
}

/// A Hermes client's hook runs inside the terminal's own process; the gateway (or a `setsid` run)
/// cannot open `/dev/tty`. Windows has no controlling terminal, so every hook counts there.
fn hook_process_has_terminal() -> bool {
    !cfg!(unix) || fs::File::open("/dev/tty").is_ok()
}

/// The Hermes homes whose breadcrumbs may name the session: the home the gateway runs the hook
/// for (it sets HERMES_HOME to the routed profile's), then the profile the payload names.
fn hermes_homes(payload: &Value) -> Vec<PathBuf> {
    let root = crate::session_chat_hermes::hermes_home();
    let mut homes = Vec::new();
    if let Some(home) = env_string("HERMES_HOME") {
        homes.push(PathBuf::from(home));
    }
    homes.push(root.clone());
    if let Some(profile) = payload
        .get("profile")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|profile| !profile.is_empty() && *profile != "default")
        .filter(|profile| !profile.contains(['/', '\\']) && *profile != "..")
    {
        homes.push(root.join("profiles").join(profile));
    }
    homes.dedup();
    homes
}

/// `(terminal id, session id)` of every breadcrumb in `directory`.
fn read_breadcrumbs(directory: &Path) -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let terminal_id = entry.file_name().to_str()?.to_string();
            let record: Value =
                serde_json::from_str(&fs::read_to_string(entry.path()).ok()?).ok()?;
            let session_id = record.get("session_id")?.as_str()?.trim().to_string();
            (!session_id.is_empty()).then_some((terminal_id, session_id))
        })
        .collect()
}

/// The terminal whose breadcrumb names `session_id` or the closest session it descends from, and
/// whether it descends as a subagent rather than as the same conversation rotated by compaction.
fn lineage_terminal(
    home: &Path,
    session_id: &str,
    crumbs: &[(String, String)],
) -> Option<(String, bool)> {
    let crumb_for = |id: &str| {
        crumbs
            .iter()
            .find(|(_, crumb)| crumb == id)
            .map(|(terminal, _)| terminal.clone())
    };
    if let Some(terminal) = crumb_for(session_id) {
        return Some((terminal, false));
    }
    let connection = crate::session_chat_hermes::open_read_only_state_db(&home.join("state.db"))?;
    let mut current = session_id.to_string();
    let mut subagent = false;
    for _ in 0..16 {
        let (parent, continues) = connection
            .query_row(
                "SELECT child.parent_session_id, COALESCE(parent.end_reason, '') = 'compression' \
                 FROM sessions AS child LEFT JOIN sessions AS parent \
                 ON parent.id = child.parent_session_id WHERE child.id = ?1",
                rusqlite::params![current],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, bool>(1)?)),
            )
            .ok()?;
        let parent = parent?;
        subagent |= !continues;
        if let Some(terminal) = crumb_for(&parent) {
            return Some((terminal, subagent));
        }
        current = parent;
    }
    None
}

/// The Ghostex pane (`zmx run <name>`) whose terminal is `terminal_id` and still runs a Hermes
/// client. Hermes names a terminal after its tty path (`/dev/ttys014` → `tty-dev-ttys014`).
#[cfg(unix)]
fn pane_on_terminal(terminal_id: &str) -> Option<String> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-A", "-o", "pid=,tty=,command="])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse::<u32>().ok()?;
            let tty = fields.next()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            let on_terminal =
                !tty.starts_with('?') && hermes_terminal_id(&format!("/dev/{tty}")) == terminal_id;
            let hermes_client = command.contains("hermes") && !command.contains(" gateway run");
            (on_terminal && hermes_client).then_some(pid)
        })
        .find_map(super::ancestor_session::zmx_session_owning_agent)
}

#[cfg(not(unix))]
fn pane_on_terminal(_terminal_id: &str) -> Option<String> {
    None
}

/// Hermes's terminal id for a tty path (`terminal_breadcrumbs.get_terminal_id`).
#[cfg(not(windows))]
fn hermes_terminal_id(tty_path: &str) -> String {
    let trimmed = tty_path.trim().trim_matches('/');
    let sanitized: String = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .take(120)
        .collect();
    format!("tty-{sanitized}")
}
