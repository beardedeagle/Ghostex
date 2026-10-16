//! Which calls to a workspace's Composio tools only read (Open Dot's `executeDecision`): reads run
//! on their own, anything that sends, posts, creates, changes or deletes asks first.

use serde_json::Value;

/// Whether a Composio call may run without asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposioDecision {
    Allow,
    Ask,
}

/// Composio's own tools that only find tools and read their schemas: they touch none of the
/// user's apps.
const FINDER_TOOLS: [&str; 2] = ["COMPOSIO_SEARCH_TOOLS", "COMPOSIO_GET_TOOL_SCHEMAS"];

/// Open Dot's `READ_VERB` words: a tool whose action starts with one of these only reads.
const READ_VERBS: &[&str] = &[
    "GET", "LIST", "SEARCH", "FETCH", "FIND", "READ", "RETRIEVE", "QUERY", "DESCRIBE", "LOOKUP",
    "VIEW", "CHECK", "COUNT", "EXPORT", "DOWNLOAD",
];

/// Words that send, change or delete something. A slug with any of them asks, even after a read
/// verb (`*_GET_OR_CREATE_*`, `*_FIND_AND_REPLACE_*`). Words that are also common nouns in read
/// slugs (RUN, ORDER, DRAFT, LABEL, UPDATES) are left out; a slug that starts with them still asks
/// because it does not start with a read verb.
const WRITE_VERBS: &[&str] = &[
    "SEND",
    "SENDS",
    "POST",
    "CREATE",
    "CREATES",
    "CREATION",
    "ADD",
    "ADDS",
    "INSERT",
    "UPSERT",
    "UPDATE",
    "EDIT",
    "MODIFY",
    "PATCH",
    "PUT",
    "SET",
    "CHANGE",
    "REPLACE",
    "RENAME",
    "DELETE",
    "DELETES",
    "REMOVE",
    "REMOVES",
    "TRASH",
    "ARCHIVE",
    "UNARCHIVE",
    "MOVE",
    "MERGE",
    "CLOSE",
    "REOPEN",
    "REPLY",
    "FORWARD",
    "SHARE",
    "INVITE",
    "ASSIGN",
    "UNASSIGN",
    "APPROVE",
    "REJECT",
    "PUBLISH",
    "UPLOAD",
    "IMPORT",
    "SCHEDULE",
    "RESCHEDULE",
    "CANCEL",
    "ACCEPT",
    "DECLINE",
    "SUBMIT",
    "EXECUTE",
    "TRIGGER",
    "ENABLE",
    "DISABLE",
    "REVOKE",
    "GRANT",
    "TRANSFER",
    "PAY",
    "PURCHASE",
    "SUBSCRIBE",
    "UNSUBSCRIBE",
    "FOLLOW",
    "UNFOLLOW",
    "BLOCK",
    "UNBLOCK",
    "APPEND",
    "WRITE",
    "SAVE",
    "RESTORE",
    "PURGE",
    "DESTROY",
    "TERMINATE",
    "MARK",
    "STAR",
    "UNSTAR",
    "PIN",
    "UNPIN",
    "CLEAR",
    "RESET",
    "DUPLICATE",
    "COMPOSE",
    "LOCK",
    "UNLOCK",
    "DEPLOY",
    "START",
    "STOP",
    "JOIN",
    "LEAVE",
    "KICK",
    "BAN",
    "MUTE",
    "UNMUTE",
    "LIKE",
    "UNLIKE",
    "REACT",
    "TOGGLE",
];

/// Composio toolkits whose slug is more than one word (`MICROSOFT_TEAMS_LIST_CHATS`); every other
/// tool slug starts with a one-word toolkit (`GMAIL_FETCH_EMAILS`).
const MULTI_WORD_TOOLKITS: &[&str] = &[
    "GOOGLE_SEARCH_CONSOLE",
    "GOOGLE_ANALYTICS",
    "GOOGLE_MAPS",
    "GOOGLE_ADS",
    "MICROSOFT_TEAMS",
    "MICROSOFT_CLARITY",
    "ONE_DRIVE",
    "SHARE_POINT",
    "ZOHO_BOOKS",
    "ZOHO_CRM",
    "ZOHO_DESK",
    "ZOHO_INVOICE",
];

/// The action words of a tool slug, without its toolkit prefix.
fn action_words(slug: &str) -> Vec<String> {
    let upper = slug.trim().to_uppercase();
    let rest = MULTI_WORD_TOOLKITS
        .iter()
        .find_map(|toolkit| upper.strip_prefix(&format!("{toolkit}_")))
        .map(str::to_string)
        .unwrap_or_else(|| {
            upper
                .split_once('_')
                .map(|(_, rest)| rest.to_string())
                .unwrap_or_default()
        });
    rest.split('_')
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// A tool that only reads: its action starts with a read verb and no write verb appears anywhere.
///
/// CDXC:Connectors 2026-10-01 WHY:
/// Open Dot treated a slug as a read when any word after the first was a read verb, so `GOOGLECALENDAR_DELETE_TASK_LIST` and `GITHUB_CREATE_A_CHECK_RUN` ran without asking. Here the toolkit prefix is stripped, the first remaining word must be a read verb, and any write verb anywhere asks; an unknown shape asks.
fn is_read_slug(slug: &str) -> bool {
    let words = action_words(slug);
    let Some(first) = words.first() else {
        return false;
    };
    READ_VERBS.contains(&first.as_str())
        && !words
            .iter()
            .any(|word| WRITE_VERBS.contains(&word.as_str()))
}

/// `COMPOSIO_MULTI_EXECUTE_TOOL`'s `tools: [{ tool_slug, arguments }]`.
fn execute_items(arguments: &Value) -> Vec<&Value> {
    arguments
        .get("tools")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn item_slug(item: &Value) -> String {
    match item.get("tool_slug") {
        Some(Value::String(slug)) => slug.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Reads run automatically; anything else asks first (Open Dot's `executeDecision`).
fn execute_decision(arguments: &Value) -> ComposioDecision {
    let items = execute_items(arguments);
    if !items.is_empty() && items.iter().all(|item| is_read_slug(&item_slug(item))) {
        ComposioDecision::Allow
    } else {
        ComposioDecision::Ask
    }
}

/// The tools a call runs, for messages: the app tools inside `COMPOSIO_MULTI_EXECUTE_TOOL`, else
/// the tool itself.
pub(crate) fn call_label(tool: &str, arguments: &Value) -> String {
    let slugs = execute_items(arguments)
        .iter()
        .map(|item| item_slug(item))
        .filter(|slug| !slug.is_empty())
        .collect::<Vec<_>>();
    if tool == "COMPOSIO_MULTI_EXECUTE_TOOL" && !slugs.is_empty() {
        slugs.join(", ")
    } else {
        tool.to_string()
    }
}

/// The tools a call runs, as rules name them: the app tools inside `COMPOSIO_MULTI_EXECUTE_TOOL`,
/// else the tool itself.
pub(crate) fn rule_tools(tool: &str, arguments: &Value) -> Vec<String> {
    let slugs = execute_items(arguments)
        .iter()
        .map(|item| item_slug(item))
        .filter(|slug| !slug.is_empty())
        .collect::<Vec<_>>();
    if tool == "COMPOSIO_MULTI_EXECUTE_TOOL" && !slugs.is_empty() {
        slugs
    } else {
        vec![tool.to_string()]
    }
}

/// `COMPOSIO_MANAGE_CONNECTIONS` that only lists connections (every toolkit's `action` is `list`;
/// a missing action means `add`).
fn lists_connections_only(arguments: &Value) -> bool {
    let toolkits = arguments
        .get("toolkits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    !toolkits.is_empty()
        && toolkits
            .iter()
            .all(|toolkit| toolkit.get("action").and_then(Value::as_str) == Some("list"))
}

/// The rule for one call: Composio's finder tools, connection lists and app reads run on their
/// own; everything else (connecting or removing an app, the workbench, any write) asks.
pub(crate) fn call_decision(tool: &str, arguments: &Value) -> ComposioDecision {
    if FINDER_TOOLS.contains(&tool) {
        return ComposioDecision::Allow;
    }
    match tool {
        "COMPOSIO_MULTI_EXECUTE_TOOL" => execute_decision(arguments),
        "COMPOSIO_MANAGE_CONNECTIONS" if lists_connections_only(arguments) => {
            ComposioDecision::Allow
        }
        _ => ComposioDecision::Ask,
    }
}
