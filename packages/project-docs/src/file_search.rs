//! Quick Access's Files tab: a project's files as its git sees them, and the fuzzy match that ranks
//! them for what the user types. gxserver answers `/api/searchProjectFiles` with it, so the desktop,
//! the GPUI web build and a remote machine's projects all list and rank the same files.
//!
//! CDXC:AppModal 2026-10-17 DECISION:
//! User: the Files tab respects .gitignore, skips build and dependency folders, and keeps typing instant on big repos; the file listing and matching live in gxserver (or a Rust package it uses) so the GPUI web build and remote machines' projects work too, and the window only draws it.
//!
//! SEE-ALSO: server/src/project_file_search.rs (the endpoint), packages/gx-core/src/quick_access/files.rs (the tab).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use crate::project::{is_skipped_directory, is_skipped_file};

/// Runs `git <args>` in a folder and returns its stdout. gxserver passes its time-boxed probe so a
/// slow repository never blocks a search thread for long.
pub type GitRunner = fn(cwd: &str, args: &[&str]) -> Option<String>;

/// More files than this in one project are not listed.
const MAX_FILES: usize = 300_000;
/// Directory depth for the walk of a folder that is not a git repository.
const MAX_WALK_DEPTH: usize = 16;
/// A listing younger than this answers searches as it is. An older one still answers at once while
/// a fresh listing is made in the background, so a keystroke never waits for git.
const FRESH_FOR: Duration = Duration::from_secs(10);
const MAX_LISTINGS: usize = 32;

pub struct FileMatch {
    /// Relative to the project root, `/`-separated.
    pub path: String,
}

pub struct FileSearchOutcome {
    pub matches: Vec<FileMatch>,
    /// How many files matched, before `max_matches` cut the list.
    pub total: usize,
    /// The project has more than [`MAX_FILES`] files; the rest were not searched.
    pub incomplete: bool,
}

struct Listing {
    built: Instant,
    files: Vec<Box<str>>,
    incomplete: bool,
    refreshing: AtomicBool,
}

static LISTINGS: OnceLock<Mutex<HashMap<PathBuf, Arc<Listing>>>> = OnceLock::new();

fn listings() -> &'static Mutex<HashMap<PathBuf, Arc<Listing>>> {
    LISTINGS.get_or_init(Default::default)
}

/// CDXC:AppModal 2026-10-17 WHY:
/// Listing a big repository takes git tens to hundreds of milliseconds, far too slow to repeat on every keystroke. One listing per project is kept and every search filters it in memory; a listing older than [`FRESH_FOR`] still answers while a background thread replaces it, so new and deleted files show up a moment later without a keystroke ever waiting.
fn listing(root: &Path, git: GitRunner) -> Arc<Listing> {
    if let Some(cached) = cached_listing(root) {
        if cached.built.elapsed() >= FRESH_FOR && !cached.refreshing.swap(true, Ordering::SeqCst) {
            let root = root.to_path_buf();
            let spawned = std::thread::Builder::new()
                .name("ghostex-file-list".into())
                .spawn(move || store(&root, Arc::new(build_listing(&root, git))));
            if spawned.is_err() {
                cached.refreshing.store(false, Ordering::SeqCst);
            }
        }
        return cached;
    }
    // Keystrokes that arrive while the first listing is being made wait for it instead of each
    // starting their own.
    let lock = build_locks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(root.to_path_buf())
        .or_default()
        .clone();
    let _building = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cached_listing(root) {
        return cached;
    }
    let built = Arc::new(build_listing(root, git));
    store(root, built.clone());
    built
}

fn cached_listing(root: &Path) -> Option<Arc<Listing>> {
    listings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(root)
        .cloned()
}

static BUILD_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

fn build_locks() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    BUILD_LOCKS.get_or_init(Default::default)
}

fn store(root: &Path, listing: Arc<Listing>) {
    let mut listings = listings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if listings.len() >= MAX_LISTINGS && !listings.contains_key(root) {
        if let Some(oldest) = listings
            .iter()
            .min_by_key(|(_, listing)| listing.built)
            .map(|(root, _)| root.clone())
        {
            listings.remove(&oldest);
        }
    }
    listings.insert(root.to_path_buf(), listing);
}

fn build_listing(root: &Path, git: GitRunner) -> Listing {
    let (mut files, incomplete) = git_files(root, git).unwrap_or_else(|| walk_files(root));
    files.sort_unstable();
    files.dedup();
    Listing {
        built: Instant::now(),
        files,
        incomplete,
        refreshing: AtomicBool::new(false),
    }
}

/// A path inside a folder the Files view never lists (`node_modules`, `target`, `dist`, …), or a
/// file it never shows.
fn is_skipped_path(path: &str) -> bool {
    let mut segments = path.split('/').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_some() {
            if is_skipped_directory(segment) {
                return true;
            }
        } else if is_skipped_file(segment) {
            return true;
        }
    }
    false
}

/// Tracked files plus untracked files git does not ignore: exactly what `.gitignore` leaves. A
/// submodule is one gitlink entry, not files, so its tree is never listed.
fn git_files(root: &Path, git: GitRunner) -> Option<(Vec<Box<str>>, bool)> {
    if !root.join(".git").exists() {
        return None;
    }
    let cwd = root.to_string_lossy();
    let staged = git(&cwd, &["ls-files", "-z", "--stage"])?;
    let untracked =
        git(&cwd, &["ls-files", "-z", "--others", "--exclude-standard"]).unwrap_or_default();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut files: Vec<Box<str>> = Vec::new();
    let mut incomplete = false;
    let tracked = staged.split('\0').filter_map(|entry| {
        // `<mode> <object> <stage>\t<path>`; mode 160000 is a submodule, and a symlink (120000)
        // to a folder is not a file either.
        let (meta, path) = entry.split_once('\t')?;
        let keep = !meta.starts_with("160000")
            && !(meta.starts_with("120000") && root.join(path).is_dir());
        keep.then_some(path)
    });
    let others = untracked
        .split('\0')
        // A nested repository shows up as `folder/`.
        .filter(|path| !path.ends_with('/'));
    for path in tracked.chain(others) {
        if path.is_empty() || is_skipped_path(path) || !seen.insert(path) {
            continue;
        }
        if files.len() >= MAX_FILES {
            incomplete = true;
            break;
        }
        files.push(path.into());
    }
    Some((files, incomplete))
}

/// A folder that is not a git repository: every file, nearest folders first, skipping the Files
/// view's build and dependency folders and hidden folders, never following a symlinked folder.
fn walk_files(root: &Path) -> (Vec<Box<str>>, bool) {
    let mut files = Vec::new();
    let mut queue: VecDeque<(PathBuf, String, usize)> =
        VecDeque::from([(root.to_path_buf(), String::new(), 0)]);
    while let Some((directory, relative, depth)) = queue.pop_front() {
        let Ok(children) = fs::read_dir(&directory) else {
            continue;
        };
        for child in children.flatten() {
            let name = child.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            if file_type.is_dir() {
                if depth < MAX_WALK_DEPTH && !name.starts_with('.') && !is_skipped_directory(&name)
                {
                    queue.push_back((child.path(), path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() || is_skipped_file(&name) {
                continue;
            }
            if files.len() >= MAX_FILES {
                return (files, true);
            }
            files.push(path.into_boxed_str());
        }
    }
    (files, false)
}

/// The best `max_matches` files under `root` for `query`, best first. Every word of the query must
/// match (its letters in order, case-insensitively); a word that matches the file name ranks above
/// one that only matches its folders. An empty query lists the files nearest the root.
pub fn search(root: &Path, query: &str, max_matches: usize, git: GitRunner) -> FileSearchOutcome {
    let listing = listing(root, git);
    let terms: Vec<Vec<u8>> = query
        .split_whitespace()
        .map(|term| term.bytes().map(|byte| byte.to_ascii_lowercase()).collect())
        .collect();
    let mut found: Vec<(i64, &str)> = if terms.is_empty() {
        listing
            .files
            .iter()
            .map(|path| {
                let depth = path.matches('/').count() as i64;
                (-(depth * 1_000) - path.len() as i64, &**path)
            })
            .collect()
    } else {
        listing
            .files
            .iter()
            .filter_map(|path| score_path(path, &terms).map(|score| (score, &**path)))
            .collect()
    };
    let total = found.len();
    let order = |a: &(i64, &str), b: &(i64, &str)| {
        b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(b.1))
    };
    if found.len() > max_matches && max_matches > 0 {
        found.select_nth_unstable_by(max_matches - 1, order);
        found.truncate(max_matches);
    }
    found.sort_unstable_by(order);
    FileSearchOutcome {
        matches: found
            .into_iter()
            .take(max_matches)
            .map(|(_, path)| FileMatch {
                path: path.to_string(),
            })
            .collect(),
        total,
        incomplete: listing.incomplete,
    }
}

/// Drops the cached listing of `root`, so the next search lists it again.
pub fn forget(root: &Path) {
    listings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(root);
}

fn is_separator(byte: u8) -> bool {
    matches!(byte, b'/' | b'\\' | b'_' | b'-' | b'.' | b' ')
}

/// The sum of every term's score, less a little for long and deep paths; `None` when a term does
/// not match.
fn score_path(path: &str, terms: &[Vec<u8>]) -> Option<i64> {
    let bytes = path.as_bytes();
    let name_start = path.rfind('/').map(|index| index + 1).unwrap_or(0);
    let mut total = 0i64;
    for term in terms {
        total += score_term(bytes, name_start, term)?;
    }
    let depth = path.matches('/').count() as i64;
    Some(total - bytes.len() as i64 / 8 - depth * 2)
}

/// One term: matched in the file name if it can be (a large bonus, more for a prefix or the whole
/// name), otherwise anywhere in the path. A term with a `/` in it is a path and only matches the
/// whole path.
fn score_term(path: &[u8], name_start: usize, term: &[u8]) -> Option<i64> {
    if !term.contains(&b'/') {
        if let Some((start, end)) = window(path, name_start, term) {
            let name = &path[name_start..];
            let stem_len = name
                .iter()
                .rposition(|byte| *byte == b'.')
                .filter(|index| *index > 0)
                .unwrap_or(name.len());
            let mut bonus = 40;
            if start == name_start && end - start == term.len() {
                bonus += 30;
                if term.len() == stem_len || term.len() == name.len() {
                    bonus += 60;
                }
            }
            return Some(score_window(path, start, end, name_start, term) + bonus);
        }
    }
    let (start, end) = window(path, 0, term)?;
    Some(score_window(path, start, end, name_start, term))
}

/// The shortest stretch of `path[from..]` holding `term`'s letters in order, ending at their first
/// complete occurrence (fzf's v1 match): `(start, end)`, end exclusive.
fn window(path: &[u8], from: usize, term: &[u8]) -> Option<(usize, usize)> {
    if term.is_empty() {
        return None;
    }
    let mut next = 0;
    let mut end = None;
    for (index, byte) in path.iter().enumerate().skip(from) {
        if byte.to_ascii_lowercase() == term[next] {
            next += 1;
            if next == term.len() {
                end = Some(index + 1);
                break;
            }
        }
    }
    let end = end?;
    let mut remaining = term.len();
    for index in (from..end).rev() {
        if path[index].to_ascii_lowercase() == term[remaining - 1] {
            remaining -= 1;
            if remaining == 0 {
                return Some((index, end));
            }
        }
    }
    None
}

/// Points for every matched letter, more at word starts (after `/`, `_`, `-`, `.`, a space, or a
/// lower-to-upper case change) and for runs of consecutive letters, less for gaps.
fn score_window(path: &[u8], start: usize, end: usize, name_start: usize, term: &[u8]) -> i64 {
    let mut score = 0i64;
    let mut next = 0;
    let mut previous: Option<usize> = None;
    for index in start..end {
        if next == term.len() {
            break;
        }
        if path[index].to_ascii_lowercase() != term[next] {
            continue;
        }
        next += 1;
        score += 16;
        if index == 0 || is_separator(path[index - 1]) {
            score += if index == name_start { 24 } else { 12 };
        } else if path[index - 1].is_ascii_lowercase() && path[index].is_ascii_uppercase() {
            score += 10;
        }
        match previous {
            Some(previous) if previous + 1 == index => score += 10,
            Some(previous) => score -= ((index - previous - 1).min(8) as i64) + 2,
            None => {}
        }
        previous = Some(index);
    }
    score
}
