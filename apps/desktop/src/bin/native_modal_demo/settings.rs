//! Settings preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (General at the top,
//! the Storybook story's settings), `search` (the query "font", or
//! `GHOSTEX_NATIVE_MODAL_DEMO_QUERY`), `search-other` (a query only
//! other pages match: the no-matches notice), `advanced` (Show Advanced on),
//! `terminal` (Show Advanced on, scrolled to Terminal), `pick-color` (a custom terminal background
//! with its Pick Color dialog open), `select` (the Command Pane Side dropdown open), `tags`
//! (Sidebar Tags with the New tag form, from the sidebar's New tag deep link), `hotkeys` (the
//! Hotkeys entry point), `marked-preview` (the Marked sections style Preview dialog open). Saves are
//! applied back to the modal the way the app rehydrates it.
//! `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1` previews the frosted palette.
//! `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS='{"actionsHidden":false}'` merges saved settings over the
//! state's (a built-in extension switched on or off, for example).
use super::settings_modal::model::SettingsModalHost;
use super::settings_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

#[path = "settings_cloud_boxes.rs"]
mod settings_cloud_boxes;
#[path = "settings_e.rs"]
mod settings_e;
#[path = "settings_f1.rs"]
mod settings_f1;
#[path = "settings_f2.rs"]
mod settings_f2;
#[path = "settings_workspaces.rs"]
mod settings_workspaces;

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiSettingsModalWindow>)>>>;

fn story_settings(state: &str) -> serde_json::Map<String, Value> {
    let mut settings = serde_json::Map::new();
    // `modalSettings` of packages/core-ui/settings-modal.stories.tsx (deleted 2026-10-01).
    settings.insert("agentManagerZoomPercent".into(), json!(95));
    settings.insert(
        "sessionCardHoverButtons".into(),
        json!([
            { "enabled": false, "id": "rename" },
            { "enabled": false, "id": "pin" },
            { "enabled": false, "id": "note" },
            { "enabled": false, "id": "snooze" },
            { "enabled": false, "id": "closeAfterDone" },
            { "enabled": false, "id": "tag" },
            { "enabled": false, "id": "park" },
            { "enabled": false, "id": "sleep" },
            { "enabled": true, "id": "chevron" },
            { "enabled": true, "id": "close" }
        ]),
    );
    settings.insert("terminalFontSize".into(), json!(16));
    settings.insert("terminalFontWeight".into(), json!(400));
    settings.insert("terminalLineHeight".into(), json!(1.35));
    if matches!(state, "advanced" | "terminal" | "pick-color") {
        settings.insert("showAdvancedSettings".into(), json!(true));
    }
    if state == "pick-color" {
        settings.insert("terminalBackgroundMode".into(), json!("custom"));
        settings.insert("workspaceBackgroundColor".into(), json!("#1d3b53"));
    }
    settings_f2::story_settings(state, &mut settings);
    settings_f1::story_settings(state, &mut settings);
    settings_e::story_settings(state, &mut settings);
    settings_workspaces::story_settings(state, &mut settings);
    if let Ok(Value::Object(extra)) = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS")
        .map_err(|_| ())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|_| ()))
    {
        settings.extend(extra);
    }
    settings
}

fn sidebar_state(settings: &serde_json::Map<String, Value>) -> Value {
    let mut message = json!({
        "customSessionTags": {
            "order": ["custom-demo0001"],
            "tags": {
                "custom-demo0001": { "color": "#59d9ff", "icon": "rocket", "name": "Launch", "tagId": "custom-demo0001" }
            }
        },
        "hud": { "settings": Value::Object(settings.clone()) },
        "revision": 1,
        "type": "hydrate",
    });
    settings_f2::extend_sidebar_state(&mut message);
    settings_f1::extend_sidebar_state(&mut message);
    settings_e::extend_sidebar_state(&mut message);
    message
}

/// Hands a host payload (`agentHookStatus`, `remoteGxserverInstallState`, ...) to the open modal.
fn deliver(slot: &Slot, payload: Value, cx: &mut App) {
    let target = slot.borrow().clone();
    if let Some((window, view)) = target {
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, _, cx| {
                view.update(cx, |modal, cx| modal.receive_host_payload(payload, cx));
            });
        });
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let settings = Rc::new(RefCell::new(story_settings(&state)));
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let host_settings = settings.clone();
    let host_state = state.clone();
    let host: SettingsModalHost = Rc::new(move |command, cx: &mut App| {
        let apply = |patch: &serde_json::Map<String, Value>, replace: bool, cx: &mut App| {
            {
                let mut settings = host_settings.borrow_mut();
                if replace {
                    *settings = patch.clone();
                } else {
                    for (key, value) in patch {
                        settings.insert(key.clone(), value.clone());
                    }
                }
            }
            let message = sidebar_state(&host_settings.borrow());
            let target = host_slot.borrow().clone();
            if let Some((window, view)) = target {
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, _, cx| {
                        view.update(cx, |modal, cx| modal.receive_sidebar_state(message, cx));
                    });
                });
            }
        };
        match command {
            SettingsModalCommand::SavePatch { patch, source } => {
                eprintln!("save patch ({source}): {}", Value::Object(patch.clone()));
                apply(&patch, false, cx);
            }
            SettingsModalCommand::SaveSettings { settings, source } => {
                eprintln!("save settings ({source}): {} keys", settings.len());
                apply(&settings, true, cx);
            }
            SettingsModalCommand::PostMessage(message) => {
                eprintln!("post: {message}");
                for payload in settings_f2::answers(&message) {
                    deliver(&host_slot, payload, cx);
                }
                for payload in settings_e::answers(&message) {
                    deliver(&host_slot, payload, cx);
                }
            }
            // The system colour panel belongs to the app host (settings_modal_lifecycle.rs).
            SettingsModalCommand::PickSystemColor { key, initial } => {
                eprintln!("system colour panel: {key} from {initial}")
            }
            SettingsModalCommand::GxserverRpc {
                path,
                params,
                reply,
                ..
            } => {
                eprintln!("gxserver rpc: {path} {params}");
                if settings_workspaces::owns(&host_state)
                    && let Some(result) = settings_workspaces::rpc(&host_state, &path, &params)
                {
                    cx.defer(move |cx| reply(result, cx));
                    return;
                }
                if let Some(answer) = settings_cloud_boxes::rpc(&host_state, &path, &params) {
                    if let Some(result) = answer {
                        cx.defer(move |cx| reply(result, cx));
                    }
                    return;
                }
                let result = match settings_f1::rpc(&host_state, &path, &params) {
                    settings_f1::Answer::Never => return,
                    settings_f1::Answer::Reply(result) => result,
                    settings_f1::Answer::NotMine => settings_f2::rpc(&path, &params),
                };
                cx.defer(move |cx| reply(result, cx));
            }
            SettingsModalCommand::CopyToClipboard(text) => eprintln!("copy: {text}"),
            SettingsModalCommand::HttpGet { url, reply } => {
                eprintln!("http get: {url}");
                let result = settings_f1::http_get(&url);
                cx.defer(move |cx| reply(result, cx));
            }
            SettingsModalCommand::HostMessage(message) => eprintln!("host message: {message}"),
            SettingsModalCommand::Toast {
                level,
                title,
                description,
            } => {
                eprintln!("toast {level}: {title} - {description}")
            }
            SettingsModalCommand::Close => {
                eprintln!("close");
                cx.quit();
            }
            SettingsModalCommand::OpenAccounts => eprintln!("open accounts"),
            SettingsModalCommand::SaveOpenForms(stored) => eprintln!("open forms: {stored:?}"),
        }
    });
    let open_message = match state.as_str() {
        "search" => {
            let query = super::env("GHOSTEX_NATIVE_MODAL_DEMO_QUERY");
            json!({ "initialSearchQuery": if query.is_empty() { "font".to_string() } else { query } })
        }
        "search-other" => json!({ "initialSearchQuery": "tailscale" }),
        "terminal" | "pick-color" => json!({ "initialSection": "terminal" }),
        "tags" => {
            json!({ "initialSection": "sidebarTags", "initialSidebarTagsAction": "createTag" })
        }
        _ => settings_workspaces::open_message(&state)
            .or_else(|| settings_f1::open_message(&state))
            .or_else(|| settings_cloud_boxes::open_message(&state))
            .or_else(|| settings_f2::open_message(&state))
            .or_else(|| settings_e::open_message(&state))
            .unwrap_or_else(|| json!({})),
    };
    let modal_id = if state == "hotkeys" {
        "hotkeys"
    } else {
        "settings"
    };
    let mut request = SettingsOpenRequest::from_open_message(modal_id, &open_message);
    if state == "pick-color" {
        request.open_color_picker = Some("workspaceBackgroundColor".to_string());
    }
    if state == "select" {
        request.open_select = Some("commandsPanelSide".to_string());
    }
    request.gxserver_rpc_available = if settings_workspaces::owns(&state) {
        true
    } else if settings_cloud_boxes::owns(&state) {
        settings_cloud_boxes::gxserver_rpc_available(&state)
    } else {
        settings_f2::gxserver_rpc_available(&state) || settings_f1::gxserver_rpc_available(&state)
    };
    request.preview_state = (!state.is_empty()).then(|| state.clone());
    // `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1`: the frosted palette the app passes under window glass
    // (a translucent fill stands in for the blurred one).
    let palette = if super::env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let fill = super::native_modal_kit::rgba_of(demo.palette.surface, 0.72);
        demo.palette.frosted(fill)
    } else {
        demo.palette
    };
    let config = SettingsModalConfig {
        palette,
        request,
        sidebar_state: sidebar_state(&settings.borrow()),
        open_forms: serde_json::Value::Null,
    };
    let (window, view) = super::open_settings_dialog_window(
        SETTINGS_MODAL_WIDTH,
        SETTINGS_MODAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiSettingsModalWindow::new(config, host, window, cx)),
        cx,
    );
    settings_f2::after_open(window, cx);
    *slot.borrow_mut() = Some((window, view));
}
