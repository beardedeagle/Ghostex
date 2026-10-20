/*
CDXC:AgentScreenDetection 2026-10-17 WHY:
Grok Build's tool permission card, read off the screen (measured against Grok 1.0.62 in its default mode on a zmx pty):

    ┃  ›  Remove test file and fetch example.com headers
    ┃  │ rm grok-perm-test.txt && curl -sI https://example.com | head -1
    ┃  ← → narrow scope
    ┃
    ┃  1 (○) Yes, and don't ask again for anything (always-approve mode)
    ┃  2 (●) Yes, proceed
    ┃  3 (○) No, reject (type to add feedback)
    ┃  4 (○) Never allow: rm grok-perm-test.txt
    ┃
    1/4:select  │  Tab:next option  │  ←/→:scope  │  Ctrl+o:always-approve  │  Ctrl+c:cancel  │  Esc:scrollback

It owns the input until it is answered, and the chat showed it only as the generic keyboard card. It becomes the same `permissionPrompt` notice Claude's dialog does, with one choice per row. A row's digit picks and commits it (`handle_permission_key` in Grok's `app/agent_view/interactions.rs`), so the rows are answered by their printed number. Liveness is the shortcut footer as the screen's last row.
SEE-ALSO: server/src/session_chat_grok_blocking.rs (the keyboard card this replaces), the `terminalChoice` arm of handle_answer_session_chat_prompt_http in server/src/session_chat_send/answer_http.rs.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_resume_prompt::{
    SessionChatTerminalPicker, SessionChatTerminalPickerKind, SessionChatTerminalPickerRow,
};

/// The card's title, its command or path, the scope hint and the rows.
const SCAN_LINES: usize = 60;
/// The shortcut footer is the screen's last line.
const TAIL_LINES: usize = 2;
const DETAIL_MAX_CHARS: usize = 400;
const REQUEST_MAX_LINES: usize = 8;

/// A card line without its left accent bar.
fn inner(line: &str) -> &str {
    line.trim().trim_start_matches('\u{2503}').trim()
}

/// `2 (●) Yes, proceed` → (selected, number, label).
fn parse_row(line: &str) -> Option<(bool, u32, String)> {
    let text = inner(line);
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse::<u32>().ok()?;
    let rest = text[digits.len()..].trim_start();
    let (selected, label) = if let Some(label) = rest.strip_prefix("(\u{25cf})") {
        (true, label)
    } else {
        (false, rest.strip_prefix("(\u{25cb})")?)
    };
    let label = label.trim();
    (!label.is_empty()).then(|| (selected, number, label.to_string()))
}

/// What the live card shows: its title, the lines under it, and its option rows.
struct GrokPermissionCard {
    title: String,
    request: Vec<String>,
    rows: Vec<(bool, u32, String)>,
}

fn read_card(text: &str) -> Option<GrokPermissionCard> {
    let mut window: Vec<String> = text
        .lines()
        .rev()
        .map(|raw| {
            normalize_spaces(&strip_ansi_sgr(raw))
                .trim_end()
                .to_string()
        })
        .filter(|line| !inner(line).is_empty())
        .take(SCAN_LINES)
        .collect();
    window.reverse();
    let hint = window
        .iter()
        .rposition(|line| line.contains("Ctrl+o:always-approve") && line.contains(":select"))?;
    if window.len() - hint > TAIL_LINES {
        return None;
    }
    let last_row = (0..hint)
        .rev()
        .find(|&index| parse_row(&window[index]).is_some())?;
    let mut first_row = last_row;
    while first_row > 0 && parse_row(&window[first_row - 1]).is_some() {
        first_row -= 1;
    }
    let rows: Vec<(bool, u32, String)> = window[first_row..=last_row]
        .iter()
        .filter_map(|line| parse_row(line))
        .collect();
    if rows.len() < 2 || rows.first()?.1 != 1 {
        return None;
    }
    let title = (0..first_row)
        .rev()
        .find(|&index| inner(&window[index]).starts_with('\u{203a}'))?;
    let request = window[title + 1..first_row]
        .iter()
        .map(|line| inner(line))
        .filter(|line| !line.contains("narrow scope"))
        .take(REQUEST_MAX_LINES)
        .map(str::to_string)
        .collect();
    Some(GrokPermissionCard {
        title: inner(&window[title])
            .trim_start_matches('\u{203a}')
            .trim()
            .to_string(),
        request,
        rows,
    })
}

/// The live permission card, or `None` when the screen shows none.
///
/// CDXC:AgentScreenDetection 2026-10-17 WHY: Grok lists "Yes, and don't ask again for anything (always-approve mode)" first, and the chat's Cmd+Enter answers a card's first row, so that order put a session-wide always-approve on the shortcut. The always-approve row moves below the plain approval; every row keeps its own digit, so the answer is the same key either way.
pub fn detect_grok_permission_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    let card = read_card(text)?;
    let (always, mut ordered): (Vec<_>, Vec<_>) = card
        .rows
        .into_iter()
        .partition(|(_, _, label)| label.contains("always-approve"));
    let after_yes = ordered
        .iter()
        .rposition(|(_, _, label)| label.starts_with("Yes"))
        .map_or(0, |index| index + 1);
    ordered.splice(after_yes..after_yes, always);
    let rows: Vec<SessionChatTerminalPickerRow> = ordered
        .into_iter()
        .map(|(selected, number, label)| SessionChatTerminalPickerRow {
            number,
            label,
            selected,
        })
        .collect();
    let selected_index = rows.iter().position(|row| row.selected)?;
    let mut parts = vec![card.title];
    parts.extend(
        card.request
            .iter()
            .map(|line| line.trim_start_matches('\u{2502}').trim().to_string())
            .filter(|line| !line.is_empty()),
    );
    let mut detail = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if detail.chars().count() > DETAIL_MAX_CHARS {
        detail = format!(
            "{}…",
            detail
                .chars()
                .take(DETAIL_MAX_CHARS)
                .collect::<String>()
                .trim_end()
        );
    }
    if !detail.ends_with('…') {
        detail.push('.');
    }
    Some(SessionChatTerminalPicker {
        kind: SessionChatTerminalPickerKind::GrokPermission,
        detail: Some(detail),
        rows,
        selected_index,
    })
}

/// The shell command the card asks to run (its `│ ` lines), for the folded card.
pub fn grok_permission_command(text: &str) -> Option<String> {
    let lines: Vec<String> = read_card(text)?
        .request
        .iter()
        .filter_map(|line| line.strip_prefix('\u{2502}'))
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
