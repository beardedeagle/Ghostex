//! Runs a command line tool as its own macOS privacy identity instead of Ghostex's, so the
//! Accessibility and Screen Recording grants it checks and requests are its own.

use std::{path::Path, time::Duration};

use crate::app::helpers::*;

/// CDXC:OsIntegration 2026-10-14 WHY:
/// macOS charges a child process's privacy checks and requests to its responsible process, so `spaceo` started by Ghostex asks for Ghostex's grants and its prompt registers Ghostex, never SpaceO (why SpaceO was missing from System Settings > Screen Recording). Spawning it with responsibility disclaimed makes it its own responsible process, the identity its launchd-run daemon has, the way Chromium and Xcode start helpers that hold their own grants.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_disclaimed_command_with_captured_output_timeout(
    command: &Path,
    args: &[&str],
    timeout: Duration,
    max_capture_bytes: usize,
) -> Result<GpuiCapturedCommandOutput, String> {
    use std::{
        ffi::CString,
        fs::File,
        os::{
            fd::FromRawFd,
            unix::ffi::{OsStrExt, OsStringExt},
        },
        time::Instant,
    };

    unsafe extern "C" {
        /// Private but stable since macOS 10.14 (libsystem `spawn_private.h`).
        fn responsibility_spawnattrs_setdisclaim(
            attrs: *mut libc::posix_spawnattr_t,
            disclaim: libc::c_int,
        ) -> libc::c_int;
    }

    let to_cstring =
        |bytes: Vec<u8>| CString::new(bytes).map_err(|_| "invalid argument".to_string());
    let program = to_cstring(command.as_os_str().as_bytes().to_vec())?;
    let mut argv_owned = vec![program.clone()];
    for arg in args {
        argv_owned.push(to_cstring(arg.as_bytes().to_vec())?);
    }
    let mut envp_owned = Vec::new();
    for (key, value) in std::env::vars_os() {
        let mut entry = key.into_vec();
        entry.push(b'=');
        entry.extend(value.into_vec());
        if let Ok(entry) = CString::new(entry) {
            envp_owned.push(entry);
        }
    }
    let mut argv: Vec<*mut libc::c_char> = argv_owned
        .iter()
        .map(|arg| arg.as_ptr().cast_mut())
        .collect();
    argv.push(std::ptr::null_mut());
    let mut envp: Vec<*mut libc::c_char> = envp_owned
        .iter()
        .map(|entry| entry.as_ptr().cast_mut())
        .collect();
    envp.push(std::ptr::null_mut());

    let mut pipe = [0 as libc::c_int; 2];
    // SAFETY: `pipe` has room for the two descriptors pipe(2) writes.
    if unsafe { libc::pipe(pipe.as_mut_ptr()) } != 0 {
        return Err("pipe failed".to_string());
    }
    let [read_fd, write_fd] = pipe;
    // Other threads' spawns must not inherit the write end, or the reader never sees EOF.
    // SAFETY: both are open descriptors this function owns.
    unsafe {
        libc::fcntl(read_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(write_fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    let dev_null = c"/dev/null";
    let mut pid: libc::pid_t = 0;
    // SAFETY: the attribute and file-action objects are initialised before use and destroyed
    // after the spawn; argv and envp are null-terminated arrays of live C strings; the pipe ends
    // are valid descriptors this function owns.
    let spawned = unsafe {
        let mut attr: libc::posix_spawnattr_t = std::mem::zeroed();
        let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        libc::posix_spawnattr_init(&mut attr);
        libc::posix_spawn_file_actions_init(&mut actions);
        // Close every descriptor the child would otherwise inherit from Ghostex.
        libc::posix_spawnattr_setflags(&mut attr, libc::POSIX_SPAWN_CLOEXEC_DEFAULT as _);
        responsibility_spawnattrs_setdisclaim(&mut attr, 1);
        libc::posix_spawn_file_actions_addopen(
            &mut actions,
            0,
            dev_null.as_ptr(),
            libc::O_RDONLY,
            0,
        );
        libc::posix_spawn_file_actions_adddup2(&mut actions, write_fd, 1);
        libc::posix_spawn_file_actions_addopen(
            &mut actions,
            2,
            dev_null.as_ptr(),
            libc::O_WRONLY,
            0,
        );
        let status = libc::posix_spawn(
            &mut pid,
            program.as_ptr(),
            &actions,
            &attr,
            argv.as_ptr(),
            envp.as_ptr(),
        );
        libc::posix_spawn_file_actions_destroy(&mut actions);
        libc::posix_spawnattr_destroy(&mut attr);
        libc::close(write_fd);
        status
    };
    if spawned != 0 {
        // SAFETY: the read end is still open and owned here.
        unsafe { libc::close(read_fd) };
        return Err("process spawn failed".to_string());
    }
    // SAFETY: `read_fd` is an open pipe end that nothing else owns.
    let reader =
        gpui_capture_child_output_stream(unsafe { File::from_raw_fd(read_fd) }, max_capture_bytes);
    let started = Instant::now();
    let mut success = false;
    loop {
        let mut status: libc::c_int = 0;
        // SAFETY: `pid` is this function's child and `status` is writable.
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if waited == pid {
            success = libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
            break;
        }
        if waited < 0 {
            break;
        }
        if started.elapsed() >= timeout {
            // SAFETY: as above; the child is killed and then reaped.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(GpuiCapturedCommandOutput {
        stderr: String::new(),
        stdout: gpui_join_captured_output(Some(reader)),
        success,
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_run_disclaimed_command_with_captured_output_timeout(
    _command: &Path,
    _args: &[&str],
    _timeout: Duration,
    _max_capture_bytes: usize,
) -> Result<GpuiCapturedCommandOutput, String> {
    Err("Only macOS has per-app privacy grants.".to_string())
}
