//! `AccountConnectFlow` (accounts/connect-flow.tsx (deleted 2026-10-01)): the email and shared-conversations consent,
//! Add / Reconnect account (its login command in the tooltip), then while the sign-in runs its
//! status, Open sign-in page, Show terminal, Cancel and a sign-in code field, the Codex windows
//! that stopped it, and the helper's output.
//!
//! CDXC:Settings 2026-09-07 DECISION:
//! Login commands are hidden in Accounts and its tutorial. Click to run login starts the flow; hovering the button reveals the command. This replaces the visible login command boxes and Log in to fix label.
//! The command stays hidden behind the start button's tooltip, but that button reads Add account or Reconnect account and runs the sign-in inside Settings (server/src/accounts/setup.rs, 2026-09-08 DECISION); React stopped using its Click to run login button (`AccountLoginButton`) on 2026-09-08, well before it was deleted.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    FieldStates, SizedButtonSize, SizedButtonVariant, checkbox_control, settings_sized_button,
    settings_text_input, sized_button_colors, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::AccountsTab;
use super::client::ACCOUNT_SETUP_OWNER;
use super::data::{Account, provider_label};
use super::widgets::account_text;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
    div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

/// `setInterval(poll, 1000)` while a sign-in runs.
const JOB_POLL: Duration = Duration::from_secs(1);

/// One flow's state (its React `useState`s).
pub(crate) struct FlowState {
    pub(crate) email: String,
    pub(crate) consent: bool,
    pub(crate) job: Option<Value>,
    pub(crate) error: String,
    pub(crate) starting: bool,
    pub(crate) terminal: bool,
    pub(crate) code: String,
    pub(crate) poll: Option<Task<()>>,
}

fn job_active(job: &Value) -> bool {
    !matches!(job["status"].as_str(), Some("complete" | "failed"))
}

/// `quote(value)` for the tooltip's shell command.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl AccountsTab {
    /// Forgets the Add account flows of `provider` (a finished sign-in's success message, its
    /// email and consent) so the next Add account starts from an empty form.
    pub(crate) fn reset_setup_flows(&mut self, provider: &str) {
        let prefix = format!("setup:{provider}:");
        self.flows.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Starts the one-second status poll of a flow whose job is running.
    fn ensure_flow_poll(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(flow) = self.flows.get_mut(key) else {
            return;
        };
        let active = flow.job.as_ref().is_some_and(job_active);
        if !active {
            flow.poll = None;
            return;
        }
        if flow.poll.is_some() {
            return;
        }
        let key = key.to_string();
        flow.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(JOB_POLL).await;
                let keep = this
                    .update(cx, |page, cx| page.poll_flow_job(&key, cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    /// One poll: the job's latest state. Returns whether to keep polling.
    fn poll_flow_job(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(job_id) = self
            .flows
            .get(key)
            .and_then(|flow| flow.job.as_ref())
            .filter(|job| job_active(job))
            .and_then(|job| job["id"].as_str().map(str::to_string))
        else {
            if let Some(flow) = self.flows.get_mut(key) {
                flow.poll = None;
            }
            return false;
        };
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": "setupStatus", "owner": ACCOUNT_SETUP_OWNER }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let Some(flow) = page.flows.get_mut(&key) else {
                        return;
                    };
                    match result {
                        Ok(state) => {
                            if let Some(next) = state["setupJobs"].as_array().and_then(|jobs| {
                                jobs.iter().find(|job| job["id"] == job_id.as_str())
                            }) {
                                flow.job = Some(next.clone());
                            }
                        }
                        Err(error) => flow.error = error,
                    }
                    cx.notify();
                });
            },
            cx,
        );
        true
    }

    /// `start(stop?)`: `setupStart`, optionally sleeping the sessions and closing the Codex
    /// windows that stopped the last attempt.
    fn start_flow(
        &mut self,
        key: &str,
        provider: &'static str,
        account: Option<Account>,
        stop: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.flows.get_mut(key) else {
            return;
        };
        flow.starting = true;
        flow.error.clear();
        let mut params = json!({
            "operation": "setupStart",
            "owner": ACCOUNT_SETUP_OWNER,
            "provider": provider,
            "email": flow.email,
            "shareHistory": true,
        });
        if let Some(account) = &account {
            if account.registered() {
                params["accountId"] = json!(account.id());
            }
            params["selector"] = json!(account.selector());
        }
        if let Some(stop) = stop {
            params["stopCodex"] = json!(true);
            params["sleepSessions"] = Value::Array(
                stop["sessions"]
                    .as_array()
                    .map(|sessions| {
                        sessions
                            .iter()
                            .map(|session| {
                                json!({ "projectId": session["projectId"], "sessionId": session["sessionId"] })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        cx.notify();
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            params,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Some(flow) = page.flows.get_mut(&key) {
                        flow.starting = false;
                        match result {
                            Ok(state) => {
                                flow.job = state["setupJobs"].as_array().and_then(|jobs| {
                                    jobs.iter()
                                        .filter(|job| {
                                            job["acknowledged"].as_bool() != Some(true)
                                                && job["provider"] == provider
                                        })
                                        .next_back()
                                        .cloned()
                                });
                            }
                            Err(error) => {
                                flow.error = if error.trim().is_empty() {
                                    "Could not start login.".into()
                                } else {
                                    error
                                };
                            }
                        }
                    }
                    page.ensure_flow_poll(&key, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// A job operation (`setupCancel`, `setupInput`) whose failure shows as the flow's error.
    fn flow_job_call(
        &mut self,
        key: &str,
        params: Value,
        clear_code: bool,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            params,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Some(flow) = page.flows.get_mut(&key) {
                        match result {
                            Ok(_) if clear_code => flow.code.clear(),
                            Ok(_) => {}
                            Err(error) => flow.error = error,
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The flow keyed `key` (created on first render). `guide` draws it in the guide dialog's
    /// 12px type instead of the Settings 14px.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_connect_flow(
        &mut self,
        p: &SettingsPalette,
        key: String,
        provider: &'static str,
        account: Option<Account>,
        initial_job: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_connect_flow_sized(p, key, provider, account, initial_job, false, window, cx)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_connect_flow_sized(
        &mut self,
        p: &SettingsPalette,
        key: String,
        provider: &'static str,
        account: Option<Account>,
        initial_job: Option<Value>,
        guide: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        if !self.flows.contains_key(&key) {
            self.flows.insert(
                key.clone(),
                FlowState {
                    email: account.as_ref().map(Account::email).unwrap_or_default(),
                    consent: account.as_ref().is_some_and(Account::registered),
                    job: initial_job,
                    error: String::new(),
                    starting: false,
                    terminal: false,
                    code: String::new(),
                    poll: None,
                },
            );
        }
        self.ensure_flow_poll(&key, cx);
        let (email, consent, job, error, starting, terminal, code) = {
            let flow = &self.flows[&key];
            (
                flow.email.clone(),
                flow.consent,
                flow.job.clone(),
                flow.error.clone(),
                flow.starting,
                flow.terminal,
                flow.code.clone(),
            )
        };
        let (text_size, line_height) = if guide { (12.0, 19.2) } else { (14.0, 20.0) };
        let paragraph = |text: String| {
            div()
                .text_size(px(text_size))
                .line_height(px(line_height))
                .text_color(hsla(if guide { p.muted } else { p.foreground }))
                .child(text)
                .into_any_element()
        };
        let mut children: Vec<AnyElement> = Vec::new();
        let status = job
            .as_ref()
            .and_then(|job| job["status"].as_str().map(str::to_string));
        let active = job.as_ref().is_some_and(job_active);
        if status.as_deref() == Some("complete") {
            let email = job
                .as_ref()
                .and_then(|job| job["email"].as_str())
                .unwrap_or_default();
            children.push(paragraph(format!(
                "Account connected. {}",
                account_text(email, hide)
            )));
        } else if active {
            let job = job.clone().unwrap_or(Value::Null);
            children.push(paragraph(
                if job["status"] == "saving" {
                    "Verifying and adding your account…"
                } else {
                    "Finish signing in through your browser. We’ll finish adding the account automatically."
                }
                .to_string(),
            ));
            let mut actions: Vec<AnyElement> = Vec::new();
            if let Some(url) = job["url"].as_str().filter(|url| !url.is_empty()) {
                let url = url.to_string();
                let foreground = p.foreground;
                actions.push(
                    div()
                        .id(SharedString::from(format!("{key}-signin-link")))
                        .text_size(px(12.0))
                        .underline()
                        .text_color(hsla(foreground))
                        .cursor_pointer()
                        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                            post_store_message(
                                &page.store,
                                json!({ "type": "openExternalUrl", "url": url }),
                                cx,
                            );
                        }))
                        .child("Open sign-in page")
                        .into_any_element(),
                );
            }
            {
                let key = key.clone();
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-terminal")),
                    if terminal {
                        "Hide terminal"
                    } else {
                        "Show terminal"
                    },
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        if let Some(flow) = page.flows.get_mut(&key) {
                            flow.terminal = !flow.terminal;
                        }
                        cx.notify();
                    },
                    cx,
                ));
            }
            {
                let key = key.clone();
                let job_id = job["id"].clone();
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-cancel")),
                    "Cancel",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.flow_job_call(
                            &key,
                            json!({ "operation": "setupCancel", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id }),
                            false,
                            cx,
                        )
                    },
                    cx,
                ));
            }
            children.push(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(actions)
                    .into_any_element(),
            );
            let code_id = SharedString::from(format!("{key}-code"));
            let input = FieldStates::text_state(
                self,
                &code_id,
                &code,
                Some("Paste a sign-in code here if prompted"),
                {
                    let key = key.clone();
                    move |page: &mut Self, text, _window, cx| {
                        if let Some(flow) = page.flows.get_mut(&key) {
                            flow.code = text;
                        }
                        cx.notify();
                    }
                },
                window,
                cx,
            );
            let send = {
                let key = key.clone();
                let job_id = job["id"].clone();
                settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-send-code")),
                    "Send code",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    code.trim().is_empty(),
                    None,
                    move |page: &mut Self, _window, cx| {
                        let input = page
                            .flows
                            .get(&key)
                            .map(|flow| flow.code.clone())
                            .unwrap_or_default();
                        page.flow_job_call(
                            &key,
                            json!({ "operation": "setupInput", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id, "input": input }),
                            true,
                            cx,
                        )
                    },
                    cx,
                )
            };
            children.push(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .child(settings_text_input(p, &input, None, false, window, cx))
                    .child(send)
                    .into_any_element(),
            );
        } else {
            let email_id = SharedString::from(format!("{key}-email"));
            let input = FieldStates::text_state(
                self,
                &email_id,
                &email,
                None,
                {
                    let key = key.clone();
                    move |page: &mut Self, text, _window, cx| {
                        if let Some(flow) = page.flows.get_mut(&key) {
                            flow.email = text;
                        }
                        cx.notify();
                    }
                },
                window,
                cx,
            );
            super::widgets::sync_masked(
                &mut self.masked_inputs,
                &email_id,
                &input,
                hide,
                window,
                cx,
            );
            let disabled = account.is_some();
            children.push(
                // `.gx-account-field { margin: 15px 0 }`, `margin: 0 0 12px` inside `.gx-account-inset`.
                v_flex()
                    .w_full()
                    .gap(px(7.0))
                    .when(guide, |field| field.my(px(15.0)))
                    .when(!guide, |field| field.mb(px(12.0)))
                    .text_size(px(text_size))
                    .line_height(px(if guide { 17.14 } else { 20.0 }))
                    .text_color(hsla(p.foreground))
                    .child("Email")
                    .child(if disabled {
                        // `<SettingsInput disabled>`: the saved address, not editable.
                        div()
                            .w_full()
                            .h(px(32.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .bg(hsla(p.input_background()))
                            .opacity(0.5)
                            .text_size(px(14.0))
                            .child(if hide {
                                "•".repeat(email.chars().count())
                            } else {
                                email.clone()
                            })
                            .into_any_element()
                    } else {
                        settings_text_input(p, &input, None, false, window, cx)
                    })
                    .into_any_element(),
            );
            if !account.as_ref().is_some_and(Account::registered) {
                let key = key.clone();
                children.push(checkbox_control(
                    p,
                    SharedString::from(format!("{key}-consent")),
                    consent,
                    Some(
                        div()
                            .text_size(px(if guide { 11.0 } else { 13.0 }))
                            .line_height(px(if guide { 15.71 } else { 18.57 }))
                            .text_color(hsla(p.foreground))
                            .child(format!(
                                "Share conversations between my {} accounts.",
                                provider_label(provider)
                            ))
                            .into_any_element(),
                    ),
                    8.0,
                    move |page: &mut Self, next, _window, cx| {
                        if let Some(flow) = page.flows.get_mut(&key) {
                            flow.consent = next;
                        }
                        cx.notify();
                    },
                    cx,
                ));
            }
            let command = if provider == "claude" {
                format!(
                    "ghostex account-login claude --email {} --json{}",
                    quote(&email),
                    account
                        .as_ref()
                        .map(|account| format!(" --account {}", quote(&account.selector())))
                        .unwrap_or_default()
                )
            } else if let Some(account) = &account {
                format!("xswap login {}", quote(&account.selector()))
            } else {
                format!(
                    "xswap add --login --share-history --email {} --json",
                    quote(&email)
                )
            };
            let label = if starting {
                "Starting sign-in…"
            } else if status.as_deref() == Some("failed") {
                "Try again"
            } else if account.is_some() {
                "Reconnect account"
            } else {
                "Add account"
            };
            let enabled = !starting && consent && email.contains('@');
            let (fills, text) = sized_button_colors(p, SizedButtonVariant::Secondary);
            let start_key = key.clone();
            let start_account = account.clone();
            children.push(
                div()
                    .id(SharedString::from(format!("{key}-start-tooltip")))
                    .flex()
                    .tooltip(tooltip_text(account_text(&command, hide)))
                    .child(
                        h_flex()
                            .id(SharedString::from(format!("{key}-start")))
                            .h(px(32.0))
                            .px(px(12.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(if guide { 10.0 } else { MODAL_RADIUS_CONTROL }))
                            .bg(hsla(fills.rest))
                            .text_size(px(text_size))
                            .when(guide, |this| this.font_weight(FontWeight::MEDIUM))
                            .text_color(hsla(text))
                            .whitespace_nowrap()
                            .when(!enabled, |this| this.opacity(0.5))
                            .when(enabled, |this| {
                                this.cursor_pointer()
                                    .hover(move |this| this.bg(hsla(fills.hover)))
                                    .active(move |this| this.bg(hsla(fills.pressed)))
                                    .on_click(cx.listener(
                                        move |page, _: &ClickEvent, _window, cx| {
                                            page.start_flow(
                                                &start_key,
                                                provider,
                                                start_account.clone(),
                                                None,
                                                cx,
                                            )
                                        },
                                    ))
                            })
                            .child(label),
                    )
                    .into_any_element(),
            );
        }
        let job_error = job
            .as_ref()
            .and_then(|job| job["error"].as_str())
            .filter(|error| !error.is_empty())
            .map(str::to_string);
        if let Some(message) = (!error.is_empty()).then(|| error.clone()).or(job_error) {
            children.push(paragraph(account_text(&message, hide)));
        }
        let blockers = job
            .as_ref()
            .filter(|job| job["status"] == "failed")
            .map(|job| job["blockers"].clone())
            .filter(Value::is_object);
        if let Some(blockers) = blockers {
            let sessions: Vec<Value> = blockers["sessions"].as_array().cloned().unwrap_or_default();
            let others = blockers["others"].as_u64().unwrap_or(0);
            let plural = |count: usize, word: &str| {
                format!("{count} {word}{}", if count == 1 { "" } else { "s" })
            };
            let mut block: Vec<AnyElement> = Vec::new();
            if !sessions.is_empty() {
                block.push(
                    v_flex()
                        .pl(px(18.0))
                        .children(sessions.iter().map(|session| {
                            div().child(format!(
                                "• {}",
                                session["title"].as_str().unwrap_or_default()
                            ))
                        }))
                        .into_any_element(),
                );
            }
            if others > 0 {
                block.push(
                    div()
                        .text_color(hsla(p.muted))
                        .child(format!(
                            "{}{} outside Ghostex.",
                            if sessions.is_empty() {
                                "Closes "
                            } else {
                                "Also closes "
                            },
                            plural(others as usize, "Codex window")
                        ))
                        .into_any_element(),
                );
            }
            let stop_key = key.clone();
            let stop_account = account.clone();
            let stop = blockers.clone();
            block.push(settings_sized_button(
                p,
                SharedString::from(format!("{key}-stop-codex")),
                if sessions.is_empty() {
                    "Close Codex and continue".to_string()
                } else {
                    format!("Sleep {} and continue", plural(sessions.len(), "session"))
                },
                None,
                None,
                SizedButtonVariant::Secondary,
                SizedButtonSize::Default,
                starting,
                None,
                move |page: &mut Self, _window, cx| {
                    page.start_flow(
                        &stop_key,
                        provider,
                        stop_account.clone(),
                        Some(stop.clone()),
                        cx,
                    )
                },
                cx,
            ));
            children.push(
                v_flex()
                    .items_start()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .children(block)
                    .into_any_element(),
            );
        }
        let output = job
            .as_ref()
            .and_then(|job| job["output"].as_str())
            .filter(|output| !output.is_empty())
            .map(str::to_string);
        if (terminal || status.as_deref() == Some("failed"))
            && let Some(output) = output
        {
            children.push(
                div()
                    .id(SharedString::from(format!("{key}-output")))
                    .w_full()
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .bg(hsla(if p.glass {
                        p.modal.solid_surface
                    } else {
                        p.surface
                    }))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.foreground))
                    .child(account_text(&output, hide))
                    .into_any_element(),
            );
        }
        v_flex()
            .w_full()
            .items_stretch()
            .gap(px(8.0))
            .children(children)
            .into_any_element()
    }
}
