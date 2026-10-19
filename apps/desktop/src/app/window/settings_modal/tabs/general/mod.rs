//! The General page (the `settings` tab of packages/core-ui/settings-modal.tsx (deleted 2026-10-01)): Sidebar,
//! Session Cards, Sidebar Tags, Chat, Status Indicators, Browser, Dev Servers, Editor, File
//! opening, Terminal, Terminal Behavior, Terminal Scrolling, Auto Sleep, Power, Sounds, Sleeping
//! Sessions and Experimental, in that order (CDXC:Settings 2026-08-24: each rail group's sections stay
//! contiguous), then Reset to defaults.
mod chat_tools;
mod marked_sections_preview;
mod powershell;
mod sidebar;
mod system;
mod terminal;
mod terminal_font;
mod terminal_transparency;

use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::fields::{
    ButtonVariant, FieldStates, PageAction, RowSpec, SettingsPage, SliderBinding, reset_key,
    segmented_field, select_field, settings_button, slider_number_field, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, page_separator, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::GeneralSearch;
use super::super::store::{SettingsStore, SettingsValues};
use gpui::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};
use gpui_component::h_flex;
use serde_json::{Value, json};

pub(crate) struct GeneralTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    powershell: powershell::PowerShellState,
    /// The Marked sections style Preview dialog, while it is open (its focus, for Escape).
    marked_preview: Option<gpui::FocusHandle>,
    /// Orchestrator instructions for all projects and their editor.
    orchestrator_instructions: super::orchestrator_instructions::OrchestratorInstructionsState,
}

impl GeneralTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let preview_state = store.read(cx).request().preview_state.clone();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            powershell: powershell::PowerShellState::default(),
            // The preview binary's `marked-preview` state opens the Marked sections Preview.
            marked_preview: (preview_state.as_deref() == Some("marked-preview"))
                .then(|| cx.focus_handle()),
            orchestrator_instructions: Default::default(),
        };
        if settings_catalog().flag(module::SEARCH_CATALOG, "IS_WINDOWS_HOST") {
            page.load_powershell(cx);
        }
        // The preview binary's `orchestrator-instructions-edit` state opens their editor.
        if preview_state.as_deref() == Some("orchestrator-instructions-edit") {
            super::orchestrator_instructions::open_instructions_sheet(&mut page, None, window, cx);
        }
        page
    }
}

impl super::orchestrator_instructions::HasOrchestratorInstructions for GeneralTab {
    fn orchestrator_instructions(
        &mut self,
    ) -> &mut super::orchestrator_instructions::OrchestratorInstructionsState {
        &mut self.orchestrator_instructions
    }
}

impl SettingsPage for GeneralTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// What every General section reads while it renders.
pub(super) struct GeneralCx {
    pub(super) p: SettingsPalette,
    pub(super) values: SettingsValues,
    pub(super) search: GeneralSearch,
    /// `showBetaFeatures`: Power shows only with Enable Experimental Features.
    pub(super) power_visible: bool,
    /// The Sidebar Tags deep link asked for the New tag form.
    pub(super) create_tag: bool,
}

impl GeneralCx {
    /// `mainSettingVisible(settingsSearch[section], key)`.
    pub(super) fn visible(&self, section: &str, key: &str) -> bool {
        self.search.setting_visible(section, key)
    }

    pub(super) fn options(&self, name: &str) -> Vec<SettingOption> {
        settings_catalog().options(module::SETTINGS, name)
    }

    pub(super) fn number(&self, name: &str) -> f64 {
        settings_catalog().number(module::SETTINGS, name)
    }

    /// The keyed row of a setting with `getSettingModificationProps(key)`.
    pub(super) fn spec(&self, key: &str, label: &str, description: &str) -> RowSpec {
        RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&self.values, key)
    }
}

fn save<V: SettingsPage>(page: &mut V, key: &'static str, value: Value, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| store.update_setting(key, value, cx));
}

impl GeneralTab {
    /// A `ToggleField` bound to `key`.
    pub(super) fn toggle(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        dependent: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        Some(toggle_field(self, &g.p, key, spec, g.values.bool(key), cx))
    }

    /// A `SelectField` bound to a string `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        options: Vec<SettingOption>,
        width: Option<f32>,
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice(key, &allowed);
        Some(select_field(
            self,
            &g.p,
            key,
            spec,
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            width,
            move |page: &mut Self, next, _window, cx| save(page, key, json!(next), cx),
            window,
            cx,
        ))
    }

    /// A `SelectField` over numeric options (`String(draft[key])`, `Number(value)`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn number_select(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        options: Vec<SettingOption>,
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let value = g.values.number_string(key);
        Some(select_field(
            self,
            &g.p,
            key,
            spec,
            Some(reset_key::<Self>(key)),
            &options,
            &value,
            None,
            move |page: &mut Self, next, _window, cx| {
                let number: f64 = next.parse().unwrap_or_default();
                let value = if number.fract() == 0.0 {
                    json!(number as i64)
                } else {
                    json!(number)
                };
                save(page, key, value, cx);
            },
            window,
            cx,
        ))
    }

    /// A `SliderNumberField` bound to `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slider(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        label: &str,
        description: &str,
        range: (f64, f64, f64),
        dependent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let mut spec = g.spec(key, label, description);
        if dependent {
            spec = spec.dependent();
        }
        let (min, max, step) = range;
        Some(slider_number_field(
            self,
            &g.p,
            spec,
            Some(reset_key::<Self>(key)),
            SliderBinding {
                key,
                min,
                max,
                step,
            },
            g.values.f64(key),
            window,
            cx,
        ))
    }

    /// A segmented field bound to a string `key`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn segmented(
        &mut self,
        g: &GeneralCx,
        section: &str,
        key: &'static str,
        spec: RowSpec,
        on_reset: Option<PageAction<Self>>,
        options: Vec<SettingOption>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible(section, key) {
            return None;
        }
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice(key, &allowed);
        Some(segmented_field(
            &g.p,
            key,
            spec,
            on_reset,
            &options,
            Some(&value),
            None,
            move |page: &mut Self, next, _window, cx| save(page, key, json!(next), cx),
            cx,
        ))
    }
}

/// `resetSettings`: the bundled app icon, then every setting back to its default except the
/// saved remote machines.
fn reset_all(page: &mut GeneralTab, cx: &mut Context<GeneralTab>) {
    let store = page.store.clone();
    store.update(cx, |store, cx| {
        store.post_message(json!({ "sourceId": "", "type": "setAppIcon" }), cx);
        let mut defaults = settings_catalog().defaults().clone();
        defaults.insert("remoteMachines".to_string(), store.value("remoteMachines"));
        store.apply_settings(defaults, "settings:bulk", cx);
    });
}

impl Render for GeneralTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (g, matching_pages) = {
            let store = self.store.read(cx);
            let g = GeneralCx {
                p: store.palette(),
                values: store.values(),
                search: store.general_search().clone(),
                power_visible: store.bool("showBetaFeatures"),
                create_tag: store.request().initial_sidebar_tags_action.as_deref()
                    == Some("createTag"),
            };
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (g, matching)
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        blocks.extend(sidebar::sections(self, &g, window, cx));
        blocks.extend(chat_tools::sections(self, &g, window, cx));
        blocks.extend(terminal::sections(self, &g, window, cx));
        blocks.extend(system::sections(self, &g, window, cx));
        let has_visible =
            !super::super::search::general_navigation(&g.search, g.power_visible).is_empty();
        if !has_visible {
            let store = self.store.clone();
            // `SettingsSearchNoMatches` is not a section: no 12px section margin above it.
            blocks.push(PageBlock::plain(render_no_matches(
                &g.p,
                SettingsTabId::General,
                &matching_pages,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        }
        // The preview binary's `select` state opens a dropdown; its `pick-color` state (and a host
        // without a system colour panel) the terminal background's Pick Color dialog.
        if let Some(id) = self
            .store
            .update(cx, |store, _| store.request_mut().open_select.take())
        {
            self.fields.pending_open_select = Some(id.into());
            cx.notify();
        }
        let open_picker = self
            .store
            .update(cx, |store, _| store.request_mut().open_color_picker.take());
        if open_picker.as_deref() == Some("workspaceBackgroundColor") {
            super::super::fields::open_picker(
                self,
                "workspaceBackgroundColor",
                "#121212",
                window,
                cx,
            );
        }
        blocks.push(PageBlock::plain(page_separator(&g.p)));
        blocks.push(PageBlock::plain(
            h_flex()
                .w_full()
                .justify_between()
                .gap_3()
                .child(settings_button(
                    &g.p,
                    "settings-reset-to-defaults",
                    "Reset to defaults",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    |page: &mut Self, _window, cx| reset_all(page, cx),
                    cx,
                )),
        ));
        let preview = self.marked_sections_preview(&g, window, cx);
        let instructions =
            super::orchestrator_instructions::render_instructions_sheet(self, &g.p, window, cx);
        div()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::General,
                &g.p,
                blocks,
                cx,
            ))
            .children(preview)
            .children(instructions)
    }
}
