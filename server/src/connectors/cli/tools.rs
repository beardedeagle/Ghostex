//! The command-line tools a workspace can sign in to on its own (GitHub's `gh` and Google
//! Workspace's `gws`): where each keeps the workspace's sign-in, whether it has one, and the
//! variables that point an agent's copy of the tool at it.
//!
//! CDXC:Connectors 2026-10-16 DECISION:
//! User (Q16, Q17): each workspace gets its own `gh` and `gws` sign-in, handed to the agents that start there and shown on the Connectors page with a Sign in button; `gws` is Google's Workspace CLI. A workspace without its own sign-in leaves the tool as the user set it up, so their usual sign-in applies.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! `gws` keeps its credentials encrypted with a key in the system keychain by default, under one name for every config folder, so a second folder's sign-in replaced the key and the first one's credentials no longer decrypted ("Could not decrypt", seen on this computer). Each workspace's `gws` keeps its key in its own folder (`GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND=file`).

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::super::store::file_stem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliTool {
    Gh,
    Gws,
}

impl CliTool {
    pub(crate) const ALL: [CliTool; 2] = [CliTool::Gh, CliTool::Gws];

    pub(crate) fn parse(text: &str) -> Option<CliTool> {
        match text.trim().to_ascii_lowercase().as_str() {
            "gh" | "github" => Some(CliTool::Gh),
            "gws" | "google" | "google-workspace" => Some(CliTool::Gws),
            _ => None,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            CliTool::Gh => "gh",
            CliTool::Gws => "gws",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            CliTool::Gh => "GitHub CLI (gh)",
            CliTool::Gws => "Google Workspace CLI (gws)",
        }
    }
}

/// `<gxserver state>/connectors/cli/<workspace>/<tool>`: the tool's config folder for the
/// workspace.
pub(crate) fn config_dir(paths: &GxserverPaths, workspace: &str, tool: CliTool) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join("cli")
        .join(file_stem(workspace))
        .join(tool.id())
}

/// The tool on this computer, found the way a program started now would find it.
pub(crate) fn executable(tool: CliTool) -> Option<PathBuf> {
    crate::platform::live_path::find(tool.id(), &[])
}

/// The GitHub account a `gh` config folder is signed in as (`user:` under `github.com:`).
fn gh_account(paths: &GxserverPaths, workspace: &str) -> Option<String> {
    let hosts =
        fs::read_to_string(config_dir(paths, workspace, CliTool::Gh).join("hosts.yml")).ok()?;
    let mut in_github = false;
    for line in hosts.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_github = line.trim_end() == "github.com:";
            continue;
        }
        let trimmed = line.trim();
        if in_github && line.starts_with("    user:") {
            return trimmed
                .strip_prefix("user:")
                .map(str::trim)
                .filter(|user| !user.is_empty())
                .map(str::to_string);
        }
    }
    None
}

fn gws_signed_in(paths: &GxserverPaths, workspace: &str) -> bool {
    let dir = config_dir(paths, workspace, CliTool::Gws);
    dir.join("credentials.enc").is_file() || dir.join("credentials.json").is_file()
}

/// Whether the workspace has its own sign-in for the tool.
pub(crate) fn signed_in(paths: &GxserverPaths, workspace: &str, tool: CliTool) -> bool {
    match tool {
        CliTool::Gh => gh_account(paths, workspace).is_some(),
        CliTool::Gws => gws_signed_in(paths, workspace),
    }
}

/// The workspace's tools as Settings shows them.
pub(crate) fn status(paths: &GxserverPaths, workspace: &str) -> Value {
    let mut tools = serde_json::Map::new();
    for tool in CliTool::ALL {
        let account = match tool {
            CliTool::Gh => gh_account(paths, workspace),
            CliTool::Gws => None,
        };
        tools.insert(
            tool.id().to_string(),
            json!({
                "name": tool.name(),
                "installed": executable(tool).is_some(),
                "signedIn": signed_in(paths, workspace, tool),
                "account": account,
                "signingIn": super::signin::signing_in(paths, workspace, tool),
            }),
        );
    }
    Value::Object(tools)
}

/// The variables that point a tool at the workspace's config folder.
pub(crate) fn tool_env(
    paths: &GxserverPaths,
    workspace: &str,
    tool: CliTool,
) -> Vec<(String, String)> {
    let dir = config_dir(paths, workspace, tool)
        .to_string_lossy()
        .into_owned();
    match tool {
        CliTool::Gh => vec![("GH_CONFIG_DIR".to_string(), dir)],
        CliTool::Gws => vec![
            ("GOOGLE_WORKSPACE_CLI_CONFIG_DIR".to_string(), dir),
            (
                "GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND".to_string(),
                "file".to_string(),
            ),
        ],
    }
}

/// The variables an agent in the workspace gets: one set per tool the workspace signed in to.
pub(crate) fn session_env(paths: &GxserverPaths, workspace: &str) -> Vec<(String, String)> {
    CliTool::ALL
        .into_iter()
        .filter(|tool| signed_in(paths, workspace, *tool))
        .flat_map(|tool| tool_env(paths, workspace, tool))
        .collect()
}
