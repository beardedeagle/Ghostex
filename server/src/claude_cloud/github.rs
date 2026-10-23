//! A cloud session's branch and pull request on GitHub: the branch Ghostex names in the task, and
//! what `gh` says about it (the row's status when the claude.ai page cannot be read).

use serde_json::Value;

use crate::session_git_status::run_gh_command;

/// `claude/<task slug, at most 24>-<6 lowercase hex>`: the branch a new cloud session is told to
/// push to, so Ghostex knows where to look for its commits and pull request.
///
/// CDXC:ClaudeCloud 2026-10-23 WHY: claude.ai names a cloud session's branch itself and nothing reports that name back to the CLI, so without a branch named in the task Ghostex could not find the session's work on GitHub. The `claude/` prefix matches the branches Claude's own cloud sessions push.
pub(crate) fn new_cloud_branch(task: &str) -> String {
    let mut slug = String::new();
    for character in task.chars() {
        let character = character.to_ascii_lowercase();
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "task" } else { slug };
    let tail: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(6)
        .collect();
    format!("claude/{slug}-{tail}")
}

/// The task's line about the branch, added unless the task already names it.
pub(crate) fn task_with_branch(task: &str, branch: &str) -> String {
    if task.contains(branch) {
        return task.to_string();
    }
    format!(
        "{}\n\nCreate the branch `{branch}` from the default branch, work on it and push your commits there.",
        task.trim_end()
    )
}

/// What GitHub shows for a cloud session's branch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BranchReport {
    /// The branch's head commit, when the branch exists on GitHub.
    pub(crate) head: Option<String>,
    /// Commits on the branch that the default branch does not have.
    pub(crate) ahead_by: Option<u64>,
    /// The newest pull request from the branch: number, URL and state (`OPEN`, `MERGED`, `CLOSED`).
    pub(crate) pull_request: Option<(u64, String, String)>,
}

/// Reads the branch and its pull request with `gh` (blocking). `None` when `gh` is missing, signed
/// out or GitHub did not answer, so the caller keeps what it had.
pub(crate) fn read_branch(repo: &str, branch: &str) -> Option<BranchReport> {
    let head = run_gh_command(
        None,
        &[
            "api",
            &format!("repos/{repo}/branches/{branch}"),
            "--jq",
            ".commit.sha",
        ],
    )
    .map(|sha| sha.trim().to_string())
    .filter(|sha| sha.len() >= 7);
    let pulls = run_gh_command(
        None,
        &[
            "pr",
            "list",
            "--repo",
            repo,
            "--head",
            branch,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,url,state",
        ],
    )?;
    let pull_request = serde_json::from_str::<Value>(pulls.trim())
        .ok()
        .and_then(|value| value.as_array()?.first().cloned())
        .and_then(|pull| {
            Some((
                pull.get("number")?.as_u64()?,
                pull.get("url")?.as_str()?.to_string(),
                pull.get("state")?.as_str()?.to_string(),
            ))
        });
    let ahead_by = head.as_ref().and_then(|_| {
        let default_branch = run_gh_command(
            None,
            &["api", &format!("repos/{repo}"), "--jq", ".default_branch"],
        )?;
        run_gh_command(
            None,
            &[
                "api",
                &format!("repos/{repo}/compare/{}...{branch}", default_branch.trim()),
                "--jq",
                ".ahead_by",
            ],
        )?
        .trim()
        .parse()
        .ok()
    });
    Some(BranchReport {
        head,
        ahead_by,
        pull_request,
    })
}

/// The row's status line for a report, or `None` while the branch has no work yet.
pub(crate) fn status_line(branch: &str, report: &BranchReport) -> Option<String> {
    if let Some((number, _, state)) = &report.pull_request {
        return Some(match state.as_str() {
            "MERGED" => format!("Pull request #{number} merged"),
            "CLOSED" => format!("Pull request #{number} closed"),
            _ => format!("Pull request #{number} opened"),
        });
    }
    report.head.as_ref()?;
    Some(match report.ahead_by {
        Some(1) => format!("Pushed 1 commit to {branch}"),
        Some(count) if count > 1 => format!("Pushed {count} commits to {branch}"),
        _ => format!("Pushed to {branch}"),
    })
}
