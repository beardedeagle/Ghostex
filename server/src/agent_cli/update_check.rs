use super::{catalog::Definition, process};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

/// What the CLI's own update check answered.
#[derive(Clone, Default)]
pub(crate) struct Outcome {
    /// `None` when the answer named neither an update nor "up to date".
    pub available: Option<bool>,
    /// The CLI's own words, such as "Update available: 14220 commits behind origin/main."
    pub summary: Option<String>,
}

pub(crate) enum Status {
    Ready(Outcome),
    Checking,
}

#[derive(Clone)]
enum Entry {
    Done(Instant, Outcome),
    Running,
}

const FOUND_TTL: Duration = Duration::from_secs(60 * 60);
const FAILED_TTL: Duration = Duration::from_secs(5 * 60);
/// Hermes's check fetches its git remote, which took 30 s on the Mac.
const CHECK_TIMEOUT: Duration = Duration::from_secs(120);

static CACHE: LazyLock<Mutex<HashMap<(String, String, String), Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Hermes updates by git commit, so its version line said v0.21.4 while `hermes update --check` reported 14k commits behind, and Ghostex had no release channel to compare with. An agent with `updateCheck` in the catalog asks the CLI itself. The check can take half a minute, so it runs in the background, one at a time per installed version, and is cached for an hour; until it answers the state says `updateChecking` and makes no claim either way.
pub(crate) fn status(
    definition: &'static Definition,
    executable: &str,
    version: &str,
    home: &Path,
) -> Option<Status> {
    let args = definition.update_check.clone()?;
    let key = (
        definition.agent_id.clone(),
        executable.to_string(),
        version.to_string(),
    );
    let mut cache = CACHE.lock().ok()?;
    match cache.get(&key) {
        Some(Entry::Running) => return Some(Status::Checking),
        Some(Entry::Done(at, outcome)) => {
            let ttl = if outcome.available.is_some() {
                FOUND_TTL
            } else {
                FAILED_TTL
            };
            if at.elapsed() < ttl {
                return Some(Status::Ready(outcome.clone()));
            }
        }
        None => {}
    }
    cache.insert(key.clone(), Entry::Running);
    drop(cache);
    let executable = executable.to_string();
    let home: PathBuf = home.to_path_buf();
    tokio::spawn(async move {
        let output = Arc::new(Mutex::new(String::new()));
        let capture = output.clone();
        let result =
            process::run_executable(&executable, &args, &home, CHECK_TIMEOUT, move |chunk| {
                if let Ok(mut output) = capture.lock() {
                    if output.len() < 16 * 1024 {
                        output.push_str(&chunk);
                    }
                }
            })
            .await;
        let outcome = match result {
            Ok(()) => output.lock().map(|text| parse(&text)).unwrap_or_default(),
            Err(_) => Outcome::default(),
        };
        if let Ok(mut cache) = CACHE.lock() {
            cache.insert(key, Entry::Done(Instant::now(), outcome));
        }
    });
    Some(Status::Checking)
}

fn parse(text: &str) -> Outcome {
    for line in text.lines() {
        let clean = line
            .trim()
            .trim_start_matches(|ch: char| !ch.is_alphanumeric())
            .trim();
        let lower = clean.to_lowercase();
        if lower.contains("update available") || lower.contains("new version available") {
            return Outcome {
                available: Some(true),
                summary: Some(clean.chars().take(160).collect()),
            };
        }
    }
    let lower = text.to_lowercase();
    let current = [
        "up to date",
        "up-to-date",
        "already on the latest",
        "no update",
        "no newer release",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    Outcome {
        available: current.then_some(false),
        summary: None,
    }
}
