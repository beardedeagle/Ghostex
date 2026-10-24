//! A session's stored links, `runtimeSettings.sessionLinks`: the links that joined it (from an
//! earlier branch, what the agent opened or reported, the issues a PR closes, the Linear tickets
//! the chat mentioned, the ones a person added) and the links a person removed.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "x on an entry, need to click twice to remove it (to confirm removal)", "also if removed": a removed link goes onto `removed` and never joins the session again by itself, whatever names it later (the branch, the agent, the chat). Only adding it again by hand (or `ghostex link add`), or restoring it, takes it off the list.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "when a link is gone i see that i can't bring it back why not?", "i mean when i hit x button twice on it". A removed link keeps what it was (`removedItems`: where it came from), the lists show it under a collapsed "Removed (N)" group with Restore, and right after a removal its row reads "Removed · Undo" for a few seconds. Restoring takes it off `removed`, puts it back as it was, and from then on it can join by itself again.

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
    /// What a removed stored link was, so Restore puts it back with its source.
    pub(crate) removed_items: Vec<StoredSessionLink>,
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

    /// Drops a link and remembers that it was removed, and what it was.
    pub(crate) fn remove(&mut self, key: &str) -> bool {
        let before = self.items.len();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|item| item.link.key() == key);
        self.items = kept;
        self.removed_items.retain(|item| item.link.key() != key);
        self.removed_items.extend(gone);
        let newly_removed = !self.is_removed(key);
        if newly_removed {
            self.removed.push(key.to_string());
        }
        newly_removed || self.items.len() != before
    }

    /// Takes a link off the removed list and puts back the stored link it was, when it had one.
    /// Returns whether anything changed.
    pub(crate) fn restore(&mut self, key: &str) -> bool {
        let was_removed = self.is_removed(key);
        self.removed.retain(|removed| removed != key);
        let (back, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.removed_items)
            .into_iter()
            .partition(|item| item.link.key() == key);
        self.removed_items = kept;
        let mut changed = was_removed;
        for item in back {
            if !self.contains(key) {
                self.items.push(item);
                changed = true;
            }
        }
        changed
    }

    fn to_value(&self) -> Value {
        let item_value = |item: &StoredSessionLink| {
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
        };
        let removed_items: Vec<Value> = self.removed_items.iter().map(item_value).collect();
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
        json!({ "items": items, "removed": self.removed, "removedItems": removed_items })
    }
}

pub(crate) fn stored_session_links(session: &Value) -> StoredSessionLinks {
    let Some(links) = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("sessionLinks"))
    else {
        return StoredSessionLinks::default();
    };
    let parse_items = |field: &str| -> Vec<StoredSessionLink> {
        links
            .get(field)
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
            .unwrap_or_default()
    };
    let items = parse_items("items");
    let removed_items = parse_items("removedItems");
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
    StoredSessionLinks {
        items,
        removed,
        removed_items,
    }
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
