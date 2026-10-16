//! The custom view editor and the view scope editor as Settings remembers them between visits
//! (CDXC:Settings 2026-10-16 in settings_modal/restore.rs).
use super::super::super::model::SettingsTabId;
use super::ExtensionsTab;
use super::data::{ViewScope, custom_views};
use super::view_editor::ViewEditorState;
use gpui::{App, Context};
use serde_json::{Map, Value, json};

impl super::super::open_forms::RemembersOpenForm for ExtensionsTab {
    /// The open custom view editor and view scope editor, each with its draft.
    fn open_form(&self, _cx: &App) -> Option<Value> {
        if self.view_editor.is_none() && self.scope_editor.is_none() {
            return None;
        }
        Some(json!({
            "kind": "views",
            "view": self.view_editor.as_ref().map(|editor| json!({
                "id": editor.id,
                "draft": Value::Object(editor.draft.clone()),
            })),
            "scope": self.scope_editor.as_ref().map(|editor| json!({
                "key": editor.key,
                "draft": editor.draft.to_value(),
            })),
        }))
    }
}

impl ExtensionsTab {
    /// Reopens the editors the last visit left open; the edit of a view removed since, and the
    /// scope of a view that is gone, are dropped.
    pub(super) fn restore_open_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.store.update(cx, |store, _| {
            store.open_forms_mut().take(SettingsTabId::Extensions)
        }) else {
            return;
        };
        if let Some(view) = form.get("view").filter(|view| view.is_object()) {
            let id = view.get("id").and_then(Value::as_str).map(str::to_string);
            let exists = id.as_deref().is_none_or(|id| {
                custom_views(&self.store.read(cx).values())
                    .iter()
                    .any(|view| view.id() == id)
            });
            if exists {
                self.view_editor = Some(ViewEditorState {
                    draft: view
                        .get("draft")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_else(Map::new),
                    id,
                    error: None,
                    notice: None,
                });
            }
        }
        if let Some(scope) = form.get("scope")
            && let Some(key) = scope.get("key").and_then(Value::as_str)
            && let Some(title) = self.scope_editor_title(key, cx)
        {
            self.scope_editor = Some(super::ScopeEditorState {
                draft: ViewScope::from_value(scope.get("draft").unwrap_or(&Value::Null)),
                key: key.to_string(),
                title,
            });
        }
    }
}
