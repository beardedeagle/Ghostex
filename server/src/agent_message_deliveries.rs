//! Where each message `ghostex agents send` left waiting stands, for the chat of the session that
//! sent it: the `agentMessageDeliveries` chat field.
//!
//! CDXC:SessionChat 2026-10-14 WHY:
//! A send answers at once, and a busy or starting recipient answers `pending`, `accepted` or `queued`, so the sender's card stayed on "Waiting for agent" after the message had arrived. The CLI hands every such send to gxserver (`/api/watchAgentMessageDelivery`), the delivery watch (server/src/server/agent_message_delivery_runtime.rs) settles it against the recipient's transcript and queue, and the sender's chat frames carry the settled outcomes by `sendRequestId`, which gx-chat-core applies to the card. Decided here rather than in a client, so the desktop, web and phone chats agree and a reload or a restart still shows Delivered. Frames carry only settled sends (a waiting card already says so), the field is absent (unchanged) on a state frame unless an outcome moved since the last one, and the builders read only this cache: they run under the stream's emit lock and must not touch the database.
//! SEE-ALSO: server/src/ghostex_cli/agents/delivery.rs (registers the watch), server/src/coordinators/delivery.rs (the transcript match it shares with the orchestrator's "did not reach it" report), packages/gx-chat-core/src/transcript/agent_message_card.rs `apply_live_delivery` (the card).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::coordinators::{delivery_needles, SessionKey};
use crate::domain::DomainStateError;

pub const WATCH_AGENT_MESSAGE_DELIVERY_ENDPOINT: &str = "/api/watchAgentMessageDelivery";

pub const DELIVERY_WAITING: &str = "waiting";
pub const DELIVERY_DELIVERED: &str = "delivered";
pub const DELIVERY_FAILED: &str = "failed";

/// How long rows are kept, and how many settled ones per sender a chat is told about.
const RETENTION_MS: i64 = 14 * 24 * 60 * 60 * 1000;
const MAX_SETTLED_PER_SENDER: i64 = 200;
/// A send still waiting after this long is no longer watched; its card keeps what the send said.
pub const WATCH_FOR_MS: i64 = 24 * 60 * 60 * 1000;

/// One send the watch is following.
#[derive(Clone, Debug)]
pub struct WatchedSend {
    pub sender: SessionKey,
    pub send_request_id: String,
    pub recipient: SessionKey,
    pub needles: Vec<String>,
    pub sent_at_ms: i64,
}

impl WatchedSend {
    pub fn key(&self) -> (SessionKey, String) {
        (self.sender.clone(), self.send_request_id.clone())
    }
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// `/api/watchAgentMessageDelivery`: `{senderProjectId, senderSessionId, projectId, sessionId,
/// sendRequestId, message, sentAtMs}`. A send already settled keeps its outcome (a retried send
/// repeats its id).
pub fn handle_watch_agent_message_delivery(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let field = |key: &str| {
        text(params, key).ok_or_else(|| {
            DomainStateError::bad_request(format!("watchAgentMessageDelivery needs {key}."))
        })
    };
    let (sender_project, sender_session) = (field("senderProjectId")?, field("senderSessionId")?);
    let (project_id, session_id) = (field("projectId")?, field("sessionId")?);
    let send_request_id = field("sendRequestId")?;
    let needles = delivery_needles(&field("message")?);
    if needles.is_empty() {
        return Err(DomainStateError::bad_request(
            "watchAgentMessageDelivery needs a message to look for.",
        ));
    }
    let now = now_ms();
    let sent_at_ms = params
        .get("sentAtMs")
        .and_then(Value::as_i64)
        .filter(|sent_at| *sent_at <= now)
        .unwrap_or(now);
    db.execute(
        r#"
        INSERT INTO agent_message_deliveries (
          senderProjectId, senderSessionId, sendRequestId, projectId, sessionId, needles,
          sentAtMs, state, failure, updatedAtMs
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'waiting', NULL, ?8)
        ON CONFLICT (senderProjectId, senderSessionId, sendRequestId) DO NOTHING
        "#,
        params![
            sender_project,
            sender_session,
            send_request_id,
            project_id,
            session_id,
            json!(needles).to_string(),
            sent_at_ms,
            now,
        ],
    )
    .map_err(sql_error)?;
    let state = db
        .query_row(
            r#"
            SELECT state FROM agent_message_deliveries
            WHERE senderProjectId = ?1 AND senderSessionId = ?2 AND sendRequestId = ?3
            "#,
            params![sender_project, sender_session, send_request_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .unwrap_or_else(|| DELIVERY_WAITING.to_string());
    Ok(json!({ "watching": state == DELIVERY_WAITING, "state": state }))
}

/// Every send still waiting that is young enough to watch, oldest first. Prunes old rows.
pub fn waiting_sends(db: &Connection) -> Vec<WatchedSend> {
    let now = now_ms();
    let _ = db.execute(
        "DELETE FROM agent_message_deliveries WHERE updatedAtMs < ?1",
        params![now - RETENTION_MS],
    );
    let Ok(mut statement) = db.prepare(
        r#"
        SELECT senderProjectId, senderSessionId, sendRequestId, projectId, sessionId, needles,
               sentAtMs
        FROM agent_message_deliveries
        WHERE state = 'waiting' AND sentAtMs >= ?1
        ORDER BY sentAtMs
        "#,
    ) else {
        return Vec::new();
    };
    statement
        .query_map(params![now - WATCH_FOR_MS], |row| {
            Ok(WatchedSend {
                sender: (row.get(0)?, row.get(1)?),
                send_request_id: row.get(2)?,
                recipient: (row.get(3)?, row.get(4)?),
                needles: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                sent_at_ms: row.get(6)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// Records how a waiting send ended (`delivered`, or `failed` with why) and refreshes its
/// sender's cached field. True when the sender's chat should be republished.
pub fn settle_send(
    db: &Connection,
    send: &WatchedSend,
    state: &str,
    failure: Option<&str>,
) -> bool {
    let changed = db
        .execute(
            r#"
            UPDATE agent_message_deliveries SET state = ?4, failure = ?5, updatedAtMs = ?6
            WHERE senderProjectId = ?1 AND senderSessionId = ?2 AND sendRequestId = ?3
              AND state = 'waiting'
            "#,
            params![
                send.sender.0,
                send.sender.1,
                send.send_request_id,
                state,
                failure,
                now_ms()
            ],
        )
        .unwrap_or(0)
        > 0;
    changed && refresh_sender(db, &send.sender)
}

/// The sender's settled sends, newest first and bounded, as the chat field:
/// `{ "<sendRequestId>": { "state": "delivered" } | { "state": "failed", "failure": "…" } }`.
fn read_settled(db: &Connection, sender: &SessionKey) -> Map<String, Value> {
    let mut settled = Map::new();
    let Ok(mut statement) = db.prepare(
        r#"
        SELECT sendRequestId, state, failure FROM agent_message_deliveries
        WHERE senderProjectId = ?1 AND senderSessionId = ?2 AND state <> 'waiting'
        ORDER BY updatedAtMs DESC LIMIT ?3
        "#,
    ) else {
        return settled;
    };
    let rows = statement.query_map(params![sender.0, sender.1, MAX_SETTLED_PER_SENDER], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    });
    for (id, state, failure) in rows.into_iter().flatten().flatten() {
        let mut outcome = json!({ "state": state });
        if let Some(failure) = failure.filter(|failure| !failure.is_empty()) {
            outcome["failure"] = json!(failure);
        }
        settled.insert(id, outcome);
    }
    settled
}

/// One sender's cached field, and the revision the last state frame carried.
struct CachedDeliveries {
    value: Value,
    revision: u64,
    state_frame_revision: u64,
}

fn cache() -> &'static Mutex<HashMap<SessionKey, CachedDeliveries>> {
    static CACHE: OnceLock<Mutex<HashMap<SessionKey, CachedDeliveries>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Re-reads one sender's settled sends into the cache; true when they changed.
pub fn refresh_sender(db: &Connection, sender: &SessionKey) -> bool {
    let value = Value::Object(read_settled(db, sender));
    let Ok(mut cache) = cache().lock() else {
        return false;
    };
    match cache.get_mut(sender) {
        Some(cached) if cached.value == value => false,
        Some(cached) => {
            cached.value = value;
            cached.revision += 1;
            true
        }
        None => {
            cache.insert(
                sender.clone(),
                CachedDeliveries {
                    value,
                    revision: 1,
                    state_frame_revision: 0,
                },
            );
            true
        }
    }
}

/// Fills the cache for every sender with settled sends, once at startup.
pub fn load_settled_deliveries(db: &Connection) {
    let Ok(mut statement) = db.prepare(
        "SELECT DISTINCT senderProjectId, senderSessionId FROM agent_message_deliveries WHERE state <> 'waiting'",
    ) else {
        return;
    };
    let senders: Vec<SessionKey> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();
    for sender in senders {
        refresh_sender(db, &sender);
    }
}

/// Which frame `agentMessageDeliveries` is being added to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeliveriesFrame {
    /// A snapshot or a read result: always carries the field.
    Whole,
    /// A state frame: carries it only when it moved since the last state frame did.
    State,
}

/// Adds `agentMessageDeliveries` to a chat frame or read result of a session that sent messages
/// the watch settled.
pub fn insert_agent_message_deliveries(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
    kind: DeliveriesFrame,
) {
    let Ok(mut cache) = cache().lock() else {
        return;
    };
    let Some(cached) = cache.get_mut(&(project_id.to_string(), session_id.to_string())) else {
        return;
    };
    if kind == DeliveriesFrame::State {
        if cached.state_frame_revision == cached.revision {
            return;
        }
        cached.state_frame_revision = cached.revision;
    }
    frame.insert("agentMessageDeliveries".to_string(), cached.value.clone());
}
