//! The one-line labels simple mode puts on a folded group.
//!
//! Ported from `packages/shared/session-chat-presentation/simple.ts`.

/// "Edited 3 files", the label a folded turn's file changes carry.
pub fn simple_edit_label(count: usize) -> String {
    format!(
        "Edited {count} {}",
        if count == 1 { "file" } else { "files" }
    )
}

/// CDXC:SessionChat 2026-10-17 WHY:
/// A Codex edit the user denied ("aborted by user", "patch rejected by user") still carries its patch, so the card counted it and read "Edited 1 file" while the file never changed. Only files with at least one applied change count as edited; files whose every change failed or was denied are counted apart, so the label says so ("Failed to edit 1 file", "Edited 2 files, 1 failed") and the row keeps its "Failed" tag and the agent's reason.
pub fn edit_label(edited: usize, failed: usize) -> String {
    let files = |count: usize| if count == 1 { "file" } else { "files" };
    match (edited, failed) {
        (_, 0) => simple_edit_label(edited),
        (0, _) => format!("Failed to edit {failed} {}", files(failed)),
        _ => format!("Edited {edited} {}, {failed} failed", files(edited)),
    }
}

/// The paths a list of file rows edited, and the paths whose every change failed.
pub fn edited_and_failed_paths<'a>(
    rows: impl IntoIterator<Item = (&'a str, bool)>,
) -> (Vec<&'a str>, Vec<&'a str>) {
    let rows: Vec<(&str, bool)> = rows.into_iter().collect();
    let mut edited: Vec<&str> = Vec::new();
    for (path, failed) in &rows {
        if !failed && !edited.contains(path) {
            edited.push(path);
        }
    }
    let mut failed_only: Vec<&str> = Vec::new();
    for (path, failed) in &rows {
        if *failed && !edited.contains(path) && !failed_only.contains(path) {
            failed_only.push(path);
        }
    }
    (edited, failed_only)
}

/// "3 tool calls", or "Tool output" when the group has no call of its own.
pub fn tool_count_label(count: usize) -> String {
    if count == 0 {
        "Tool output".to_string()
    } else {
        format!("{count} tool {}", if count == 1 { "call" } else { "calls" })
    }
}
