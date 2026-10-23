//! The transcript list as the projection keeps it: the settled turns' rows in shared chunks, then
//! the rows the last pass built.
//!
//! CDXC:SessionChat 2026-10-23 WHY:
//! A working chat rebuilt, copied, compared and dropped its whole row list on every event (29 MB of
//! rows on a 3,400-message chat, about a third of each event's time). The settled turns
//! (`transcript/settled.rs`) are built once and shared by every list after them, so a rebuild
//! allocates only the rows still moving, and the splice skips the shared chunks by pointer.

use std::sync::Arc;

use serde::ser::{Serialize, SerializeSeq, Serializer};

use crate::document::TranscriptItem;

/// The rows of one transcript, oldest first.
#[derive(Clone, Default)]
pub struct TranscriptItems {
    /// The settled turns' rows, each chunk shared with every list built while it stood.
    settled: Vec<Arc<Vec<TranscriptItem>>>,
    /// Where each settled chunk starts, then where the live rows start.
    starts: Vec<usize>,
    /// The rows this pass built.
    live: Vec<TranscriptItem>,
}

impl TranscriptItems {
    /// A list built whole, sharing nothing.
    pub fn from_items(items: Vec<TranscriptItem>) -> Self {
        Self {
            settled: Vec::new(),
            starts: vec![0],
            live: items,
        }
    }

    /// The settled chunks, shared, then the rows this pass built.
    pub fn with_settled(settled: &[Arc<Vec<TranscriptItem>>], live: Vec<TranscriptItem>) -> Self {
        let mut starts = Vec::with_capacity(settled.len() + 1);
        let mut at = 0;
        for chunk in settled {
            starts.push(at);
            at += chunk.len();
        }
        starts.push(at);
        Self {
            settled: settled.to_vec(),
            starts,
            live,
        }
    }

    fn settled_len(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.settled_len() + self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<&TranscriptItem> {
        let settled = self.settled_len();
        if index >= settled {
            return self.live.get(index - settled);
        }
        let (chunk, offset) = self.locate(index);
        self.settled[chunk].get(offset)
    }

    /// The settled chunk a settled row is in, and its place there.
    fn locate(&self, index: usize) -> (usize, usize) {
        // The last chunk that starts at or before `index`.
        let chunk = self.starts.partition_point(|start| *start <= index) - 1;
        (chunk, index - self.starts[chunk])
    }

    /// Whether row `index` here and row `other_index` of `other` are one row of one shared chunk,
    /// and so the same value without comparing them.
    pub fn same_row(&self, index: usize, other: &Self, other_index: usize) -> bool {
        if index >= self.settled_len() || other_index >= other.settled_len() {
            return false;
        }
        let (mine, offset) = self.locate(index);
        let (theirs, other_offset) = other.locate(other_index);
        offset == other_offset && Arc::ptr_eq(&self.settled[mine], &other.settled[theirs])
    }

    pub fn iter(&self) -> impl Iterator<Item = &TranscriptItem> {
        self.settled
            .iter()
            .flat_map(|chunk| chunk.iter())
            .chain(self.live.iter())
    }

    /// Every row, copied out.
    pub fn to_vec(&self) -> Vec<TranscriptItem> {
        self.iter().cloned().collect()
    }

    /// How many leading rows the two lists share by construction: the settled chunks both hold,
    /// in the same places. Those rows are the same values without comparing them.
    pub fn shared_prefix(&self, other: &Self) -> usize {
        self.settled
            .iter()
            .zip(other.settled.iter())
            .take_while(|(mine, theirs)| Arc::ptr_eq(mine, theirs))
            .map(|(chunk, _)| chunk.len())
            .sum()
    }

    /// Whether this list holds exactly `items`.
    pub fn eq_slice(&self, items: &[TranscriptItem]) -> bool {
        self.len() == items.len() && self.iter().zip(items.iter()).all(|(a, b)| a == b)
    }
}

impl PartialEq for TranscriptItems {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let shared = self.shared_prefix(other);
        self.iter()
            .zip(other.iter())
            .skip(shared)
            .all(|(a, b)| a == b)
    }
}

impl std::fmt::Debug for TranscriptItems {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

/// Written as the one flat array the list stands for.
impl Serialize for TranscriptItems {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.len()))?;
        for item in self.iter() {
            sequence.serialize_element(item)?;
        }
        sequence.end()
    }
}

/// A row list the frame's splice reads: a projection's [`TranscriptItems`], or a plain list.
pub trait ItemRows {
    fn row_count(&self) -> usize;
    fn row(&self, index: usize) -> &TranscriptItem;
    /// Whether two rows are one shared row, the same value without comparing them.
    fn same_row(&self, index: usize, other: &Self, other_index: usize) -> bool;
}

impl ItemRows for TranscriptItems {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn row(&self, index: usize) -> &TranscriptItem {
        self.get(index).expect("a row inside the list")
    }

    fn same_row(&self, index: usize, other: &Self, other_index: usize) -> bool {
        TranscriptItems::same_row(self, index, other, other_index)
    }
}

impl ItemRows for [TranscriptItem] {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn row(&self, index: usize) -> &TranscriptItem {
        &self[index]
    }

    fn same_row(&self, _index: usize, _other: &Self, _other_index: usize) -> bool {
        false
    }
}
