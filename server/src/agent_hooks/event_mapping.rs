use std::env;

use serde_json::{json, Map, Value};

use super::notify_runtime::read_state_string;
use super::probing::{normalize_prompt_text, now_iso};

pub(crate) fn nested_get<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    Some(current)
}

pub(crate) fn first_string<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(normalize_prompt_text)
            .filter(|text| !text.is_empty())
        {
            return Some(text);
        }
    }
    None
}

pub(crate) fn first_path<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

pub(crate) fn env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| normalize_prompt_text(&value))
        .filter(|value| !value.is_empty())
}

pub(crate) fn normalized_hook_agent_key(value: &str) -> String {
    let normalized = normalize_prompt_text(&value.to_ascii_lowercase());
    let mapped = match normalized.as_str() {
        "claude" | "claude code" => "claude",
        "openclaude" | "open claude" | "openclaude cli" => "openclaude",
        "command-code" | "commandcode" | "command code" => "command-code",
        "codex" | "openai codex" | "codex cli" => "codex",
        "kimi" | "kimi code" => "kimi",
        "mastra" | "mastracode" | "mastra code" => "mastra",
        "devin" => "devin",
        "pi" | "π" => "pi",
        "zcode" | "zcode-cli" => "zcode",
        "omp" => "omp",
        "opencode" | "open code" => "opencode",
        "grok" | "grok build" => "grok",
        "amp" | "amp cli" => "amp",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "gemini" | "gemini cli" => "gemini",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "copilot" | "github copilot" => "copilot",
        "codebuddy" | "code buddy" => "codebuddy",
        "droid" | "factory" | "factory droid" => "droid",
        "kiro" | "kiro-cli" | "kiro cli" => "kiro",
        "qoder" | "qodercli" => "qoder",
        "empryo" | "em" => "empryo",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "codex".to_string()
    } else {
        cleaned
    }
}

pub(crate) fn activity_for_hook_event(
    agent_key: &str,
    event_name: &str,
    payload: &Value,
) -> Option<String> {
    if agent_key == "mastra" {
        return mastra_hook_activity(event_name, payload);
    }
    let normalized_event_name = normalize_prompt_text(event_name);
    let lower = normalized_event_name.to_ascii_lowercase();
    let compact = lower.replace(['_', '-', '.'], "");
    /*
    CDXC:AgentHooks 2026-08-27:
    Subagent and teammate lifecycle events describe a CHILD of the session, not
    the lead pane, so they must never move the lead session's activity. They are
    roster-only. Returning early — before any agent-specific or generic
    matching — keeps a future generic rule from accidentally claiming them
    (today's compact matching is exact-string, so "subagentstop" would not hit
    the "stop" arm, but the intent should not depend on that).
    */
    if matches!(
        compact.as_str(),
        "subagentstart" | "subagentstop" | "teammateidle"
    ) {
        return None;
    }
    /*
    PreCompact (registered by Copilot) fires before the compaction is validated
    and an aborted compact emits it alone, so it carries no usable activity
    signal.
    */
    if compact == "precompact" {
        return None;
    }
    /*
    Copilot's ErrorOccurred ends the turn unless the runtime says it recovered
    and kept going.
    */
    if compact == "erroroccurred" {
        let recoverable = payload_boolean(
            payload,
            &[
                "recoverable",
                "metadata.recoverable",
                "properties.recoverable",
            ],
        );
        return Some(
            if recoverable == Some(true) {
                "working"
            } else {
                "idle"
            }
            .to_string(),
        );
    }
    if agent_key == "codex" {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "interrupt" | "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    /*
    CDXC:Notifications 2026-10-05 WHY:
    Ghostex's Pi and OMP extensions send Stop only for a turn the agent finished on its own: once `agent_settled` says no retry or queued follow-up continues it, and never for an Esc (Interrupt) or a provider failure (StopFailure). Its Amp plugin does the same from `agent.end`'s status (`done`, `cancelled`, `error`). That makes their Stop the same completed-turn boundary as Claude's and Codex's, so a finished Pi, OMP or Amp turn enters attention and rings instead of settling silently to idle.
    SEE-ALSO: build_pi_extension_source, build_omp_extension_source and build_amp_plugin_source in server/src/agent_hooks/plugin_sources.rs, normalize_agent_hook_activity in server/src/agents/activity.rs.
    */
    if matches!(agent_key, "pi" | "omp" | "amp") && lower == "stop" {
        return Some("attention".to_string());
    }
    // OpenClaude emits Claude's hook contract verbatim, and Empryo runs Claude-format hooks
    // (CDXC:AgentHooks 2026-10-06 in config.rs), so both share every Claude-specific rule below
    // instead of falling through to the generic tables (which have no PostCompact trigger check
    // and no StopFailure arm).
    if matches!(agent_key, "claude" | "openclaude" | "empryo") {
        /*
        CDXC:Notifications 2026-09-15 DECISION:
        User: suppress completion attention while Claude reports background work remaining; notify when Claude finishes after that work completes, and preserve question/permission alerts.
        This narrows the 2026-09-04 decision to ring on every finished Claude turn: Stop also fires after progress updates while background agents and monitors continue.
        Claude's background_tasks array contains in-flight tasks; an empty or absent array retains ordinary completion attention.
        SEE-ALSO: normalize_agent_hook_event_activity in server/src/agents/activity.rs and background_tasks forwarding in server/src/agent_hooks/notify_runtime.rs.
        */
        if lower == "stop" {
            return Some(
                if payload
                    .get("background_tasks")
                    .and_then(Value::as_array)
                    .is_some_and(|tasks| !tasks.is_empty())
                {
                    "working"
                } else {
                    "attention"
                }
                .to_string(),
            );
        }
        /*
        CDXC:AgentHooks 2026-08-27:
        Claude skips Stop after a model error and emits StopFailure instead, so
        without this arm the pane spins "working" forever on every failed turn.
        */
        if matches!(lower.as_str(), "stopfailure" | "idle" | "sessionend") {
            return Some("idle".to_string());
        }
        /*
        A MANUAL /compact ends at an idle input prompt with no Stop behind it,
        so it is a real turn boundary. An AUTO-compact fires mid-turn; mapping
        that to idle would blip a working session, so it stays unmapped.
        */
        if lower == "postcompact" {
            return payload_compact_trigger_is_manual(payload).then(|| "idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit"
                | "prompt-submit"
                | "pretooluse"
                | "pre-tool-use"
                | "posttooluse"
                | "posttoolusefailure"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(agent_key, "copilot" | "codebuddy" | "droid" | "qoder") {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if agent_key == "antigravity" {
        let fully_idle = payload_boolean(
            payload,
            &[
                "fullyIdle",
                "fully_idle",
                "metadata.fullyIdle",
                "properties.fullyIdle",
            ],
        );
        if fully_idle == Some(false)
            && matches!(lower.as_str(), "stop" | "turn-completion" | "notification")
        {
            return Some("working".to_string());
        }
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "postinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    /*
    CDXC:AgentHooks 2026-09-28 WHY:
    SessionStart (Kiro: agentSpawn) fires while the CLI waits at its input prompt: startup, resume, a new or cleared conversation, and it is the only hook Claude fires when /clear finishes. Most agents settled idle on it only through the notify hook's stateless default status, which also invented idle for every mid-turn event nothing maps, so the rule is explicit now. Two exceptions stay unmapped: OpenCode's TUI plugin reports SessionStart whenever the TUI opens another conversation, which may be mid-turn, and Claude's compaction SessionStart (source compact) also fires mid-turn after an auto-compact, while a manual /compact settles through PostCompact. gxserver's table leaves SessionStart to the status posted here, because the Claude rule needs the payload.
    */
    if compact == "agentspawn"
        || (compact == "sessionstart"
            && agent_key != "opencode"
            && !(matches!(agent_key, "claude" | "openclaude")
                && payload.get("source").and_then(Value::as_str) == Some("compact")))
    {
        return Some("idle".to_string());
    }
    /*
    CDXC:AgentHooks 2026-09-28 WHY:
    Hermes fires post_tool_call after every tool, mid-turn; only post_llm_call and on_session_end end its turn. Left unmapped, the hook posted its stateless default status (idle) and gxserver applied it, so each finished tool dropped the session to idle until the next tool began, and a long execute_code or the final reply showed no dot (observed 2026-09-28 in the Dobby bot, session G9eas). Its clarify tool asks the user and waits in the terminal, like an approval, so that pre_tool_call needs the user.
    */
    if compact == "pretoolcall"
        && first_string([payload.get("tool_name"), payload.get("toolName")])
            .is_some_and(|tool| crate::session_chat_interactive::is_ask_user_question_tool(&tool))
    {
        return Some("attention".to_string());
    }
    if matches!(
        compact.as_str(),
        "agentstart"
            | "aftertool"
            | "beforeagentstart"
            | "beforeagent"
            | "beforemcpexecution"
            | "beforeshellexecution"
            | "beforesubmitprompt"
            | "beforetool"
            | "messagepart"
            | "onsessionreset"
            | "onsessionstart"
            | "ontoolpermission"
            | "postapprovalresponse"
            // Devin's post-compaction event fires mid-turn, so the turn is
            // still running (unlike Claude's manual PostCompact).
            | "postcompaction"
            | "posttoolcall"
            | "posttooluse"
            | "posttoolusefailure"
            | "prellmcall"
            | "pretoolcall"
            | "preinvocation"
            | "postinvocation"
            | "pretooluse"
            | "promptsubmit"
            | "sessionbusy"
            | "userpromptsubmit"
    ) {
        return Some("working".to_string());
    }
    if matches!(
        compact.as_str(),
        "askuserquestion" | "notification" | "notify" | "permissionrequest" | "preapprovalrequest"
    ) {
        return Some("attention".to_string());
    }
    if matches!(
        compact.as_str(),
        "afteragent"
            | "afteragentresponse"
            | "agentend"
            | "agentresponse"
            | "oncomplete"
            | "onerror"
            | "onsessionend"
            | "onsessionfinalize"
            | "postllmcall"
            | "release"
            | "interrupt"
            | "sessionend"
            | "sessionidle"
            | "sessionshutdown"
            | "stop"
            | "stopfailure"
            | "turncompletion"
    ) {
        return Some("idle".to_string());
    }
    None
}

/*
CDXC:AgentHooks 2026-08-27:
Claude's PostCompact payload distinguishes a user-run /compact from an
auto-compact through a top-level `trigger` field. Providers that wrap hook
payloads repeat it one level down, so accept the common wrappers too.
*/
fn payload_compact_trigger_is_manual(payload: &Value) -> bool {
    first_string([
        payload.get("trigger"),
        nested_get(payload, &["payload", "trigger"]),
        nested_get(payload, &["metadata", "trigger"]),
        nested_get(payload, &["properties", "trigger"]),
    ])
    .is_some_and(|trigger| trigger.eq_ignore_ascii_case("manual"))
}

/*
CDXC:SessionChat 2026-08-24:
Claude Code sends two very different things through the same Notification hook:
permission requests ("Claude needs your permission to use …"), which are real
attention — a prompt delivered now would be swallowed as the ANSWER — and the
60-second idle reminder ("Claude is waiting for your input"), which means the
input line is empty and waiting. Treating the reminder as attention permanently
blockaded the prompt-queue scheduler whenever a session was stuck "working"
(e.g. after a local command that never fires Stop). This predicate identifies
the reminder so both mapping layers can refuse to escalate a stuck session on
it; genuine permission notifications keep their attention transition.
*/
pub(crate) fn claude_notification_is_idle_input(payload: &Value) -> bool {
    first_string([payload.get("message")])
        .map(|message| {
            message
                .to_ascii_lowercase()
                .contains("waiting for your input")
        })
        .unwrap_or(false)
}

fn payload_boolean(payload: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        let value = if key.contains('.') {
            nested_get(payload, &key.split('.').collect::<Vec<_>>())
        } else {
            payload.get(*key)
        };
        match value {
            Some(Value::Bool(value)) => return Some(*value),
            Some(Value::String(value)) if matches!(value.as_str(), "true" | "1") => {
                return Some(true)
            }
            Some(Value::String(value)) if matches!(value.as_str(), "false" | "0") => {
                return Some(false)
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn update_hook_status(state: &mut Map<String, Value>, status: &str) {
    let timestamp = now_iso();
    state.insert("status".to_string(), json!(status));
    state.insert("statusUpdatedAt".to_string(), json!(timestamp.clone()));
    state.insert("lastActivityAt".to_string(), json!(timestamp.clone()));
    if status == "attention" {
        state.insert(
            "attentionEventId".to_string(),
            json!(format!("{timestamp}:attention")),
        );
        state.insert("attentionAcknowledgedAt".to_string(), json!(""));
        state.insert("attentionAcknowledgedEventId".to_string(), json!(""));
    } else if status == "working" {
        state.insert("attentionAcknowledgedAt".to_string(), json!(timestamp));
        let event_id = read_state_string(state, "attentionEventId").unwrap_or_default();
        state.insert("attentionAcknowledgedEventId".to_string(), json!(event_id));
    }
}

pub(crate) fn is_prompt_event(event_name: &str) -> bool {
    let lower = event_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "userpromptsubmit"
            | "beforeagent"
            | "preinvocation"
            | "pretooluse"
            | "beforesubmitprompt"
            | "beforeshellexecution"
            | "pre_llm_call"
            | "pre_tool_call"
            | "on_tool_permission"
            | "agent_start"
            | "agent.start"
            | "before_agent_start"
    )
}

/// CDXC:AgentHooks 2026-09-05 WHY:
/// Mastra uses Pi's TUI components but its own root-level JSON hooks and user_message payload.
/// AgentEnd preserves suspended/aborted/error reasons, while Mastra 0.35's Stop reports suspended runs as complete, so Stop is deliberately not installed.
/// The agent_done Notification duplicates AgentEnd and must not ring a second time; child events must not move the lead session.
/// SEE-ALSO: server/src/agents/activity.rs and server/src/agent_hooks/notify_runtime.rs.
fn mastra_hook_activity(event_name: &str, payload: &Value) -> Option<String> {
    let activity = match event_name.to_ascii_lowercase().as_str() {
        "sessionstart" | "sessionend" | "interrupt" => "idle",
        "userpromptsubmit" | "agentstart" | "pretooluse" | "posttooluse" => "working",
        "permissionrequest" => "attention",
        "permissionresult" => match payload.get("decision").and_then(Value::as_str) {
            Some("dismissed") => "idle",
            Some("approved" | "auto_approved" | "declined") => "working",
            _ => return None,
        },
        "agentend" => match payload.get("stop_reason").and_then(Value::as_str) {
            Some("complete" | "suspended") => "attention",
            Some("aborted" | "error") => "idle",
            _ => return None,
        },
        "notification" if payload.get("reason").and_then(Value::as_str) != Some("agent_done") => {
            "attention"
        }
        _ => return None,
    };
    Some(activity.to_string())
}
