use super::*;

// ---------------------------------------------------------------------------
// Clear burst (upstream chat spec §7.2) — measured, not derived
// ---------------------------------------------------------------------------

/// Logical line count: `text.split(/\r\n|\r|\n/).length`. Wrapping is
/// irrelevant; one Ctrl+U clears exactly one logical line.
pub fn count_agent_tui_input_lines(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut lines = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines += 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'\n' => lines += 1,
            _ => {}
        }
        index += 1;
    }
    lines
}

/// The 2N-1 law: repetitions = 2*lines - 1 of Ctrl+U, then the same count of
/// Ctrl+K. The known line count is a LOWER bound (the user can also type into
/// the TUI directly), so bias upward — overshoot measured perfectly clean on
/// both Claude and Codex; undershoot leaves residue glued onto the next
/// message.
pub fn build_agent_tui_clear_input(line_count: usize) -> String {
    let lines = line_count.clamp(1, AGENT_TUI_CLEAR_MAX_LINES);
    let repetitions = 2 * lines - 1;
    format!(
        "{}{}",
        AGENT_TUI_CLEAR_INPUT_LINE.repeat(repetitions),
        AGENT_TUI_CLEAR_INPUT_FORWARD.repeat(repetitions)
    )
}

pub fn build_agent_tui_clear_input_for_text(text: &str) -> String {
    build_agent_tui_clear_input(count_agent_tui_input_lines(text) + AGENT_TUI_CLEAR_LINE_SLACK)
}

// ---------------------------------------------------------------------------
// Bracketed paste & sanitization (upstream chat spec §7.3/§7.4)
// ---------------------------------------------------------------------------

/// An embedded ESC (e.g. a pasted `\x1b[201~` from scrollback) would close
/// the paste frame early and run the tail as KEYSTROKES; it becomes ␛, and
/// every other control character its Control Pictures sign the same way.
pub fn sanitize_bracketed_paste_text(text: &str) -> String {
    picture_terminal_control_characters(text).into_owned()
}

/// CDXC:SessionChat 2026-10-04 WHY:
/// A control character in a message reaches the agent CLI as a keystroke, not as text: a form feed that PowerShell made out of a backticked `fsutil` sat in a `ghostex agents send` paste, and Claude Code then ignored every Return, so the message stayed in its input box while the sender was told "accepted". Every C0 control except tab, line feed and carriage return, and DEL, is therefore written as its visible Control Pictures sign (U+2400 block, ␌ for a form feed), the rule ESC already followed: the agent still sees that something was there, and nothing in the text can act as a key. Applied where the send decides what the agent is handed (session_chat_queue_runtime/send.rs), so the paste check and the delivery watchdog compare the same text the terminal shows.
pub fn picture_terminal_control_characters(text: &str) -> std::borrow::Cow<'_, str> {
    let is_key =
        |character: char| character.is_ascii_control() && !matches!(character, '\t' | '\n' | '\r');
    if !text.contains(is_key) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(
        text.chars()
            .map(|character| match character {
                '\u{7f}' => '\u{2421}',
                character if is_key(character) => {
                    char::from_u32(0x2400 + character as u32).unwrap_or('\u{fffd}')
                }
                character => character,
            })
            .collect(),
    )
}

/// xterm's native paste converts every clipboard newline to CR; direct frames
/// must match, or ConPTY TUIs treat raw LF as submit.
pub fn normalize_terminal_paste_line_endings(text: &str) -> String {
    text.replace("\r\n", "\r").replace('\n', "\r")
}

/*
Agent composers reserve a final backslash followed by Return for inserting a
newline. Ghostex sends the body and Return as separate pty writes, so a prompt
whose final byte is `\\` otherwise triggers that shortcut instead of submitting.

Stage one terminal-only trailing space to disambiguate the Return. Supported
agent composers trim trailing whitespace when they submit, so the logical
prompt still ends in the user's backslash. Apply this at the shared terminal
encoding boundary so direct chat sends, queued sends, interactive free-text
answers, and generic submitted session messages all obey the same rule.
*/
pub fn disambiguate_agent_tui_submit_text(text: &str) -> String {
    let mut staged = text.to_string();
    if staged.ends_with('\\') || ends_with_mention_token(&staged) {
        staged.push(' ');
    }
    staged
}

/// CDXC:SessionChat 2026-09-26 WHY:
/// A message whose last word is a `$skill` or `@file` mention leaves the cursor inside that token, so the composer's mention popup is open when Ghostex's Return arrives and the popup takes it: Codex 0.156 kept "… More details: use $ghostex-agents" (the Copy Details trailer) in its input box, unsent, until the user pressed Enter in the terminal. The trailing space closes the popup, as it does for a person typing; verified live, the same text with the space submits.
fn ends_with_mention_token(text: &str) -> bool {
    text.rsplit(char::is_whitespace)
        .next()
        .is_some_and(|token| token.len() > 1 && token.starts_with(['$', '@']))
}

pub fn wrap_terminal_bracketed_paste_text(text: &str) -> String {
    format!(
        "{BRACKETED_PASTE_START}{}{BRACKETED_PASTE_END}",
        sanitize_bracketed_paste_text(&normalize_terminal_paste_line_endings(text))
    )
}

/// Trailing newline alone counts as multiline.
pub fn is_multiline_draft(text: &str) -> bool {
    text.contains(['\r', '\n'])
}

/// CDXC:SessionChat 2026-10-22 WHY:
/// The terminal delivers a large write to the agent in several reads, and Claude Code treats each unframed read as a separate paste, so a long single-line message was recorded in pieces split mid-word. The transcript then no longer matched the send: the delivery watchdog reported "Your message was not delivered", the echo stayed as a second bubble, and the paste check retyped the message. Inside a bracketed-paste frame Claude Code joins the reads into one paste. Short text stays unframed so short sends are unchanged, and a real `/command` or `!command` (`parse_agent_cli_command`) stays unframed because a frame turns a long command into a `[Pasted text]` placeholder instead of running it; prose that merely opens with a slash, such as a dictated message starting with a `/Users/...` path, is framed like any other long line.
fn is_long_single_line_message(text: &str) -> bool {
    text.len() > SESSION_CHAT_UNFRAMED_SINGLE_LINE_MAX_BYTES
        && parse_agent_cli_command(text).is_none()
}

/// Input the agent CLI runs itself instead of sending to the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentCliCommand<'a> {
    /// The `/command` name, without its `/`.
    Slash(&'a str),
    /// The `!command` body, with the `!` stripped.
    Shell(&'a str),
}

/*
CDXC:AgentScreenDetection 2026-08-20:
A message the CLI executes itself instead of sending to the model. Both agents
have exactly two of these, and both use the same two prefixes:
  - `/command` — Claude's local commands and Codex's `SlashCommand` popup, from
    `/usage` and `/model` (pure UI, nothing recorded anywhere) through `/init`
    and `/compact` (a turn is recorded, but its text is the command's expanded
    prompt, never what the user typed);
  - `!command` — a shell escape in both CLIs, run locally and never sent.
Recognition is deliberately strict, because everything downstream of it either
adds evidence or REMOVES an alarm: a single line, an alphabetic first character
after the prefix, and no other punctuation inside the name, so a pasted path
(`/Users/...`) or a prose message that opens with a slash stays an ordinary
message. Namespaced plugin commands (`/plugin-dev:agent-creator`) keep their
colon — the CLI logs the full name and the delivery watchdog's needle has to
match it. The delivery watchdog (`InterceptedInput::detect`) and the send's
paste framing (`is_long_single_line_message`) share this one check, so a
message the watchdog reads as a command is exactly one the send types unframed.
*/
pub(crate) fn parse_agent_cli_command(text: &str) -> Option<AgentCliCommand<'_>> {
    let trimmed = text.trim();
    if trimmed.lines().count() != 1 {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('/') {
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':')))
            .unwrap_or(rest.len());
        if !rest[end..].chars().next().is_none_or(char::is_whitespace) {
            return None;
        }
        let name = &rest[..end];
        if !name.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            return None;
        }
        return Some(AgentCliCommand::Slash(name));
    }
    let command = trimmed.strip_prefix('!')?.trim();
    if !command.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return None;
    }
    Some(AgentCliCommand::Shell(command))
}

/// Multiline or long single-line → framed (NO submit); short single-line → sanitized unframed text.
pub fn build_session_chat_paste_bytes(text: &str) -> String {
    let staged = disambiguate_agent_tui_submit_text(text);
    // Measured after sanitizing: each control character becomes a three-byte sign.
    let unframed = sanitize_bracketed_paste_text(&staged);
    if is_multiline_draft(text) || is_long_single_line_message(&unframed) {
        wrap_terminal_bracketed_paste_text(&staged)
    } else {
        unframed
    }
}

/// Empryo's Shift+Enter in the CSI-u form its input reads as a new line.
pub const EMPRYO_NEW_LINE: &str = "\u{1b}[13;2u";

/// CDXC:SessionChat 2026-10-06 WHY:
/// The spec has multi-line text go in as a bracketed paste, but Empryo 3.9.0-beta answers every bracketed paste of text by also reading the system clipboard and attaching any image on it, so a chat message went out with a screenshot the user never sent (seen live 2026-10-06, `screenshot-192834.png`). Shift+Enter in its CSI-u form does insert a new line, so Empryo's text is typed line by line instead. A tab would switch Empryo's tabs, so it is typed as spaces. Image attachments still go in as a pasted path: a paste of image paths only is attached as those files without the clipboard read.
pub fn build_empryo_input_bytes(text: &str) -> String {
    normalize_terminal_paste_line_endings(&disambiguate_agent_tui_submit_text(text))
        .split('\r')
        .map(|line| sanitize_bracketed_paste_text(&line.replace('\t', "    ")))
        .collect::<Vec<_>>()
        .join(EMPRYO_NEW_LINE)
}

/// Image paths must LOOK like a real terminal image paste; a plain typed
/// path/@mention is read as text/file-read.
pub fn build_session_chat_image_paste_bytes(path: &str) -> String {
    wrap_terminal_bracketed_paste_text(path)
}
