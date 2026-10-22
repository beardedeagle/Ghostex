use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct Artwork {
    icons: HashMap<String, String>,
}

/// packages/gx-chat-core/visual/reference-visual.json, embedded once for this artwork and the chat's
/// reference pill colours (`native_chat/markdown_links.rs`).
pub(crate) static REFERENCE_VISUAL_JSON: &str =
    include_str!("../../../../packages/gx-chat-core/visual/reference-visual.json");

static ARTWORK: LazyLock<Artwork> = LazyLock::new(|| {
    serde_json::from_str(REFERENCE_VISUAL_JSON).expect("shared reference artwork")
});

pub(super) fn asset(key: &str) -> Option<&'static [u8]> {
    ARTWORK
        .icons
        .get(key.strip_suffix(".svg")?)
        .map(String::as_bytes)
}
