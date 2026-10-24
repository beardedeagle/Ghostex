//! The Environment section: where the session runs, its folder, branch and account.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q13): Environment shows where the session runs ("This computer" or its box), its folder, its branch and its account, with no Remote Control switch (Ghostex's own phone app already reaches every session).

use std::path::Path;

use serde_json::{json, Value};

use crate::server::read_runtime_text;

/// `~/dev/app` for a folder under the home folder, else the path itself.
fn folder_label(folder: &str, home: &Path) -> String {
    match Path::new(folder).strip_prefix(home) {
        Ok(relative) if relative.as_os_str().is_empty() => "~".to_string(),
        Ok(relative) => format!("~/{}", relative.to_string_lossy()),
        Err(_) => folder.to_string(),
    }
}

/// `{location: {kind, label}, project, folder, folderLabel, worktree, account}`.
pub(super) fn read_environment(
    session: &Value,
    project: Option<&Value>,
    folder: Option<&str>,
    home: &Path,
) -> Value {
    let location = match crate::agentbox::session_agentbox(session) {
        Some(agentbox) => json!({
            "kind": "box",
            "label": crate::agentbox::provider_label(&agentbox.provider),
        }),
        None => json!({"kind": "local", "label": "This computer"}),
    };
    let project_name = project
        .and_then(|project| project.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    // A linked worktree's `.git` is a file pointing at the main checkout.
    let worktree = folder.is_some_and(|folder| Path::new(folder).join(".git").is_file())
        || project.is_some_and(|project| project.pointer("/worktree/parentProjectId").is_some());
    let account = match (
        read_runtime_text(session, "accountSlot"),
        read_runtime_text(session, "accountName"),
    ) {
        (None, None) => None,
        (slot, name) => Some(crate::accounts::launch::account_label(
            slot.as_deref().unwrap_or_default(),
            name.as_deref().unwrap_or_default(),
        )),
    };
    json!({
        "location": location,
        "project": project_name,
        "folder": folder,
        "folderLabel": folder.map(|folder| folder_label(folder, home)),
        "worktree": worktree,
        "account": account,
    })
}
