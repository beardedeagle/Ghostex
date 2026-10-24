use serde_json::{Map, Value};

use crate::ghostex_cli::args::{parse_args, parse_boolean};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{actions, selector, sessions};

use super::*;

pub fn send_message_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let rest = parsed.rest;
    let explicit_selector = selector::session_selector_from_args(&[], &flags).unwrap_or_default();
    let mut selector_text = explicit_selector;
    let mut agent_id: Option<String> = flags.string_value("agent").map(str::to_string);
    let mut text_start_index = 0usize;

    let agent_truthy = agent_id
        .as_deref()
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    if selector_text.is_empty()
        && !agent_truthy
        && rest.first().map(|arg| !arg.is_empty()).unwrap_or(false)
    {
        let first_arg = rest[0].clone();
        let list = sessions::fetch_session_list(&flags, false)?;
        let matches = resolve_listed_sessions(&first_arg, &list, &flags)?;
        if matches.len() > 1 {
            return Err(CliError::Other(format!(
                "Multiple sessions matched \"{first_arg}\":\n{}",
                format_session_matches(&matches)
            )));
        }
        if matches.len() == 1 {
            selector_text = first_arg;
            text_start_index = 1;
        } else {
            agent_id = Some(first_arg);
            text_start_index = 1;
        }
    }

    let text = flags.text("text").unwrap_or_else(|| {
        rest.iter()
            .skip(text_start_index)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    });
    let mut payload = Map::new();
    if let Some(group_id) = flags.0.get("groupId") {
        payload.insert("groupId".to_string(), group_id.as_json());
    }
    if flags.contains("sendDelayMs") {
        payload.insert(
            "sendDelayMs".to_string(),
            json_finite_number(flags.number("sendDelayMs")),
        );
    }
    payload.insert(
        "submit".to_string(),
        Value::Bool(match flags.0.get("submit") {
            None => true,
            Some(value) => parse_boolean(value),
        }),
    );
    payload.insert("text".to_string(), Value::String(text));
    if !selector_text.is_empty() {
        // From inside an agent session the message carries its sender block (agents/outbound.rs).
        if let Some(write) =
            crate::ghostex_cli::agents::outbound::agent_write(&selector_text, &flags)?
        {
            let signed = write.signed(payload["text"].as_str().unwrap_or_default());
            payload.insert("text".to_string(), Value::String(signed));
        }
        let session = selector::resolve_cli_session_selector(&selector_text, &flags)?;
        insert_session_field(&mut payload, "projectId", &session);
        insert_session_field(&mut payload, "sessionId", &session);
    } else if let Some(agent_id) = agent_id {
        payload.insert("agentId".to_string(), Value::String(agent_id));
    }
    let result = actions::send_gxserver_cli_action("sendMessage", &Value::Object(payload), &flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}

/// JS `payload.key = session.key`: present keys copy through (null included),
/// missing keys stay omitted like JSON.stringify dropping undefined.
pub(super) fn insert_session_field(payload: &mut Map<String, Value>, key: &str, session: &Value) {
    if let Some(value) = session.get(key) {
        payload.insert(key.to_string(), value.clone());
    }
}

/// `result.error ?? fallback` with JS string coercion of the error value.
pub(super) fn result_error_message(result: &Value, fallback: impl FnOnce() -> String) -> String {
    match result.get("error") {
        None | Some(Value::Null) => fallback(),
        Some(error) => js_string(error),
    }
}

pub(super) fn limit_text_lines(text: &str, lines: Option<f64>) -> String {
    let Some(lines) = lines else {
        return text.to_string();
    };
    if !lines.is_finite() || lines <= 0.0 {
        return text.to_string();
    }
    let count = lines.trunc() as usize;
    let split: Vec<&str> = text
        .split('\n')
        .map(|part| part.strip_suffix('\r').unwrap_or(part))
        .collect();
    let start = split.len().saturating_sub(count);
    split[start..].join("\n")
}

pub(super) fn json_finite_number(value: Option<f64>) -> Value {
    match value {
        Some(number) if number.is_finite() => {
            if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 {
                Value::from(number as i64)
            } else {
                serde_json::Number::from_f64(number)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        _ => Value::Null,
    }
}
