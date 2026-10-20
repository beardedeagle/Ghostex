//! Measures what the chat adds to a phone library: built as a `cdylib` for a phone target
//! (`cargo ndk -t arm64-v8a build --release -p ghostex-gpui-mobile-chat --example size_probe`),
//! it keeps the whole public API alive, so the stripped `.so` is gpui plus the chat, without a
//! platform backend.

#[unsafe(no_mangle)]
pub extern "C" fn ghostex_chat_size_probe() -> usize {
    let init: fn(&mut gpui::App, ghostex_gpui_mobile_chat::ChatInit) =
        ghostex_gpui_mobile_chat::init;
    let open: fn(
        &mut gpui::Window,
        &mut gpui::App,
        &str,
        &str,
    ) -> gpui::Entity<ghostex_gpui_mobile_chat::ChatTranscript> =
        ghostex_gpui_mobile_chat::open_transcript;
    let send = ghostex_gpui_mobile_chat::ChatTranscript::send_text;
    let assets = ghostex_gpui_mobile_chat::load_asset;
    init as usize ^ open as usize ^ send as usize ^ assets as usize
}

#[allow(dead_code)]
fn main() {}
