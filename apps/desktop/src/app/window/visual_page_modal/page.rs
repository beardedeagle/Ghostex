//! What the floating page window adds to its page and reads back from it: a click on a picture
//! opens the chat's image viewer over the window with every picture of the page, and the page's
//! scroll offset is remembered so closing and reopening the page goes back to where the reader was.
//!
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: "closing and reopening the same page brings back its scroll position". The offset is kept in memory per page for as long as the app runs, and put back once the page has loaded (and again after Refresh).
//! SEE-ALSO: apps/desktop/src/cef/shell/page_bridge.rs (the queue the script posts to), apps/desktop/src/app/native_chat/image_viewer/window.rs (the viewer over the page window), apps/mobile/app/src/chat/native/cards/pageSheetScript.ts (the phone's copy of the script, posting through `ReactNativeWebView.postMessage`).

use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap};

/// Added to every document the page window loads. Chromium's own scroll restoration on reload is
/// turned off: the window restores the offset itself (`restore_scroll_script`). Pictures inside
/// links, buttons, form controls
/// and the question kit's choices keep doing what the page made them do; so does a click the page
/// handled itself (`defaultPrevented`). `blob:` pictures and data URIs that are not base64 are
/// turned into base64 data URIs here, where they can still be read.
pub(super) const PAGE_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__ghostexFloatingPageScript) return;
  Object.defineProperty(window, '__ghostexFloatingPageScript', { value: true });
  // The window puts the scroll back itself, after the card's picture of the page top is taken.
  try { history.scrollRestoration = 'manual'; } catch (_) {}
  const send = (message) => { try { window.__ghostexPageBridge(JSON.stringify(message)); } catch (_) {} };
  const SKIP = 'a[href],button,label,summary,select,input,textarea,[role=button],[role=link],[role=radio],[role=checkbox],[role=option],[onclick],[contenteditable],gx-option,gx-questions .gx-choice,[data-gx-no-viewer]';
  const sourceOf = (img) => img.currentSrc || img.src || '';
  const viewable = (img) => {
    if (!(img instanceof HTMLImageElement) || !img.isConnected || !img.complete || !img.naturalWidth) return false;
    if (img.closest(SKIP)) return false;
    const box = img.getBoundingClientRect();
    return box.width >= 24 && box.height >= 24 && /^(data:image\/|https:|blob:)/i.test(sourceOf(img));
  };
  const nameOf = (src) => {
    if (/^data:/i.test(src)) {
      const type = ((/^data:image\/([a-z0-9.+-]+)/i.exec(src) || [])[1] || 'png').toLowerCase();
      return 'image.' + ({ jpeg: 'jpg', 'svg+xml': 'svg' }[type] || type);
    }
    try { return decodeURIComponent(new URL(src).pathname.split('/').pop()) || 'image'; } catch (_) { return 'image'; }
  };
  const readable = (img) => {
    const src = sourceOf(img);
    if (/^https:/i.test(src) || /^data:[^,]*;base64,/i.test(src)) return src;
    if (/^data:/i.test(src)) {
      const comma = src.indexOf(',');
      try { return src.slice(0, comma) + ';base64,' + btoa(unescape(encodeURIComponent(decodeURIComponent(src.slice(comma + 1))))); } catch (_) { return null; }
    }
    try {
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      canvas.getContext('2d').drawImage(img, 0, 0);
      return canvas.toDataURL('image/png');
    } catch (_) { return null; }
  };
  document.addEventListener('mouseover', (event) => {
    const img = event.target;
    if (img instanceof HTMLImageElement && !img.style.cursor && viewable(img)) img.style.cursor = 'zoom-in';
  }, true);
  document.addEventListener('click', (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const clicked = event.target;
    if (!viewable(clicked)) return;
    event.preventDefault();
    const images = [];
    let index = -1;
    for (const img of document.images) {
      if (!viewable(img)) continue;
      const src = readable(img);
      if (!src) continue;
      if (img === clicked) index = images.length;
      images.push({ src, name: nameOf(src) });
    }
    if (index >= 0) send({ type: 'images', index, images });
  });
  let timer = 0;
  addEventListener('scroll', () => {
    if (timer) return;
    timer = setTimeout(() => { timer = 0; send({ type: 'scroll', x: scrollX, y: scrollY }); }, 150);
  }, { passive: true });
})();"#;

/// The most pictures one click hands the viewer.
const IMAGES_MAX: usize = 200;
/// The most pages whose scroll offset is remembered.
const SCROLL_MEMORY_MAX: usize = 500;

pub(super) enum PageMessage {
    /// The reader clicked picture `index` of `images` (image values the chat's viewer reads).
    Images {
        images: Vec<Value>,
        index: usize,
    },
    Scroll {
        x: f64,
        y: f64,
    },
}

/// One message from the page's script; anything else the page sends is ignored.
pub(super) fn read_page_message(payload: &str) -> Option<PageMessage> {
    let message: Value = serde_json::from_str(payload).ok()?;
    match message["type"].as_str()? {
        "images" => {
            let clicked = usize::try_from(message["index"].as_u64()?).ok()?;
            let mut index = None;
            let images: Vec<Value> = message["images"]
                .as_array()?
                .iter()
                .enumerate()
                .filter_map(|(at, image)| {
                    let src = image["src"].as_str()?;
                    let transport = if src.starts_with("data:image/") {
                        "data"
                    } else if src.starts_with("https://") {
                        "url"
                    } else {
                        return None;
                    };
                    Some((
                        at,
                        src,
                        transport,
                        image["name"].as_str().unwrap_or("image"),
                    ))
                })
                .take(IMAGES_MAX)
                .enumerate()
                .map(|(kept, (at, src, transport, name))| {
                    if at == clicked {
                        index = Some(kept);
                    }
                    json!({"transport": transport, "url": src, "fileName": name})
                })
                .collect();
            Some(PageMessage::Images {
                images,
                index: index?,
            })
        }
        "scroll" => Some(PageMessage::Scroll {
            x: message["x"].as_f64().filter(|x| x.is_finite())?.max(0.0),
            y: message["y"].as_f64().filter(|y| y.is_finite())?.max(0.0),
        }),
        _ => None,
    }
}

thread_local! {
    static SCROLL_MEMORY: RefCell<HashMap<String, (f64, f64)>> = RefCell::new(HashMap::new());
}

pub(super) fn remember_scroll(page: &str, offset: (f64, f64)) {
    SCROLL_MEMORY.with(|memory| {
        let mut memory = memory.borrow_mut();
        if memory.len() >= SCROLL_MEMORY_MAX && !memory.contains_key(page) {
            memory.clear();
        }
        memory.insert(page.to_string(), offset);
    });
}

pub(super) fn remembered_scroll(page: &str) -> Option<(f64, f64)> {
    SCROLL_MEMORY
        .with(|memory| memory.borrow().get(page).copied())
        .filter(|(x, y)| *x > 0.0 || *y > 0.0)
}

/// Scrolls the loaded page back to `offset`, trying again for a few seconds while its pictures
/// and fonts are still growing it, and giving up as soon as the reader scrolls themselves.
pub(super) fn restore_scroll_script((x, y): (f64, f64)) -> String {
    format!(
        r#"(() => {{
  let tries = 0, stopped = false;
  const stop = () => {{ stopped = true; }};
  for (const kind of ['wheel', 'keydown', 'mousedown', 'touchstart']) addEventListener(kind, stop, {{ once: true, passive: true, capture: true }});
  const go = () => {{
    if (stopped) return;
    scrollTo({{ left: {x}, top: {y}, behavior: 'instant' }});
    if ((Math.abs(scrollY - {y}) > 1 || Math.abs(scrollX - {x}) > 1) && ++tries < 30) setTimeout(go, 100);
  }};
  go();
}})();"#
    )
}

/// The page's id when `url` is a page gxserver serves (`…/visual/<32 hex>`).
pub(super) fn page_id(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let id = path.trim_end_matches('/').rsplit_once("/visual/")?.1;
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| id.to_ascii_lowercase())
}
