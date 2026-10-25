//! The File diff dialog: one patch in a window of its own, the desktop twin of the React
//! `GitFileDiffModal` (packages/core-ui/git-file-diff-modal.tsx (deleted 2026-10-01)). Files uses it
//! to compare an open document with the copy on disk. The commit review window that shared this
//! diff surface was replaced by the Changes view (app/native_changes/) on 2026-10-25.
//!
//! This module depends only on gpui, gpui-component (with its gpui-base text selection) and
//! `native_modal_kit/` so the preview binary can include it.
mod diff;
mod diff_select;
mod diff_view;
mod diff_wrap;
mod file_diff_window;
mod focus;
mod model;

pub(crate) use file_diff_window::*;
pub(crate) use model::GitFileDiffDraft;
// Named only by the preview binary (src/bin/native_modal_demo/file_diff.rs).
#[allow(unused_imports)]
pub(crate) use diff::{GitDiffPrefs, GitDiffViewMode};
