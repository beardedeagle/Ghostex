//! What the floating page window adds to its page: one script (`page_script.js`) that opens the
//! page's pictures in a viewer inside the page and remembers where the reader left the page.
//!
//! CDXC:SessionChat 2026-10-23 DECISION:
//! User: "fam can we make the image preview thingy that opens in side the html just a simple js script that we embed in all those?? why we bridging to rust?? do simple thing fam", and for the scroll memory "use js for this too better". The page's own script draws the picture viewer over the page (the chat viewer's look, buttons and zoom, pan and stepping, as far as a sandboxed page allows) and saves and restores its scroll position through its gxserver (server/src/visual_pages/scroll.rs), with nothing sent to the app. This supersedes the 2026-10-22 viewer, which opened the chat's own image viewer over the window through the page bridge, and the window's in-memory scroll memory: the position now outlasts restarts too.
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/pageSheetScript.ts (the phone's page sheet, which still opens the phone's own viewer through `ReactNativeWebView.postMessage`), apps/desktop/src/app/visual_page_thumbnails.rs (holds the page at its top while the card's picture is taken).

/// Added to every document the page window loads.
pub(super) const PAGE_SCRIPT: &str = include_str!("page_script.js");

/// The page's id when `url` is a page gxserver serves (`…/visual/<32 hex>`).
pub(super) fn page_id(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let id = path.trim_end_matches('/').rsplit_once("/visual/")?.1;
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| id.to_ascii_lowercase())
}
