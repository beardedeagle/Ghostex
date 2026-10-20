#[cfg(unix)]
use std::fs;
use std::{
    collections::HashMap,
    env, io,
    io::{BufRead as _, BufReader, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use interprocess::{
    TryClone as _,
    local_socket::{GenericFilePath, Listener, ListenerOptions, Name, Stream, prelude::*},
};
use serde_json::Value;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
#[cfg(target_os = "windows")]
use wry::WebContext;

use crate::*;

pub(crate) fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let socket_arg = parse_daemon_args(&args)?;
    let endpoint = resolve_socket_endpoint(socket_arg.as_deref())?;

    if ping_existing_daemon(&endpoint) {
        return Ok(());
    }
    remove_stale_socket(&endpoint.cleanup_path);

    let listener = ListenerOptions::new()
        .name(endpoint.name.clone())
        .create_sync()
        .map_err(|error| {
            format!(
                "unable to bind editor daemon socket {}: {error}",
                endpoint.display_path
            )
        })?;

    let web_root = Arc::new(resolve_web_root()?);
    if !web_root.join("index.html").is_file() {
        return Err(format!(
            "missing editor web entry at {}",
            web_root.join("index.html").display()
        ));
    }

    let mut event_loop = EventLoopBuilder::<DaemonEvent>::with_user_event().build();
    set_process_window_policy(&mut event_loop);
    let proxy = event_loop.create_proxy();
    install_signal_handler(proxy.clone());
    spawn_accept_thread(listener, proxy.clone());

    let mut app = EditorApp {
        socket_cleanup_path: endpoint.cleanup_path,
        web_root,
        #[cfg(target_os = "windows")]
        web_context: WebContext::new(Some(windows_webview_data_directory())),
        proxy,
        windows: HashMap::new(),
        sessions: HashMap::new(),
        warm_window: None,
        warm_waiters: Vec::new(),
        open_count_watchers: Vec::new(),
        pending_shutdown: false,
        should_exit: false,
        cascade_offset: 0,
        last_cursor_snapshot: None,
    };

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(_) => app.ensure_warm_window(target),
            Event::UserEvent(event) => app.handle_daemon_event(event, target),
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => app.handle_window_close(window_id),
            Event::LoopDestroyed => app.cleanup_socket(),
            _ => {}
        }
        if app.should_exit {
            app.cleanup_socket();
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn parse_daemon_args(args: &[String]) -> Result<Option<String>, String> {
    if args.len() == 2 && args[1] == "--daemon" {
        return Ok(None);
    }
    if args.len() == 4 && args[1] == "--daemon" && args[2] == "--socket" {
        return Ok(Some(args[3].clone()));
    }
    Err("usage: ghostex-editor --daemon [--socket <path>]".to_string())
}

fn resolve_socket_endpoint(socket_arg: Option<&str>) -> Result<SocketEndpoint, String> {
    let display_path = env::var("GHOSTEX_EDITOR_SOCKET")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| socket_arg.map(str::to_string))
        .unwrap_or_else(default_socket_path);

    #[cfg(unix)]
    {
        let path = PathBuf::from(&display_path);
        if !path.is_absolute() {
            return Err(format!("socket path must be absolute: {display_path}"));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "unable to create socket directory {}: {error}",
                    parent.display()
                )
            })?;
            set_directory_private(parent);
        }
    }

    let name = socket_name(&display_path)
        .map_err(|error| format!("invalid editor daemon socket path {display_path}: {error}"))?;
    let cleanup_path = unix_cleanup_path(&display_path);
    Ok(SocketEndpoint {
        display_path,
        name,
        cleanup_path,
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn default_socket_path() -> String {
    if let Some(ghostex_home) = absolute_environment_path("GHOSTEX_HOME") {
        return ghostex_home
            .join("runtime")
            .join(SOCKET_FILE_NAME)
            .to_string_lossy()
            .into_owned();
    }
    if let Some(runtime_dir) = absolute_environment_path("XDG_RUNTIME_DIR") {
        return runtime_dir
            .join("ghostex")
            .join(SOCKET_FILE_NAME)
            .to_string_lossy()
            .into_owned();
    }
    resolved_state_directory()
        .join("runtime")
        .join(SOCKET_FILE_NAME)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "windows")]
fn default_socket_path() -> String {
    ghostex_editor_client::default_pipe_path()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn default_socket_path() -> String {
    "/tmp/ghostex-editor.sock".to_string()
}

fn socket_name(path: &str) -> io::Result<Name<'static>> {
    path.to_fs_name::<GenericFilePath>().map(Name::into_owned)
}

#[cfg(unix)]
fn unix_cleanup_path(path: &str) -> Option<PathBuf> {
    Some(PathBuf::from(path))
}

#[cfg(not(unix))]
fn unix_cleanup_path(_path: &str) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn set_directory_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_directory_private(_path: &Path) {}

#[cfg(windows)]
fn ping_existing_daemon(endpoint: &SocketEndpoint) -> bool {
    let Ok(mut stream) = ghostex_editor_client::PipeStream::connect(
        &endpoint.display_path,
        Duration::from_millis(750),
    ) else {
        return false;
    };
    if stream.write_all(b"{\"v\":1,\"type\":\"ping\"}\n").is_err() {
        return false;
    }
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).is_ok()
        && serde_json::from_str::<Value>(&line)
            .ok()
            .is_some_and(|reply| reply["type"] == "pong")
}

#[cfg(unix)]
fn ping_existing_daemon(endpoint: &SocketEndpoint) -> bool {
    let Ok(mut stream) = Stream::connect(endpoint.name.clone()) else {
        return false;
    };
    let _ = stream.set_recv_timeout(Some(Duration::from_millis(750)));
    let _ = stream.set_send_timeout(Some(Duration::from_millis(750)));
    if stream.write_all(b"{\"v\":1,\"type\":\"ping\"}\n").is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .as_deref()
        == Some("pong")
}

#[cfg(unix)]
pub(crate) fn remove_stale_socket(path: &Option<PathBuf>) {
    use std::os::unix::fs::FileTypeExt as _;
    let Some(path) = path else {
        return;
    };
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_socket()
    {
        let _ = fs::remove_file(path);
    }
}

#[cfg(not(unix))]
pub(crate) fn remove_stale_socket(_path: &Option<PathBuf>) {}

fn resolve_web_root() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("GHOSTEX_EDITOR_WEB_ROOT")
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }
    let executable = env::current_exe()
        .map_err(|error| format!("unable to resolve current executable: {error}"))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "unable to resolve executable directory".to_string())?;
    Ok(executable_dir.join("web"))
}

fn spawn_accept_thread(listener: Listener, proxy: EventLoopProxy<DaemonEvent>) {
    thread::spawn(move || {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => spawn_connection_thread(stream, proxy.clone()),
                Err(error) => eprintln!("ghostex-editor: accept failed: {error}"),
            }
        }
    });
}

fn spawn_connection_thread(stream: Stream, proxy: EventLoopProxy<DaemonEvent>) {
    thread::spawn(move || {
        let writer_stream = match stream.try_clone() {
            Ok(writer) => writer,
            Err(error) => {
                eprintln!("ghostex-editor: unable to clone client stream: {error}");
                return;
            }
        };
        let connection = ClientConnection {
            writer: Arc::new(Mutex::new(writer_stream)),
        };
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed.is_empty() {
                        connection.send_error("malformed JSON");
                        continue;
                    }
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) if value.is_object() => {
                            let _ = proxy.send_event(DaemonEvent::Request {
                                request: value,
                                connection: connection.clone(),
                            });
                        }
                        Ok(_) => connection.send_error("request must be a JSON object"),
                        Err(_) => connection.send_error("malformed JSON"),
                    }
                }
                Err(_) => return,
            }
        }
    });
}

fn install_signal_handler(proxy: EventLoopProxy<DaemonEvent>) {
    let _ = ctrlc::set_handler(move || {
        let _ = proxy.send_event(DaemonEvent::SaveAllAndExit);
    });
}

#[cfg(target_os = "macos")]
fn set_process_window_policy(event_loop: &mut tao::event_loop::EventLoop<DaemonEvent>) {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn set_process_window_policy(_event_loop: &mut tao::event_loop::EventLoop<DaemonEvent>) {}
