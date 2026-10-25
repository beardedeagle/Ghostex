//! The desktop's side of the Changes view (app/native_changes/): rendering it in the view panel,
//! opening it from the Git entry points, and doing what it asks (commits, comments into the
//! session's composer, opening files, toasts).

use gpui::{
    AnyElement, AnyView, AppContext as _, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, StyleRefinement, Styled as _, Window, div,
};

use crate::GhostexGpuiApp;
use crate::app::gx_store::{read_preference_value, write_client_document_value};
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::native_changes::{
    ChangesHostFn, ChangesHostInfo, ChangesPreset, ChangesRequest, ChangesView,
};
use crate::app::session_chat_context_menu::GpuiResolvedSessionChatFile;
use crate::shared_settings;

/// The commit agent the user last picked (client-storage store `commitAgent`, which the commit
/// review window used before the Changes view replaced it, so the old pick carries over).
const COMMIT_AGENT_STORAGE_KEY: &str = "ghostex.promptAgent.gitCommit";

impl GhostexGpuiApp {
    pub(crate) fn render_native_changes_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let view = match self.native_changes.clone() {
            Some(view) => view,
            None => {
                let app = cx.weak_entity();
                let host: ChangesHostFn = std::rc::Rc::new(move |request, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.handle_changes_request(request, window, cx)
                    });
                });
                let view = cx.new(|cx| ChangesView::new(host, cx));
                self.native_changes = Some(view.clone());
                view
            }
        };
        let (agents, default_agent_id) = self.changes_commit_agents();
        let saved_agent_id = read_preference_value(COMMIT_AGENT_STORAGE_KEY)
            .ok()
            .flatten()
            .map(|id| id.trim().to_string())
            .filter(|id| agents.iter().any(|agent| &agent.0 == id));
        let default_agent_id = saved_agent_id.or(default_agent_id);
        let info = ChangesHostInfo {
            palette: self.gpui_native_modal_palette(),
            agents,
            default_agent_id,
        };
        let target = self.changes_target_for_active_project();
        let preset = self.native_changes_preset.take();
        let changed = view.update(cx, |view, cx| view.sync(target, info, preset, window, cx));
        if changed {
            window.render_view_this_frame(view.entity_id());
        }
        div()
            .id("ghostex-gpui-native-changes-host")
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.focus_project_editor_surface(TitlebarMode::Changes, window, cx);
                }),
            )
            .child(AnyView::from(view).cached(StyleRefinement::default().size_full()))
            .into_any_element()
    }

    /// Opens the Changes tab for the active project, on `preset`'s scope and commit action.
    pub(crate) fn open_changes_view(
        &mut self,
        preset: Option<ChangesPreset>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.native_changes_preset = preset;
        self.defer_in_main_window(cx, |this, window, cx| {
            if !this.open_view_tab(TitlebarMode::Changes, window, cx) {
                this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Changes is not available for this project",
                    "",
                    cx,
                );
            }
            if let Some(view) = this.native_changes.clone() {
                gpui::App::notify(cx, view.entity_id());
            }
        });
    }

    fn handle_changes_request(
        &mut self,
        request: ChangesRequest,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match request {
            ChangesRequest::Commit {
                project_key,
                action,
                message,
                agent_id,
                file_paths,
                new_branch,
            } => self.changes_commit(
                &project_key,
                action,
                message,
                agent_id,
                file_paths,
                new_branch,
                cx,
            ),
            ChangesRequest::MultipleCommits {
                project_key,
                agent_id,
            } => self.changes_multiple_commits(&project_key, agent_id, cx),
            ChangesRequest::MergeIntoMain {
                project_key,
                message,
                agent_id,
                file_paths,
            } => self.changes_merge_into_main(&project_key, message, agent_id, file_paths, cx),
            ChangesRequest::Comment { text, done } => {
                // The comment goes where Files annotations go: the session last clicked in the
                // sidebar for this project, its chat composer or its terminal, else the clipboard.
                let target = self.docs_annotation_feedback_target();
                self.deliver_docs_annotation_feedback(text, None, cx, move |_, delivery, cx| {
                    let label = match (delivery, target) {
                        (Ok("chat"), Some(target)) => format!("{}'s chat", target.agent_label),
                        (Ok("terminal"), Some(target)) => {
                            format!("{}'s terminal", target.agent_label)
                        }
                        (Ok(_), _) => "the clipboard".to_string(),
                        (Err(_), _) => String::new(),
                    };
                    done(label, cx);
                });
            }
            ChangesRequest::OpenInFiles {
                project_key,
                path,
                line,
            } => {
                let Some(root) = self.changes_project_root(&project_key) else {
                    return self.dispatch_gpui_app_modal_toast(
                        "info",
                        "Files can only open files of a project on this computer",
                        "",
                        cx,
                    );
                };
                let project_id = project_key.clone();
                self.open_resolved_project_file(
                    GpuiResolvedSessionChatFile {
                        file_path: root.join(&path),
                        is_directory: false,
                        project_id,
                        project_path: root.clone(),
                        project_root: root,
                    },
                    line,
                    None,
                    Some(shared_settings::SharedChatFileOpenView::Docs),
                    None,
                    window,
                    cx,
                );
            }
            ChangesRequest::RevealFile { project_key, path } => {
                let Some(root) = self.changes_project_root(&project_key) else {
                    return self.dispatch_gpui_app_modal_toast(
                        "info",
                        "This needs a project on this computer",
                        "",
                        cx,
                    );
                };
                let full = root.join(&path);
                let target = if full.exists() { full } else { root };
                if let Err(error) = gpui_reveal_path_in_finder(&target) {
                    self.dispatch_gpui_app_modal_toast(
                        "error",
                        "Could not open the location",
                        &error,
                        cx,
                    );
                }
            }
            ChangesRequest::AgentPicked(agent_id) => {
                cx.background_executor()
                    .spawn(async move {
                        let _ =
                            write_client_document_value(COMMIT_AGENT_STORAGE_KEY, Some(&agent_id));
                    })
                    .detach();
            }
            ChangesRequest::Copied => gpui_play_copy_sound(),
            ChangesRequest::Toast {
                title,
                description,
                error,
            } => self.dispatch_gpui_app_modal_toast(
                if error { "error" } else { "info" },
                &title,
                description.as_deref().unwrap_or(""),
                cx,
            ),
            ChangesRequest::WorkingTreeChanged { project_key } => {
                self.git_refresh_scoped(&project_key, cx);
            }
        }
    }
}
