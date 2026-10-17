/*
CDXC:AgentProviders 2026-10-17 WHY:
Antigravity CLI's footer prints no context figure, so the chat's context meter said "Not yet reported" for the whole session. agy 1.4.0 writes every model step's token counts into its plaintext step log (`brain/<conversation>/.system_generated/logs/transcript_full.jsonl`, `input_tokens`, `cache_read_tokens`, `output_tokens` on each `PLANNER_RESPONSE`), so the meter shows the context the newest step filled: its uncached and cached input plus its output. agy's `statusLine` command payload also names the window size, but registering a command there replaces agy's own footer, which is where Ghostex reads the model, effort and mode; and neither `agy models` nor anything on disk names a model's window, so the meter shows tokens without a percentage.
SEE-ALSO: server/src/session_chat_antigravity_mirror.rs owns the step log's layout; packages/gx-chat-core/src/menus/context/ draws the meter from `contextUsage`.
*/

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::session_chat_antigravity_mirror::find_antigravity_transcript;
use crate::session_chat_options::{SessionChatContextUsage, SessionChatDetectedSelection};

/// One step log's reading, kept until the file changes: detection reads it about once a second
/// while a session works, and agy appends only when a step ends.
struct CachedTokens {
    len: u64,
    modified: Option<SystemTime>,
    tokens: Option<u64>,
}

static TOKENS_CACHE: Mutex<Option<HashMap<PathBuf, CachedTokens>>> = Mutex::new(None);

/// The context the session's newest model step filled, as a detection layer.
pub fn read_antigravity_context_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let conversation_id = session
        .get("runtimeSettings")
        .and_then(|runtime| runtime.get("agentSessionId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let path = find_antigravity_transcript(conversation_id)?;
    let tokens = newest_step_context_tokens(&path)?;
    Some(SessionChatDetectedSelection {
        context_usage: Some(SessionChatContextUsage {
            used_percentage: None,
            used_tokens: Some(tokens),
            window_size: None,
        }),
        ..SessionChatDetectedSelection::default()
    })
}

fn newest_step_context_tokens(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    if let Ok(cache) = TOKENS_CACHE.lock() {
        if let Some(cached) = cache.as_ref().and_then(|cache| cache.get(path)) {
            if cached.len == len && cached.modified == modified {
                return cached.tokens;
            }
        }
    }
    let tokens = scan_step_log(path);
    if let Ok(mut cache) = TOKENS_CACHE.lock() {
        cache.get_or_insert_with(HashMap::new).insert(
            path.to_path_buf(),
            CachedTokens {
                len,
                modified,
                tokens,
            },
        );
    }
    tokens
}

fn scan_step_log(path: &Path) -> Option<u64> {
    let file = std::fs::File::open(path).ok()?;
    let mut newest = None;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        // Only model steps carry counts; the rest are skipped unparsed.
        if !line.contains("\"input_tokens\"") {
            continue;
        }
        let Ok(step) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let count = |key: &str| step.get(key).and_then(Value::as_u64).unwrap_or(0);
        let tokens = count("input_tokens") + count("cache_read_tokens") + count("output_tokens");
        if tokens > 0 {
            newest = Some(tokens);
        }
    }
    newest
}
