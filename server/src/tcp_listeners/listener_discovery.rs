//! Every listening TCP socket on this machine, for `ghostex ports` and the project views.
use anyhow::Result;

#[cfg(not(windows))]
use super::posix_listeners::read_posix_tcp_listeners;

pub(crate) fn read_all_tcp_listeners() -> Result<Vec<TcpListenerDetail>> {
    #[cfg(windows)]
    return super::windows_listeners::read_windows_tcp_listeners();
    #[cfg(not(windows))]
    read_posix_tcp_listeners()
}

/// One listening TCP socket exactly as the platform tool reported it. `pid` and
/// `command` are absent when the socket belongs to a process this user cannot
/// inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TcpListenerDetail {
    pub(crate) address: String,
    pub(crate) command: Option<String>,
    pub(crate) pid: Option<u32>,
    pub(crate) port: u16,
}
