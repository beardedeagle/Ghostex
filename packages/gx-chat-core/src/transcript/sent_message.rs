//! A message the session's own agent sent another agent: `ghostex agents send` (or `agents create
//! --task`) run through its shell, or Claude's own `SendMessage` tool.
//!
//! CDXC:SessionChat 2026-09-30 DECISION:
//! User: "when the current agent chat that i'm in sends a message to another agent can we add a small card that shows the message being sent when expanded? i want it collapsed by default", looking like the card a received message gets. The send's tool row turns into that card the way an answered question does: a run standing alone draws the card instead of the row, a heading's disclosure or a turn's work fold keeps the row, and a finished turn lifts its cards out of the "Worked for" fold.
//! SEE-ALSO: agent_message.rs parses the received side; sent_message_script.rs reads the message out of variables and decides NOT SENT; server/src/ghostex_cli/agents/delivery.rs prints the result JSON read here; apps/desktop/src/app/native_chat/inter_agent_message.rs and apps/mobile/app/src/chat/native/transcript/SystemRows.tsx draw both cards.

use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage};
use serde_json::{json, Value};

use crate::document::TranscriptItem;
use crate::event::KnownSession;
use crate::transcript::agent_message_card::{
    agent_display_name, dress_received, dress_sent, subagent_label,
};
use crate::transcript::jsstr::{ascii_lower, js_trim, last_path_segment};
use crate::transcript::line_breaks::{agent_line_breaks, AgentLineBreaks};
use crate::transcript::markdown_links::markdown_references;
use crate::transcript::native_markdown::native_markdown;
use crate::transcript::sent_message_script::{send_error_line, Resolved, Script, SCRIPTED_BODY};
use crate::transcript::shell_script::ShellCommand;
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_rows::is_command_tool;
use crate::transcript::tool_summary::{command_detail, tool_file_path};

/// One message a tool call sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SentAgentMessage {
    /// The recipient as the sender addressed it: a global reference, a session id or title, an
    /// agent id for `create`, or the `to` of `SendMessage`.
    pub target: String,
    /// The recipient session's global reference, when the send's result named it.
    pub global_ref: String,
    pub agent_name: String,
    /// The recipient's agent id (`claude`, `codex`), which names its logo.
    pub agent_id: String,
    pub session_title: String,
    pub body: String,
    /// The send reported that it did not go out.
    pub failed: bool,
    /// Where the send left the message (`delivered`, `queued`, `pending`, `accepted`), as the
    /// result printed it; empty when no result was read back.
    pub status: String,
    /// Why the send did not go out, when its result said.
    pub failure: String,
    /// `target` is a name the agent called its teammate by (Claude's `SendMessage`), not a
    /// reference, so it may head the card when nothing else names the recipient.
    pub by_name: bool,
}

/// Every message one tool call sent, in the order it sent them. `written` is the files the same
/// message's tools wrote (`(path, content)`), where a `--body-file` body can be read back.
pub fn sent_agent_messages(pair: &ToolPair<'_>, written: &[(&str, &str)]) -> Vec<SentAgentMessage> {
    let Some(name) = pair.call_name() else {
        return Vec::new();
    };
    let input = pair.call_input().unwrap_or(&Value::Null);
    if ascii_lower(name) == "sendmessage" {
        return claude_send_message(input, pair).into_iter().collect();
    }
    if !is_command_tool(name) {
        return Vec::new();
    }
    let Some(command) = command_detail(input) else {
        return Vec::new();
    };
    if !command.contains("agents") {
        return Vec::new();
    }
    let script = Script::parse(&command, written);
    let mut sent: Vec<(usize, SentAgentMessage)> = script
        .commands
        .iter()
        .enumerate()
        .filter_map(|(at, shell)| cli_send(shell, at, &script).map(|sent| (at, sent)))
        .collect();
    if sent.is_empty() {
        return Vec::new();
    }
    let output = pair.result_output().unwrap_or_default();
    let results = send_results(output);
    let receipts = if results.is_empty() {
        let sends: Vec<SentAgentMessage> =
            sent.iter().map(|(_, message)| message.clone()).collect();
        filtered_receipts(output, &sends)
    } else {
        Vec::new()
    };
    for (index, (at, message)) in sent.iter_mut().enumerate() {
        match results.get(index) {
            Some(result) => {
                let recipient = result
                    .get("recipient")
                    .or_else(|| result.get("session"))
                    .unwrap_or(&Value::Null);
                let field = |key: &str| {
                    recipient
                        .get(key)
                        .and_then(Value::as_str)
                        .map(js_trim)
                        .unwrap_or_default()
                        .to_string()
                };
                message.global_ref = field("globalRef");
                message.agent_name = field("agentName");
                message.agent_id = field("agentId");
                let title = field("title");
                if !title.is_empty() {
                    message.session_title = title;
                }
                message.failed = result.get("ok") == Some(&Value::Bool(false));
                let said = |key: &str| {
                    result
                        .get(key)
                        .and_then(Value::as_str)
                        .map(js_trim)
                        .unwrap_or_default()
                        .to_string()
                };
                message.status = said("status");
                message.failure = said("error");
                // What the CLI echoes is what went out, whatever the script built it from.
                let echoed = result
                    .get("message")
                    .or_else(|| result.get("task"))
                    .and_then(Value::as_str)
                    .map(js_trim)
                    .unwrap_or_default();
                if !echoed.is_empty() {
                    message.body = echoed.to_string();
                }
            }
            // No result read back (the agent filtered it, or the call is still running).
            None => match receipts.get(index).cloned().flatten() {
                Some(Receipt::Status(status)) => message.status = status,
                Some(Receipt::Failed(failure)) => {
                    message.failed = true;
                    message.failure = failure;
                }
                None => {
                    message.failed = results.is_empty()
                        && script.send_failed(output, pair.result_is_error(), *at);
                    if message.failed {
                        message.failure = send_error_line(output).unwrap_or_default().to_string();
                    }
                }
            },
        }
    }
    sent.into_iter().map(|(_, message)| message).collect()
}

/// What a filtered output says about one send.
#[derive(Clone)]
enum Receipt {
    Status(String),
    Failed(String),
}

/// Each send's receipt when the agent kept only the status lines (`grep -o '"status": "[a-z]*"'`,
/// `Select-String '"status"'`): by the session id a line starts with (`G0hh9   "status": …`), else
/// in order when there is exactly one status per send. Empty when neither reads unambiguously.
fn filtered_receipts(output: &str, sends: &[SentAgentMessage]) -> Vec<Option<Receipt>> {
    let mut by_prefix: Vec<Option<Receipt>> = vec![None; sends.len()];
    let mut prefixed = false;
    let mut in_order: Vec<String> = Vec::new();
    for line in output.lines() {
        let statuses = line_statuses(line);
        let failure = send_error_line(line);
        in_order.extend(statuses.iter().cloned());
        let receipt = match (statuses.first(), failure) {
            (_, Some(failure)) => Receipt::Failed(failure.to_string()),
            (Some(status), None) => Receipt::Status(status.clone()),
            (None, None) => continue,
        };
        let token = line
            .trim_start()
            .split(|character: char| character.is_whitespace() || character == ':')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        let send = sends.iter().enumerate().position(|(index, send)| {
            by_prefix[index].is_none()
                && !token.is_empty()
                && send.target.rsplit(':').next().unwrap_or_default().to_lowercase() == token
        });
        if let Some(index) = send {
            // `G0hh9 Message delivery failed …`: the id the agent printed is not part of why.
            let receipt = match receipt {
                Receipt::Failed(failure) if failure.to_lowercase().starts_with(&token) => {
                    Receipt::Failed(
                        failure
                            .get(token.len()..)
                            .unwrap_or(&failure)
                            .trim_start_matches([':', ' ', '\t'])
                            .to_string(),
                    )
                }
                receipt => receipt,
            };
            by_prefix[index] = Some(receipt);
            prefixed = true;
        }
    }
    if prefixed {
        return by_prefix;
    }
    if in_order.len() == sends.len() {
        return in_order.into_iter().map(|status| Some(Receipt::Status(status))).collect();
    }
    Vec::new()
}

/// The `"status": "<word>"` values one line holds, in order.
fn line_statuses(line: &str) -> Vec<String> {
    line.match_indices("\"status\"")
        .filter_map(|(at, marker)| {
            let rest = line[at + marker.len()..].trim_start().strip_prefix(':')?;
            let rest = rest.trim_start().strip_prefix('"')?;
            let end = rest.find('"')?;
            let word = &rest[..end];
            (!word.is_empty() && word.chars().all(|character| character.is_ascii_lowercase()))
                .then(|| word.to_string())
        })
        .collect()
}

/// Claude's `SendMessage`: `{to, message, summary}`. A structured `message` (a shutdown request,
/// a plan approval) is protocol, not a message the reader wrote to anyone.
fn claude_send_message(input: &Value, pair: &ToolPair<'_>) -> Option<SentAgentMessage> {
    let body = input.get("message")?.as_str()?;
    let target = input.get("to")?.as_str()?;
    let failed = pair.result_is_error()
        || pair
            .result_output()
            .and_then(|output| serde_json::from_str::<Value>(js_trim(output)).ok())
            .is_some_and(|result| result.get("success") == Some(&Value::Bool(false)));
    // Claude's teammates are Claude sessions; a result read back means the message reached them.
    let answered = pair
        .result_output()
        .is_some_and(|output| !js_trim(output).is_empty());
    // Its `summary` describes the message, not the recipient, so it names nothing on the card.
    Some(SentAgentMessage {
        target: js_trim(target).to_string(),
        agent_id: "claude".to_string(),
        status: if answered && !failed {
            "delivered".to_string()
        } else {
            String::new()
        },
        body: js_trim(body).to_string(),
        failed,
        by_name: true,
        ..SentAgentMessage::default()
    })
}

/// One `ghostex agents send` or `ghostex agents create --task` in a script.
fn cli_send(shell: &ShellCommand, at: usize, script: &Script<'_>) -> Option<SentAgentMessage> {
    let start = shell.words.windows(3).position(|words| {
        matches!(last_path_segment(&words[0]), "ghostex" | "gx")
            && words[1] == "agents"
            && matches!(words[2].as_str(), "send" | "create")
    })?;
    // Only a command that runs the CLI: `echo ghostex agents send …` sends nothing. Environment
    // assignments, a `bash -lc` wrapper (how Codex records a shell call) and a `timeout 60` may
    // come first.
    let runs_cli = shell.words[..start].iter().all(|word| {
        (word.contains('=') && !word.starts_with('-'))
            || matches!(
                word.as_str(),
                "env" | "command" | "exec" | "bash" | "sh" | "zsh" | "-c" | "-lc" | "-l"
                    | "timeout" | "nohup" | "time" | "-k" | "-s" | "--foreground"
            )
            || word
                .trim_end_matches(['s', 'm', 'h'])
                .parse::<f64>()
                .is_ok()
    });
    if !runs_cli {
        return None;
    }
    let create = shell.words[start + 2] == "create";
    let mut positional: Vec<&str> = Vec::new();
    let mut body_file: Option<&str> = None;
    let mut task: Option<&str> = None;
    let mut title: Option<&str> = None;
    let mut words = shell.words[start + 3..].iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--" => {
                positional.extend(words.by_ref().map(String::as_str));
                break;
            }
            "--body-file" => body_file = words.next().map(String::as_str),
            "--task" => task = words.next().map(String::as_str),
            "--title" => title = words.next().map(String::as_str),
            "--server" | "--project-id" | "--request-id" => {
                words.next();
            }
            flag if flag.starts_with('-') => {}
            value => positional.push(value),
        }
    }
    // A reference the script does not show (`$t` outside a loop it can read) is no reference at
    // all, so the card names nobody rather than the raw word.
    let target = match script.value(positional.first()?, at, shell) {
        Resolved::Text(target) if !target.contains(['$', '`']) => target,
        _ => String::new(),
    };
    let body = if create {
        task
    } else {
        positional.get(1).copied()
    };
    let body = match (body, body_file) {
        (Some(word), _) => match script.value(word, at, shell) {
            Resolved::Text(body) => body,
            Resolved::Unknown => SCRIPTED_BODY.to_string(),
        },
        (None, Some(path)) => script.file_text(path, shell).unwrap_or_default(),
        // `create` without a task starts an agent; it sends it nothing.
        (None, None) if create => return None,
        (None, None) => String::new(),
    };
    Some(SentAgentMessage {
        target,
        session_title: title.unwrap_or_default().to_string(),
        body: js_trim(&body).to_string(),
        ..SentAgentMessage::default()
    })
}

/// The JSON objects `agents send` / `create` printed, in order. Other output (a `wake` line, a
/// filtered tail) is skipped; a result the agent cut short does not parse and is left out.
fn send_results(output: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let mut offset = 0;
    while offset < output.len() {
        let line_end = output[offset..]
            .find('\n')
            .map_or(output.len(), |at| offset + at);
        if output[offset..line_end].trim_start().starts_with('{') {
            let mut stream =
                serde_json::Deserializer::from_str(&output[offset..]).into_iter::<Value>();
            if let Some(Ok(value)) = stream.next() {
                let consumed = stream.byte_offset();
                if value.get("recipient").is_some() || value.get("session").is_some() {
                    results.push(value);
                }
                offset += consumed.max(1);
                continue;
            }
        }
        offset = line_end + 1;
    }
    results
}

/// The files a message's tools wrote, for a `--body-file` sent later in the same message.
pub fn written_files(message: &ChatMessage) -> Vec<(&str, &str)> {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::ToolCall { input, .. } => {
                let content = input.get("content")?.as_str()?;
                Some((tool_file_path(input)?, content))
            }
            _ => None,
        })
        .collect()
}

/// The `sentMessages` a projected message carries: one card per message its tool calls sent, keyed
/// `<message id>:<n>`. Sends of the same message in one call (a loop over several sessions) share
/// one card that lists each recipient. The names are completed across the transcript and the host's
/// session list by [`resolve_recipients`].
pub fn project_sent_messages(
    message: &ChatMessage,
    pairs: &[ToolPair<'_>],
    line_breaks: AgentLineBreaks,
) -> Vec<Value> {
    let written = written_files(message);
    pairs
        .iter()
        .enumerate()
        .flat_map(|(tool, pair)| {
            group_by_body(sent_agent_messages(pair, &written))
                .into_iter()
                .map(move |group| (tool, group))
        })
        .enumerate()
        .map(|(index, (tool, group))| {
            let first = &group[0];
            let body = agent_line_breaks(&first.body, line_breaks);
            let markdown = native_markdown(&body, false);
            let mut card = json!({
                "key": format!("{}:{index}", message.id),
                "tool": tool,
                "target": first.target,
                "globalRef": first.global_ref,
                "agentName": first.agent_name,
                "agentId": first.agent_id,
                "sessionTitle": first.session_title,
                "body": body,
                "markdownReferences": markdown_references(&markdown),
                "markdown": markdown,
                "failed": group.iter().all(|sent| sent.failed),
                "status": first.status,
                "failure": first.failure,
                "recipients": group.iter().map(recipient).collect::<Vec<_>>(),
            });
            // Named again once the whole transcript is read (`resolve_recipients`).
            complete(&Directory::new(), &HostDirectory::default(), std::slice::from_mut(&mut card));
            card
        })
        .collect()
}

/// The sends of one call grouped by their message, in the order each message was first sent. A
/// second send to the same session (a retry) replaces the first.
fn group_by_body(sent: Vec<SentAgentMessage>) -> Vec<Vec<SentAgentMessage>> {
    let mut groups: Vec<Vec<SentAgentMessage>> = Vec::new();
    for message in sent {
        let Some(group) = groups.iter_mut().find(|group| group[0].body == message.body) else {
            groups.push(vec![message]);
            continue;
        };
        let key = directory_key(&message.target);
        match group
            .iter_mut()
            .find(|known| !key.is_empty() && directory_key(&known.target) == key)
        {
            Some(known) => *known = message,
            None => group.push(message),
        }
    }
    groups
}

/// One recipient of a card, as its send's result left it; `name` is written by [`complete`].
fn recipient(sent: &SentAgentMessage) -> Value {
    json!({
        "target": sent.target,
        "globalRef": sent.global_ref,
        "agentName": sent.agent_name,
        "agentId": sent.agent_id,
        "sessionTitle": sent.session_title,
        "failed": sent.failed,
        "status": sent.status,
        "failure": sent.failure,
        "byName": sent.by_name,
        "name": "",
        "titled": false,
    })
}

/// What the transcript says about a session or a background agent.
#[derive(Clone, Default)]
struct Known {
    agent: String,
    title: String,
    /// One of this session's own background agents, named "<type> subagent".
    subagent: bool,
}

/// The recipient a reference names, as far as the transcript tells: the header of a message that
/// session sent here, the call that started a background agent, or the result of an earlier send.
type Directory = HashMap<String, Known>;

fn directory_key(reference: &str) -> String {
    js_trim(reference).to_lowercase()
}

fn remember(directory: &mut Directory, reference: &str, known: Known) {
    if js_trim(reference).is_empty() || (known.agent.is_empty() && known.title.is_empty()) {
        return;
    }
    directory.entry(directory_key(reference)).or_insert(known);
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn learn(directory: &mut Directory, message: &Value) {
    let inter = &message["interAgentMessage"];
    if inter.is_object() && inter["subagent"] != true {
        let known = Known {
            agent: text(inter, "agentName").to_string(),
            title: text(inter, "sessionTitle").to_string(),
            subagent: false,
        };
        remember(directory, text(inter, "replyTo"), known.clone());
        remember(directory, text(inter, "sessionId"), known);
    }
    // The call that started a background agent: its id, type and task.
    for tool in message["tools"].as_array().into_iter().flatten() {
        let subagent = &tool["subagent"];
        if subagent.is_object() && subagent["self"] != true {
            remember(
                directory,
                text(subagent, "selector"),
                Known {
                    agent: text(subagent, "agentType").to_string(),
                    title: text(subagent, "name").to_string(),
                    subagent: true,
                },
            );
        }
    }
    let recipients = message["sentMessages"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|sent| sent["recipients"].as_array().into_iter().flatten());
    for sent in recipients {
        if !text(sent, "agentName").is_empty() {
            let known = Known {
                agent: text(sent, "agentName").to_string(),
                title: text(sent, "sessionTitle").to_string(),
                subagent: false,
            };
            remember(directory, text(sent, "globalRef"), known.clone());
            remember(directory, text(sent, "target"), known);
        }
    }
}

/// The background agents on the Subagents roster (`{agents: [{id, name, task}]}`).
fn learn_fleet(directory: &mut Directory, fleet: Option<&Value>) {
    let agents = fleet.and_then(|fleet| fleet["agents"].as_array());
    for agent in agents.into_iter().flatten() {
        remember(
            directory,
            text(agent, "id"),
            Known {
                agent: text(agent, "name").to_string(),
                title: text(agent, "task").to_string(),
                subagent: true,
            },
        );
    }
}

/// The host's session list, by every reference an agent addresses a session with: its global
/// reference, `<project>:<session>`, its session id, and its title.
#[derive(Default)]
struct HostDirectory<'a> {
    by_reference: HashMap<String, &'a KnownSession>,
}

impl<'a> HostDirectory<'a> {
    fn new(sessions: &'a [KnownSession]) -> Self {
        let mut by_reference = HashMap::new();
        for session in sessions {
            let keys = [
                session.global_ref.clone(),
                format!("{}:{}", session.project_id, session.session_id),
                session.session_id.clone(),
                session.title.clone(),
            ];
            for key in keys {
                let key = directory_key(&key);
                if !key.is_empty() && key != ":" {
                    by_reference.entry(key).or_insert(session);
                }
            }
        }
        HostDirectory { by_reference }
    }

    /// The session a reference names: whole, then by its last two parts (`<project>:<session>`),
    /// then by its session id alone, for a machine prefix this host spells differently.
    fn find(&self, reference: &str) -> Option<&'a KnownSession> {
        let reference = directory_key(reference);
        if reference.is_empty() {
            return None;
        }
        if let Some(session) = self.by_reference.get(&reference) {
            return Some(session);
        }
        let parts: Vec<&str> = reference.split(':').collect();
        if parts.len() == 3 {
            if let Some(session) = self.by_reference.get(&format!("{}:{}", parts[1], parts[2])) {
                return Some(session);
            }
        }
        if parts.len() > 1 {
            return self.by_reference.get(parts[parts.len() - 1]).copied();
        }
        None
    }
}

/// A reference rather than a name: a global reference or an agent id the agent addressed by.
fn looks_like_reference(target: &str) -> bool {
    let target = js_trim(target);
    target.contains(':')
        || (target.len() >= 8
            && target.chars().all(|character| character.is_ascii_alphanumeric())
            && target.chars().any(|character| character.is_ascii_digit()))
}

/// A message from another session or a background agent, named from the host's session list or
/// the transcript, then dressed again.
fn complete_received(directory: &Directory, hosts: &HostDirectory, message: &mut Value) {
    let inter = &mut message["interAgentMessage"];
    if !inter.is_object() {
        return;
    }
    if inter["subagent"] == true {
        if let Some(known) = directory.get(&directory_key(text(inter, "replyTo"))) {
            inter["agentName"] = known.agent.clone().into();
            inter["sessionTitle"] = known.title.clone().into();
            dress_received(message);
        }
        return;
    }
    let Some(session) = hosts
        .find(text(inter, "replyTo"))
        .or_else(|| hosts.find(text(inter, "sessionId")))
    else {
        return;
    };
    // The session list's title is the one the sidebar shows now, after any rename.
    if !session.title.is_empty() {
        inter["sessionTitle"] = session.title.clone().into();
    }
    if text(inter, "agentId").is_empty() && !session.agent_id.is_empty() {
        inter["agentId"] = session.agent_id.clone().into();
    }
    if text(inter, "agentName").is_empty() && !session.agent_name.is_empty() {
        inter["agentName"] = session.agent_name.clone().into();
    }
    dress_received(message);
}

/// Names one recipient: the session's title (the host's session list first, then its send's
/// result, then the transcript), else its agent's name; a background agent is "<type> subagent".
/// Empty when nothing names it, which the card reads as "another agent".
fn name_recipient(directory: &Directory, hosts: &HostDirectory, recipient: &mut Value) {
    let target = text(recipient, "target").to_string();
    let global_ref = text(recipient, "globalRef").to_string();
    let (target, global_ref) = (target.as_str(), global_ref.as_str());
    let known = directory
        .get(&directory_key(global_ref))
        .or_else(|| directory.get(&directory_key(target)))
        .cloned()
        .unwrap_or_default();
    let host = hosts.find(global_ref).or_else(|| hosts.find(target));
    if known.subagent && host.is_none() {
        recipient["name"] = subagent_label(&known.agent).into();
        recipient["titled"] = false.into();
        return;
    }
    let mut agent_id = text(recipient, "agentId").to_string();
    let mut agent_name = text(recipient, "agentName").to_string();
    let mut title = String::new();
    if let Some(session) = host {
        title = session.title.clone();
        // A send addressed by bare session id still opens that session.
        if global_ref.is_empty() {
            let machine = session.global_ref.split(':').next().unwrap_or_default();
            recipient["globalRef"] = format!(
                "{}:{}:{}",
                if machine.is_empty() { "local" } else { machine },
                session.project_id,
                session.session_id
            )
            .into();
        }
        if agent_id.is_empty() {
            agent_id = session.agent_id.clone();
        }
        if agent_name.is_empty() {
            agent_name = session.agent_name.clone();
        }
    }
    for fallback in [text(recipient, "sessionTitle"), known.title.as_str()] {
        if title.is_empty() {
            title = fallback.to_string();
        }
    }
    if agent_name.is_empty() {
        agent_name = known.agent.clone();
    }
    // A title is quoted on the card; an agent name or a teammate name is not.
    recipient["titled"] = (!title.is_empty()).into();
    let name = if !title.is_empty() {
        title
    } else if recipient["byName"] == true && !target.is_empty() && !looks_like_reference(target) {
        target.to_string()
    } else {
        agent_display_name(&agent_id, &agent_name)
    };
    recipient["agentId"] = agent_id.into();
    recipient["agentName"] = agent_name.into();
    recipient["name"] = name.into();
}

/// Names each card's recipients and writes its `card`.
fn complete(directory: &Directory, hosts: &HostDirectory, cards: &mut [Value]) {
    for sent in cards.iter_mut() {
        if let Some(recipients) = sent["recipients"].as_array_mut() {
            for recipient in recipients {
                name_recipient(directory, hosts, recipient);
            }
        }
        dress_sent(sent);
    }
}

fn messages_mut(item: &mut TranscriptItem) -> Vec<&mut Value> {
    match item {
        TranscriptItem::Message { message } => vec![message],
        TranscriptItem::Summary {
            user,
            final_message,
            earlier_replies,
            work,
            outcome,
            ..
        } => std::iter::once(user)
            .chain(final_message.iter_mut())
            .chain(earlier_replies.iter_mut())
            .chain(work.iter_mut())
            .chain(outcome.iter_mut())
            .collect(),
        TranscriptItem::CompletedWork {
            work,
            artifacts,
            final_message,
            ..
        } => work
            .iter_mut()
            .chain(artifacts.iter_mut())
            .chain(final_message.iter_mut())
            .collect(),
        TranscriptItem::Unknown(_) => Vec::new(),
    }
}

/// Names every message between agents by what the host's session list and the whole transcript
/// know about the other session: an agent that filtered away its send's result, or addressed a
/// session by reference only, still gets that session's title.
pub fn resolve_recipients(
    items: &mut [TranscriptItem],
    agent_fleet: Option<&Value>,
    sessions: &[KnownSession],
) {
    let hosts = HostDirectory::new(sessions);
    let mut directory = Directory::new();
    learn_fleet(&mut directory, agent_fleet);
    for item in items.iter_mut() {
        for message in messages_mut(item) {
            learn(&mut directory, message);
        }
    }
    for item in items.iter_mut() {
        for message in messages_mut(item) {
            complete_received(&directory, &hosts, message);
            if let Some(cards) = message
                .get_mut("sentMessages")
                .and_then(Value::as_array_mut)
            {
                complete(&directory, &hosts, cards);
            }
        }
        // A finished turn's hoisted cards are copies of its work rows' own.
        if let TranscriptItem::CompletedWork { sent_messages, .. } = item {
            complete(&directory, &hosts, sent_messages);
        }
    }
}
