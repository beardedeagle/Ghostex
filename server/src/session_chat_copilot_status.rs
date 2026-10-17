//! CDXC:AgentScreenDetection 2026-10-17 WHY:
//! Copilot CLI (1.0.98) draws one footer row under its composer, its hints on the left and the model on the right, after a wide gap:
//!
//! ```text
//!  ← open sidebar · Interactive · Manual Approval · / commands · ? help · tab next tab        Auto → gpt-6-luna
//!  ◉ Working · 80 B esc interrupt                                                              Auto → gpt-6-luna
//! ```
//!
//! The mode (Interactive, Plan, Autopilot; Shift+Tab cycles them in that order) shows while the composer is idle, and the model always: `Auto` until Auto routes a turn, then `Auto → <model>`, or the model the user picked. Chat shows both on its pills; the mode pill presses Shift+Tab, and the model is changed in the CLI with `/model` (Copilot's lineup depends on the account's plan, which only the CLI knows).
//! SEE-ALSO: `copilot_catalog` in packages/gx-chat-core/src/menus/option_catalog.rs.

use crate::session_chat_options::{
    SessionChatDetectedChoice, SessionChatDetectedSelection, SessionChatOptionEvidence,
};

const MODES: [(&str, &str); 3] = [
    ("Interactive", "interactive"),
    ("Plan", "plan"),
    ("Autopilot", "autopilot"),
];

fn is_copilot_footer(left: &str) -> bool {
    ["open sidebar", "esc interrupt", "@ files", "/ commands"]
        .iter()
        .any(|hint| left.contains(hint))
}

pub(crate) fn match_copilot_statusline(line: &str) -> Option<SessionChatDetectedSelection> {
    let line = line.trim();
    let (left, right) = match line.rfind("   ") {
        Some(gap) => (line[..gap].trim(), Some(line[gap..].trim())),
        None => (line, None),
    };
    if !is_copilot_footer(left) {
        return None;
    }
    let terminal = |value: &str, label: &str| SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Terminal,
    };
    let mode = left
        .split('\u{00b7}')
        .map(str::trim)
        .find_map(|segment| MODES.iter().find(|(label, _)| *label == segment))
        .map(|(label, value)| terminal(value, label));
    let model = right.filter(|text| !text.is_empty()).map(|label| {
        let value = label.split('\u{2192}').next().unwrap_or(label).trim();
        terminal(&value.to_ascii_lowercase(), label)
    });
    (mode.is_some() || model.is_some()).then(|| SessionChatDetectedSelection {
        model,
        mode,
        ..SessionChatDetectedSelection::default()
    })
}
