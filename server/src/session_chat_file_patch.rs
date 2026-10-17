//! What a file edit changed, read from the agent's own record of it.
//!
//! An edit tool's call holds only the text it replaced, so the chat's file change card could not
//! say where in the file the change landed. Claude writes the real patch next to every edit result
//! (`toolUseResult.structuredPatch`, plus `type: "create"` for a new file and the pre-edit
//! `originalFile`); this turns it into the bounded [`SessionChatToolPatch`] the chat draws its diff
//! from, with each hunk headed by the nearest declaration above it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The most diff lines one result carries; a bigger edit keeps its first hunks.
const MAX_PATCH_LINES: usize = 600;
/// A longer line is cut, the way the tool input is bounded.
const MAX_LINE_CHARS: usize = 400;
/// A hunk heading longer than this is cut.
const MAX_HEADING_CHARS: usize = 80;

/// Mirrors `ToolPatch` in `packages/gx-protocol/src/chat.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatToolPatch {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub created: bool,
    #[serde(default)]
    pub hunks: Vec<SessionChatPatchHunk>,
}

/// Mirrors `PatchHunk` in `packages/gx-protocol/src/chat.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChatPatchHunk {
    pub old_start: u32,
    pub new_start: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default)]
    pub lines: Vec<String>,
}

fn clipped(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((at, _)) => format!("{}\u{2026}", &text[..at]),
        None => text.to_string(),
    }
}

/// `git diff`'s default hunk heading: the closest line above the hunk that starts with a letter,
/// `_` or `$` (a declaration, not an indented statement or a closing brace).
fn hunk_heading(original: &[&str], old_start: u32) -> Option<String> {
    let first = usize::try_from(old_start).ok()?.checked_sub(1)?;
    original[..first.min(original.len())]
        .iter()
        .rev()
        .find(|line| {
            line.chars()
                .next()
                .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        })
        .map(|line| clipped(line.trim_end(), MAX_HEADING_CHARS))
}

/// The patch a Claude edit result records, or `None` for a result that is not a file edit.
pub(crate) fn claude_tool_patch(tool_use_result: &Value) -> Option<SessionChatToolPatch> {
    let record = tool_use_result.as_object()?;
    let created = record.get("type").and_then(Value::as_str) == Some("create");
    let hunks = record.get("structuredPatch").and_then(Value::as_array);
    if !created && hunks.is_none_or(|hunks| hunks.is_empty()) {
        return None;
    }
    let original: Vec<&str> = record
        .get("originalFile")
        .and_then(Value::as_str)
        .map(|text| text.split('\n').collect())
        .unwrap_or_default();
    let mut budget = MAX_PATCH_LINES;
    let mut patch = SessionChatToolPatch {
        created,
        hunks: Vec::new(),
    };
    for hunk in hunks.into_iter().flatten() {
        let lines: Vec<String> = hunk
            .get("lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|line| clipped(line, MAX_LINE_CHARS))
            .collect();
        if lines.len() > budget {
            break;
        }
        budget -= lines.len();
        let start = |key: &str| {
            hunk.get(key)
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        };
        let old_start = start("oldStart");
        patch.hunks.push(SessionChatPatchHunk {
            old_start,
            new_start: start("newStart"),
            heading: hunk_heading(&original, old_start),
            lines,
        });
    }
    Some(patch)
}

/// The patch a unified diff describes (`--- a/…`, `+++ b/…`, then `@@ -a,b +c,d @@ heading` hunks),
/// or `None` when the text holds no hunk.
///
/// CDXC:SessionChat 2026-10-17 WHY: an edit Pi's `cursor:local` provider runs is recorded as one generic `cursor` call, with the real patch only as a unified diff in its result (`details.diffString`); read here it gives the chat the same file change card a Claude edit gets instead of a raw `cursor` row (agent flow tests, 2026-10-17).
/// The patch in an OMP edit result's `details.diff`: a unified diff, or omp 18.11's numbered form
/// for a hashline edit, one `-N|text`, `+N|text` or ` N|text` line per changed line, a jump in the
/// numbers starting a new hunk.
pub(crate) fn omp_edit_patch(diff: &str) -> Option<SessionChatToolPatch> {
    if diff.lines().any(|line| line.starts_with("@@ -")) {
        return unified_diff_patch(diff);
    }
    let mut budget = MAX_PATCH_LINES;
    let mut patch = SessionChatToolPatch::default();
    let mut current: Option<SessionChatPatchHunk> = None;
    // The next old and new line numbers the current hunk expects.
    let (mut next_old, mut next_new) = (0u32, 0u32);
    for line in diff.lines() {
        let mut chars = line.chars();
        let Some(sign) = chars.next().filter(|sign| matches!(sign, ' ' | '+' | '-')) else {
            if let Some(hunk) = current.take() {
                patch.hunks.push(hunk);
            }
            continue;
        };
        let Some((number, text)) = chars.as_str().split_once('|') else {
            continue;
        };
        let Ok(number) = number.trim().parse::<u32>() else {
            continue;
        };
        let continues = match sign {
            '-' => number == next_old,
            '+' => number == next_new,
            _ => number == next_old || number == next_new,
        };
        if current.is_none() || !continues {
            if let Some(hunk) = current.take() {
                patch.hunks.push(hunk);
            }
            let (old_start, new_start) = match sign {
                '-' => (number, number),
                '+' => (number.saturating_sub(1).max(1), number),
                _ => (number, number),
            };
            next_old = old_start;
            next_new = new_start;
            current = Some(SessionChatPatchHunk {
                old_start,
                new_start,
                heading: None,
                lines: Vec::new(),
            });
        }
        match sign {
            '-' => next_old = number + 1,
            '+' => next_new = number + 1,
            _ => {
                next_old += 1;
                next_new += 1;
            }
        }
        if budget == 0 {
            break;
        }
        budget -= 1;
        if let Some(hunk) = current.as_mut() {
            hunk.lines
                .push(clipped(&format!("{sign}{text}"), MAX_LINE_CHARS));
        }
    }
    patch.hunks.extend(current);
    (!patch.hunks.is_empty()).then_some(patch)
}

pub(crate) fn unified_diff_patch(diff: &str) -> Option<SessionChatToolPatch> {
    let mut budget = MAX_PATCH_LINES;
    let mut patch = SessionChatToolPatch::default();
    let mut current: Option<SessionChatPatchHunk> = None;
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ -") {
            if let Some(hunk) = current.take() {
                patch.hunks.push(hunk);
            }
            let (ranges, heading) = header.split_once(" @@")?;
            let (old, new) = ranges.split_once(" +")?;
            let start = |range: &str| range.split(',').next()?.parse::<u32>().ok();
            current = Some(SessionChatPatchHunk {
                old_start: start(old)?,
                new_start: start(new)?,
                heading: Some(heading.trim())
                    .filter(|heading| !heading.is_empty())
                    .map(|heading| clipped(heading, MAX_HEADING_CHARS)),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        if !line.starts_with([' ', '+', '-', '\\']) {
            continue;
        }
        if budget == 0 {
            break;
        }
        budget -= 1;
        hunk.lines.push(clipped(line, MAX_LINE_CHARS));
    }
    patch.hunks.extend(current);
    (!patch.hunks.is_empty()).then_some(patch)
}
