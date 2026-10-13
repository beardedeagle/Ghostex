use super::*;

/// The id of the card's Sign in action. It rides the `restartAgent` answer every client already
/// sends, with this id as `actionId` (session_chat_agent_restart.rs), so a client built before the
/// action existed still restarts the agent instead of doing nothing.
pub const SESSION_CHAT_NOTICE_ACTION_SIGN_IN: &str = "signInAccount";
/// The id of the card's Try again action, the same restart the Restart action of an exited agent is.
pub const SESSION_CHAT_NOTICE_ACTION_RETRY_START: &str = "retryStart";

/// What claude-swap prints when `cswap run <slot>` finds no saved login for that slot.
const CSWAP_NO_CREDENTIALS: &str = "has no stored credentials";

/// CDXC:AgentProviders 2026-10-14 WHY:
/// Waking a session on a cswap account with no saved login printed "Error: Account-5 has no stored credentials", the restore script's fallback failed the same way, and the terminal dropped to the shell; nothing classified that screen, so gxserver kept the session running and idle, the chat showed nothing, and the message the user had sent waited as a startup send for good. The screen now becomes a sign-in card that names the account and offers Switch account (every `loginExpired` card has it), Sign in to that account and Try again. Waiting cannot bring a login back, so the queue holds the message behind this card instead of retrying it, and sends it once the agent is up (session_chat_agent_restart.rs heals the start when a working account or the login appears).
/// SEE-ALSO: accounts/wake_account.rs (a wake avoids such an account when it may switch), session_chat_queue_runtime/scheduler.rs (holds rows behind the card).
pub(super) fn detect_claude_missing_login(
    screen: &NoticeScreen,
) -> Option<SessionChatTerminalNotice> {
    if !screen.ends_with_shell_prompt() {
        return None;
    }
    let slot = cswap_missing_login_slot(&flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES))?;
    let email = claude_slot_email(&slot);
    let account = match email {
        Some(email) => format!("account {slot} ({email})"),
        None => format!("account {slot}"),
    };
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Screen,
            format!("Claude couldn't start: {account} has no saved login on this computer"),
        )
        .with_input_blocking(true)
        .with_detail(format!(
            "Switch this session to an account that works, or sign in to account {slot} on this computer. A message you send meanwhile waits in the chat and goes out as soon as Claude is up."
        ))
        .with_screen_tail(screen.screen_tail())
        .with_actions(vec![
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_SIGN_IN.to_string(),
                label: format!("Sign in to account {slot}"),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_RETRY_START.to_string(),
                label: "Try again".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction::switch_to_terminal(OPEN_TERMINAL.label),
        ]),
    )
}

/// The last email seen for each Claude cswap slot.
static CLAUDE_SLOT_EMAILS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

/// The email of Claude cswap `slot`, from the shared account snapshot or, while that has no row
/// for it, the last one seen.
/// WHY: a cswap read that fails leaves the snapshot without its Claude rows for a couple of
/// minutes, and a title without the email made the card a new instance, which brought back a
/// card the user had dismissed.
fn claude_slot_email(slot: &str) -> Option<String> {
    let current = crate::accounts::runtime::current_snapshot()
        .accounts
        .into_iter()
        .find(|account| {
            account.provider == crate::accounts::model::Provider::Claude && account.selector == slot
        })
        .map(|account| account.email)
        .filter(|email| !email.is_empty());
    let mut emails = CLAUDE_SLOT_EMAILS.lock().ok()?;
    match current {
        Some(email) => {
            emails.insert(slot.to_string(), email.clone());
            Some(email)
        }
        None => emails.get(slot).cloned(),
    }
}

/// The cswap slot named by the last "Account-<slot> has no stored credentials" in `text`.
pub fn cswap_missing_login_slot(text: &str) -> Option<String> {
    let before = &text[..text.rfind(CSWAP_NO_CREDENTIALS)?];
    let after_label = &before[before.rfind("Account-")? + "Account-".len()..];
    // A narrow terminal can wrap the line right after the hyphen.
    let slot = after_label.trim();
    (!slot.is_empty() && slot.chars().all(|ch| ch.is_ascii_digit())).then(|| slot.to_string())
}

impl SessionChatTerminalNotice {
    /// The agent could not start because its account has no saved login on this computer
    /// (`detect_claude_missing_login`): only the user, or a switch to another account, fixes it.
    pub fn missing_login(&self) -> bool {
        self.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED
            && self
                .actions
                .iter()
                .any(|action| action.id == SESSION_CHAT_NOTICE_ACTION_SIGN_IN)
    }

    /// The cswap slot a `missing_login` card is about, read back from its screen tail.
    pub fn missing_login_slot(&self) -> Option<String> {
        self.missing_login()
            .then(|| cswap_missing_login_slot(&self.screen_tail.as_deref()?.replace('\n', " ")))
            .flatten()
    }
}

/// Whether the newest non-blank line of `screen_text` is a bare shell prompt, so nothing (no agent,
/// no login) runs in front of the shell.
pub fn session_chat_screen_ends_with_shell_prompt(screen_text: &str) -> bool {
    NoticeScreen::new(screen_text).ends_with_shell_prompt()
}
