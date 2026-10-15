//! The OS-aware keyboard-shortcut label used by the terminal overlays, the
//! hotkey settings rows, and the native modals.
//!
//! CDXC:Hotkeys 2026-09-16 WHY:
//! This lived in terminal_element.rs, which the native-modal-demo binary cannot
//! include, so a modal that labels a shortcut failed to compile there.
//! It is a self-contained formatter, so it owns its own module and every
//! binary includes just this file.
//! SEE-ALSO: packages/shared/hotkey-label.ts, the TypeScript twin.

pub(crate) fn terminal_overlay_hotkey_chord_label(chord: &str) -> String {
    hotkey_chord_label(chord, cfg!(target_os = "macos"))
}

/// The label in the Mac spelling (glyphs, no separator) or the Windows/Linux one (`Ctrl+Enter`).
/// The browser build's chat passes whether the browser is on a Mac; everything else uses
/// [`terminal_overlay_hotkey_chord_label`], which follows the compiled platform like the app
/// hotkey bindings it labels.
pub(crate) fn hotkey_chord_label(chord: &str, mac: bool) -> String {
    let parts = chord
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_primary_modifier = parts
        .iter()
        .any(|part| matches!(part.as_str(), "cmd" | "command" | "meta"));
    let has_option_modifier = parts
        .iter()
        .any(|part| matches!(part.as_str(), "alt" | "opt" | "option"));
    let mut labels = Vec::with_capacity(parts.len());
    for part in parts {
        let label = if mac {
            match part.as_str() {
                "cmd" | "command" | "meta" => "⌘".to_string(),
                "ctrl" | "control" => "⌃".to_string(),
                "alt" | "opt" | "option" => "⌥".to_string(),
                "shift" => "⇧".to_string(),
                "up" | "arrowup" => "↑".to_string(),
                "right" | "arrowright" => "→".to_string(),
                "down" | "arrowdown" => "↓".to_string(),
                "left" | "arrowleft" => "←".to_string(),
                "tab" => "Tab".to_string(),
                "enter" | "return" => "Enter".to_string(),
                "escape" | "esc" => "Esc".to_string(),
                "ß" if has_option_modifier => "S".to_string(),
                value if value.len() == 1 => value.to_uppercase(),
                value if value.starts_with('f') => value.to_uppercase(),
                value => capitalize_key_name(value),
            }
        } else {
            match part.as_str() {
                "cmd" | "command" | "meta" => "Ctrl".to_string(),
                "ctrl" | "control" if has_primary_modifier => "Alt".to_string(),
                "ctrl" | "control" => "Ctrl".to_string(),
                "alt" | "opt" | "option" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                "up" | "arrowup" => "↑".to_string(),
                "right" | "arrowright" => "→".to_string(),
                "down" | "arrowdown" => "↓".to_string(),
                "left" | "arrowleft" => "←".to_string(),
                "tab" => "Tab".to_string(),
                "enter" | "return" => "Enter".to_string(),
                "escape" | "esc" => "Esc".to_string(),
                "ß" if has_option_modifier => "S".to_string(),
                value if value.len() == 1 => value.to_uppercase(),
                value if value.starts_with('f') => value.to_uppercase(),
                value => capitalize_key_name(value),
            }
        };
        if labels.last() != Some(&label) {
            labels.push(label);
        }
    }
    labels.join(if mac { "" } else { "+" })
}

/// Named keys (`space`, `backspace`) read with a capital first letter, as in
/// packages/shared/hotkey-label.ts.
fn capitalize_key_name(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
