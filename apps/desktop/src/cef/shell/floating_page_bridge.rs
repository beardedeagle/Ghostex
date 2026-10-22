//! What the floating page window hears from its page: a DevTools binding (`Runtime.addBinding`)
//! the window's own script calls when the reader clicks a picture or scrolls, and that script
//! itself, added to every document the browser loads (`app/window/visual_page_modal/page.rs`).
//!
//! CDXC:SessionChat 2026-10-22 WHY:
//! A published page is untrusted and sandboxed, so it gets no renderer bridge (the allowlist in cef/sidebar_bridge_manifest.rs). The DevTools protocol the screenshot and keep-awake code already drive lets the browser process add one function to this one browser and hear its calls, with nothing added to the helper renderer. The page can call it with anything it likes; the window only reads picture addresses (data: and https:) and a scroll offset from it. Answers arrive on the observer while CEF's message pump runs, which can be inside a gpui update, so it only fills a channel.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use futures::channel::mpsc;

/// The function the window's script calls with one JSON string per message.
const FLOATING_PAGE_BINDING: &str = "__ghostexFloatingPage";

/// The page's messages, while the bridge is kept. Dropping it stops listening.
pub struct FloatingPageBridge {
    pub messages: mpsc::UnboundedReceiver<String>,
    _registration: Option<cef::Registration>,
}

wrap_dev_tools_message_observer! {
    struct FloatingPageObserver {
        sender: mpsc::UnboundedSender<String>,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut cef::Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            if method.map(CefString::to_string).as_deref() != Some("Runtime.bindingCalled") {
                return;
            }
            let Some(params) =
                params.and_then(|params| serde_json::from_slice::<serde_json::Value>(params).ok())
            else {
                return;
            };
            if params["name"] != FLOATING_PAGE_BINDING {
                return;
            }
            if let Some(payload) = params["payload"].as_str() {
                let _ = self.sender.unbounded_send(payload.to_string());
            }
        }
    }
}

impl CefBrowser {
    /// Adds the binding and `script` (run in every document this browser loads from now on, the
    /// current one included) and starts listening. `None` when the browser has no host yet.
    pub fn install_floating_page_bridge(&self, script: &str) -> Option<FloatingPageBridge> {
        let browser = self.browser.borrow().clone();
        let host = browser.host()?;
        let (sender, messages) = mpsc::unbounded();
        let mut observer = FloatingPageObserver::new(sender);
        let registration = host.add_dev_tools_message_observer(Some(&mut observer));
        // Binding calls are reported only while the Runtime domain is enabled.
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Runtime.enable")),
            None,
        );
        let mut binding = cef::dictionary_value_create()?;
        binding.set_string(
            Some(&CefString::from("name")),
            Some(&CefString::from(FLOATING_PAGE_BINDING)),
        );
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Runtime.addBinding")),
            Some(&mut binding),
        );
        // `Page.enable`, which scripts for new documents need, is on for every browser
        // (`install_page_input_tracker`).
        let mut params = cef::dictionary_value_create()?;
        params.set_string(
            Some(&CefString::from("source")),
            Some(&CefString::from(script)),
        );
        params.set_bool(Some(&CefString::from("runImmediately")), 1);
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Page.addScriptToEvaluateOnNewDocument")),
            Some(&mut params),
        );
        Some(FloatingPageBridge {
            messages,
            _registration: registration,
        })
    }
}
