//! The hotkey catalog's shape and the views of it Settings, Quick Access and the Help files read.

use crate::hotkey_definitions::HOTKEY_DEFINITIONS;
use crate::hotkey_label::hotkey_label;
use crate::json::{Json, ToJson, J};
use crate::Platform;

/// One configurable hotkey action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotkeyDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The macOS default (`cmd` is the primary modifier on every platform); empty when unassigned.
    pub default_key: &'static str,
    /// The Windows and Linux default where it differs; `Some("")` means unassigned there.
    pub windows_linux_default_key: Option<&'static str>,
    /// More chords that also run the action while its own binding is not cleared.
    pub alternate_default_keys: &'static [&'static str],
    /// Former defaults: a saved binding equal to one moves to the current default.
    pub retired_default_keys: &'static [&'static str],
    /// `{ id, kind, ... }`: what the hotkey does.
    pub action: J,
}

impl HotkeyDefinition {
    /// `action.kind`.
    pub fn kind(&self) -> &'static str {
        self.action
            .get("kind")
            .and_then(J::as_str)
            .unwrap_or_default()
    }

    /// The default chord on `platform`.
    pub fn platform_default_key(&self, platform: Platform) -> &'static str {
        match platform {
            Platform::MacOs => self.default_key,
            Platform::Windows | Platform::Linux => {
                self.windows_linux_default_key.unwrap_or(self.default_key)
            }
        }
    }

    /// The macOS default formatted for `platform`'s labels (`defaultKeyLabel`); empty when unassigned.
    pub fn default_key_label(&self, platform: Platform) -> String {
        if self.default_key.is_empty() {
            String::new()
        } else {
            hotkey_label(self.default_key, platform)
        }
    }
}

/// Hotkeys that act only while one view has focus, so their chord may also belong to an app-wide
/// hotkey without the two clashing (Settings marks no duplicate for them).
pub const VIEW_SCOPED_HOTKEY_IDS: &[&str] = &["searchProjectFiles"];

pub fn hotkey_definitions() -> &'static [HotkeyDefinition] {
    HOTKEY_DEFINITIONS
}

pub fn hotkey_definition(id: &str) -> Option<&'static HotkeyDefinition> {
    HOTKEY_DEFINITIONS
        .iter()
        .find(|definition| definition.id == id)
}

/// `DEFAULT_GHOSTEX_HOTKEYS`: action id to its macOS default chord, in catalog order.
pub fn default_hotkeys() -> Vec<(&'static str, &'static str)> {
    HOTKEY_DEFINITIONS
        .iter()
        .map(|definition| (definition.id, definition.default_key))
        .collect()
}

impl ToJson for HotkeyDefinition {
    fn to_json(&self) -> Json {
        let mut entries = vec![
            ("action", self.action.to_json()),
            ("defaultKey", Json::str(self.default_key)),
        ];
        if let Some(key) = self.windows_linux_default_key {
            entries.push(("windowsLinuxDefaultKey", Json::str(key)));
        }
        entries.push(("description", Json::str(self.description)));
        entries.push(("id", Json::str(self.id)));
        if !self.alternate_default_keys.is_empty() {
            entries.push((
                "alternateDefaultKeys",
                self.alternate_default_keys.to_json(),
            ));
        }
        if !self.retired_default_keys.is_empty() {
            entries.push(("retiredDefaultKeys", self.retired_default_keys.to_json()));
        }
        entries.push(("title", Json::str(self.title)));
        Json::obj(entries)
    }
}
