//! What the app hears from a page's own script without a renderer bridge: the script calls
//! `window.__ghostexPageBridge(json)`, which queues the message in the page, and the app keeps one
//! DevTools `Runtime.evaluate` per page waiting on that queue (`install_page_bridge` in
//! app/element/cef_surface.rs drives it). Used by the floating page window
//! (app/window/visual_page_modal/page.rs) and by Browser pages' notifications
//! (app/browser_page_notifications.rs).
//!
//! CDXC:SessionChat 2026-10-22 WHY:
//! A published page is untrusted and sandboxed, so it gets no renderer bridge (the allowlist in cef/sidebar_bridge_manifest.rs). The DevTools protocol the screenshot and keep-awake code already drive lets the browser process put a script into this one browser and read what it hands back, with nothing added to the helper renderer. The page can call the queue with anything it likes; each reader only takes the messages it knows.
//!
//! CDXC:Browser 2026-10-23 WHY:
//! The bridge used to be a DevTools binding (`Runtime.addBinding`) whose calls arrive as `Runtime.bindingCalled` events. On 2026-10-23 neither the page window's picture clicks nor Browser notifications got through, which first looked like CEF not delivering binding events; the real cause was that the observer registration was dropped right after install (see `PageBridge` in app/element/cef_surface.rs). The queue read by a waiting evaluate replaced it in the meantime and was kept: one mechanism for every page, built only on method results, which the claude.ai page reader also relies on.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use futures::channel::oneshot;

/// Put in front of every bridge script: the queue the script posts to, and what the app's evaluate
/// waits on (the queued messages as a JSON array of strings).
const PAGE_BRIDGE_PRELUDE: &str = r#"(() => {
  if (window.__ghostexPageBridge) return;
  const queue = [];
  let waiter = null;
  const flush = () => {
    if (!waiter || !queue.length) return;
    const resolve = waiter;
    waiter = null;
    resolve(JSON.stringify(queue.splice(0)));
  };
  Object.defineProperty(window, '__ghostexPageBridge', {
    value: (message) => {
      if (typeof message !== 'string' || queue.length >= 100) return;
      queue.push(message);
      flush();
    },
  });
  Object.defineProperty(window, '__ghostexNextPageMessages', {
    value: () => new Promise((resolve) => { waiter = resolve; flush(); }),
  });
})();
"#;

/// The app's evaluate: the queued messages, `"[]"` after two seconds when the bridge is not in the
/// page yet (so asking again at once does not spin).
const NEXT_PAGE_MESSAGES: &str = "(window.__ghostexNextPageMessages ? window.__ghostexNextPageMessages() : new Promise((resolve) => setTimeout(() => resolve('[]'), 2000)))";

/// What one waiting evaluate answers: the JSON array of messages, or `None` when it failed (the
/// page navigated away and its context went with it).
pub type PageBridgeAnswer = oneshot::Receiver<Option<String>>;

#[derive(Default)]
struct PageBridgeWait {
    registration: Option<cef::Registration>,
    pending: Option<(c_int, oneshot::Sender<Option<String>>)>,
}

thread_local! {
    static PAGE_BRIDGE_WAITS: RefCell<HashMap<c_int, PageBridgeWait>> =
        RefCell::new(HashMap::new());
}

wrap_dev_tools_message_observer! {
    struct PageBridgeObserver {}

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
            let sender = PAGE_BRIDGE_WAITS.with(|waits| {
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

pub(crate) fn forget_page_bridge_wait(browser_id: c_int) {
    PAGE_BRIDGE_WAITS.with(|waits| {
        waits.borrow_mut().remove(&browser_id);
    });
}

impl CefBrowser {
    /// Runs the bridge queue and `script` in every document this browser loads from now on, the
    /// current one included.
    pub fn install_page_bridge_script(&self, script: &str) -> bool {
        let browser = self.browser.borrow().clone();
        let Some(host) = browser.host() else {
            return false;
        };
        let Some(mut params) = cef::dictionary_value_create() else {
            return false;
        };
        // `Page.enable`, which scripts for new documents need, is on for every browser
        // (`install_page_input_tracker`).
        params.set_string(
            Some(&CefString::from("source")),
            Some(&CefString::from(
                format!("{PAGE_BRIDGE_PRELUDE}{script}").as_str(),
            )),
        );
        params.set_bool(Some(&CefString::from("runImmediately")), 1);
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Page.addScriptToEvaluateOnNewDocument")),
            Some(&mut params),
        ) != 0
    }

    /// Asks the page for its next messages; a question already on its way is replaced.
    pub fn wait_for_page_messages(&self) -> Option<PageBridgeAnswer> {
        let browser = self.browser.borrow().clone();
        let host = browser.host()?;
        let browser_id = browser.identifier();
        let needs_observer = PAGE_BRIDGE_WAITS.with(|waits| {
            waits
                .borrow()
                .get(&browser_id)
                .is_none_or(|wait| wait.registration.is_none())
        });
        // Host calls stay outside the registry borrow: CEF may run callbacks while they execute.
        let registration = if needs_observer {
            let mut observer = PageBridgeObserver::new();
            host.add_dev_tools_message_observer(Some(&mut observer))
        } else {
            None
        };
        let mut params = cef::dictionary_value_create()?;
        params.set_string(
            Some(&CefString::from("expression")),
            Some(&CefString::from(NEXT_PAGE_MESSAGES)),
        );
        params.set_bool(Some(&CefString::from("awaitPromise")), 1);
        params.set_bool(Some(&CefString::from("returnByValue")), 1);
        let message_id = next_page_appearance_devtools_message_id();
        let (sender, answer) = oneshot::channel();
        // Registered before sending, so an answer that comes back at once finds it.
        PAGE_BRIDGE_WAITS.with(|waits| {
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
