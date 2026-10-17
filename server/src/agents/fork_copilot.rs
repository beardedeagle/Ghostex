//! Copilot fork: a copy of the session folder under a new id, resumed with `copilot --resume`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::fork_empryo::{create_private_dir, write_private_file};
use super::*;
use crate::domain::DomainStateError;

/// CDXC:SessionFork 2026-10-17 WHY:
/// Copilot CLI (1.0.98) has no fork flag; its `/fork` only works inside a running session. A session is a folder, `session-state/<id>/` with `events.jsonl` and `workspace.yaml`, and `copilot --resume <id>` replays any folder there: a copy whose files name a new id resumed with the whole conversation (measured: the copy answered from the original's turns). So a fork writes those two files under the new id with every mention of the old id rewritten, and leaves the rest behind (locks, `inuse.*`, checkpoints and file snapshots, which Copilot recreates). The copy drops `mc_session_id`/`mc_task_id`, which tie the original to its GitHub remote task, and its name gets the `Fork: ` prefix, which Copilot shows in its terminal title and Ghostex adopts as the session name (CDXC:SessionFork 2026-09-11).
pub(crate) fn fork_copilot_session_folder(
    source_session: &Value,
    fork_id: &str,
) -> Result<CopilotForkFolder, DomainStateError> {
    let runtime = object_field(source_session, "runtimeSettings");
    let source_id = read_text_from_map(&runtime, "agentSessionId")
        .filter(|id| is_safe_session_id(id))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This Copilot session has no saved conversation to fork yet.",
            )
        })?;
    let source_dir = copilot_session_dir(&runtime, &source_id).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Copilot's saved conversation {source_id} was not found, so it cannot be forked."
        ))
    })?;
    let fork_dir = source_dir.with_file_name(fork_id);
    let log = fs::read_to_string(source_dir.join("events.jsonl")).map_err(io_error)?;
    // Copilot may be appending right now; only whole lines are copied.
    let log = &log[..log.rfind('\n').map_or(0, |end| end + 1)];
    if log.trim().is_empty() {
        return Err(DomainStateError::bad_request(
            "This Copilot session has no saved conversation to fork yet.",
        ));
    }
    let workspace = fs::read_to_string(source_dir.join("workspace.yaml")).map_err(io_error)?;
    let workspace = fork_workspace(&workspace, &source_id, fork_id);
    create_private_dir(&fork_dir).map_err(io_error)?;
    let folder = CopilotForkFolder { dir: fork_dir };
    let written = write_private_file(
        &folder.session_log(),
        log.replace(&source_id, fork_id).as_bytes(),
    )
    .and_then(|()| write_private_file(&folder.dir.join("workspace.yaml"), workspace.as_bytes()));
    if let Err(error) = written {
        folder.discard();
        return Err(io_error(error));
    }
    Ok(folder)
}

pub(crate) struct CopilotForkFolder {
    dir: PathBuf,
}

impl CopilotForkFolder {
    /// The fork's `events.jsonl`, recorded on the fork's row as its `agentSessionPath`.
    pub(crate) fn session_log(&self) -> PathBuf {
        self.dir.join("events.jsonl")
    }

    /// Removes the fork's folder, for a fork whose session was never created. The fork created
    /// the folder itself (`create` refuses an existing one), so it holds only the fork's state.
    pub(crate) fn discard(self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// The folder the hook-reported `events.jsonl` sits in, else `session-state/<id>` under Copilot's
/// home, which is where `copilot --resume` looks.
fn copilot_session_dir(
    runtime: &serde_json::Map<String, Value>,
    source_id: &str,
) -> Option<PathBuf> {
    let from_hook = runtime
        .get("agentSessionPath")
        .and_then(Value::as_str)
        .map(Path::new)
        .and_then(Path::parent)
        .filter(|dir| dir.file_name().and_then(|name| name.to_str()) == Some(source_id))
        .map(Path::to_path_buf);
    let from_home =
        crate::session_chat_paths::configured_agent_directory("COPILOT_HOME", ".copilot")
            .join("session-state")
            .join(source_id);
    [from_hook, Some(from_home)]
        .into_iter()
        .flatten()
        .find(|dir| dir.join("events.jsonl").is_file() && dir.join("workspace.yaml").is_file())
}

fn fork_workspace(workspace: &str, source_id: &str, fork_id: &str) -> String {
    let mut lines = Vec::new();
    for line in workspace.lines() {
        let key = line.split(':').next().unwrap_or_default().trim();
        match key {
            "mc_session_id" | "mc_task_id" => continue,
            "fork_count" => lines.push("fork_count: 0".to_string()),
            "name" => {
                let name = line["name:".len()..].trim();
                let name = name.trim_matches('"');
                lines.push(if name.is_empty() || name.starts_with("Fork: ") {
                    line.to_string()
                } else {
                    format!("name: {}", yaml_string(&format!("Fork: {name}")))
                });
            }
            _ => lines.push(line.replace(source_id, fork_id)),
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Copilot fork failed: {error}"))
}
