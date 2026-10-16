//! Where Settings reopens: the page and its scroll position (`settingsModalNavigation`, saved by
//! the store with the settings) and the in-page dialog or form each page had open, with its draft
//! (`OpenFormMemory`, one client-storage value the host reads and writes).
//!
//! CDXC:Settings 2026-10-16 DECISION:
//! User: "i want reopening settings to always reopen to the exact same spot i was in before (same scroll pos too and same open modals if possible) do good system for this pls", and for Remote's Add machine dialog "save my last entered values there until i add them so i can restart the app and they stay there (clear them when i connect it ofc)". However Settings closes (a click away, Escape, the X, quitting the app), the next open returns to the same page, scrolled to the same place, with the dialog or form that was open (pages implementing `RemembersOpenForm` in tabs/open_forms.rs) reopened on its draft, across restarts. A form's draft is forgotten when it is saved, cancelled or dismissed inside Settings, never when Settings closes. Passwords never go into the draft. A deep link (a named row, section, search, machine or path card) still wins for the page it opens; that page's remembered form waits for a later plain open.
use super::model::{SettingsOpenRequest, SettingsTabId};
use serde_json::{Map, Value};
use std::time::Duration;

/// The client-storage key of the remembered open forms (`settingsOpenForms` in
/// packages/client-storage/catalog.ts).
pub(crate) const OPEN_FORMS_STORAGE_KEY: &str = "ghostex.settingsModal.openForms.v1";

/// How long typing in a form settles before its draft is written.
pub(crate) const OPEN_FORMS_SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// Whether an open request deep-links into its page (a row, section, search, machine, path card,
/// view or preview state), which wins over that page's remembered form.
pub(crate) fn request_deep_links(request: &SettingsOpenRequest) -> bool {
    request.initial_section.is_some()
        || request.initial_sidebar_tags_action.is_some()
        || request
            .initial_search_query
            .as_deref()
            .is_some_and(|query| !query.trim().is_empty())
        || request.initial_remote_machine_id.is_some()
        || request.initial_remote_section.is_some()
        || request.initial_agents_section.is_some()
        || request.initial_custom_view_id.is_some()
        || request.initial_view_scope_key.is_some()
        || request.open_color_picker.is_some()
        || request.open_select.is_some()
        || request.preview_state.is_some()
}

/// The forms the last visit left open, by page id, until the pages of this visit take them back.
#[derive(Default)]
pub(crate) struct OpenFormMemory {
    /// Saved forms no page of this visit has taken back (or discarded) yet.
    untaken: Map<String, Value>,
    /// The page this open deep-linked into: its saved form stays untaken.
    deferred: Option<SettingsTabId>,
    /// The value last written, so an unchanged draft is not written again.
    last_saved: Map<String, Value>,
}

impl OpenFormMemory {
    /// The stored value as the host read it (`{ "version": 1, "forms": { "<page id>": form } }`).
    pub(crate) fn from_stored(stored: &Value) -> Self {
        let forms = stored
            .get("forms")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Self {
            untaken: forms.clone(),
            deferred: None,
            last_saved: forms,
        }
    }

    /// Keeps `tab`'s saved form for a later open (this open deep-linked into `tab`).
    pub(crate) fn defer(&mut self, tab: SettingsTabId) {
        self.deferred = Some(tab);
    }

    /// The form `tab` had open, for the page to reopen once; `None` when it had none or this open
    /// deep-linked into it.
    pub(crate) fn take(&mut self, tab: SettingsTabId) -> Option<Value> {
        if self.deferred == Some(tab) {
            return None;
        }
        self.untaken.remove(tab.id())
    }

    /// The page opened a form of its own, which replaces the one it had saved.
    pub(crate) fn discard(&mut self, tab: SettingsTabId) {
        self.untaken.remove(tab.id());
    }

    /// The value to store now from the forms the open pages report, or `None` when it did not
    /// change; `Some(None)` removes the stored value.
    pub(crate) fn next_stored(
        &mut self,
        live: Vec<(SettingsTabId, Option<Value>)>,
    ) -> Option<Option<String>> {
        for (tab, form) in &live {
            // A page that opened a form of its own has replaced the one it saved.
            if form.is_some() {
                self.untaken.remove(tab.id());
            }
        }
        let mut forms = self.untaken.clone();
        for (tab, form) in live {
            if let Some(form) = form {
                forms.insert(tab.id().to_string(), form);
            }
        }
        if forms == self.last_saved {
            return None;
        }
        self.last_saved = forms.clone();
        if forms.is_empty() {
            return Some(None);
        }
        let mut stored = Map::new();
        stored.insert("forms".to_string(), Value::Object(forms));
        stored.insert("version".to_string(), Value::from(1));
        Some(serde_json::to_string(&Value::Object(stored)).ok())
    }
}
