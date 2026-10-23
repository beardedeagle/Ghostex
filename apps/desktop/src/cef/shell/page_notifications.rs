//! A Browser page's notifications reach the app through one DevTools `Runtime.evaluate` per page
//! that waits for the page's next notifications (`app/browser_page_notifications.rs` has the page
//! script that queues them and what the app does with them).
//!
//! CDXC:Browser 2026-10-23 WHY:
//! The page script first called a DevTools binding (`Runtime.addBinding`, as cef/shell/floating_page_bridge.rs does), but for Browser tabs its `Runtime.bindingCalled` events never reached the app's DevTools observer (seen 2026-10-23: the page called it, an outside DevTools session attached to the same page heard its own binding, the app's diagnostics saw nothing, across reloads), while method results do arrive (the claude.ai page reader relies on them). So the page queues what it shows and the app keeps an evaluate waiting on that queue, whose result is a method result.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use futures::channel::oneshot;

/// The app's evaluate for one page: the queued notifications as a JSON array string, `"[]"` when
/// the page script is not there yet (the page waits two seconds first, so asking again at once
/// does not spin).
const NEXT_NOTIFICATIONS: &str = "(window.__ghostexNextNotifications ? window.__ghostexNextNotifications() : new Promise((resolve) => setTimeout(() => resolve('[]'), 2000)))";

/// What one page's waiting evaluate answers: the JSON array string, or `None` when it failed (the
/// page navigated away and its context went with it).
pub type PageNotificationAnswer = oneshot::Receiver<Option<String>>;

#[derive(Default)]
struct PageNotificationWait {
    registration: Option<cef::Registration>,
    pending: Option<(c_int, oneshot::Sender<Option<String>>)>,
}

thread_local! {
    static PAGE_NOTIFICATION_WAITS: RefCell<HashMap<c_int, PageNotificationWait>> =
        RefCell::new(HashMap::new());
}

wrap_dev_tools_message_observer! {
    struct PageNotificationObserver {}

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            browser: Option<&mut cef::Browser>,
            message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let Some(browser) = browser else {
                return;
            };
            let browser_id = browser.identifier();
            let sender = PAGE_NOTIFICATION_WAITS.with(|waits| {
                let mut waits = waits.borrow_mut();
                let wait = waits.get_mut(&browser_id)?;
                if wait.pending.as_ref().map(|(id, _)| *id) != Some(message_id) {
                    return None;
                }
                wait.pending.take().map(|(_, sender)| sender)
            });
            let Some(sender) = sender else {
                return;
            };
            let answer = (success != 0)
                .then_some(result)
                .flatten()
                .and_then(|result| serde_json::from_slice::<serde_json::Value>(result).ok())
                .and_then(|value| value.pointer("/result/value")?.as_str().map(str::to_string));
            let _ = sender.send(answer);
        }
    }
}

pub(crate) fn forget_page_notification_wait(browser_id: c_int) {
    PAGE_NOTIFICATION_WAITS.with(|waits| {
        waits.borrow_mut().remove(&browser_id);
    });
}

impl CefBrowser {
    /// Runs `script` in every document this browser loads from now on, the current one included.
    pub fn install_page_notification_script(&self, script: &str) -> bool {
        let browser = self.browser.borrow().clone();
        let Some(host) = browser.host() else {
            return false;
        };
        let Some(mut params) = cef::dictionary_value_create() else {
            return false;
        };
        params.set_string(
            Some(&CefString::from("source")),
            Some(&CefString::from(script)),
        );
        params.set_bool(Some(&CefString::from("runImmediately")), 1);
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Page.addScriptToEvaluateOnNewDocument")),
            Some(&mut params),
        ) != 0
    }

    /// Asks the page for its next notifications; a question already on its way is replaced.
    pub fn wait_for_page_notifications(&self) -> Option<PageNotificationAnswer> {
        let browser = self.browser.borrow().clone();
        let host = browser.host()?;
        let browser_id = browser.identifier();
        let needs_observer = PAGE_NOTIFICATION_WAITS.with(|waits| {
            waits
                .borrow()
                .get(&browser_id)
                .is_none_or(|wait| wait.registration.is_none())
        });
        // Host calls stay outside the registry borrow: CEF may run callbacks while they execute.
        let registration = if needs_observer {
            let mut observer = PageNotificationObserver::new();
            host.add_dev_tools_message_observer(Some(&mut observer))
        } else {
            None
        };
        let mut params = cef::dictionary_value_create()?;
        params.set_string(
            Some(&CefString::from("expression")),
            Some(&CefString::from(NEXT_NOTIFICATIONS)),
        );
        params.set_bool(Some(&CefString::from("awaitPromise")), 1);
        params.set_bool(Some(&CefString::from("returnByValue")), 1);
        let message_id = next_page_appearance_devtools_message_id();
        let (sender, answer) = oneshot::channel();
        // Registered before sending, so an answer that comes back at once finds it.
        PAGE_NOTIFICATION_WAITS.with(|waits| {
            let mut waits = waits.borrow_mut();
            let wait = waits.entry(browser_id).or_default();
            if registration.is_some() {
                wait.registration = registration;
            }
            wait.pending = Some((message_id, sender));
        });
        let sent = host.execute_dev_tools_method(
            message_id,
            Some(&CefString::from("Runtime.evaluate")),
            Some(&mut params),
        ) != 0;
        sent.then_some(answer)
    }
}
