//! A workspace's Executor: whether it is on, its fixed port and the two keys Ghostex made for it,
//! sealed in the connectors store; and the folder its data lives in.

use std::net::TcpListener;
use std::path::PathBuf;

use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

use super::super::store;

const PROVIDER: &str = "executor";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorkspaceExecutor {
    pub(super) enabled: bool,
    /// Picked once: Executor's OAuth sign-ins come back to this port, so it never changes.
    pub(super) port: u16,
    /// `EXECUTOR_API_KEY`: the bearer key for its API.
    pub(super) api_key: String,
    /// `EXECUTOR_ENCRYPTION_KEY`: encrypts the app sign-ins in its database.
    pub(super) encryption_key: String,
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("Couldn't find a free port for Executor: {error}"))
}

/// The workspace's saved Executor, or none when it was never turned on.
pub(super) fn load(
    paths: &GxserverPaths,
    workspace: &str,
) -> Result<Option<WorkspaceExecutor>, String> {
    let Some(plain) = store::read_secret(paths, PROVIDER, workspace)? else {
        return Ok(None);
    };
    serde_json::from_str(&plain)
        .map(Some)
        .map_err(|error| format!("The saved Executor settings are unreadable: {error}"))
}

pub(super) fn save(
    paths: &GxserverPaths,
    workspace: &str,
    settings: &WorkspaceExecutor,
) -> Result<(), String> {
    let plain = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    store::write_secret(paths, PROVIDER, workspace, &plain)
}

/// The saved settings, or new ones (a free port and fresh keys) saved first.
pub(super) fn load_or_create(
    paths: &GxserverPaths,
    workspace: &str,
) -> Result<WorkspaceExecutor, String> {
    if let Some(settings) = load(paths, workspace)? {
        return Ok(settings);
    }
    let settings = WorkspaceExecutor {
        enabled: false,
        port: free_port()?,
        api_key: random_hex(24),
        encryption_key: random_hex(32),
    };
    save(paths, workspace, &settings)?;
    Ok(settings)
}

/// `<gxserver state>/connectors/executor/<workspace>`: everything Executor stores for the
/// workspace (its database, accounts and app builds).
pub(super) fn data_dir(paths: &GxserverPaths, workspace: &str) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join("executor")
        .join(store::file_stem(workspace))
}
