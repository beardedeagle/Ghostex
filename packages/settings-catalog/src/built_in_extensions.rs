//! Whether a built-in extension (an entry of `GHOSTEX_OFFICIAL_EXTENSIONS`) is on, and what a
//! whole-feature entry takes with it when it is off: its Settings pages, its rows on other pages and
//! its hotkeys. The desktop, the GPUI web build, gx-core (Quick Access) and gxserver (`ghostex`
//! verbs, `ghostex settings`) all ask here, so one switch means the same thing everywhere.
//!
//! A whole-feature entry has `placement: "feature"` and lists what it owns:
//! `settingsPages` (Settings page ids), `settingKeys` (rows on other pages) and `hotkeys` (hotkey
//! ids). Adding another feature is one entry in `data/official_extensions.rs`; the readers below and
//! the Settings window pick it up without new code.
//!
//! CDXC:Extensions 2026-10-01 DECISION:
//! User: "We need to simplify the app." Actions, Open In and Spaces become built-in extensions with an on/off switch on Settings > Extensions. Turning one off disables the whole feature: its Settings pages and rows, its header button, its hotkeys, its Quick Access rows and menus, and its `ghostex` verbs (which say it is turned off). Its saved configuration is kept, so turning it back on restores everything. More features are expected to follow with one entry each.
//! SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs, apps/desktop/src/app/window/settings_modal/tabs/extensions/sections.rs, server/src/ghostex_cli/built_in_extensions.rs.

use crate::data::GHOSTEX_OFFICIAL_EXTENSIONS;
use crate::json::J;
use crate::Platform;
use serde_json::{Map, Value};

/// The Actions feature (Start button, Settings > Actions, action hotkeys). The id predates the
/// rename and is kept because view scopes are stored under it.
pub const ACTIONS: &str = "quickActions";
/// The Open In feature (Open button, Settings > Open In, Open In menus).
pub const OPEN_IN: &str = "openIn";
/// Spaces (the sidebar's Space row, Space menus and Settings rows).
pub const SPACES: &str = "spaces";
/// Cloud Boxes (Settings > Cloud Boxes, the new thread's Run on choice, Run in box menus, the
/// `ghostex agentbox` verbs). macOS and Linux only.
pub const CLOUD_BOXES: &str = "cloudBoxes";
/// Workspaces and work mode (the workspace tile, Settings > Workspaces, Work mode on projects,
/// Link to, Create Linear ticket, the Work view, the team's Slack flow, the `ghostex workspace`,
/// `work-mode`, `link-session`, `team` and `slack` verbs).
pub const WORKSPACES: &str = "workspaces";

fn entry(id: &str) -> Option<&'static J> {
    GHOSTEX_OFFICIAL_EXTENSIONS
        .as_array()
        .iter()
        .find(|entry| entry.get("id").and_then(J::as_str) == Some(id))
}

fn strings(entry: &'static J, key: &str) -> impl Iterator<Item = &'static str> {
    entry
        .get(key)
        .map(J::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(J::as_str)
}

fn features() -> impl Iterator<Item = &'static J> {
    GHOSTEX_OFFICIAL_EXTENSIONS
        .as_array()
        .iter()
        .filter(|entry| entry.get("placement").and_then(J::as_str) == Some("feature"))
}

/// The setting that switches `id` on or off, and whether `true` means on (`settingsKeyEnables`)
/// rather than the usual inverted "hidden" key.
pub fn switch_key(id: &str) -> Option<(&'static str, bool)> {
    let entry = entry(id)?;
    let key = entry.get("settingsKey").and_then(J::as_str)?;
    Some((key, entry.get("settingsKeyEnables") == Some(&J::Bool(true))))
}

/// Whether built-in extension `id` exists on `platform` (its `platforms` list; none means every
/// platform). One that does not is left off the Extensions page and is always off there.
pub fn available_on(id: &str, platform: Platform) -> bool {
    let Some(platforms) = entry(id).and_then(|entry| entry.get("platforms")) else {
        return true;
    };
    let name = crate::help::platform_id(platform);
    platforms
        .as_array()
        .iter()
        .any(|value| value.as_str() == Some(name))
}

/// The Help sentence for an entry that exists on some platforms only.
pub fn availability_note(id: &str) -> Option<String> {
    entry(id)?.get("platforms")?;
    let names: Vec<&str> = [Platform::MacOs, Platform::Windows, Platform::Linux]
        .into_iter()
        .filter(|platform| available_on(id, *platform))
        .map(crate::availability::platform_name)
        .collect();
    Some(format!("Only on {}.", names.join(" and ")))
}

/// The built-in extension whose switch is setting `key`, if any.
pub fn feature_switched_by(key: &str) -> Option<&'static str> {
    GHOSTEX_OFFICIAL_EXTENSIONS
        .as_array()
        .iter()
        .find(|entry| entry.get("settingsKey").and_then(J::as_str) == Some(key))
        .and_then(|entry| entry.get("id").and_then(J::as_str))
}

/// Whether setting `key` belongs to a built-in extension that does not exist on `platform`: its
/// Extensions card row (keyed by its id), its switch or a row it owns on another page.
pub fn key_unavailable_on(platform: Platform, key: &str) -> bool {
    GHOSTEX_OFFICIAL_EXTENSIONS.as_array().iter().any(|entry| {
        let id = entry.get("id").and_then(J::as_str);
        id.is_some_and(|id| !available_on(id, platform))
            && (id == Some(key)
                || entry.get("settingsKey").and_then(J::as_str) == Some(key)
                || strings(entry, "settingKeys").any(|owned| owned == key))
    })
}

/// Whether Settings page `page_id` exists on `platform`: no feature owns it, or its feature does.
pub fn page_available_on(page_id: &str, platform: Platform) -> bool {
    feature_owning_page(page_id).is_none_or(|id| available_on(id, platform))
}

/// Whether built-in extension `id` is on, reading saved values through `read` and falling back to
/// the catalog default. An unknown id counts as on; one this platform does not have is off.
pub fn enabled_with(id: &str, read: impl Fn(&str) -> Option<bool>) -> bool {
    if !available_on(id, Platform::current()) {
        return false;
    }
    let Some((key, enables)) = switch_key(id) else {
        return true;
    };
    let value = read(key).unwrap_or_else(|| crate::default_value(key) == Some(&J::Bool(true)));
    value == enables
}

/// [`enabled_with`] over a settings object (the settings file or the desktop's snapshot).
pub fn enabled(settings: &Map<String, Value>, id: &str) -> bool {
    enabled_with(id, |key| settings.get(key).and_then(Value::as_bool))
}

/// [`enabled`] over any settings value; a missing or non-object value reads the defaults.
pub fn enabled_in_value(settings: Option<&Value>, id: &str) -> bool {
    enabled_with(id, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// The whole-feature entry that owns Settings page `page_id`, if any.
pub fn feature_owning_page(page_id: &str) -> Option<&'static str> {
    features()
        .find(|entry| strings(entry, "settingsPages").any(|page| page == page_id))
        .and_then(|entry| entry.get("id").and_then(J::as_str))
}

/// The whole-feature entry that owns hotkey `hotkey_id`, if any.
pub fn feature_owning_hotkey(hotkey_id: &str) -> Option<&'static str> {
    features()
        .find(|entry| strings(entry, "hotkeys").any(|owned| owned == hotkey_id))
        .and_then(|entry| entry.get("id").and_then(J::as_str))
}

/// Every row (setting key) owned by a whole-feature extension that is off, for a Settings search
/// to leave out.
pub fn hidden_setting_keys_with(read: impl Fn(&str) -> Option<bool>) -> Vec<&'static str> {
    features()
        .filter(|entry| {
            entry
                .get("id")
                .and_then(J::as_str)
                .is_some_and(|id| !enabled_with(id, &read))
        })
        .flat_map(|entry| strings(entry, "settingKeys"))
        .collect()
}

/// Settings page `page_id` shows: no feature owns it, or its feature is on.
pub fn page_shown_with(page_id: &str, read: impl Fn(&str) -> Option<bool>) -> bool {
    feature_owning_page(page_id).is_none_or(|id| enabled_with(id, read))
}

/// Hotkey `hotkey_id` is listed and fires: no feature owns it, or its feature is on.
pub fn hotkey_shown_with(hotkey_id: &str, read: impl Fn(&str) -> Option<bool>) -> bool {
    feature_owning_hotkey(hotkey_id).is_none_or(|id| enabled_with(id, read))
}

/// The customer-facing name of built-in extension `id` (its card title).
pub fn title(id: &str) -> &'static str {
    entry(id)
        .and_then(|entry| entry.get("title").and_then(J::as_str))
        .unwrap_or("This feature")
}

/// The one sentence every surface shows when something belongs to a feature that is off.
pub fn turned_off_message(id: &str) -> String {
    format!(
        "{} is turned off. Turn it on in Settings > Extensions > Store.",
        title(id)
    )
}
