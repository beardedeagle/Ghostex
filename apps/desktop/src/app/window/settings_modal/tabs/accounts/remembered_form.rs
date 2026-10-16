//! The Add account setup as Settings remembers it between visits (CDXC:Settings 2026-10-16 in
//! settings_modal/restore.rs): the provider, and for "Sign in to a new account" the email and
//! consent typed so far. A sign-in that started (or finished) is never brought back, since its
//! browser step cannot resume; a sign-in code is never stored.
use super::super::super::model::SettingsTabId;
use super::connect::{FlowState, job_active};
use super::data::PROVIDERS;
use super::manager::SetupDraft;
use super::{AccountsTab, Pane};
use gpui::{App, Context};
use serde_json::{Value, json};

impl super::super::open_forms::RemembersOpenForm for AccountsTab {
    /// The Add account setup the user opened.
    fn open_form(&self, _cx: &App) -> Option<Value> {
        let Some(Pane::Add(provider)) = self.pane.as_ref() else {
            return None;
        };
        let mut form = json!({ "kind": "addAccount", "provider": provider });
        let new_login = self
            .setups
            .get(*provider)
            .is_none_or(|draft| draft.selected == "new");
        let flow = self
            .flows
            .get(&format!("setup:{provider}:new"))
            .filter(|flow| new_login && flow.job.is_none() && !flow.starting);
        if let Some(flow) = flow {
            form["email"] = json!(flow.email);
            form["consent"] = json!(flow.consent);
        }
        Some(form)
    }
}

impl AccountsTab {
    /// Reopens the Add account setup the last visit left open.
    pub(super) fn restore_open_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.store.update(cx, |store, _| {
            store.open_forms_mut().take(SettingsTabId::Accounts)
        }) else {
            return;
        };
        let Some(provider) = form
            .get("provider")
            .and_then(Value::as_str)
            .and_then(|provider| PROVIDERS.into_iter().find(|known| *known == provider))
        else {
            return;
        };
        self.pane = Some(Pane::Add(provider));
        self.setups.insert(
            provider.to_string(),
            SetupDraft {
                selected: "new".into(),
            },
        );
        if let Some(email) = form.get("email").and_then(Value::as_str) {
            let consent = form.get("consent").and_then(Value::as_bool) == Some(true);
            let key = format!("setup:{provider}:new");
            // Never over a sign-in this page already runs.
            if self
                .flows
                .get(&key)
                .and_then(|flow| flow.job.as_ref())
                .is_none_or(|job| !job_active(job))
            {
                self.flows
                    .insert(key, FlowState::new(email.to_string(), consent, None));
            }
        }
    }
}
