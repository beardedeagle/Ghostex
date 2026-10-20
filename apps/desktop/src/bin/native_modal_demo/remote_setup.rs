//! Preview of the native Remote Setup dialog. States: `android` (popover
//! open), `connecting` (Connect never answers), `error` (Connect fails),
//! `noserver` (Connect disabled), `notailscale` (Tailscale card hidden).
use super::remote_setup_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let slot: Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiRemoteSetupModalWindow>)>>> =
        Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let fail = demo.state == "error";
    let pending = demo.state == "connecting";
    let host: RemoteSetupModalHost = Rc::new(move |command, cx: &mut App| match command {
        RemoteSetupModalCommand::OpenExternalUrl(url) => eprintln!("open external url {url}"),
        RemoteSetupModalCommand::AndroidLinkCopied => eprintln!("android link copied (copy sound)"),
        RemoteSetupModalCommand::Connect => {
            eprintln!("connect");
            if pending {
                return;
            }
            let slot = host_slot.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let target = slot.borrow().clone();
                if let Some((window, view)) = target {
                    cx.update(|cx| {
                        let _ = window.update(cx, |_root, window, cx| {
                            view.update(cx, |modal, cx| {
                                let result = if fail {
                                    Err("SSH access could not be turned on. Remote Login is managed by your organization.".to_string())
                                } else {
                                    Ok(())
                                };
                                modal.connect_finished(result, window, cx);
                            });
                        });
                    });
                }
            })
            .detach();
        }
        RemoteSetupModalCommand::OpenRemoteSettings(section) => {
            eprintln!(
                "open settings remote section {}",
                section.open_message_value()
            );
            cx.quit();
        }
        RemoteSetupModalCommand::Close => {
            eprintln!("close");
            cx.quit();
        }
    });
    let config = RemoteSetupModalConfig {
        palette: demo.palette,
        tailscale_enabled: demo.state != "notailscale",
        gxserver_available: demo.state != "noserver",
        android_open: demo.state == "android",
    };
    let (window, view) = super::open_modal_window(
        REMOTE_SETUP_MODAL_WIDTH,
        REMOTE_SETUP_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiRemoteSetupModalWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view.clone()));
    // `android-late` expands the popover one second after open, the way a
    // click does, so the window keeps its opening height and the body scrolls.
    if demo.state == "android-late" {
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            cx.update(|cx| {
                let _ = window.update(cx, |_root, _window, cx| {
                    view.update(cx, |modal, cx| modal.preview_toggle_android(cx));
                });
            });
        })
        .detach();
    }
}
