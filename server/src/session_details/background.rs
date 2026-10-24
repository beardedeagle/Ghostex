//! Stopping a background command a Claude session started.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q11): background tasks get stop buttons like Claude Desktop's, with the finished ones under a fold. Claude has no way to be asked from outside, so the stop ends the command's own processes: the shell under the session's Claude whose output file is the task's (`…/tasks/<task id>.output`, which Claude opens as the shell's output), and everything that shell started. Tested 2026-10-25: Claude then reports "Background command … failed with exit code 143" and takes a short turn to react, as when a command ends by itself. Only processes under the session's own Claude are ever touched.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// The processes below `root`, nearest first.
#[cfg(unix)]
fn descendants(root: i64) -> Result<Vec<i64>, String> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .map_err(|error| format!("The process list couldn't be read: {error}"))?;
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut columns = line.split_whitespace();
        if let (Some(Ok(pid)), Some(Ok(ppid))) = (
            columns.next().map(str::parse::<i64>),
            columns.next().map(str::parse::<i64>),
        ) {
            children.entry(ppid).or_default().push(pid);
        }
    }
    let mut found = Vec::new();
    let mut seen = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(pid) = queue.pop_front() {
        for child in children.get(&pid).into_iter().flatten() {
            if seen.insert(*child) {
                found.push(*child);
                queue.push_back(*child);
            }
        }
    }
    Ok(found)
}

fn is_task_output(path: &Path, task_id: &str) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(format!("{task_id}.output").as_str())
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            == Some("tasks")
}

/// Ends background command `task_id` of the Claude running in zmx session `zmx_name`.
#[cfg(unix)]
pub(super) fn stop_background_task(
    home: &Path,
    zmx_name: &str,
    task_id: &str,
) -> Result<(), String> {
    let identities = crate::zmx::read_zmx_session_process_identities(&[zmx_name.to_string()], home)
        .map_err(|_| "The session's agent couldn't be found.".to_string())?;
    let agent = identities
        .get(zmx_name)
        .and_then(|identity| identity.process_id)
        .ok_or_else(|| "The session's agent isn't running.".to_string())?;
    let below = descendants(agent)?;
    let holders: Vec<i64> = below
        .iter()
        .copied()
        .filter(|pid| {
            crate::zmx::process_open_file_paths(*pid)
                .iter()
                .any(|path| is_task_output(path, task_id))
        })
        .collect();
    if holders.is_empty() {
        return Err("That command has already ended.".to_string());
    }
    // The shell and everything it started; the shell first so it cannot start anything new.
    let mut targets: Vec<i64> = Vec::new();
    for holder in &holders {
        if !targets.contains(holder) {
            targets.push(*holder);
        }
        for pid in descendants(*holder)? {
            if !targets.contains(&pid) {
                targets.push(pid);
            }
        }
    }
    for pid in targets {
        if let Ok(pid) = i32::try_from(pid) {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn stop_background_task(
    _home: &Path,
    _zmx_name: &str,
    _task_id: &str,
) -> Result<(), String> {
    Err("Stopping a background command isn't available on Windows yet.".to_string())
}
