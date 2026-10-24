//! "claude.ai in Ghostex" while a Sign in to claude.ai is under way: a spinner beside the button
//! until the account's claude.ai page reports the sign-in, and the row turning to Signed in and
//! Sign out as soon as it does, with Settings open.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User, after signing in from Settings: "it takes a while for "sign in" to turn to "sign out" i think let's do some spinner to it's left and hwne you hover on it should say waiting for you to sign-in". From the Sign in to claude.ai click until the account's page is seen signed in (or 15 minutes pass), a spinner sits left of the button with the tooltip "Waiting for you to sign in to claude.ai. This updates a moment after you sign in.", and the open page re-reads the sign-in every second, so the row changes without reopening Settings.
//!
//! SEE-ALSO: app/claude_cloud_page.rs (writes the sign-in the page reader sees),
//! app/claude_cloud_sign_in_return.rs (Settings closes for the sign-in and reopens on this account).

use super::AccountsTab;
use gpui::Context;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A sign-in nobody finished in this long stops showing as under way.
const WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);
const WAIT_POLL: Duration = Duration::from_secs(1);

pub(super) const WAITING_TOOLTIP: &str =
    "Waiting for you to sign in to claude.ai. This updates a moment after you sign in.";

thread_local! {
    /// The accounts whose Sign in to claude.ai was clicked, kept while Settings is closed.
    static WAITING: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
    /// The account Settings reopens on, taken by the next Accounts page.
    static REOPEN_ON: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Sign in to claude.ai was clicked for `account_id`.
pub(super) fn start(account_id: &str) {
    WAITING.with(|waiting| {
        waiting
            .borrow_mut()
            .insert(account_id.to_string(), Instant::now())
    });
    REOPEN_ON.with(|reopen| *reopen.borrow_mut() = Some(account_id.to_string()));
}

/// Whether `account_id`'s sign-in is under way; `signed_in` ends it.
pub(super) fn waiting(account_id: &str, signed_in: bool) -> bool {
    WAITING.with(|waiting| {
        let mut waiting = waiting.borrow_mut();
        waiting.retain(|_, started| started.elapsed() < WAIT_LIMIT);
        if signed_in {
            waiting.remove(account_id);
        }
        waiting.contains_key(account_id)
    })
}

fn any_waiting() -> bool {
    WAITING.with(|waiting| {
        let mut waiting = waiting.borrow_mut();
        waiting.retain(|_, started| started.elapsed() < WAIT_LIMIT);
        !waiting.is_empty()
    })
}

/// The account the Accounts page opens on after a sign-in closed Settings, once.
pub(super) fn take_reopen_account() -> Option<String> {
    REOPEN_ON.with(|reopen| reopen.borrow_mut().take())
}

impl AccountsTab {
    /// Re-reads the sign-ins every second while one is under way.
    pub(super) fn watch_claude_ai_sign_ins(&mut self, cx: &mut Context<Self>) {
        if self.claude_ai_polling || !any_waiting() {
            return;
        }
        self.claude_ai_polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WAIT_POLL).await;
                let more = this.update(cx, |page, cx| {
                    cx.notify();
                    if any_waiting() {
                        return true;
                    }
                    page.claude_ai_polling = false;
                    false
                });
                if !matches!(more, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}
