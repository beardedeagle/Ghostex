use super::*;
use x11rb::protocol::xinput::{ConnectionExt as _, Device, EventMask, XIEventMask};

static OWNERS: Mutex<Option<HashMap<X11Window, X11Window>>> = Mutex::new(None);

/// CDXC:FocusRouting 2026-09-19 WHY:
/// A window manager can activate GPUI's parent on a Chromium click while Chromium retains only logical focus; keys then follow the pointer or the stale address input.
/// Observe XI2 presses on Chromium's own input child, without grabbing, suppressing, or redispatching pointer events, and grant that child native focus using the original event timestamp.
pub(super) fn install(cx: &gpui::App) {
    let (connection, _) = x11_connection();
    connection
        .xinput_xi_query_version(2, 0)
        .expect("XI2 query")
        .reply()
        .expect("XI2 is required for CEF pointer focus");
    let (tx, mut rx) = mpsc::unbounded();
    std::thread::Builder::new()
        .name("cef-x11-focus".into())
        .spawn(move || {
            while let Ok(event) = connection.wait_for_event() {
                if let x11rb::protocol::Event::XinputButtonPress(event) = event {
                    if event.detail != 1 {
                        continue;
                    }
                    let owner = OWNERS
                        .lock()
                        .expect("CEF pointer registry")
                        .as_ref()
                        .and_then(|owners| owners.get(&event.event).copied());
                    if let Some(owner) = owner {
                        if tx.unbounded_send((owner, event.event, event.time)).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .expect("CEF pointer event thread");
    let app = cx.to_async();
    cx.foreground_executor()
        .spawn(async move {
            while let Some((owner, child, time)) = rx.next().await {
                let native_view = owner as usize as *mut c_void;
                if super::super::shell::cef_native_view_is_hidden(native_view) {
                    continue;
                }
                let Some(host) = embed_host_for_cef_window(owner) else {
                    continue;
                };
                // A click in a page never reaches GPUI's own mouse-down, so it clicks away from
                // open popups here (popup_dismissal.rs), except from the window the page is in.
                if let Some(window) = gpui_window_of_embed_host(host) {
                    app.update(|cx| {
                        crate::app::window::popup_dismissal::note_main_window_pointer_press(
                            window, cx,
                        )
                    });
                }
                let (connection, _) = x11_connection();
                let _ = connection.set_input_focus(InputFocus::PARENT, child, time);
                let _ = connection.flush();
                // X rejects an event older than a newer chrome focus request.
                if native_view_owns_first_responder(native_view) {
                    super::super::shell::receive_native_pointer_focus(native_view);
                }
            }
        })
        .detach();
}

/// The GPUI window an embed host was created in (its X11 parent).
fn gpui_window_of_embed_host(host: X11Window) -> Option<gpui::WindowId> {
    let (connection, _) = x11_connection();
    let parent = connection.query_tree(host).ok()?.reply().ok()?.parent;
    super::super::shell::gpui_window_of_native_parent(parent as usize)
}

pub(super) fn observe(owner: X11Window) {
    let (connection, _) = x11_connection();
    let children = connection
        .query_tree(owner)
        .expect("CEF input children query")
        .reply()
        .expect("CEF input children")
        .children;
    for child in children {
        connection
            .xinput_xi_select_events(
                child,
                &[EventMask {
                    deviceid: Device::ALL_MASTER.into(),
                    mask: vec![XIEventMask::BUTTON_PRESS],
                }],
            )
            .expect("CEF XI2 selection")
            .check()
            .expect("CEF XI2 pointer observation");
        OWNERS
            .lock()
            .expect("CEF pointer registry")
            .get_or_insert_with(HashMap::new)
            .insert(child, owner);
    }
    let _ = connection.flush();
}

pub(super) fn forget(owner: X11Window) {
    if let Some(owners) = OWNERS.lock().expect("CEF pointer registry").as_mut() {
        owners.retain(|_, registered_owner| *registered_owner != owner);
    }
}
