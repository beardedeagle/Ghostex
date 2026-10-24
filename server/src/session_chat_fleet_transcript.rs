//! CDXC:SessionStatus 2026-09-10 WHY:
//! Subagent liveness belongs to the latest persisted child turn, including turns resumed after an earlier completion. Read complete records from the tail and cache by file identity/version; a partial final write is not a new lifecycle event.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{
    read_exact_at, read_transcript_file_version, SessionChatTranscriptAgent,
    SessionChatTurnLifecycleState, TranscriptFileVersion, TAIL_CHUNK_BYTES,
};
use crate::session_chat_tail::{find_last_complete_line_end, TailLineAccumulator};

#[derive(Clone, Debug)]
pub(crate) struct ChildTurn {
    pub working: bool,
    pub timestamp: i64,
    pub started_at: i64,
}

pub(crate) fn timestamp(record: &Value) -> Option<i64> {
    record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_time)
}

pub(crate) fn parse_time(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// Returning false stops the scan. The first record is visited too, even in a one-line file.
pub(crate) fn scan_tail(
    path: &Path,
    visit: impl FnMut(&str) -> io::Result<bool>,
) -> io::Result<()> {
    let file = File::open(path)?;
    let end = find_last_complete_line_end(&file, file.metadata()?.len())?;
    scan_backward(&file, 0, end, visit).map(|_| ())
}

/// Visits the complete records in `floor..end`, newest first, where `floor` is the start of a line
/// and `end` the end of one. Returns true when `visit` stopped the scan before `floor`.
fn scan_backward(
    file: &File,
    floor: u64,
    end: u64,
    mut visit: impl FnMut(&str) -> io::Result<bool>,
) -> io::Result<bool> {
    let mut cursor = end.saturating_sub(1).max(floor);
    let mut line = TailLineAccumulator::new();
    let mut oversized = 0;
    let mut buffer = vec![0; TAIL_CHUNK_BYTES];
    while cursor > floor {
        let start = cursor.saturating_sub(TAIL_CHUNK_BYTES as u64).max(floor);
        let length = (cursor - start) as usize;
        read_exact_at(file, &mut buffer[..length], start)?;
        let mut segment_end = length;
        for index in (0..length).rev() {
            if buffer[index] != b'\n' {
                continue;
            }
            line.retain_part(&buffer[index + 1..segment_end], &mut oversized);
            if line.oversized {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Subagent lifecycle record is too large",
                ));
            }
            if let Some(record) = line.take_line() {
                if !visit(&record)? {
                    return Ok(true);
                }
            }
            segment_end = index;
        }
        line.retain_part(&buffer[..segment_end], &mut oversized);
        cursor = start;
    }
    if line.oversized {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Subagent lifecycle record is too large",
        ));
    }
    if let Some(record) = line.take_line() {
        if !visit(&record)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// CDXC:SessionStatus 2026-10-19 WHY:
/// Claude can write a child's last answer with `stop_reason: null` (the block lands before the stop reason does), and a null reason is also how text streams before a tool call, so the record alone cannot end the turn. The scan then walked back to an earlier tool call and kept a finished teammate "running" until the session was reloaded (GitHub #259). The `SubagentStop` and `TeammateIdle` hooks Ghostex installs fire only once the child has stopped, so their records end the turn; a hook that blocked the stop is followed by new turn records, which the backward scan meets first.
fn child_stop_hook(record: &Value) -> bool {
    let Some(attachment) = record.get("attachment") else {
        return false;
    };
    matches!(
        attachment.get("hookEvent").and_then(Value::as_str),
        Some("SubagentStop" | "TeammateIdle")
    ) && attachment.get("type").and_then(Value::as_str) != Some("hook_blocking_error")
}

/// One child transcript's latest turn as of `end`, the end of the last complete record read.
#[derive(Clone)]
struct CachedChildTurn {
    version: TranscriptFileVersion,
    end: u64,
    /// A hash of the bytes just before `end`, so a rewritten file is read again from the start.
    guard: u64,
    turn: Option<ChildTurn>,
}

type Cache = HashMap<PathBuf, CachedChildTurn>;

/// How many bytes before a cached scan's end must be unchanged for the scan to resume there.
const RESUME_GUARD_BYTES: u64 = 256;

fn resume_guard(file: &File, end: u64) -> io::Result<u64> {
    let start = end.saturating_sub(RESUME_GUARD_BYTES);
    let mut bytes = vec![0; (end - start) as usize];
    read_exact_at(file, &mut bytes, start)?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(hasher.finish())
}

/// The backward scan's step for one record; false ends the scan.
fn visit_child_record(
    line: &str,
    agent: SessionChatTranscriptAgent,
    turn: &mut Option<ChildTurn>,
) -> io::Result<bool> {
    let record: Value =
        serde_json::from_str(line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let Some(at) = timestamp(&record) else {
        return Ok(true);
    };
    if agent == SessionChatTranscriptAgent::Codex {
        if let Some(event) = crate::session_chat_decode_codex::decode_codex_turn_lifecycle(line, "")
        {
            *turn = Some(ChildTurn {
                working: event.state == SessionChatTurnLifecycleState::Working,
                timestamp: at,
                started_at: at,
            });
            return Ok(false);
        }
        return Ok(true);
    }
    let event = crate::session_chat_decode_claude::decode_claude_turn_lifecycle(line, "");
    if child_stop_hook(&record)
        || event
            .as_ref()
            .is_some_and(|event| event.state != SessionChatTurnLifecycleState::Working)
    {
        if turn.is_none() {
            *turn = Some(ChildTurn {
                working: false,
                timestamp: at,
                started_at: at,
            });
        }
        return Ok(false);
    }
    let kind = record.get("type").and_then(Value::as_str);
    let blocks = record.pointer("/message/content");
    let child_message = |text: &str| {
        text.trim_start().starts_with("<agent-message")
            || text.trim_start().starts_with("<teammate-message")
    };
    let resumes_child = blocks.and_then(Value::as_str).is_some_and(child_message)
        || blocks.and_then(Value::as_array).is_some_and(|blocks| {
            blocks.iter().any(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    && block
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(child_message)
            })
        });
    // Compaction summaries and completion notifications are not new child prompts.
    let user_prompt = kind == Some("user")
        && (resumes_child
            || event.is_some_and(|event| event.state == SessionChatTurnLifecycleState::Working));
    if kind == Some("assistant") || user_prompt {
        let latest = turn.get_or_insert(ChildTurn {
            working: true,
            timestamp: at,
            started_at: at,
        });
        latest.started_at = at;
        if user_prompt {
            return Ok(false);
        }
    }
    Ok(true)
}

/// CDXC:SessionStatus 2026-10-23 WHY:
/// A working child's transcript changes between every fleet read (the 5-second status pass and every chat screen detection), and the backward scan then re-parsed its whole current turn each time, the largest steady cost in gxserver while subagents ran. A changed file of the same identity whose bytes before the last scanned end are unchanged (Claude and Codex only append) is now scanned from its end back to that point only. When the new records settle the answer (a stop, a finished event, a new prompt) it is theirs; when they hold only more of the turn, the turn's start is the cached one's, which is exactly where the full scan would have stopped. Any other change reads the file again from the end to the start.
pub(crate) fn latest_child_turn(
    path: &Path,
    agent: SessionChatTranscriptAgent,
) -> io::Result<Option<ChildTurn>> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    let previous = cache.lock().ok().and_then(|cache| cache.get(path).cloned());
    if let Some(previous) = previous.as_ref().filter(|old| old.version == version) {
        return Ok(previous.turn.clone());
    }
    let file = File::open(path)?;
    let end = find_last_complete_line_end(&file, file.metadata()?.len())?;
    let resume = match previous {
        Some(previous)
            if previous.version.identity == version.identity
                && previous.end <= end
                && resume_guard(&file, previous.end)? == previous.guard =>
        {
            Some(previous)
        }
        _ => None,
    };
    let floor = resume.as_ref().map_or(0, |previous| previous.end);
    let mut turn: Option<ChildTurn> = None;
    let stopped = scan_backward(&file, floor, end, |line| {
        visit_child_record(line, agent, &mut turn)
    })?;
    let turn = match resume {
        Some(previous) if !stopped => match (turn, previous.turn) {
            // Nothing decisive was appended: the older records decide, as before.
            (None, older) => older,
            // More of a turn that was already working when last read: it started where that one did.
            (Some(mut newer), Some(older)) if older.working => {
                newer.started_at = older.started_at;
                Some(newer)
            }
            (newer, _) => newer,
        },
        _ => turn,
    };
    let guard = resume_guard(&file, end)?;
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(
            path.to_path_buf(),
            CachedChildTurn {
                version,
                end,
                guard,
                turn: turn.clone(),
            },
        );
    }
    Ok(turn)
}
