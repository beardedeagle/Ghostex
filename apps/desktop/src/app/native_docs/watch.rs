//! Keeping Docs current while it is on screen: the open file is checked for changes on disk, and
//! the files list is listed again every few seconds.

use std::time::Duration;

use gpui::Context;
use serde_json::json;

use super::entry::is_review_path;
use super::state::{DocsDiskState, DocsFileKind};
use crate::GhostexGpuiApp;
use crate::app::model::TitlebarMode;

/// `stat` of the open file, as the page polled it.
const FILE_POLL: Duration = Duration::from_millis(400);
/// The files list refresh while visible.
const LIST_POLL: Duration = Duration::from_secs(5);
/// A change is acted on once the file has stopped changing for this long.
const CHANGE_SETTLE: Duration = Duration::from_millis(500);

/// `manageFileMetadataSignature`.
pub(crate) fn disk_signature(file: &serde_json::Value) -> Option<String> {
    let path = file["path"].as_str()?;
    let modified = file["modifiedAt"].as_str().unwrap_or_default();
    let size = file["size"]
        .as_u64()
        .map(|size| size.to_string())
        .unwrap_or_default();
    Some(format!("{path}\0{modified}\0{size}"))
}

impl GhostexGpuiApp {
    /// Starts the poll once; it idles while Docs is not the view on screen.
    pub(crate) fn native_docs_ensure_watch(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.watch_task.is_some() {
            return;
        }
        self.native_docs.watch_task = Some(cx.spawn(async move |this, cx| {
            let mut since_list = Duration::ZERO;
            loop {
                cx.background_executor().timer(FILE_POLL).await;
                since_list += FILE_POLL;
                let refresh_list = since_list >= LIST_POLL;
                if refresh_list {
                    since_list = Duration::ZERO;
                }
                let alive = this.update(cx, |this, cx| {
                    if this.active_mode != TitlebarMode::Manage {
                        return;
                    }
                    this.native_docs_poll_active_file(cx);
                    if refresh_list {
                        this.native_docs_refresh(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// The browser area's page (an Excalidraw drawing's autosave) is writing `path`: the change
    /// poll stands down until [`Self::native_docs_finish_page_save`] records the written file.
    ///
    /// CDXC:Docs 2026-10-01 WHY:
    /// The page's own saves went around the document's state, so the poll saw each autosave as a change on disk and reloaded the page (a new `revision`), remounting the drawing every few seconds while the user drew. The save is marked like a native one (`saving`, then the written file's signature) so only a write from elsewhere reloads the page.
    pub(crate) fn native_docs_begin_page_save(&mut self, path: &str) {
        if let Some(document) = self.native_docs.document_mut(path) {
            document.page_saves_in_flight += 1;
            document.saving = true;
        }
    }

    /// The page's save answered: its file becomes the known disk state.
    pub(crate) fn native_docs_finish_page_save(
        &mut self,
        path: &str,
        response: &serde_json::Value,
    ) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.page_saves_in_flight = document.page_saves_in_flight.saturating_sub(1);
        document.saving = document.page_saves_in_flight > 0;
        if response.get("error").is_none()
            && let Some(signature) = disk_signature(&response["file"])
        {
            document.disk_signature = Some(signature);
            document.size = response["file"]["size"].as_u64().or(document.size);
        }
    }

    /// Checks the open file on disk; a change that settles goes to
    /// [`Self::native_docs_on_disk_changed`], a missing file to [`Self::native_docs_on_disk_missing`].
    /// Unsaved edits do not stop the check: VS Code keeps watching a dirty file so it can flag the
    /// conflict instead of overwriting either side.
    fn native_docs_poll_active_file(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.active_document() else {
            return;
        };
        if is_review_path(&document.path)
            || document.kind == DocsFileKind::SystemApp
            || document.load != super::state::DocsDocumentLoad::Ready
            || document.saving
            || self.native_docs.stat_in_flight
        {
            return;
        }
        let path = document.path.clone();
        let generation = self.native_docs.generation;
        self.native_docs.stat_in_flight = true;
        let request = self.native_docs_request("stat", json!({ "path": path }));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            this.native_docs.stat_in_flight = false;
            if this.native_docs.generation != generation {
                return;
            }
            if response["file"]["missing"].as_bool() == Some(true) {
                this.native_docs_on_disk_missing(&path, cx);
                return;
            }
            // A failed stat is transient (the file mid-write, a slow remote): ignore it.
            let Some(signature) = disk_signature(&response["file"]) else {
                return;
            };
            let Some(document) = this.native_docs.document_mut(&path) else {
                return;
            };
            if document.saving {
                return;
            }
            // A deleted file that is back counts as changed, whatever it holds now.
            let returned = document.disk == DocsDiskState::Deleted;
            if returned {
                document.disk = DocsDiskState::InSync;
            }
            match document.disk_signature.as_deref() {
                None if !returned => document.disk_signature = Some(signature),
                Some(known) if known == signature => {}
                _ => {
                    document.disk_signature = Some(signature.clone());
                    document.size = response["file"]["size"].as_u64().or(document.size);
                    let settled = path.clone();
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(CHANGE_SETTLE).await;
                        let _ = this.update(cx, |this, cx| {
                            let Some(document) = this.native_docs.document(&settled) else {
                                return;
                            };
                            // Changed again meanwhile, or being saved: the later poll acts on it.
                            if document.disk_signature.as_deref() != Some(signature.as_str())
                                || document.saving
                            {
                                return;
                            }
                            this.native_docs_on_disk_changed(&settled, cx);
                        });
                    })
                    .detach();
                }
            }
        });
    }
}
