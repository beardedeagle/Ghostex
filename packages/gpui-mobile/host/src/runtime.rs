//! Starting GPUI and feeding it commands.
//!
//! GPUI lives on gpui-mobile's `gpui-main` thread for the life of the process: it is started once,
//! builds its window when the first surface arrives, and survives Activity recreation, surface
//! loss and React Native reloads. Commands can be posted from any thread at any time after
//! [`start`]; ones sent before the first surface wait in the channel until the window exists.

use std::sync::OnceLock;

use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
};
use gpui::{App, AppContext as _, WindowOptions};
use serde_json::{Value, json};

use crate::{EventSink, HostConfig, host_view::HostView};

static COMMANDS: OnceLock<UnboundedSender<Value>> = OnceLock::new();

/// Starts the GPUI thread. Returns `false` when it was already running (a recreated Activity or a
/// reloaded JS bundle calling `start` again); the first configuration stays in force.
#[cfg(target_os = "android")]
pub fn start(config: HostConfig) -> bool {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    if COMMANDS.set(sender).is_err() {
        return false;
    }
    log::info!(
        "starting GPUI (files dir {}, config {})",
        config.files_dir.display(),
        config.raw
    );
    // `backend`: "vulkan" or "gles" limits wgpu to that backend (for comparing them on a device);
    // absent, GPUI picks, preferring Vulkan.
    use gpui_mobile::android::AndroidBackend;
    let backend = match config.str("backend") {
        Some("vulkan") => Some(AndroidBackend::Vulkan),
        Some("gles") | Some("gl") => Some(AndroidBackend::Gles),
        _ => None,
    };
    gpui_mobile::android::force_backend(backend);
    // Every GPUI copy (a text selection's Copy, the chat's copy buttons) goes to the phone's
    // clipboard through the host, which owns the Java side.
    gpui_mobile::android::platform::set_clipboard_writer(|text| {
        EventSink.emit(json!({"type": "copy", "text": text}));
    });
    // CDXC:Mobile 2026-09-27 WHY: GPUI's recognizer already stops a fling without a tap when a
    // finger lands on it; gpui-mobile's fling guard turned that touch into a tap that opened the
    // row under the finger. `flingGuard: true` restores upstream behaviour.
    gpui_mobile::android::window::set_fling_guard_enabled(
        config.bool("flingGuard").unwrap_or(false),
    );
    // The chat's icons and artwork load through the app's asset source.
    gpui_mobile::android::host::start_with_assets(
        ghostex_gpui_mobile_chat::asset_source(),
        move |cx| launch(cx, config, receiver),
    );
    true
}

/// Starts the host on iOS. Returns `false` when it was already started (a reloaded JS bundle); the
/// first configuration stays in force. GPUI itself launches on the main thread when the first
/// view asks for its window (`ios::launch`), so this may run on any thread.
#[cfg(target_os = "ios")]
pub fn start(config: HostConfig) -> bool {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    if COMMANDS.set(sender).is_err() {
        return false;
    }
    crate::ios::prepare(config, receiver);
    true
}

pub fn is_started() -> bool {
    COMMANDS.get().is_some()
}

/// Queues one JSON command (an object with a `type`) for the root view.
pub fn post_command(json: &str) -> anyhow::Result<()> {
    let command: Value = serde_json::from_str(json)?;
    anyhow::ensure!(command.is_object(), "a command must be a JSON object");
    post_value(command)
}

/// Queues an already-built command (the platform layer's own requests, such as `redraw`).
pub fn post_value(command: Value) -> anyhow::Result<()> {
    let sender = COMMANDS
        .get()
        .ok_or_else(|| anyhow::anyhow!("GPUI is not started; call start(config) first"))?;
    sender
        .unbounded_send(command)
        .map_err(|_| anyhow::anyhow!("the GPUI command channel is closed"))
}

/// Runs on the GPUI thread once the first surface exists (on iOS: the main thread, once the first
/// view joins a window).
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
pub(crate) fn launch(cx: &mut App, config: HostConfig, mut commands: UnboundedReceiver<Value>) {
    let events = EventSink;
    crate::fonts::register(cx, &config);
    let opened = cx.open_window(
        WindowOptions {
            focus: true,
            show: true,
            ..Default::default()
        },
        |window, cx| {
            let content = crate::root::build_root(window, cx, &config, events);
            cx.new(|_| HostView::new(content, events))
        },
    );
    let handle = match opened {
        Ok(handle) => handle,
        Err(error) => {
            log::error!("could not open the GPUI window: {error:#}");
            events.emit(json!({"type": "error", "message": format!("open window: {error:#}")}));
            return;
        }
    };

    let gpu = handle
        .update(cx, |_, window, _| window.gpu_specs())
        .ok()
        .flatten();
    let gpu = gpu.map(|gpu| {
        json!({
            "device": gpu.device_name,
            "driver": gpu.driver_name,
            "driverInfo": gpu.driver_info,
            "software": gpu.is_software_emulated,
        })
    });
    log::info!("window open, gpu {gpu:?}");
    events.emit(json!({"type": "ready", "gpu": gpu}));

    cx.spawn(async move |cx| {
        while let Some(command) = commands.next().await {
            let result = handle.update(cx, |host, window, cx| host.command(command, window, cx));
            if let Err(error) = result {
                log::error!("command dropped, window gone: {error:#}");
            }
        }
    })
    .detach();
}
