use super::*;
use crate::*;

pub(crate) fn gpui_remote_sidebar_request_path_allowed(path: &str) -> bool {
    matches!(
        path,
        "/api/createSession"
            | "/api/createAgentSession"
            | "/api/readAgentHookStatus"
            | "/api/installAgentHooks"
            | "/api/forkSession"
            | "/api/scheduleDelayedSend"
            | "/api/cancelDelayedSend"
            | "/api/postponeDelayedSend"
            | "/api/sleepSession"
            | "/api/wakeSession"
            | "/api/killSession"
            /*
            CDXC:RemoteMachines 2026-08-18:
            Creating a remote agent session is a two-step daemon operation:
            `/api/createAgentSession` writes the row and queues the agent's
            launch startup text, then `/api/startSessionProvider` spawns the
            zmx provider that actually runs the agent, and `/api/queueSessionChatPrompt`
            durably queues the workflow prompt for verified delivery. Leaving the second and third steps off
            this allowlist made every remote agent launch report "Remote agent
            failed" at the Rust boundary and silently dropped Git/worktree
            workflow prompts. Params below are reshaped to the two ids (plus a
            bounded message body) so CEF still cannot tunnel startup text,
            commands, or daemon flags to a remote machine.
            */
            | "/api/startSessionProvider"
            | "/api/sendSessionMessage"
            | "/api/queueSessionChatPrompt"
            | "/api/updateSession"
            | "/api/requestSessionRename"
            /*
            CDXC:TranscriptExport 2026-08-20:
            A remote session's transcript only exists on the machine that runs
            the agent, so Export Transcript is an id-scoped read-and-write on
            that machine's own daemon, exactly like sleep/wake. Params are
            reshaped to the two ids below and the answer is reduced to the
            written path plus its size.
            */
            | "/api/exportSessionTranscript"
            /*
            CDXC:StateSync 2026-07-29:
            Sidebar V2's settle/snooze commands are id-scoped session mutations
            on a remote machine's own daemon, exactly like sleep/wake/kill.
            Their params are reshaped below so CEF can only ever send the two
            ids plus a bounded ISO wake time.
            */
            | "/api/settleSession"
            | "/api/unsettleSession"
            | "/api/snoozeSession"
            | "/api/unsnoozeSession"
            | "/api/listPreviousSessions"
            | "/api/removeSession"
            | "/api/updateProject"
            | "/api/listRecentProjects"
            | "/api/closeProjectToRecent"
            | "/api/restoreRecentProject"
            | "/api/removeRecentProject"
            | "/api/removeProject"
            | "/api/listProjectWorktrees"
            | "/api/createProjectWorktree"
            | "/api/openProjectWorktree"
            /*
            CDXC:StateSync 2026-07-29:
            Sidebar V2's worktree flow, allow-listed for remote machines: only
            the daemon that holds the repository can cut or delete a checkout in
            it, so these are project-scoped mutations on that machine's own
            gxserver, exactly like the settle/snooze pair above. Params are
            reshaped below so CEF can only ever send a project id, bounded
            agent/branch/prompt strings, and one nested existing-worktree path;
            responses are reduced to the created session's ids plus the removal
            verdict.
            */
            | "/api/createWorktreeSession"
            | "/api/removeSessionWorktree"
            | "/api/mergeWorktreeIntoMain"
            | "/api/checkoutProjectNewBranch"
            | "/api/readPresentationSnapshot"
            /*
            CDXC:RemoteMachines 2026-08-29:
            A project's Actions are stored by the daemon that owns the project,
            so the sidebar's project-row quick actions for a remote project can
            only come from that machine's own HUD projection. This is an
            id-scoped read; its params are reduced and its answer is cut down to
            the Action button lists in `sidebar_hud/buttons.rs`.
            */
            | "/api/readSidebarHud"
            | "/api/updateSidebarProjectCollections"
            /*
            CDXC:RemoteMachines 2026-09-21:
            Editing a Space, reordering projects, Switch Account and saving a
            note are the four sidebar writes the old runtime has always sent
            down a machine's tunnel and this list never carried, so each one
            failed at the Rust boundary on a remote tab while working on this
            computer. Their params are shaped in
            `sidebar_bridge_writes.rs`, which says why each is the same kind
            of write as the allowlisted sibling beside it and why the server
            needs no change.
            */
            | "/api/updateSidebarSpaces"
            | "/api/updateWorkspaceSessionGroups"
            | "/api/switchSessionAgent"
            | "/api/saveSessionAgentNote"
            // The account pages' list, a row's account list and a pick; shaped both ways in
            // `sidebar_bridge_writes.rs`.
            | "/api/agentAccounts"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
            | "/api/deleteWorktreeProject"
    )
}

pub(crate) fn gpui_remote_sidebar_iso_timestamp_allowed(value: &str) -> bool {
    (20..=40).contains(&value.len())
        && value.starts_with(|ch: char| ch.is_ascii_digit())
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | '.' | 'T' | 'Z' | '+'))
}

pub(crate) fn gpui_remote_sidebar_agent_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/*
The optional first prompt is real user prose, so newlines and tabs are legal
where every other bridged string forbids control characters. Everything else in
the control range is still rejected, and the length is bounded so this endpoint
cannot become a bulk channel into the remote daemon.
*/
pub(crate) fn gpui_remote_sidebar_first_prompt_allowed(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= 4_000
        && !value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
}

pub(crate) fn gpui_remote_sidebar_worktree_path_allowed(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 1_024
        && !value.chars().any(char::is_control)
        && !value.contains("..")
}

pub(crate) fn gpui_remote_sidebar_project_id_allowed(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=32).contains(&bytes.len())
        && bytes[0] == b'P'
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/*
CDXC:RemoteMachines 2026-07-30:
The Agents workspace, presentation focus state, and parked shell-state models
key projects by either a raw local gxserver project id or a machine-scoped
remote project id. Both shapes are opaque workspace keys; everything that
persists or validates a workspace project key must accept both, or remote
workspaces are silently dropped and focus snapshots swap to bogus projects.
*/
pub(crate) fn gpui_remote_sidebar_worktree_key_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn gpui_remote_sidebar_git_ref_allowed(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value.len() <= 200
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

pub(crate) fn gpui_remote_sidebar_slug_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
}

pub(crate) fn gpui_remote_sidebar_bounded_text_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_remote_sidebar_git_preferences_update_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let confirm_commit = json_bool_field(source, "confirmCommit")?;
    let generate_commit_body = json_bool_field(source, "generateCommitBody")?;
    let primary_action = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(value))?;
    Some(serde_json::json!({
        "confirmCommit": confirm_commit,
        "generateCommitBody": generate_commit_body,
        "primaryAction": primary_action,
    }))
}

pub(crate) fn gpui_remote_sidebar_request_refreshes_presentation(path: &str) -> bool {
    !matches!(
        path,
        "/api/listPreviousSessions"
            | "/api/readAgentHookStatus"
            | "/api/installAgentHooks"
            | "/api/listRecentProjects"
            | "/api/listProjectWorktrees"
            | "/api/readPresentationSnapshot"
            | "/api/readSidebarHud"
            | "/api/checkoutProjectNewBranch"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
    )
}
