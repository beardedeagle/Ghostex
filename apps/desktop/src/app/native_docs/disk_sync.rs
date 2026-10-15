//! Following the open file when it changes on disk, the way VS Code's text file models do: a
//! document with no unsaved edits takes the new text in place (caret and scroll stay), unsaved
//! edits are never overwritten (the document goes into conflict until the user compares, keeps
//! theirs or takes the disk's), and a deleted file keeps its text, marked deleted, until it is
//! saved again or comes back.
//!
//! CDXC:Docs 2026-10-15 DECISION:
//! User: "can we please remove the refresh button and do something smarter? look at what vscode does for files when they get edited". The header has no Reload button: a clean document reloads silently from disk, a document with unsaved edits shows a notice with Compare, Keep mine (overwrite the file) and Take theirs (revert to the disk), and a file deleted on disk is marked deleted and comes back on Save. Supersedes the Markdown-only "Reload to show new changes" dot on the Reload button.

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

use super::files_list::ROW_STRIP_HEIGHT;
use super::palette::DocsPalette;
use super::state::{DocsDiskState, DocsFileKind};
use crate::GhostexGpuiApp;
use crate::app::consts::{TITLEBAR_BUTTON_RADIUS, TITLEBAR_CONTROL_HEIGHT};

impl GhostexGpuiApp {
    /// The watch saw the file missing on disk.
    pub(crate) fn native_docs_on_disk_missing(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        if document.disk == DocsDiskState::Deleted {
            return;
        }
        document.disk = DocsDiskState::Deleted;
        document.disk_signature = None;
        self.native_docs_notify(cx);
    }

    /// The file on disk settled on new content: reload it, or hold it as a conflict when the
    /// document has unsaved edits.
    pub(crate) fn native_docs_on_disk_changed(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document(path) else {
            return;
        };
        // Pages (HTML, Excalidraw), pictures and media have no buffer of their own to protect:
        // they load the file again.
        if document.editor.is_none() && document.live.is_none() {
            if let Some(document) = self.native_docs.document_mut(path) {
                document.disk = DocsDiskState::InSync;
            }
            self.native_docs_reload(path, cx);
            return;
        }
        let generation = self.native_docs.generation;
        let request = self.native_docs_request("read", json!({ "path": path }));
        let path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            let file = &response["file"];
            if response.get("error").is_some()
                || file.get("error").is_some()
                || file["kind"].as_str() == Some("unsupported")
            {
                return;
            }
            let Some(text) = file["content"].as_str().map(str::to_string) else {
                return;
            };
            let signature = super::watch::disk_signature(file);
            let Some(document) = this.native_docs.document_mut(&path) else {
                return;
            };
            if signature.is_some() {
                document.disk_signature = signature;
            }
            document.size = file["size"].as_u64().or(document.size);
            let current = super::live::document_text(document, cx);
            if current.as_deref() == Some(text.as_str()) {
                // The disk caught up with the buffer (the same edit made elsewhere).
                document.saved_text = text;
                document.dirty = false;
                document.disk = DocsDiskState::InSync;
                this.native_docs_clear_draft(&path, cx);
                this.native_docs_notify(cx);
            } else if document.dirty {
                document.disk = DocsDiskState::Conflict(text);
                this.native_docs_notify(cx);
            } else {
                this.native_docs_apply_disk_text(&path, text, cx);
            }
        });
    }

    /// Puts `text` (the file as it is on disk) into the document without rebuilding its editor,
    /// so the caret and the scroll position stay where they were.
    pub(crate) fn native_docs_apply_disk_text(
        &mut self,
        path: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.saved_text = text.clone();
        document.dirty = false;
        document.disk = DocsDiskState::InSync;
        if let Some(live) = document.live.clone() {
            live.update(cx, |editor, cx| {
                let selection = editor.selected_range();
                editor.set_text(text.clone(), cx);
                editor.select_range(selection, cx);
            });
            document.changes = document
                .git_base
                .as_deref()
                .map(|base| super::gutter::diff(base, &text));
            let light = self.native_docs.palette.as_ref().is_some_and(|p| p.light);
            let scope = self.manage_docs_resource_scope();
            let resource_path = self.native_docs_resource_path(path);
            super::blocks::prerender(
                &live,
                &self.native_docs.blocks,
                &resource_path,
                light,
                scope,
                cx,
            );
        } else {
            // The code editor takes text only with a window at hand: the next draw applies it.
            document.disk_reload = Some(text);
        }
        let redraw_picture = self
            .native_docs
            .document(path)
            .is_some_and(|document| document.kind == DocsFileKind::Image);
        if redraw_picture {
            self.native_docs_read_image(path, cx);
        }
        self.native_docs.highlights_stale = true;
        self.native_docs_clear_draft(path, cx);
        self.native_docs_notify(cx);
    }

    /// Take theirs: drops the unsaved edits for the file as it is on disk.
    pub(crate) fn native_docs_take_disk_version(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document(path) else {
            return;
        };
        let DocsDiskState::Conflict(text) = document.disk.clone() else {
            return;
        };
        self.native_docs_apply_disk_text(path, text, cx);
    }

    /// Keep mine: writes the unsaved edits over the file on disk.
    pub(crate) fn native_docs_keep_my_version(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(document) = self.native_docs.document_mut(path) {
            document.disk = DocsDiskState::InSync;
        }
        self.native_docs_save(path, cx, |_, _| {});
    }

    /// Compare: the File diff dialog, from the file on disk to the unsaved edits.
    pub(crate) fn native_docs_compare_with_disk(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document(path) else {
            return;
        };
        let DocsDiskState::Conflict(disk) = &document.disk else {
            return;
        };
        let Some(mine) = super::live::document_text(document, cx) else {
            return;
        };
        let diff = similar::TextDiff::from_lines(disk.as_str(), mine.as_str());
        let (mut additions, mut deletions) = (0u64, 0u64);
        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Insert => additions += 1,
                similar::ChangeTag::Delete => deletions += 1,
                similar::ChangeTag::Equal => {}
            }
        }
        let patch = diff
            .unified_diff()
            .context_radius(3)
            .header("On disk", "Your changes")
            .to_string();
        let message = json!({
            "gitFileDiff": {
                "filePath": format!("{} (on disk ↔ your changes)", document.display_path),
                "patch": patch,
                "additions": additions,
                "deletions": deletions,
            }
        });
        self.open_gpui_git_file_diff_modal(&message, cx);
    }

    /// The row under the header while the open file is in conflict with the disk or deleted from
    /// it: what happened, and the ways out.
    ///
    /// CDXC:Docs 2026-10-16 DECISION: User: "make height as big as these pls" (the header's wrap and panel buttons) about the notice's "too small" buttons. The row is as tall as the header row and its buttons as tall as the header's tiles, from the same constants, so the gaps above and below match the header's.
    pub(crate) fn render_native_docs_disk_notice(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let document = self.native_docs.active_document()?;
        let path = document.path.clone();
        let (message, actions): (&str, Vec<(&'static str, DiskAction)>) = match &document.disk {
            DocsDiskState::InSync => return None,
            DocsDiskState::Conflict(_) => (
                "This file changed on disk while you had unsaved changes.",
                vec![
                    ("Compare", DiskAction::Compare),
                    ("Keep mine", DiskAction::KeepMine),
                    ("Take theirs", DiskAction::TakeTheirs),
                ],
            ),
            // Only a document with its own text can write the file back.
            DocsDiskState::Deleted if document.editor.is_some() || document.live.is_some() => (
                "This file was deleted on disk. Save to bring it back.",
                vec![("Save", DiskAction::Save), ("Close", DiskAction::Close)],
            ),
            DocsDiskState::Deleted => (
                "This file was deleted on disk.",
                vec![("Close", DiskAction::Close)],
            ),
        };
        let button = |label: &'static str, action: DiskAction| {
            let path = path.clone();
            div()
                .id(SharedString::from(format!("docs-disk-{label}")))
                .flex()
                .flex_none()
                .items_center()
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(10.0))
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .bg(p.control_hover)
                .text_color(p.text)
                .text_size(px(12.0))
                .cursor_pointer()
                .hover(|style| style.bg(p.row_surface))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| match action {
                    DiskAction::Compare => this.native_docs_compare_with_disk(&path, cx),
                    DiskAction::KeepMine => this.native_docs_keep_my_version(&path, cx),
                    DiskAction::TakeTheirs => this.native_docs_take_disk_version(&path, cx),
                    DiskAction::Save => this.native_docs_save(&path, cx, |_, _| {}),
                    DiskAction::Close => this.native_docs_request_close(&path, window, cx),
                }))
        };
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(ROW_STRIP_HEIGHT))
                .px(px(13.0))
                .border_b_1()
                .border_color(p.border)
                .bg(p.chrome)
                .text_size(px(12.0))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(message),
                )
                .children(
                    actions
                        .into_iter()
                        .map(|(label, action)| button(label, action)),
                )
                .into_any_element(),
        )
    }
}

#[derive(Clone, Copy)]
enum DiskAction {
    Compare,
    KeepMine,
    TakeTheirs,
    Save,
    Close,
}
