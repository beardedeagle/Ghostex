// C1 wave-3 re-cluster: terminal text-input, IME and sleeping-placeholder wake helpers, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:Terminal 2026-06-23-10:13:
Phase 2 terminal input parity forwards only committed GPUI `key_char` text. Do not synthesize from `key`, do not map physical keys without a native keycode, reject Cmd/Super and Control modified events so shortcuts and control-key terminal bindings can use a later Ghostty key-event bridge, and keep Option-generated characters when GPUI has already committed them as text.

CDXC:Terminal 2026-06-23-10:18:
Focused text delivery must choose an explicit terminal target before touching a surface: Agents requires Agents mode plus an Agents-pane focus target, command requires command-pane focus, and Browser/project-editor focus must no-op instead of falling through to a terminal helper.

CDXC:Terminal 2026-06-23-10:45:
Terminal IME/preedit delivery is a text-service path, not a keyboard fallback. GPUI may register input handling only for the exact focused mounted terminal body, then send committed text/preedit bytes to the matching Ghostty owner while retaining only sanitized UTF-16 marked ranges and no raw typed or terminal content.

CDXC:Terminal 2026-06-23-11:50:
The local GPUI app event API exposes `KeyDownEvent { keystroke, is_held, prefer_character_input }`; `Keystroke` exposes only modifiers, layout-derived `key`, and committed `key_char`. The macOS backend uses `NSEvent.keyCode()` while constructing `Keystroke` but drops that native keycode before app listeners run, so Ghostty physical-key forwarding must remain blocked until GPUI exposes a stable native keycode or UIEvents-code physical key identity.

CDXC:Terminal 2026-06-23-14:23:
Committed `key_char` text is a text-input signal, not evidence for physical-key or key-binding parity. Layout `key` values, Control shortcuts, and Cmd/Super shortcuts must stay rejected by this helper until GPUI can pass a stable native keycode or UIEvents-code identity to Ghostty without guessing.
*/
pub(crate) fn committed_terminal_text_from_key_down_event(event: &KeyDownEvent) -> Option<&str> {
    committed_terminal_text_from_keystroke(&event.keystroke)
}

pub(crate) fn committed_terminal_text_from_keystroke(keystroke: &Keystroke) -> Option<&str> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control {
        return None;
    }

    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() { None } else { Some(text) }
}

pub(crate) fn command_pane_sleeping_placeholder_keystroke_requests_wake(
    keystroke: &Keystroke,
) -> bool {
    /*
    CDXC:SessionSleep 2026-06-25-14:49:
    Sleeping command placeholders wake on plain alphanumeric key-downs like native AppKit. Reject Cmd, Control, and Option/Alt modified keys, and use GPUI's layout key only as a wake-affordance identity for shifted digits rather than as terminal input.

    CDXC:SessionSleep 2026-06-25-19:02:
    Native AppKit rejects Function-modified wake keys before inspecting alphanumeric text. GPUI must keep Function inert too, while still allowing plain and Shift-only letters/digits that AppKit treats as `charactersIgnoringModifiers`.
    */
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt || modifiers.function {
        return false;
    }

    command_pane_sleeping_placeholder_wake_text_is_alphanumeric(keystroke.key_char.as_deref())
        || command_pane_sleeping_placeholder_wake_text_is_alphanumeric(Some(&keystroke.key))
}

pub(crate) fn command_pane_sleeping_placeholder_wake_text_is_alphanumeric(
    text: Option<&str>,
) -> bool {
    let Some(text) = text else {
        return false;
    };
    let mut chars = text.chars();
    let Some(character) = chars.next() else {
        return false;
    };
    chars.next().is_none() && character.is_ascii_alphanumeric()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextTarget {
    Agents,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedTerminalTextMountTarget {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalTextMarkedRange {
    pub(crate) target: FocusedTerminalTextMountTarget,
    pub(crate) range: Range<usize>,
}

pub(crate) fn focused_terminal_text_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
) -> Option<FocusedTerminalTextTarget> {
    let _ = active_mode;
    match shell_focus {
        ShellFocusTarget::AgentsPane(_) => Some(FocusedTerminalTextTarget::Agents),
        ShellFocusTarget::CommandPane => Some(FocusedTerminalTextTarget::Command),
        ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_) => None,
    }
}

pub(crate) fn terminal_text_marked_range_for_preedit(
    replacement_range: Option<Range<usize>>,
    new_text: &str,
) -> Option<Range<usize>> {
    let marked_len_utf16 = new_text.encode_utf16().count();
    if marked_len_utf16 == 0 {
        return None;
    }

    let marked_start = replacement_range.map_or(0, |range| range.start);
    Some(marked_start..marked_start.saturating_add(marked_len_utf16))
}
