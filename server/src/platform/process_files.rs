use std::{collections::HashMap, path::PathBuf, ptr, time::Duration};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_NO_MORE_ITEMS, HANDLE,
    },
    Storage::FileSystem::{
        GetFileType, GetFinalPathNameByHandleW, FILE_NAME_OPENED, FILE_TYPE_DISK,
    },
    System::{
        Diagnostics::ProcessSnapshotting::*,
        Threading::{
            GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION,
            PROCESS_VM_READ,
        },
    },
};

/// The hidden verb that runs `open_paths` in a child gxserver.
pub(crate) const OPEN_PATHS_COMMAND: &str = "process-open-files";
/// A walk normally takes milliseconds; a child that has not answered by then is ended.
const OPEN_PATHS_TIMEOUT: Duration = Duration::from_secs(10);

/// The open file paths of each process, read by a child gxserver (`process-open-files <pid>…`).
///
/// CDXC:SessionIdentity 2026-10-24 WHY:
/// The handle walk below duplicates every handle of another process into the caller and asks Windows about it, the only native work gxserver did in-process for one agent, and it ran right before the native Windows daemon died of an access violation on every Codex session start and restore on a reporter's computer (GitHub #289; not reproduced on Windows 11 26300). It now runs in a short-lived child, one per probe for all the pids, so a fault there ends only the child: the daemon logs its exit status and goes on without those paths, the way a process it may not open is skipped. `lsof` ran out of process on macOS for the same lookup until 2026-10-23.
pub(crate) fn open_paths_isolated(process_ids: &[i64]) -> HashMap<i64, Vec<PathBuf>> {
    let pids: Vec<i64> = process_ids.iter().copied().filter(|pid| *pid > 0).collect();
    if pids.is_empty() {
        return HashMap::new();
    }
    match run_open_paths_child(&pids) {
        Ok(paths) => paths,
        Err(error) => {
            log_open_paths_failure(&pids, &error);
            HashMap::new()
        }
    }
}

/// The gxserver executable beside this one: the CLI (`ghostex.exe`) is a link to it, and only the gxserver name dispatches the hidden verb.
fn gxserver_executable() -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let sibling = current.with_file_name("gxserver.exe");
    Ok(if sibling.is_file() { sibling } else { current })
}

fn run_open_paths_child(pids: &[i64]) -> Result<HashMap<i64, Vec<PathBuf>>, String> {
    use std::{io::Read, process::Stdio};
    let mut child = super::process::background_command(
        gxserver_executable().map_err(|error| format!("gxserver path unreadable: {error}"))?,
    )
    .arg(OPEN_PATHS_COMMAND)
    .args(pids.iter().map(i64::to_string))
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|error| format!("could not start: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    let text = match receiver.recv_timeout(OPEN_PATHS_TIMEOUT) {
        Ok(text) => text,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "did not answer within {} s",
                OPEN_PATHS_TIMEOUT.as_secs()
            ));
        }
    };
    let status = child
        .wait()
        .map_err(|error| format!("could not be waited on: {error}"))?;
    if !status.success() {
        return Err(match status.code() {
            Some(code) => format!("exited with status {:#010X}", code as u32),
            None => "exited without a status".to_string(),
        });
    }
    let parsed: HashMap<String, Vec<PathBuf>> = serde_json::from_str(text.trim())
        .map_err(|error| format!("answered unreadable output: {error}"))?;
    Ok(parsed
        .into_iter()
        .filter_map(|(pid, paths)| Some((pid.parse().ok()?, paths)))
        .collect())
}

fn log_open_paths_failure(pids: &[i64], error: &str) {
    use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
    static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();
    let logger = LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "processOpenFilesProbeFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(format!(
            "Reading the open files of an agent process failed: the helper {error}."
        )),
        details: Some(serde_json::json!({ "processIds": pids })),
    });
}

/// `gxserver process-open-files <pid>…`: prints `{"<pid>": [paths…]}` for the daemon (`open_paths_isolated`).
pub(crate) fn print_open_paths(arguments: &[String]) -> anyhow::Result<()> {
    let answer: HashMap<String, Vec<PathBuf>> = arguments
        .iter()
        .filter_map(|pid| pid.parse::<i64>().ok())
        .map(|pid| (pid.to_string(), open_paths(pid)))
        .collect();
    println!("{}", serde_json::to_string(&answer)?);
    Ok(())
}

/// CDXC:SessionIdentity 2026-09-14 WHY:
/// A newly started native Codex has no conversation ID in argv. Windows previously returned no open files, leaving chat at "starting" after the terminal had replied.
/// Read the exact process-owned rollout through Windows process snapshotting, matching macOS lsof and Linux procfs without guessing from a shared project directory.
fn open_paths(process_id: i64) -> Vec<PathBuf> {
    let Some(pid) = u32::try_from(process_id).ok().filter(|pid| *pid != 0) else {
        return Vec::new();
    };
    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_DUP_HANDLE,
            0,
            pid,
        );
        if process.is_null() {
            return Vec::new();
        }
        let mut snapshot = ptr::null_mut();
        let status = PssCaptureSnapshot(process, PSS_CAPTURE_HANDLES, 0, &mut snapshot);
        if status != 0 {
            CloseHandle(process);
            return Vec::new();
        }
        let mut marker = ptr::null_mut();
        if PssWalkMarkerCreate(ptr::null(), &mut marker) != 0 {
            PssFreeSnapshot(GetCurrentProcess(), snapshot);
            CloseHandle(process);
            return Vec::new();
        }
        let mut paths = Vec::new();
        let mut complete = false;
        // Each walk advances within this immutable snapshot. Bound the work for unusually large processes.
        for _ in 0..65_536 {
            let mut entry: PSS_HANDLE_ENTRY = std::mem::zeroed();
            let status = PssWalkSnapshot(
                snapshot,
                PSS_WALK_HANDLES,
                marker,
                (&mut entry as *mut PSS_HANDLE_ENTRY).cast(),
                std::mem::size_of::<PSS_HANDLE_ENTRY>() as u32,
            );
            if status != 0 {
                complete = status == ERROR_NO_MORE_ITEMS;
                break;
            }
            let mut duplicate = ptr::null_mut();
            if DuplicateHandle(
                process,
                entry.Handle,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                continue;
            }
            // Snapshot name information omits file objects. Query only duplicated disk handles;
            // named pipes must never enter a potentially blocking path-name query.
            if GetFileType(duplicate) == FILE_TYPE_DISK {
                if let Some(path) = file_path(duplicate) {
                    paths.push(path);
                }
            }
            CloseHandle(duplicate);
        }
        PssWalkMarkerFree(marker);
        PssFreeSnapshot(GetCurrentProcess(), snapshot);
        CloseHandle(process);
        if complete {
            paths
        } else {
            Vec::new()
        }
    }
}

fn file_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buffer = vec![0_u16; 1024];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_OPENED,
            )
        } as usize;
        if length == 0 || length > 32_768 {
            return None;
        }
        if length >= buffer.len() {
            buffer.resize(length + 1, 0);
            continue;
        }
        let path = String::from_utf16_lossy(&buffer[..length]);
        return Some(if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path))
        });
    }
}
