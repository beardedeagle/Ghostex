//! The append stream's merger: id-only dedup with in-place replacement that preserves first-seen
//! order.
//!
//! Ported from `packages/core-ui/chat/session-chat-merge.ts`. Source priority uses `>=` so an
//! equal-priority re-emit still refreshes content.
//!
//! The live list is NEVER trimmed. A trim would drop the OLDEST rows with no way to page them back
//! (the pagination cursor points at the pre-snapshot history), leaving a permanent
//! mid-conversation hole against the terminal. Windowing belongs to the reads that seed the list,
//! not to the append stream.

use std::collections::BTreeMap;

use ghostex_gx_protocol::ChatMessage;

use crate::session::assembler::{id_collides, shadowed_id, source_priority};
use crate::state::MessagesState;

/// Applies `incoming` to `list`, keeping `index_by_id` aligned.
fn apply_incoming(
    list: &mut Vec<ChatMessage>,
    index_by_id: &mut BTreeMap<String, usize>,
    incoming: &[ChatMessage],
) {
    for raw in incoming {
        let mut message = raw.clone();
        if let Some(at) = index_by_id.get(&message.id).copied() {
            if let Some(occupant) = list.get(at) {
                if id_collides(occupant, &message) {
                    // A different row wearing the same id (rows without a record uuid share their
                    // API response id): re-key it so neither row is lost.
                    message.id = shadowed_id(&message);
                }
            }
        }
        match index_by_id.get(&message.id).copied() {
            None => {
                index_by_id.insert(message.id.clone(), list.len());
                list.push(message);
            }
            Some(at) => {
                let replace = list.get(at).is_some_and(|existing| {
                    source_priority(&message.source) >= source_priority(&existing.source)
                });
                if replace {
                    list[at] = message;
                }
            }
        }
    }
}

/// `mergeSessionChatMessagesWith`: `existing` with `incoming` folded in.
pub fn merge_messages(existing: &[ChatMessage], incoming: &[ChatMessage]) -> Vec<ChatMessage> {
    if incoming.is_empty() {
        return existing.to_vec();
    }
    let mut list = existing.to_vec();
    let mut index_by_id = BTreeMap::new();
    for (index, entry) in list.iter().enumerate() {
        index_by_id.insert(entry.id.clone(), index);
    }
    apply_incoming(&mut list, &mut index_by_id, incoming);
    list
}

impl MessagesState {
    /// Rebuilds the list and its index from `list`.
    ///
    /// Routed through the same path as an append so a window carrying two rows that share an id
    /// (no record uuid, so a shared response id) keeps both and the index never points at the
    /// wrong row.
    pub fn replace_list(&mut self, list: &[ChatMessage]) {
        self.list = Vec::new();
        self.index_by_id = BTreeMap::new();
        let mut next = Vec::new();
        let mut index = BTreeMap::new();
        apply_incoming(&mut next, &mut index, list);
        self.list = next;
        self.index_by_id = index;
        self.list_revision = self.list_revision.wrapping_add(1);
    }

    /// Folds an append into the list in place.
    pub fn apply_append(&mut self, incoming: &[ChatMessage]) {
        let mut list = std::mem::take(&mut self.list);
        let mut index = std::mem::take(&mut self.index_by_id);
        apply_incoming(&mut list, &mut index, incoming);
        self.list = list;
        self.index_by_id = index;
        self.list_revision = self.list_revision.wrapping_add(1);
        // The merger put the incoming OBJECT in the list wherever it replaced or appended, so a
        // row that landed carries a new `deferredWork`.
        for message in incoming {
            let landed = self
                .index_by_id
                .get(&message.id)
                .and_then(|at| self.list.get(*at))
                .is_some_and(|stored| stored == message);
            if landed {
                self.note_arrival(message, false);
            }
        }
    }

    /// Drops rows the server retracted (abandoned prompts), returning whether anything went.
    ///
    /// Rebuilding the index is the only safe way to keep it aligned after a removal, and
    /// retractions are rare enough that the cost never matters.
    pub fn remove_ids(&mut self, ids: &[String]) -> bool {
        if ids.is_empty() {
            return false;
        }
        let kept: Vec<ChatMessage> = self
            .list
            .iter()
            .filter(|message| !ids.contains(&message.id))
            .cloned()
            .collect();
        if kept.len() == self.list.len() {
            return false;
        }
        self.replace_list(&kept);
        true
    }
}
