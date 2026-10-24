//! A session's stored links, `runtimeSettings.sessionLinks`: the links that joined it (from an
//! earlier branch, what the agent opened or reported, the issues a PR closes, the Linear tickets
//! the chat mentioned, the ones a person added) and the links a person removed.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "x on an entry, need to click twice to remove it (to confirm removal)", "also if removed": a removed link goes onto `removed` and never joins the session again by itself, whatever names it later (the branch, the agent, the chat). Only adding it again by hand (or `ghostex link add`) takes it off the list.

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::SessionLinkRef;

/// Where a stored link came from, which its second line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionLinkSource {
    /// The branch the session is on, or was on before.
    Branch,
    /// The agent opened it (`gh pr create`, `gh issue create`).
    AgentOpened,
    /// The agent reported it with `ghostex link add`.
    AgentReported,
    /// A linked PR closes it.
    Closes,
    /// The chat mentioned a Linear ticket of the workspace's Linear teams.
    Mentioned,
    /// A person added it.
    User,
}

impl SessionLinkSource {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::AgentOpened => "agentOpened",
            Self::AgentReported => "agentReported",
            Self::Closes => "closes",
            Self::Mentioned => "mentioned",
            Self::User => "user",
        }
    }

    pub(crate) fn from_wire(text: &str) -> Option<Self> {
        Some(match text {
            "branch" => Self::Branch,
            "agentOpened" => Self::AgentOpened,
            "agentReported" => Self::AgentReported,
            "closes" => Self::Closes,
            "mentioned" => Self::Mentioned,
            "user" => Self::User,
            _ => return None,
        })
    }
}

/// One stored link.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredSessionLink {
    pub(crate) link: SessionLinkRef,
    pub(crate) source: SessionLinkSource,
    /// The PR a `Closes` link came from (`#318`), or the branch a `Branch` link came from.
    pub(crate) via: Option<String>,
    /// A Linear issue's own URL, when it was linked by URL.
    pub(crate) url: Option<String>,
    pub(crate) added_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StoredSessionLinks {
    pub(crate) items: Vec<StoredSessionLink>,
    /// Keys of links a person removed.
    pub(crate) removed: Vec<String>,
}

impl StoredSessionLinks {
    pub(crate) fn is_removed(&self, key: &str) -> bool {
        self.removed.iter().any(|removed| removed == key)
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|item| item.link.key() == key)
    }

    /// Adds a link found automatically. Returns whether the list changed: a link already there or
    /// removed by a person is left alone.
    pub(crate) fn add_found(&mut self, link: StoredSessionLink) -> bool {
        let key = link.link.key();
        if self.is_removed(&key) || self.contains(&key) {
            return false;
        }
        self.items.push(link);
        true
    }

    /// Adds a link a person (or `ghostex link add`) asked for, taking it off the removed list.
    pub(crate) fn add_by_hand(&mut self, link: StoredSessionLink) -> bool {
        let key = link.link.key();
        let was_removed = self.is_removed(&key);
        self.removed.retain(|removed| removed != &key);
        if self.contains(&key) {
            return was_removed;
        }
        self.items.push(link);
        true
    }

    /// Drops a link and remembers that it was removed.
    pub(crate) fn remove(&mut self, key: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.link.key() != key);
        let newly_removed = !self.is_removed(key);
        if newly_removed {
            self.removed.push(key.to_string());
        }
        newly_removed || self.items.len() != before
    }

    fn to_value(&self) -> Value {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|item| {
                let mut object = Map::new();
                object.insert("key".to_string(), json!(item.link.key()));
                object.insert("source".to_string(), json!(item.source.as_wire()));
                if let Some(via) = &item.via {
                    object.insert("via".to_string(), json!(via));
                }
                if let Some(url) = &item.url {
                    object.insert("url".to_string(), json!(url));
                }
                if let Some(added_at) = &item.added_at {
                    object.insert("addedAt".to_string(), json!(added_at));
                }
                Value::Object(object)
            })
            .collect();
        json!({ "items": items, "removed": self.removed })
    }
}

pub(crate) fn stored_session_links(session: &Value) -> StoredSessionLinks {
    let Some(links) = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("sessionLinks"))
    else {
        return StoredSessionLinks::default();
    };
    let items = links
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let text = |key: &str| {
                        item.get(key)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_string)
                    };
                    Some(StoredSessionLink {
                        link: SessionLinkRef::from_key(&text("key")?)?,
                        source: text("source")
                            .as_deref()
                            .and_then(SessionLinkSource::from_wire)
                            .unwrap_or(SessionLinkSource::User),
                        via: text("via"),
                        url: text("url"),
                        added_at: text("addedAt"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let removed = links
        .get("removed")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    StoredSessionLinks { items, removed }
}

/// Writes `runtimeSettings.sessionLinks` in one statement, patched in place like `workLinks`
/// (hooks, titles and activity rewrite the same JSON all the time).
pub(crate) fn write_session_links(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    links: &StoredSessionLinks,
) -> Result<bool, DomainStateError> {
    let changed = db
        .execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.sessionLinks', json(?1)) \
             WHERE projectId = ?2 AND sessionId = ?3",
            params![links.to_value().to_string(), project_id, session_id],
        )
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite session links error: {error}"),
        })?;
    Ok(changed > 0)
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
