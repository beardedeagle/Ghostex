//! The Settings values that differ by platform: shortcut wording and the macOS copy-on-select options.

use crate::data::{GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS, GHOSTTY_COPY_ON_SELECT_OPTIONS};
use crate::hotkey_label::hotkey_label;
use crate::json::Opt;
use crate::Platform;

/// Copy on select's options as this platform's terminal applies them (see `GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS`).
pub fn copy_on_select_options(platform: Platform) -> &'static [Opt] {
    if platform == Platform::MacOs {
        GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS
    } else {
        GHOSTTY_COPY_ON_SELECT_OPTIONS
    }
}

pub fn copy_on_select_description(platform: Platform) -> &'static str {
    if platform == Platform::MacOs {
        "Copy selected terminal text automatically. On macOS both options also copy to the system clipboard."
    } else {
        "Copy selected terminal text automatically."
    }
}

pub fn paste_previewable_images_description(platform: Platform) -> String {
    format!(
        "Paste clipboard images as previewable Markdown links with {}. Hold {} over the linked path to preview it in the terminal, and see the same image preview in the {} Rich Prompt Editor.",
        hotkey_label("cmd+v", platform),
        hotkey_label("cmd", platform),
        hotkey_label("ctrl+g", platform),
    )
}
