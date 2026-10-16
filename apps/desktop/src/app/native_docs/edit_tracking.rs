//! Following a Markdown editor's edits between full recomputes. Each change to the text moves the
//! note highlights and the git stripe's marks with the edit at once; the notes are found again,
//! and the diagrams and formulas prerendered, once typing pauses, and the git stripe's diff runs
//! off the main thread.
//!
//! CDXC:Docs 2026-10-16 WHY:
//! A keystroke in a long Markdown file used to find every note's quote again, diff the whole file against its git base and scan it for diagrams, formulas and images, all on the main thread (about 3.4 ms per keystroke on a 235 KB file with 30 notes in an optimized build, 20 ms in the dev build). The marks now move with the edit instead (under 0.1 ms), so nothing visibly lags, and the full answers follow a moment later. An edit that cuts across a note's quote finds the notes again at once, since only its quote can tell where it went.

use std::ops::Range;
use std::time::Duration;

use gpui::{Context, Entity};
use zorite_editor::EditorState;

use super::annotations::DocsAnnotationRange;
use super::gutter::LineChange;
use crate::GhostexGpuiApp;

/// How long typing has to pause before the notes are found again and blocks prerendered.
const TYPING_SETTLE: Duration = Duration::from_millis(250);

/// One edit between two texts: `start..old_end` of the old text became `start..new_end` of the
/// new one. Both ends sit on character boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocsTextEdit {
    pub(crate) start: usize,
    pub(crate) old_end: usize,
    pub(crate) new_end: usize,
}

impl DocsTextEdit {
    /// The one edit that turns `old` into `new`: what lies between their common start and end.
    /// `None` when the texts are the same.
    pub(crate) fn between(old: &str, new: &str) -> Option<Self> {
        if old == new {
            return None;
        }
        let mut start = common_prefix(old.as_bytes(), new.as_bytes());
        while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
            start -= 1;
        }
        let room = old.len().min(new.len()) - start;
        let mut suffix = common_suffix(old.as_bytes(), new.as_bytes(), room);
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix)
        {
            suffix -= 1;
        }
        Some(Self {
            start,
            old_end: old.len() - suffix,
            new_end: new.len() - suffix,
        })
    }

    fn delta(&self) -> isize {
        self.new_end as isize - self.old_end as isize
    }

    /// `range` moved with the edit: unchanged before it, shifted after it, stretched when it
    /// contains it. `None` when the range overlaps only part of the edit, where only finding its
    /// quote again can tell where it goes.
    fn shift_range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        if range.end <= self.start {
            Some(range.clone())
        } else if range.start >= self.old_end {
            Some(
                range.start.saturating_add_signed(self.delta())
                    ..range.end.saturating_add_signed(self.delta()),
            )
        } else if range.start <= self.start && range.end >= self.old_end {
            Some(range.start..range.end.saturating_add_signed(self.delta()))
        } else {
            None
        }
    }

    /// Moves note ranges with the edit; `false` when one of them overlaps only part of it.
    pub(crate) fn shift_note_ranges(&self, ranges: &mut [DocsAnnotationRange]) -> bool {
        for found in ranges.iter_mut() {
            let Some(range) = self.shift_range(&found.range) else {
                return false;
            };
            found.range = range;
        }
        true
    }

    /// Moves the git stripe's marks with the edit (`new` is the edited text): the edited line keeps
    /// its mark, the lines it gained count as added, and the lines below move with it, until the
    /// diff of the new text lands.
    pub(crate) fn shift_line_changes(
        &self,
        old: &str,
        new: &str,
        changes: &mut (Vec<LineChange>, Vec<usize>),
    ) {
        let newlines = |text: &str| text.matches('\n').count();
        let row = newlines(&new[..self.start]);
        let removed = newlines(&old[self.start..self.old_end]);
        let added = newlines(&new[self.start..self.new_end]);
        let (lines, deleted_before) = changes;
        let first = (row + 1).min(lines.len());
        let end = (row + 1 + removed).min(lines.len());
        lines.splice(first..end, std::iter::repeat_n(LineChange::Added, added));
        for line in deleted_before.iter_mut() {
            if *line >= end {
                *line = (*line + first + added).saturating_sub(end);
            } else if *line > first {
                *line = first;
            }
        }
    }
}

/// Slices are compared a chunk at a time (one `memcmp` each), which stays fast in the dev build,
/// where a byte-by-byte loop over a long file costs milliseconds.
const COMPARE_CHUNK: usize = 1024;

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let len = a.len().min(b.len());
    let mut at = 0;
    while at + COMPARE_CHUNK <= len && a[at..at + COMPARE_CHUNK] == b[at..at + COMPARE_CHUNK] {
        at += COMPARE_CHUNK;
    }
    while at < len && a[at] == b[at] {
        at += 1;
    }
    at
}

/// The common end of `a` and `b`, at most `room` bytes.
fn common_suffix(a: &[u8], b: &[u8], room: usize) -> usize {
    let mut n = 0;
    while n + COMPARE_CHUNK <= room
        && a[a.len() - n - COMPARE_CHUNK..a.len() - n]
            == b[b.len() - n - COMPARE_CHUNK..b.len() - n]
    {
        n += COMPARE_CHUNK;
    }
    while n < room && a[a.len() - n - 1] == b[b.len() - n - 1] {
        n += 1;
    }
    n
}

impl GhostexGpuiApp {
    /// A change to `path`'s live editor (`EditorEvent::Changed`): moves the note highlights and
    /// git marks with the edit, starts the git diff in the background, and leaves finding the
    /// notes again and prerendering blocks to [`TYPING_SETTLE`] after the last change.
    pub(crate) fn native_docs_follow_edit(
        &mut self,
        path: &str,
        editor: &Entity<EditorState>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        // The note ranges belong to the open document once its highlights have been drawn.
        let highlighted = self.native_docs.active.as_deref() == Some(path)
            && self.native_docs.highlighted_path == self.native_docs.active;
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        let old = document.tracked_text.replace(text.to_string());
        // An editor whose text was not tracked yet has nothing to move: everything is redone.
        let edit = match old.as_deref() {
            Some(old) => match DocsTextEdit::between(old, text) {
                Some(edit) => Some(edit),
                None => return,
            },
            None => None,
        };
        if let (Some(old), Some(edit), Some(changes)) =
            (old.as_deref(), edit, document.changes.as_mut())
        {
            edit.shift_line_changes(old, text, changes);
        }
        self.native_docs_diff_in_background(path, cx);
        if highlighted {
            let followed =
                edit.is_some_and(|edit| edit.shift_note_ranges(&mut self.native_docs.note_ranges));
            if followed {
                let highlights = self.native_docs_note_highlights();
                editor.update(cx, |editor, cx| editor.set_note_highlights(highlights, cx));
            } else {
                self.native_docs.highlights_stale = true;
            }
        }
        let settled = path.to_string();
        let settle = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TYPING_SETTLE).await;
            let _ = this.update(cx, |this, cx| {
                if this.native_docs.highlighted_path.as_deref() == Some(settled.as_str()) {
                    this.native_docs.highlights_stale = true;
                }
                this.native_docs_prerender_blocks(&settled, cx);
                this.native_docs_notify(cx);
            });
        });
        if let Some(document) = self.native_docs.document_mut(path) {
            document.typing_settle = Some(settle);
        }
    }

    /// Diffs `path`'s live text against its git base on a background thread; a later edit's diff
    /// replaces one still running.
    pub(crate) fn native_docs_diff_in_background(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        let (Some(base), Some(text)) = (document.git_base.clone(), document.tracked_text.clone())
        else {
            document.changes_task = None;
            return;
        };
        let path = path.to_string();
        document.changes_task = Some(cx.spawn(async move |this, cx| {
            let changes = cx
                .background_executor()
                .spawn(async move { super::gutter::diff(&base, &text) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(document) = this.native_docs.document_mut(&path) {
                    document.changes = Some(changes);
                    document.changes_task = None;
                }
                this.native_docs_notify(cx);
            });
        }));
    }

    /// Starts renders for the diagrams, formulas and images in `path`'s live editor that are not
    /// cached yet.
    pub(crate) fn native_docs_prerender_blocks(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(editor) = self
            .native_docs
            .document(path)
            .and_then(|document| document.live.clone())
        else {
            return;
        };
        let light = self.native_docs.palette.as_ref().is_some_and(|p| p.light);
        let scope = self.manage_docs_resource_scope();
        let resource_path = self.native_docs_resource_path(path);
        super::blocks::prerender(
            &editor,
            &self.native_docs.blocks,
            &resource_path,
            light,
            scope,
            cx,
        );
    }
}
