use interprocess::{
    ConnectWaitMode, TryClone,
    os::windows::named_pipe::{DuplexPipeStream, pipe_mode::Bytes},
};
use std::{
    env,
    io::{self, Read, Write},
    os::windows::io::{AsHandle, AsRawHandle},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub fn default_pipe_path() -> String {
    let user = env::var("USERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "user".into());
    let user: String = user
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    // A portable copy runs its own editor daemon beside an installed Ghostex's (ghostex_paths::portable).
    match ghostex_paths::portable::current() {
        Some(copy) => format!(r"\\.\pipe\ghostex-editor-{user}-{}", copy.instance_key()),
        None => format!(r"\\.\pipe\ghostex-editor-{user}"),
    }
}

/// Both Ghostex.exe and resources/native/ghostex.exe resolve the same packaged helper.
pub fn bundled_executable(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()?
        .ancestors()
        .map(|dir| dir.join("resources/GhostexEditor/GhostexEditor.exe"))
        .find(|candidate| candidate.is_file())
}

/// CDXC:PromptEditor 2026-09-23 WHY:
/// Session CLIs run from the separately staged gxserver package, outside the desktop bundle, so ancestor lookup cannot find the editor shipped by the Windows installer.
/// The per-user install (release installer and local start, `%LOCALAPPDATA%\Ghostex\current`) comes before a Program Files one.
pub fn installed_executable() -> Option<PathBuf> {
    // A portable copy's CLI uses the copy's own editor, never an installed Ghostex's.
    if let Some(copy) = ghostex_paths::portable::current() {
        return Some(
            copy.root
                .join("current/resources/GhostexEditor/GhostexEditor.exe"),
        )
        .filter(|candidate| candidate.is_file());
    }
    let per_user = env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(|dir| PathBuf::from(dir).join("Ghostex/current"));
    let machine = env::var_os("ProgramW6432")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("ProgramFiles").filter(|value| !value.is_empty()))
        .map(|dir| PathBuf::from(dir).join("Ghostex"));
    [per_user, machine]
        .into_iter()
        .flatten()
        .map(|dir| dir.join("resources/GhostexEditor/GhostexEditor.exe"))
        .find(|candidate| candidate.is_file())
}

/// CDXC:PromptEditor 2026-09-16 WHY:
/// Windows named pipes reject socket receive/send timeouts. Poll nonblocking I/O with deadlines so the CLI and desktop cannot hang on a stalled editor daemon.
/// SEE-ALSO: apps/editor/desktop/src/daemon.rs and the desktop/CLI editor_daemon clients share this pipe name and newline-delimited JSON protocol.
pub struct PipeStream {
    inner: DuplexPipeStream<Bytes>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl PipeStream {
    pub fn connect(path: &str, timeout: Duration) -> io::Result<Self> {
        let inner = DuplexPipeStream::connect_by_path_with_wait_mode(
            path,
            ConnectWaitMode::Timeout(timeout),
        )?;
        inner.set_nonblocking(true)?;
        Ok(Self {
            inner,
            read_timeout: Some(timeout),
            write_timeout: Some(timeout),
        })
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: self.inner.try_clone()?,
            read_timeout: self.read_timeout,
            write_timeout: self.write_timeout,
        })
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.read_timeout = timeout;
        Ok(())
    }

    fn poll<T>(
        timeout: Option<Duration>,
        mut operation: impl FnMut() -> io::Result<T>,
    ) -> io::Result<T> {
        let started = Instant::now();
        loop {
            match operation() {
                // ERROR_NO_DATA is the nonblocking byte-pipe "nothing available" result.
                Err(error)
                    if error.raw_os_error() == Some(232)
                        || matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                result => return result,
            }
            if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Ghostex editor pipe timed out",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Read for PipeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        Self::poll(self.read_timeout, || {
            let mut available = 0;
            // A nonblocking ReadFile with no bytes is surfaced as EOF by the
            // pipe library. Peek first to distinguish an idle pipe from closure.
            let ok = unsafe {
                windows_sys::Win32::System::Pipes::PeekNamedPipe(
                    self.inner.as_handle().as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            if available == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.inner.read(buffer)
        })
    }
}

impl Write for PipeStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        Self::poll(self.write_timeout, || match self.inner.write(buffer) {
            Ok(0) if !buffer.is_empty() => Err(io::ErrorKind::WouldBlock.into()),
            result => result,
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        // Writes already reach the pipe. Protocol replies acknowledge consumption;
        // FlushFileBuffers would wait indefinitely for the remote reader.
        Ok(())
    }
}

impl Drop for PipeStream {
    fn drop(&mut self) {
        // Each request awaits its JSON reply before closing. Failed requests must
        // release their handles too, rather than creating a background flush waiter.
        self.inner.assume_flushed();
    }
}
