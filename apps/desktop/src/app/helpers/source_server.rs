mod code_server;
pub(crate) use code_server::*;

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::*;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn gpui_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
