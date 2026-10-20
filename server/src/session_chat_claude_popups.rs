//! Claude Code offers that open over the input box on their own and close on Escape.

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// Non-blank lines a popup may still draw under its last row (Claude's
/// "Enter to confirm · Esc to cancel" guide). Anything taller means something
/// else was painted below it, so the popup is no longer what the keyboard reaches.
const POPUP_TRAILING_LINES: usize = 3;

struct ClaudePopup {
    /// What the refusal and the diagnostics log call it.
    name: &'static str,
    title: fn(&str) -> bool,
    /// Every numbered row, in order; a row matches when its label starts with this.
    rows: &'static [&'static str],
    /// Drawn over the composer (which stays painted under it) instead of in its place.
    above_composer: bool,
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: pressing Enter in the chat must always send, clearing whatever is blocking the message first. Claude Code 2.1.281 opened its "LSP plugin recommendation" over the input box and every send came back until the user switched to the terminal. Claude opens these offers by itself, so the send presses Escape and then types the message. Claude's source answers Escape with "No, not now" (LSP), "No" (plugin hints), "Never mind" (Remote Control) or closes the notice (API spend), so nothing is installed, enabled or turned off.
/// WHY: matched by exact title and rows, never by panel shape: Escape on a permission prompt rejects the tool call and on the trust dialog exits Claude, so a panel that is not listed here still refuses the send.
const CLAUDE_ESCAPE_SAFE_POPUPS: &[ClaudePopup] = &[
    ClaudePopup {
        name: "LSP plugin recommendation",
        title: |line| line == "LSP plugin recommendation",
        rows: &[
            "Yes, install",
            "No, not now",
            "Never for this plugin",
            "Disable all LSP recommendations",
        ],
        above_composer: false,
    },
    ClaudePopup {
        name: "plugin recommendation",
        title: |line| line == "Plugin recommendation",
        rows: &[
            "Yes, install",
            "No",
            "No, and don't show plugin installation hints again",
        ],
        above_composer: false,
    },
    ClaudePopup {
        name: "Remote Control offer",
        title: |line| line == "Remote Control",
        rows: &["Enable Remote Control", "Never mind"],
        above_composer: false,
    },
    ClaudePopup {
        name: "API spending notice",
        title: |line| line.starts_with("You've spent $") && line.ends_with("this session."),
        rows: &["Got it, thanks!"],
        above_composer: false,
    },
    // CDXC:SessionChat 2026-09-30 WHY:
    // Claude Code 2.1.284 opens "Teach auto mode about your environment?" in auto mode after repeated denials and keeps its input box painted underneath, so the send saw a composer, pasted into the offer and failed with "The terminal did not accept the pasted message". Its Escape handler is `onCancel: () => choose("later")`, the same as "Not now" (a 7-day snooze, no settings change).
    ClaudePopup {
        name: "auto mode setup offer",
        title: |line| line == "Teach auto mode about your environment?",
        rows: &["Yes", "Not now", "Don't show again"],
        above_composer: true,
    },
];

/// The name of the listed Claude popup that currently owns the input line, if any.
pub(crate) fn claude_escape_safe_popup(screen_text: &str) -> Option<&'static str> {
    let lines: Vec<String> = screen_text
        .lines()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    CLAUDE_ESCAPE_SAFE_POPUPS
        .iter()
        .find(|popup| popup_is_live(popup, &lines))
        .map(|popup| popup.name)
}

fn popup_is_live(popup: &ClaudePopup, lines: &[String]) -> bool {
    let Some(title) = lines.iter().rposition(|line| (popup.title)(line)) else {
        return false;
    };
    let rows: Vec<(usize, u8, &str)> = lines
        .iter()
        .enumerate()
        .skip(title + 1)
        .filter_map(|(index, line)| {
            numbered_row(line).map(|(number, label)| (index, number, label))
        })
        .collect();
    rows.len() == popup.rows.len()
        && rows.iter().zip(popup.rows).zip(1u8..).all(
            |(((_, number, label), expected), position)| {
                *number == position && label.starts_with(expected)
            },
        )
        && rows.last().is_some_and(|(last, _, _)| {
            let after = &lines[last + 1..];
            if popup.above_composer {
                return composer_follows_guide(after);
            }
            after.len() <= POPUP_TRAILING_LINES && after.iter().all(|line| !line.starts_with('❯'))
        })
}

/// The popup's own "Enter to confirm · Esc to cancel" guide, then directly the composer's
/// `─` rule and its `❯` line: nothing else was painted between the popup and the input box.
fn composer_follows_guide(after: &[String]) -> bool {
    let is_rule = |line: &String| line.chars().count() >= 20 && line.chars().all(|c| c == '─');
    matches!(after, [guide, rule, input, ..]
        if guide.starts_with("Enter to confirm") && is_rule(rule) && input.starts_with('❯'))
}

/// `❯ 2. No, not now` or `2. No, not now` → `(2, "No, not now")`, for rows numbered 1..=9.
fn numbered_row(line: &str) -> Option<(u8, &str)> {
    let row = line.trim_start_matches('❯').trim_start();
    let (number, label) = row.split_once(". ")?;
    let digit = *number.as_bytes().first()?;
    (number.len() == 1 && digit.is_ascii_digit()).then(|| (digit - b'0', label.trim()))
}

/// CDXC:SessionChat 2026-09-25 WHY:
/// Down from Claude's input box moves the keyboard into the background-agents list under it ("Enter to view · x to stop"); a chat send then pasted into the list, failed with "The terminal did not accept the pasted message", and its Enter would have opened the agent. Escape hands the keyboard back to the input box without interrupting a running turn (verified live on Claude Code 2.1.281, mid-turn), so the send presses it first. The list's own hint is required too, so a statusline that happens to draw `❯` is not mistaken for it.
pub(crate) fn claude_agents_list_focused(screen_text: &str) -> bool {
    let lines: Vec<String> = screen_text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    let Some(rule) = lines.iter().rposition(|line| {
        let line = line.trim();
        line.chars().count() >= 20 && line.chars().all(|c| c == '─')
    }) else {
        return false;
    };
    let footer = &lines[rule + 1..];
    footer.iter().any(|line| line.starts_with('❯'))
        && footer.iter().any(|line| {
            let line = line.trim();
            line.starts_with("Enter to view") || line.starts_with("↑/↓ to select")
        })
}
