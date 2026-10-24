//! The Hotkeys page (packages/core-ui/settings-modal/tabs/hotkeys.tsx (deleted 2026-10-01)): the six sections of
//! `HOTKEY_SETTINGS_SECTIONS`, one recorder per action (duplicates marked), the Jump to Project
//! and Skip sleeping sessions toggles, and Reset Hotkeys. The same top search filters the rows
//! (`rail::hotkey_section_searches`), and the rail jumps to a section by its id.
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, CONTROL_LANE_WIDTH, FieldStates, HotkeyRecorder, HotkeyRecorderHost, RowSpec,
    SettingsPage, hotkey_recorder_field, is_reserved_hotkey, normalize_hotkey_text, setting_row,
    settings_button, settings_section, toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::hotkey_section_searches;
use super::super::search::{should_show_section, should_show_setting};
use super::super::store::SettingsStore;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Map, Value};
use std::collections::HashMap;

const HOTKEYS_MODULE: &str = super::super::catalog::module::HOTKEYS;

/// One `GHOSTEX_HOTKEY_DEFINITIONS` entry.
struct HotkeyDefinition {
    id: String,
    title: String,
    description: String,
    default_key: String,
    windows_linux_default_key: Option<String>,
    retired_default_keys: Vec<String>,
}

impl HotkeyDefinition {
    /// The default on this platform (`platformDefaultKey`).
    fn platform_default(&self) -> String {
        if cfg!(target_os = "macos") {
            self.default_key.clone()
        } else {
            self.windows_linux_default_key
                .clone()
                .unwrap_or_else(|| self.default_key.clone())
        }
    }
}

fn definitions() -> Vec<HotkeyDefinition> {
    settings_catalog()
        .module_value(HOTKEYS_MODULE, "GHOSTEX_HOTKEY_DEFINITIONS")
        .and_then(Value::as_array)
        .map(|definitions| {
            definitions
                .iter()
                .filter_map(|definition| {
                    let text = |key: &str| definition.get(key)?.as_str().map(str::to_string);
                    Some(HotkeyDefinition {
                        id: text("id")?,
                        title: text("title").unwrap_or_default(),
                        description: text("description").unwrap_or_default(),
                        default_key: text("defaultKey").unwrap_or_default(),
                        windows_linux_default_key: text("windowsLinuxDefaultKey"),
                        retired_default_keys: definition
                            .get("retiredDefaultKeys")
                            .and_then(Value::as_array)
                            .map(|keys| {
                                keys.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `readLegacyHotkey`: Toggle View Panel keeps a saved Companion Pane chord, and Jump to Project
/// 1-5 keep the old Focus Group 1-5 chords.
fn legacy_hotkey<'a>(source: &'a Map<String, Value>, action_id: &str) -> Option<&'a Value> {
    if action_id == "toggleViewPanel" {
        return source.get("toggleCompanionPane");
    }
    let slot = action_id.strip_prefix("jumpToProject")?;
    matches!(slot, "1" | "2" | "3" | "4" | "5")
        .then(|| source.get(&format!("focusGroup{slot}")))
        .flatten()
}

/// `normalizeghostexHotkeySettings`: every action's chord, the platform default where nothing
/// (or a retired default, the macOS default on another platform, or a reserved chord) is saved,
/// and an explicit blank kept as unassigned.
fn normalize_hotkeys(candidate: &Value, definitions: &[HotkeyDefinition]) -> Vec<(String, String)> {
    let empty = Map::new();
    let source = candidate.as_object().unwrap_or(&empty);
    definitions
        .iter()
        .map(|definition| {
            let platform_default = definition.platform_default();
            let value = source
                .get(&definition.id)
                .or_else(|| legacy_hotkey(source, &definition.id));
            let chord = match value.and_then(Value::as_str) {
                Some(value) => {
                    let text = if value.trim().is_empty() {
                        String::new()
                    } else {
                        normalize_hotkey_text(value)
                    };
                    let retired = definition.retired_default_keys.contains(&text);
                    let mac_default_elsewhere = !cfg!(target_os = "macos")
                        && definition
                            .windows_linux_default_key
                            .as_deref()
                            .is_some_and(|key| !key.is_empty())
                        && text == definition.default_key;
                    if retired || mac_default_elsewhere || is_reserved_hotkey(&text) {
                        platform_default
                    } else {
                        text
                    }
                }
                None => platform_default,
            };
            (definition.id.clone(), chord)
        })
        .collect()
}

/// `getDuplicateHotkeyIds`.
fn duplicate_ids(hotkeys: &[(String, String)]) -> Vec<String> {
    let mut by_chord: HashMap<String, Vec<String>> = HashMap::new();
    for (id, chord) in hotkeys {
        let chord = normalize_hotkey_text(chord);
        // A view-scoped hotkey (Search in Files) may share an app-wide chord on purpose.
        if chord.is_empty()
            || ghostex_settings_catalog::VIEW_SCOPED_HOTKEY_IDS.contains(&id.as_str())
        {
            continue;
        }
        by_chord.entry(chord).or_default().push(id.clone());
    }
    by_chord
        .into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .collect()
}

pub(crate) fn hotkeys_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| HotkeysTab::new(store.clone(), cx)).into()
}

pub(crate) struct HotkeysTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    recorder: HotkeyRecorder,
    definitions: Vec<HotkeyDefinition>,
    /// The preview binary's `hotkeys-recording` state arms a recorder on the first frame.
    preview_recording: bool,
}

impl HotkeysTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let preview_recording =
            store.read(cx).request().preview_state.as_deref() == Some("hotkeys-recording");
        Self {
            store,
            fields: FieldStates::default(),
            recorder: HotkeyRecorder::default(),
            definitions: definitions(),
            preview_recording,
        }
    }

    fn current_hotkeys(&self, cx: &App) -> Vec<(String, String)> {
        normalize_hotkeys(&self.store.read(cx).value("hotkeys"), &self.definitions)
    }

    fn save_hotkeys(&mut self, hotkeys: Vec<(String, String)>, cx: &mut Context<Self>) {
        let mut map = Map::new();
        for (id, chord) in hotkeys {
            map.insert(id, Value::String(chord));
        }
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            store.update_setting("hotkeys", Value::Object(map), cx)
        });
    }
}

impl super::HoldsUnsavedInput for HotkeysTab {
    /// A hotkey is being recorded.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.recorder.recording().is_some()
    }
}

impl SettingsPage for HotkeysTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl HotkeyRecorderHost for HotkeysTab {
    fn hotkey_recorder(&mut self) -> &mut HotkeyRecorder {
        &mut self.recorder
    }

    /// `updateHotkey(id, value)`: the whole normalized table with this action's chord replaced.
    fn hotkey_recorded(&mut self, id: &str, hotkey: String, cx: &mut Context<Self>) {
        let mut hotkeys: Map<String, Value> = self
            .current_hotkeys(cx)
            .into_iter()
            .map(|(id, chord)| (id, Value::String(chord)))
            .collect();
        hotkeys.insert(
            id.to_string(),
            Value::String(normalize_hotkey_text(&hotkey)),
        );
        let normalized = normalize_hotkeys(&Value::Object(hotkeys), &self.definitions);
        self.save_hotkeys(normalized, cx);
    }
}

impl HotkeysTab {
    /// A toggle row of the Projects or Navigation section.
    fn toggle(
        &mut self,
        key: &'static str,
        label: &str,
        description: &str,
        dependent: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (p, values) = {
            let store = self.store.read(cx);
            (store.palette(), store.values())
        };
        let mut spec = RowSpec::new(label.to_string())
            .description(description.to_string())
            .keyed(&values, key);
        if dependent {
            spec = spec.dependent();
        }
        toggle_field(self, &p, key, spec, values.bool(key), cx)
    }
}

impl HotkeysTab {
    /// CDXC:Hotkeys 2026-09-19 DECISION:
    /// User: the Skip sleeping sessions option lives on the Hotkeys page right below the Previous/Next Session hotkeys it changes.
    fn skip_sleeping_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.toggle(
            "sidebarSessionCycleSkipsSleeping",
            "Skip sleeping sessions",
            "Next Session and Previous Session jump over sleeping sessions in the sidebar.",
            true,
            cx,
        )
    }
}

impl Render for HotkeysTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.preview_recording) {
            super::super::fields::start_recording(self, "focusChatComposer".into(), window, cx);
        }
        let (p, values, query) = {
            let store = self.store.read(cx);
            (
                store.palette(),
                store.values(),
                store.search_query().to_string(),
            )
        };
        let searches = hotkey_section_searches(&query, &|id| {
            ghostex_settings_catalog::built_in_extensions::hotkey_shown_with(id, |key| {
                Some(values.bool(key))
            })
        });
        let hotkeys = self.current_hotkeys(cx);
        // `normalizeghostexHotkeySettings(DEFAULT_GHOSTEX_HOTKEYS)`: the macOS defaults, moved to the
        // Windows/Linux chord where an action has one.
        let default_table = settings_catalog()
            .module_value(HOTKEYS_MODULE, "DEFAULT_GHOSTEX_HOTKEYS")
            .cloned()
            .unwrap_or_default();
        let defaults = normalize_hotkeys(&default_table, &self.definitions);
        let duplicates = duplicate_ids(&hotkeys);
        let section_ids: HashMap<String, Vec<String>> = settings_catalog()
            .module_value(
                super::super::catalog::module::SETTINGS_TYPES,
                "HOTKEY_SETTINGS_SECTIONS",
            )
            .and_then(Value::as_array)
            .map(|sections| {
                sections
                    .iter()
                    .filter_map(|section| {
                        Some((
                            section.get("id")?.as_str()?.to_string(),
                            section
                                .get("ids")?
                                .as_array()?
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut blocks: Vec<PageBlock> = Vec::new();
        let mut any_visible = false;
        for (section_id, title, result) in &searches {
            if !should_show_section(result, true) {
                continue;
            }
            any_visible = true;
            let mut rows: Vec<AnyElement> = Vec::new();
            if section_id == "projects"
                && should_show_setting(result, "expandCollapsedProjectsOnJump", true)
            {
                rows.push(self.toggle(
                        "expandCollapsedProjectsOnJump",
                        "Expand Collapsed Projects on Jump",
                        "Reveal a collapsed project row before focusing it from Jump to Project hotkeys.",
                        false,
                        cx,
                    ));
            }
            for id in section_ids.get(section_id).into_iter().flatten() {
                let Some(definition) = self
                    .definitions
                    .iter()
                    .find(|definition| definition.id == *id)
                else {
                    continue;
                };
                if !should_show_setting(result, &definition.id, true) {
                    continue;
                }
                let value = hotkeys
                    .iter()
                    .find(|(hotkey_id, _)| hotkey_id == id)
                    .map(|(_, chord)| chord.clone())
                    .unwrap_or_else(|| definition.default_key.clone());
                let original = defaults
                    .iter()
                    .find(|(hotkey_id, _)| hotkey_id == id)
                    .map(|(_, chord)| chord.clone())
                    .unwrap_or_default();
                let (title, description) =
                    (definition.title.clone(), definition.description.clone());
                let recorder = hotkey_recorder_field(
                    self,
                    &p,
                    id.clone(),
                    &value,
                    &original,
                    duplicates.contains(id),
                    cx,
                );
                let control = div()
                    .w(px(CONTROL_LANE_WIDTH))
                    .max_w_full()
                    .child(recorder)
                    .into_any_element();
                rows.push(setting_row(
                    &p,
                    format!("hotkey-row-{id}"),
                    RowSpec::new(title).description(description),
                    None,
                    control,
                    cx,
                ));
                if id == "focusNextSession"
                    && searches
                        .iter()
                        .find(|(id, _, _)| id == "navigation")
                        .is_some_and(|(_, _, navigation)| {
                            should_show_setting(
                                navigation,
                                "sidebarSessionCycleSkipsSleeping",
                                true,
                            )
                        })
                {
                    rows.push(self.skip_sleeping_row(cx));
                }
            }
            blocks.extend(
                settings_section(&p, title.clone(), None, None, rows)
                    .map(|section| PageBlock::section(section_id.clone(), section)),
            );
        }
        if !any_visible {
            blocks.push(PageBlock::plain(
                div()
                    .w_full()
                    .px(px(16.0))
                    .py(px(24.0))
                    .flex()
                    .justify_center()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .child("No hotkeys match your search."),
            ));
        }
        let defaults_for_reset = defaults.clone();
        blocks.push(PageBlock::plain(h_flex().w_full().justify_end().child(
            settings_button(
                &p,
                "hotkeys-reset",
                "Reset Hotkeys",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.recorder.stop();
                    page.save_hotkeys(defaults_for_reset.clone(), cx);
                },
                cx,
            ),
        )));
        settings_page(&self.store, SettingsTabId::Hotkeys, &p, blocks, cx)
    }
}
