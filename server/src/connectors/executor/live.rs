//! What this gxserver knows right now about each workspace's Executor: its running process, the
//! app list last read from it, and whether a start is under way or failed.

use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::paths::GxserverPaths;

/// One workspace's Executor in this gxserver.
#[derive(Default)]
pub(super) struct Live {
    /// The workspace id, set on first use (the map is keyed by data folder).
    pub(super) workspace: String,
    pub(super) child: Option<Child>,
    /// When something last needed this Executor; it stops after a while unused.
    pub(super) last_used: Option<Instant>,
    /// The apps last read from it, as Settings shows them.
    pub(super) apps: Vec<Value>,
    /// `apps` was read from the running Executor at least once.
    pub(super) apps_read: bool,
    /// A background start (after a read or an install) is under way.
    pub(super) starting: bool,
    /// Why the last start failed, and when, so a read doesn't retry a failing start every time.
    pub(super) start_error: Option<(String, Instant)>,
}

/// Keyed by the workspace's data folder, which includes the gxserver root.
static LIVE: Mutex<Option<HashMap<String, Live>>> = Mutex::new(None);

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn key(paths: &GxserverPaths, workspace: &str) -> String {
    super::settings::data_dir(paths, workspace)
        .to_string_lossy()
        .into_owned()
}

/// Runs `f` on the live state of `workspace`'s Executor.
pub(super) fn with_live<R>(
    paths: &GxserverPaths,
    workspace: &str,
    f: impl FnOnce(&mut Live) -> R,
) -> R {
    let mut live = lock(&LIVE);
    let map = live.get_or_insert_with(HashMap::new);
    f(map.entry(key(paths, workspace)).or_default())
}

/// The running Executors nothing used for `after`, taken out of the live state, with their data
/// folder and workspace id.
pub(super) fn take_idle(after: Duration) -> Vec<(String, String, Child)> {
    let mut live = lock(&LIVE);
    let Some(map) = live.as_mut() else {
        return Vec::new();
    };
    map.iter_mut()
        .filter(|(_, live)| {
            live.child.is_some() && live.last_used.is_none_or(|at| at.elapsed() >= after)
        })
        .filter_map(|(key, live)| {
            live.child
                .take()
                .map(|child| (key.clone(), live.workspace.clone(), child))
        })
        .collect()
}

/// Every running Executor process, taken out of the live state (for shutdown).
pub(super) fn take_all_children() -> Vec<Child> {
    let mut live = lock(&LIVE);
    live.as_mut()
        .map(|map| {
            map.values_mut()
                .filter_map(|live| live.child.take())
                .collect()
        })
        .unwrap_or_default()
}

type ChangeListener = Arc<dyn Fn(&str) + Send + Sync>;
static CHANGE_LISTENER: Mutex<Option<ChangeListener>> = Mutex::new(None);

/// Called with the workspace id after its Executor starts, stops, installs or its apps change.
pub(crate) fn set_changed_listener(listener: Arc<dyn Fn(&str) + Send + Sync>) {
    *lock(&CHANGE_LISTENER) = Some(listener);
}

pub(super) fn notify_changed(workspace: &str) {
    let listener = lock(&CHANGE_LISTENER).clone();
    if let Some(listener) = listener {
        listener(workspace);
    }
}
