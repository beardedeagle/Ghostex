//! Settings › Voice agent: the voice provider and its voice (catalog rows,
//! `packages/settings-catalog/src/pages/voice_agent.rs`), and the API keys gxserver keeps. A key is
//! pasted, sent once to `/api/setVoiceAgentKey` (which checks it with the provider before sealing
//! it) and never shown again; the page only learns whether each provider has one
//! (`/api/readVoiceAgent`).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: the API keys live on the computer, kept by gxserver; Settings › Voice agent is a plain Settings page, not an extension card.
//! SEE-ALSO: server/src/voice_agent/http.rs, server/src/voice_agent/keys.rs.
use super::super::super::native_modal_kit::*;
use super::super::catalog::{SettingOption, settings_catalog};
use super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SettingsPage, reset_key, select_field, setting_row,
    settings_button, settings_section, settings_text_input, static_note,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use std::time::Duration;

/// `.settings-page-width grid gap-6 px-5 py-5`: 20px above the first section on top of its own 12px.
const PAGE_TOP_PADDING: f32 = 20.0;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Saving checks the key with the provider first.
const SAVE_TIMEOUT: Duration = Duration::from_secs(30);

const PROVIDER: &str = "voiceAgentProvider";
const XAI_VOICE: &str = "voiceAgentXaiVoice";
const OPENAI_VOICE: &str = "voiceAgentOpenaiVoice";
const XAI_KEY: &str = "voiceAgentXaiKey";
const OPENAI_KEY: &str = "voiceAgentOpenaiKey";

pub(crate) fn voice_agent_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| VoiceAgentTab::new(store.clone(), window, cx))
        .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    Xai,
    OpenAi,
}

impl Provider {
    fn id(self) -> &'static str {
        match self {
            Provider::Xai => "xai",
            Provider::OpenAi => "openai",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Provider::Xai => "xAI",
            Provider::OpenAi => "OpenAI",
        }
    }

    fn row_key(self) -> &'static str {
        match self {
            Provider::Xai => XAI_KEY,
            Provider::OpenAi => OPENAI_KEY,
        }
    }
}

/// One provider's key field and what the page knows about its saved key.
struct KeyField {
    input: Entity<InputState>,
    /// `None` until `/api/readVoiceAgent` answered.
    configured: Option<bool>,
    busy: bool,
    /// The last save or remove: whether it worked and what to say.
    note: Option<(bool, String)>,
}

pub(crate) struct VoiceAgentTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    xai: KeyField,
    openai: KeyField,
    read_error: Option<String>,
}

impl SettingsPage for VoiceAgentTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl VoiceAgentTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let key_field =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| KeyField {
                input: cx.new(|cx| {
                    InputState::new(window, cx)
                        .masked(true)
                        .placeholder(placeholder)
                }),
                configured: None,
                busy: false,
                note: None,
            };
        let mut tab = Self {
            store,
            fields: FieldStates::default(),
            xai: key_field("Paste your xAI API key", window, cx),
            openai: key_field("Paste your OpenAI API key", window, cx),
            read_error: None,
        };
        tab.read_status(cx);
        tab
    }

    fn key(&mut self, provider: Provider) -> &mut KeyField {
        match provider {
            Provider::Xai => &mut self.xai,
            Provider::OpenAi => &mut self.openai,
        }
    }

    /// Whether each provider has a key (never the key itself).
    fn read_status(&mut self, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/readVoiceAgent",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    match result {
                        Ok(status) => {
                            page.read_error = None;
                            page.xai.configured = status["keys"]["xai"]["configured"].as_bool();
                            page.openai.configured =
                                status["keys"]["openai"]["configured"].as_bool();
                        }
                        Err(error) => page.read_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Sends the pasted key once (or an empty one to remove it). gxserver checks it with the
    /// provider and answers whether it is saved.
    fn save_key(
        &mut self,
        provider: Provider,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.key(provider).input.clone();
        let key = if remove {
            String::new()
        } else {
            input.read(cx).value().trim().to_string()
        };
        if !remove && key.is_empty() {
            self.key(provider).note = Some((false, "Paste a key first.".to_string()));
            cx.notify();
            return;
        }
        let field = self.key(provider);
        field.busy = true;
        field.note = None;
        // The key leaves the field as soon as it is sent, so it is never on screen again.
        input.update(cx, |input, cx| input.set_value("", window, cx));
        let page = cx.entity().downgrade();
        store_gxserver_rpc(
            &self.store,
            "/api/setVoiceAgentKey",
            json!({ "provider": provider.id(), "apiKey": key }),
            SAVE_TIMEOUT,
            move |result, cx| {
                let _ = page.update(cx, |page, cx| {
                    let field = page.key(provider);
                    field.busy = false;
                    match result {
                        Ok(answer) => {
                            let configured = answer["configured"].as_bool().unwrap_or(!remove);
                            field.configured = Some(configured);
                            field.note = Some((
                                true,
                                if configured {
                                    format!("Saved. {} accepted the key.", provider.label())
                                } else {
                                    "Removed from this computer.".to_string()
                                },
                            ));
                        }
                        Err(error) => field.note = Some((false, error)),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn catalog_row(key: &str) -> Option<(String, String, Vec<SettingOption>)> {
        settings_catalog()
            .extra_tab(SettingsTabId::VoiceAgent.id())?
            .sections
            .iter()
            .flat_map(|section| section.settings.iter())
            .find(|row| row.key == key)
            .map(|row| (row.title.clone(), row.subtitle.clone(), row.options.clone()))
    }

    fn select_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, options) = Self::catalog_row(key)?;
        let values = self.store.read(cx).values();
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = values.choice(key, &allowed);
        Some(select_field(
            self,
            p,
            key,
            RowSpec::new(title)
                .description(subtitle)
                .keyed(&values, key),
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            None,
            move |page: &mut Self, next, _window, cx| {
                page.store
                    .update(cx, |store, cx| store.update_setting(key, json!(next), cx));
            },
            window,
            cx,
        ))
    }

    fn key_row(
        &mut self,
        p: &super::super::palette::SettingsPalette,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (title, subtitle, _) = Self::catalog_row(provider.row_key())?;
        let field = self.key(provider);
        let input = field.input.clone();
        let configured = field.configured;
        let busy = field.busy;
        let note = field.note.clone();
        let status: SharedString = match (busy, configured) {
            (true, _) => format!("Checking the key with {}…", provider.label()).into(),
            (false, Some(true)) => "A key is saved on this computer.".into(),
            (false, Some(false)) => "No key yet.".into(),
            (false, None) => "".into(),
        };
        let save_id = SharedString::from(format!("{}-save", provider.row_key()));
        let remove_id = SharedString::from(format!("{}-remove", provider.row_key()));
        let control = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .child(settings_text_input(p, &input, None, true, window, cx))
                    .child(settings_button(
                        p,
                        save_id,
                        if configured == Some(true) {
                            "Replace"
                        } else {
                            "Save"
                        },
                        None,
                        ButtonVariant::Outline,
                        busy,
                        None,
                        move |page: &mut Self, window, cx| {
                            page.save_key(provider, false, window, cx)
                        },
                        cx,
                    ))
                    .when(configured == Some(true), |this| {
                        this.child(settings_button(
                            p,
                            remove_id,
                            "Remove",
                            None,
                            ButtonVariant::Outline,
                            busy,
                            None,
                            move |page: &mut Self, window, cx| {
                                page.save_key(provider, true, window, cx)
                            },
                            cx,
                        ))
                    }),
            )
            .when(!status.is_empty() || note.is_some(), |this| {
                this.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(match &note {
                            Some((false, _)) => p.destructive,
                            _ => p.muted,
                        }))
                        .child(match note {
                            Some((_, text)) => SharedString::from(text),
                            None => status,
                        }),
                )
            })
            .into_any_element();
        Some(setting_row(
            p,
            provider.row_key(),
            RowSpec::new(title).description(subtitle).wide(),
            None,
            control,
            cx,
        ))
    }
}

impl Render for VoiceAgentTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, values, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.values(),
                store.tab_search(SettingsTabId::VoiceAgent),
                matching,
            )
        };
        let provider = values.string(PROVIDER);
        let mut provider_rows = Vec::new();
        for key in [PROVIDER, XAI_VOICE, OPENAI_VOICE] {
            // Only the chosen provider's voice is offered.
            let shown = match key {
                XAI_VOICE => provider != "openai",
                OPENAI_VOICE => provider == "openai",
                _ => true,
            };
            if shown
                && search.row_visible("provider", key)
                && let Some(row) = self.select_row(&p, key, window, cx)
            {
                provider_rows.push(row);
            }
        }
        let mut key_rows = Vec::new();
        for provider in [Provider::Xai, Provider::OpenAi] {
            if search.row_visible("keys", provider.row_key())
                && let Some(row) = self.key_row(&p, provider, window, cx)
            {
                key_rows.push(row);
            }
        }
        if !key_rows.is_empty() {
            key_rows.push(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(static_note(
                        &p,
                        "Get an xAI key at console.x.ai under API Keys, and an OpenAI key at platform.openai.com under API keys. Dictation in the chat box always uses the xAI key. Each provider bills your account while a conversation is open.",
                        false,
                    ))
                    .into_any_element(),
            );
            if let Some(error) = &self.read_error {
                key_rows.push(
                    div()
                        .px(px(16.0))
                        .pb(px(12.0))
                        .text_size(px(12.5))
                        .text_color(hsla(p.destructive))
                        .child(format!("Could not read the saved keys: {error}"))
                        .into_any_element(),
                );
            }
        }
        let mut blocks = Vec::new();
        if let Some(section) = settings_section(&p, "Voice agent", None, None, provider_rows) {
            let mut block = PageBlock::section("provider", section);
            block.margin_top += PAGE_TOP_PADDING;
            blocks.push(block);
        }
        if let Some(section) = settings_section(&p, "API keys", None, None, key_rows) {
            let mut block = PageBlock::section("keys", section);
            if blocks.is_empty() {
                block.margin_top += PAGE_TOP_PADDING;
            }
            blocks.push(block);
        }
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::VoiceAgent,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        settings_page(&self.store, SettingsTabId::VoiceAgent, &p, blocks, cx)
    }
}
