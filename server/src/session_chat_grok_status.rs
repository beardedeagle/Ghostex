/*
CDXC:AgentProviders 2026-10-17 WHY:
Grok Build prints its context figure only in the header at the top of its screen (`25K / 256K`), never in the footer the option detection reads, so the chat's context meter said "Not yet reported" for the whole session. Grok 1.0.62 writes the same figures into `signals.json` beside the session's `updates.jsonl` (`contextTokensUsed`, `contextWindowTokens`, `contextWindowUsage` as a whole percentage), rewritten as each turn ends, so the meter reads that file.
SEE-ALSO: packages/gx-chat-core/src/menus/context/ draws the meter from `contextUsage`.
*/

use std::path::Path;

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::session_chat_options::{SessionChatContextUsage, SessionChatDetectedSelection};

/// The context Grok last reported for this session, as a detection layer.
pub fn read_grok_context_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let updates = session
        .get("runtimeSettings")
        .and_then(|runtime| runtime.get("agentSessionPath"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())?;
    let signals = Path::new(updates).parent()?.join("signals.json");
    let text = std::fs::read_to_string(signals).ok()?;
    let context_usage = grok_signals_context_usage(&serde_json::from_str(&text).ok()?)?;
    Some(SessionChatDetectedSelection {
        context_usage: Some(context_usage),
        ..SessionChatDetectedSelection::default()
    })
}

fn grok_signals_context_usage(signals: &Value) -> Option<SessionChatContextUsage> {
    let count = |key: &str| signals.get(key).and_then(Value::as_u64).filter(|n| *n > 0);
    let usage = SessionChatContextUsage {
        used_percentage: signals
            .get("contextWindowUsage")
            .and_then(Value::as_f64)
            .map(|value| value.round().clamp(0.0, 100.0) as u32),
        used_tokens: count("contextTokensUsed"),
        window_size: count("contextWindowTokens"),
    };
    // A session that has not finished a turn reports zeros.
    usage.used_tokens.is_some().then_some(usage)
}
