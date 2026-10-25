//! A phone-sized window on the Mac showing the transcript-only chat of one session, against the
//! live local gxserver, for checking how the desktop's transcript looks and behaves at phone width
//! without a phone.
//!
//! ```sh
//! cargo run -p ghostex-gpui-mobile-chat --example preview -- --session P3lv0:G7c2p \
//!     --script "wait:6000,shot:/tmp/a.png,wheel:-3000,wait:600,shot:/tmp/b.png,mem,quit"
//! ```
//!
//! - The window opens at the right edge of the main screen WITHOUT activating the app or taking
//!   focus, so the user's keyboard stays where it is.
//! - gxserver's port comes from `~/.local/state/ghostex/gxserver/runtime/server.json` and its token
//!   from `~/.local/state/ghostex/gxserver/auth/token`; the token is never printed.
//! - The chat's own storage goes to a throwaway folder (`--data-dir`, default
//!   `target/chat-preview-data`), never the desktop app's `client-storage.sqlite3`.
//! - It only reads unless `--allow-send` is passed: nothing else types, sends, answers or saves a
//!   draft.
//!
//! `--script` steps, comma separated: `wait:<ms>`, `shot:<png path>` (the window alone, through
//! `screencapture -l`, without bringing it forward), `wheel:<dy>` (a real scroll-wheel event over the
//! transcript, positive = older rows, sent in 120px steps like a trackpad), `scroll:<dy>`
//! (programmatic), `end`, `click:<x>:<y>` (a left click in window coordinates), `session:<p>:<s>`,
//! `rows`, `mem`, `quit`, and `send:<text>`, which only runs with `--allow-send` (for a throwaway
//! session; the text cannot contain a comma).

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The preview runs on macOS only.");
}

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use ghostex_gpui_mobile_chat::{ChatInit, ChatTranscript, ChatTranscriptEvent, SessionRef};
    use gpui::{
        AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, IntoElement, Modifiers,
        MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, PlatformInput, Render,
        ScrollDelta, ScrollWheelEvent, Styled as _, TouchPhase, Window, WindowBounds, WindowKind,
        WindowOptions, canvas, div, point, px, size,
    };

    struct Args {
        session: SessionRef,
        width: f32,
        height: f32,
        light: bool,
        zoom: Option<f64>,
        script: Vec<String>,
        data_dir: PathBuf,
        /// `send:` steps run only with `--allow-send`, for a throwaway session.
        allow_send: bool,
    }

    fn args() -> Args {
        let mut session = None;
        let mut width = 412.0;
        let mut height = 860.0;
        let mut light = false;
        let mut allow_send = false;
        let mut zoom = None;
        let mut script = Vec::new();
        let mut data_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/chat-preview-data");
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--session" => session = args.next(),
                "--width" => width = args.next().and_then(|v| v.parse().ok()).unwrap_or(width),
                "--height" => height = args.next().and_then(|v| v.parse().ok()).unwrap_or(height),
                "--light" => light = true,
                "--allow-send" => allow_send = true,
                "--zoom" => zoom = args.next().and_then(|v| v.parse().ok()),
                "--data-dir" => data_dir = args.next().map(PathBuf::from).unwrap_or(data_dir),
                "--script" => {
                    script = args
                        .next()
                        .unwrap_or_default()
                        .split(',')
                        .map(|step| step.trim().to_string())
                        .filter(|step| !step.is_empty())
                        .collect()
                }
                other => panic!("unknown argument {other}"),
            }
        }
        let session = session.expect("--session <projectId>:<sessionId>");
        let (project, id) = session
            .split_once(':')
            .expect("--session <projectId>:<sessionId>");
        Args {
            session: SessionRef::new(project, id),
            width,
            height,
            light,
            zoom,
            script,
            data_dir,
            allow_send,
        }
    }

    /// `http://127.0.0.1:<port>` and the bearer token of the running gxserver.
    fn local_gxserver() -> (String, String) {
        let state = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap()
            .join(".local/state/ghostex/gxserver");
        let runtime: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(state.join("runtime/server.json"))
                .expect("gxserver runtime file"),
        )
        .expect("gxserver runtime JSON");
        let port = runtime["port"].as_u64().expect("gxserver port");
        let token = std::fs::read_to_string(state.join("auth/token")).expect("gxserver token");
        (format!("http://127.0.0.1:{port}"), token.trim().to_string())
    }

    /// Wraps the transcript and records when the first frame with transcript rows was painted.
    struct PreviewRoot {
        transcript: Entity<ChatTranscript>,
        started: Instant,
        first_rows_painted: Rc<Cell<bool>>,
    }

    impl Render for PreviewRoot {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let rows = self.transcript.read(cx).row_count(cx);
            let painted = self.first_rows_painted.clone();
            let started = self.started;
            div()
                .size_full()
                .relative()
                .child(self.transcript.clone())
                .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, _| {
                        if rows > 0 && !painted.replace(true) {
                            println!(
                                "preview: first frame with {rows} transcript rows painted at {} ms",
                                started.elapsed().as_millis()
                            );
                        }
                    },
                )
                .absolute()
                .size(px(1.0)),
            )
        }
    }

    // The desktop app's AppKit shim (`apps/desktop/native/macos/`) exports these for the chat's
    // child windows. The preview links none of it, so it answers them as a plain macOS window would.
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiPointerPressedRecently() -> bool {
        false
    }
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiApplicationIsActive() -> bool {
        false
    }
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiAttachComposerSuggestionsWindow(
        _view: *mut std::ffi::c_void,
        _parent: *mut std::ffi::c_void,
    ) {
    }
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiPrepareChatDialogWindow(_view: *mut std::ffi::c_void) {}
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiSetChildWindowAlpha(_view: *mut std::ffi::c_void, _alpha: f64) {}
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiSetChildWindowContentFrame(
        _child: *mut std::ffi::c_void,
        _main: *mut std::ffi::c_void,
        _x: f64,
        _y: f64,
        _width: f64,
        _height: f64,
    ) {
    }
    #[unsafe(no_mangle)]
    extern "C" fn GhostexGpuiStripPopupWindowFrame(_view: *mut std::ffi::c_void) {}

    /// The window's CGWindowID, which `screencapture -l` takes.
    fn window_number(window: &Window) -> Option<isize> {
        use objc2::runtime::AnyObject;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        let view = handle.ns_view.as_ptr() as *mut AnyObject;
        unsafe {
            let ns_window: *mut AnyObject = objc2::msg_send![view, window];
            if ns_window.is_null() {
                return None;
            }
            Some(objc2::msg_send![ns_window, windowNumber])
        }
    }

    /// Floats the window above other apps' windows WITHOUT activating the app or making the window
    /// key. Since macOS 14 an inactive app's `orderFront` leaves its window behind the active app,
    /// where AppKit reports it occluded and gpui stops drawing it.
    fn float_without_focus(window: &Window) {
        use objc2::runtime::AnyObject;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        let view = handle.ns_view.as_ptr() as *mut AnyObject;
        unsafe {
            let ns_window: *mut AnyObject = objc2::msg_send![view, window];
            if ns_window.is_null() {
                return;
            }
            // NSFloatingWindowLevel.
            let _: () = objc2::msg_send![ns_window, setLevel: 3isize];
            let _: () = objc2::msg_send![ns_window, orderFrontRegardless];
        }
    }

    fn memory_report() {
        let pid = std::process::id().to_string();
        let rss = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|text| text.trim().parse::<f64>().ok())
            .map_or(-1.0, |kib| kib / 1024.0);
        let footprint = std::process::Command::new("footprint")
            .args(["-p", &pid])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find(|line| line.contains("Footprint:") || line.contains("phys_footprint"))
                    .map(|line| line.trim().to_string())
            })
            .unwrap_or_default();
        println!("preview: memory rss {rss:.1} MiB {footprint}");
    }

    /// One wheel event over the middle of the window.
    fn wheel(window: &mut Window, cx: &mut App, delta: f32, phase: TouchPhase) {
        let center = window.bounds().size;
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: point(center.width / 2.0, center.height / 2.0),
                delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
                modifiers: Modifiers::default(),
                touch_phase: phase,
            }),
            cx,
        );
    }

    fn click(window: &mut Window, cx: &mut App, x: f32, y: f32) {
        let position = point(px(x), px(y));
        window.dispatch_event(
            PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
            cx,
        );
    }

    async fn run_script(
        steps: Vec<String>,
        window: AnyWindowHandle,
        transcript: Entity<ChatTranscript>,
        allow_send: bool,
        cx: &mut gpui::AsyncApp,
    ) {
        for step in steps {
            let (verb, rest) = step.split_once(':').unwrap_or((step.as_str(), ""));
            match verb {
                "wait" => {
                    let ms = rest.parse().unwrap_or(500);
                    cx.background_executor()
                        .timer(Duration::from_millis(ms))
                        .await;
                }
                "shot" => {
                    let path = rest.to_string();
                    let number = window
                        .update(cx, |_, window, _| window_number(window))
                        .ok()
                        .flatten();
                    match number {
                        Some(number) => {
                            let status = std::process::Command::new("screencapture")
                                .args(["-x", "-o", "-l", &number.to_string(), &path])
                                .status();
                            println!("preview: shot {path} ({status:?})");
                        }
                        None => println!("preview: no window number for {path}"),
                    }
                }
                // One event per frame, as a trackpad sends them: gpui's list scrolls from the
                // offset it last laid out, so events inside one frame do not add up.
                "wheel" => {
                    let delta: f32 = rest.parse().unwrap_or(-600.0);
                    let steps = (delta.abs() / 120.0).ceil().max(1.0) as usize;
                    for index in 0..steps {
                        let phase = if index == 0 {
                            TouchPhase::Started
                        } else {
                            TouchPhase::Moved
                        };
                        let _ = window.update(cx, |_, window, cx| {
                            wheel(window, cx, delta / steps as f32, phase)
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                }
                "scroll" => {
                    let delta: f32 = rest.parse().unwrap_or(600.0);
                    let _ = transcript.update(cx, |transcript, cx| transcript.scroll_by(delta, cx));
                }
                "end" => {
                    let _ = transcript.update(cx, |transcript, cx| transcript.scroll_to_end(cx));
                }
                "click" => {
                    let (x, y) = rest.split_once(':').unwrap_or(("0", "0"));
                    let (x, y) = (x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                    let _ = window.update(cx, |_, window, cx| click(window, cx, x, y));
                }
                "session" => {
                    if let Some((project, id)) = rest.split_once(':') {
                        let session = SessionRef::new(project, id);
                        let _ = transcript
                            .update(cx, |transcript, cx| transcript.open_session(session, cx));
                    }
                }
                "rows" => {
                    let rows = transcript.update(cx, |transcript, cx| transcript.row_count(cx));
                    let summary = transcript.update(cx, |transcript, cx| transcript.summary(cx));
                    println!(
                        "preview: {} rows, summary {}",
                        rows,
                        summary
                            .map(|summary| serde_json::to_string(&summary).unwrap_or_default())
                            .unwrap_or_default()
                    );
                }
                "mem" => memory_report(),
                // The host's Send: the text goes through `ChatTranscript::send_text`, the desktop
                // composer's own send path. Refused without `--allow-send`, so a script pointed at a
                // real session can never type into it.
                "send" if !allow_send => println!("preview: send refused without --allow-send"),
                "send" => {
                    let text = rest.to_string();
                    let outcome = window.update(cx, |_, window, cx| {
                        transcript.update(cx, |transcript, cx| {
                            transcript.send_text(
                                &text,
                                ghostex_gpui_mobile_chat::SendMode::Send,
                                window,
                                cx,
                            )
                        })
                    });
                    println!("preview: send {outcome:?}");
                }
                "quit" => {
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }
                other => println!("preview: unknown step {other}"),
            }
        }
    }

    pub(super) fn main() {
        let args = args();
        let started = Instant::now();
        let (base_url, auth_token) = local_gxserver();
        gpui_platform::application()
            .with_assets(ghostex_gpui_mobile_chat::asset_source())
            .run(move |cx: &mut App| {
                let mut settings = serde_json::Map::new();
                if let Some(zoom) = args.zoom {
                    settings.insert("sessionChatZoomPercent".into(), zoom.into());
                }
                ghostex_gpui_mobile_chat::init(
                    cx,
                    ChatInit {
                        data_dir: args.data_dir.clone(),
                        base_url,
                        auth_token,
                        light_appearance: args.light,
                        settings: Some(settings),
                        client_name: "gpui-mobile-preview".into(),
                        ..Default::default()
                    },
                );
                ghostex_gpui_mobile_chat::set_clipboard_handler(cx, |text, _| {
                    println!("preview: copy {} chars", text.chars().count());
                });
                let display = cx.primary_display().map(|display| display.visible_bounds());
                let window_size = size(px(args.width), px(args.height));
                let origin = display.map_or(point(px(0.0), px(0.0)), |display| {
                    point(
                        display.right() - window_size.width - px(12.0),
                        display.top() + px(40.0),
                    )
                });
                let session = args.session.clone();
                let script = args.script.clone();
                let allow_send = args.allow_send;
                let handle = cx
                    .open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds {
                                origin,
                                size: window_size,
                            })),
                            titlebar: None,
                            focus: false,
                            show: true,
                            kind: WindowKind::Normal,
                            is_resizable: true,
                            ..Default::default()
                        },
                        move |window, cx| {
                            let transcript = ghostex_gpui_mobile_chat::open_transcript(
                                window,
                                cx,
                                &session.project_id,
                                &session.session_id,
                            );
                            cx.subscribe(&transcript, |_, event: &ChatTranscriptEvent, _| match event {
                                ChatTranscriptEvent::Summary(summary) => println!(
                                    "preview: summary status={} working={} ready={} rows={} queue={}",
                                    summary.status,
                                    summary.working,
                                    summary.composer_ready,
                                    summary.has_rows,
                                    summary.queue_count
                                ),
                                ChatTranscriptEvent::HostAction { action, .. } => {
                                    println!("preview: host action {action}")
                                }
                                ChatTranscriptEvent::Toast { message, error } => {
                                    println!("preview: toast error={error} {message}")
                                }
                                ChatTranscriptEvent::Snapshot(_) => {}
                            })
                            .detach();
                            let root = cx.new(|_| PreviewRoot {
                                transcript: transcript.clone(),
                                started,
                                first_rows_painted: Rc::default(),
                            });
                            float_without_focus(window);
                            let script_window = window.window_handle();
                            cx.spawn(async move |cx| {
                                run_script(script, script_window, transcript, allow_send, cx).await
                            })
                            .detach();
                            ghostex_gpui_mobile_chat::root_view(root, window, cx)
                        },
                    )
                    .expect("open the preview window");
                println!(
                    "preview: window open at {} ms (pid {})",
                    started.elapsed().as_millis(),
                    std::process::id()
                );
                let _ = handle;
            });
    }
}
