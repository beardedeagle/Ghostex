//! The Files view's Search sidebar (Cmd+Shift+F): every line of the project's files that matches
//! what the user typed, grouped by file. Desktop (local projects, streamed as files are found) and
//! gxserver (remote projects, answered whole) both run this, so they search the same files.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User: "please add cmd + shift + f so i can search in all files in the project (ignoring files we don't need to search in like node_modules or build folders for different langs)" and "make cmd + shift + f work just like the VS Code one". With "Use exclude settings and ignore files" on (the default) the search honours `.gitignore`, `.ignore` and git's global and repository excludes, and skips the dependency and build folders below even without a `.gitignore`; turning it off searches those too, as VS Code's toggle does. Binary files, files over [`MAX_FILE_SIZE`] and `.git` are never searched, and the search stops at the caller's match cap.
//!
//! CDXC:Docs 2026-10-25 WHY:
//! Nested repositories inside a dot-folder (git submodules such as this repo's `.dependencies/` checkouts of Ghostty, Zed and code-server) are not searched while the ignore toggle is on: they hold most of a big project's files (a full search of this repo takes about 0.1s without them and 1.6 to 2.5s more with them), and their matches drown the project's own. A "files to include" pattern that names a path inside one searches it, and turning the toggle off searches all of them.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User picked "Skip nested repositories only inside dot-folders like .dependencies/, so apps/mobile/app and other nested repos in normal folders are searched by default."
//!
//! SEE-ALSO: apps/desktop/src/app/native_docs/project_search.rs (the sidebar, local streaming),
//! server/src/project_docs/project_scope.rs (`searchContent` for remote projects).

use std::{
    path::Path,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use ignore::{overrides::OverrideBuilder, WalkBuilder, WalkState};
use regex::bytes::{Regex, RegexBuilder};

use crate::project::is_skipped_directory;

/// Larger files are not searched.
pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
/// The match cap a caller uses unless it has a reason to pick another.
pub const DEFAULT_MAX_MATCHES: usize = 10_000;
/// Text kept before a match in its preview; a longer start of line is cut with an ellipsis.
const PREVIEW_BEFORE: usize = 32;
/// Text kept after a match in its preview.
const PREVIEW_AFTER: usize = 160;
/// Longest match shown whole in its preview.
const PREVIEW_MATCH: usize = 200;
/// The first bytes of a file that decide whether it is binary (a NUL byte means binary).
const BINARY_PROBE: usize = 8 * 1024;

/// Folders the search skips on top of the ones the Files view never lists
/// ([`crate::project::SKIPPED_DIRECTORY_NAMES`]): the remaining build, cache and dependency
/// folders of the common languages.
const EXTRA_SKIPPED_DIRECTORY_NAMES: &[&str] = &[
    "__pycache__",
    ".mypy_cache",
    ".dart_tool",
    ".zig-cache",
    "zig-cache",
    "Pods",
    "bower_components",
    ".parcel-cache",
    ".nyc_output",
    ".angular",
    ".expo",
    ".astro",
];

/// What to search for and where, as the sidebar's fields and toggles hold it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSearchOptions {
    pub query: String,
    pub is_regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
    /// "files to include": comma-separated globs; empty searches everything.
    pub include: String,
    /// "files to exclude": comma-separated globs.
    pub exclude: String,
    /// "Use exclude settings and ignore files".
    pub use_ignore_files: bool,
    pub max_matches: usize,
}

/// One match: where it is and the text around it for the results list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMatch {
    /// 0-based line number.
    pub line: usize,
    /// Byte columns of the match within its line.
    pub start: usize,
    pub end: usize,
    /// The preview: the text before the match (cut with "…" when long), the match, and after.
    pub before: String,
    pub matched: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMatches {
    /// Relative to the searched root, `/`-separated.
    pub path: String,
    pub matches: Vec<LineMatch>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSearchSummary {
    pub files: usize,
    pub matches: usize,
    /// The match cap was reached; more files may match.
    pub capped: bool,
    pub cancelled: bool,
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The regex the query stands for: a literal unless the regex toggle is on, bounded by word
/// boundaries for whole word (only where the query itself starts or ends with a word character,
/// so `foo(` still matches as a whole word).
pub fn build_matcher(options: &ContentSearchOptions) -> Result<Regex, String> {
    let pattern = if options.is_regex {
        if options.whole_word {
            format!(r"\b(?:{})\b", options.query)
        } else {
            options.query.clone()
        }
    } else {
        let escaped = regex::escape(&options.query);
        let bytes = options.query.as_bytes();
        let starts_word = bytes.first().copied().is_some_and(is_word_byte);
        let ends_word = bytes.last().copied().is_some_and(is_word_byte);
        if options.whole_word {
            format!(
                "{}{escaped}{}",
                if starts_word { r"\b" } else { "" },
                if ends_word { r"\b" } else { "" }
            )
        } else {
            escaped
        }
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.match_case)
        .multi_line(true)
        .build()
        .map_err(|error| match error {
            regex::Error::Syntax(message) => message
                .lines()
                .last()
                .unwrap_or("Invalid regular expression")
                .trim()
                .trim_start_matches("error: ")
                .to_string(),
            other => other.to_string(),
        })
}

/// The comma-separated globs of a field, VS Code style: `src` and `*.rs` match anywhere in the
/// project, `./src` only at its root, and a folder pattern covers everything inside it.
fn field_globs(field: &str) -> Vec<String> {
    let mut globs = Vec::new();
    for raw in field.split(',') {
        let entry = raw.trim().replace('\\', "/");
        let entry = entry.trim_end_matches('/');
        if entry.is_empty() {
            continue;
        }
        let anchored = if let Some(rest) = entry.strip_prefix("./") {
            format!("/{rest}")
        } else if entry.starts_with('/') || entry.starts_with("**/") {
            entry.to_string()
        } else {
            format!("**/{entry}")
        };
        if !anchored.ends_with("/**") {
            globs.push(format!("{anchored}/**"));
        }
        globs.push(anchored);
    }
    globs
}

/// The leading folders of an include pattern before its first glob character
/// (`.dependencies/zmx/src/*.zig` gives `.dependencies/zmx/src`).
fn literal_prefixes(field: &str) -> Vec<String> {
    field
        .split(',')
        .filter_map(|raw| {
            let entry = raw.trim().replace('\\', "/");
            let entry = entry.trim_start_matches("./").trim_start_matches('/');
            let cut = entry
                .find(['*', '?', '[', '{'])
                .map_or(entry, |index| &entry[..index]);
            let cut = cut.trim_end_matches('/');
            (!cut.is_empty()).then(|| cut.to_string())
        })
        .collect()
}

/// A folder the default filters always pass over: a dependency or build folder, or `vendor/bundle`.
pub fn is_skipped_search_directory(name: &str, relative: &str) -> bool {
    is_skipped_directory(name)
        || EXTRA_SKIPPED_DIRECTORY_NAMES.contains(&name)
        || relative == "vendor/bundle"
        || relative.ends_with("/vendor/bundle")
}

/// The preview of one match in `line` (bytes `start..end`), cut to a readable length.
fn preview(line: &[u8], start: usize, end: usize) -> (String, String, String) {
    let before_text = String::from_utf8_lossy(&line[..start]);
    let before_trimmed = before_text.trim_start();
    let before_chars = before_trimmed.chars().count();
    let before = if before_chars > PREVIEW_BEFORE {
        let skip = before_chars - PREVIEW_BEFORE;
        format!("…{}", before_trimmed.chars().skip(skip).collect::<String>())
    } else {
        before_trimmed.to_string()
    };
    let matched_text = String::from_utf8_lossy(&line[start..end]);
    let matched = if matched_text.chars().count() > PREVIEW_MATCH {
        format!(
            "{}…",
            matched_text.chars().take(PREVIEW_MATCH).collect::<String>()
        )
    } else {
        matched_text.into_owned()
    };
    let after_text = String::from_utf8_lossy(&line[end..]);
    let after = if after_text.chars().count() > PREVIEW_AFTER {
        format!(
            "{}…",
            after_text.chars().take(PREVIEW_AFTER).collect::<String>()
        )
    } else {
        after_text.into_owned()
    };
    (before, matched, after.trim_end().to_string())
}

/// Every match of `matcher` in `bytes`, line by line. `None` for a binary file.
pub fn search_bytes(bytes: &[u8], matcher: &Regex) -> Option<Vec<LineMatch>> {
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return None;
    }
    let mut matches = Vec::new();
    if !matcher.is_match(bytes) {
        return Some(matches);
    }
    for (line_number, raw_line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        for found in matcher.find_iter(line) {
            if found.start() == found.end() {
                continue;
            }
            let (before, matched, after) = preview(line, found.start(), found.end());
            matches.push(LineMatch {
                line: line_number,
                start: found.start(),
                end: found.end(),
                before,
                matched,
                after,
            });
        }
    }
    Some(matches)
}

/// Searches every file under `root`, handing each file with matches to `on_file` as it is found
/// (from the walker's threads, in no fixed order). Stops early when `cancel` is set or the match
/// cap is reached. Fails only for an invalid regex or glob.
pub fn search_content(
    root: &Path,
    options: &ContentSearchOptions,
    cancel: &AtomicBool,
    on_file: impl Fn(FileMatches) + Sync,
) -> Result<ContentSearchSummary, String> {
    let matcher = build_matcher(options)?;
    let mut overrides = OverrideBuilder::new(root);
    for glob in field_globs(&options.include) {
        overrides
            .add(&glob)
            .map_err(|error| format!("files to include: {error}"))?;
    }
    for glob in field_globs(&options.exclude) {
        overrides
            .add(&format!("!{glob}"))
            .map_err(|error| format!("files to exclude: {error}"))?;
    }
    let overrides = overrides.build().map_err(|error| error.to_string())?;
    let include_prefixes = literal_prefixes(&options.include);
    let use_ignore_files = options.use_ignore_files;
    let max_matches = options.max_matches.max(1);
    let root_path = root.to_path_buf();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(use_ignore_files)
        .ignore(use_ignore_files)
        .git_ignore(use_ignore_files)
        .git_global(use_ignore_files)
        .git_exclude(use_ignore_files)
        .max_filesize(Some(MAX_FILE_SIZE))
        .overrides(overrides)
        .filter_entry(move |entry| {
            if entry.depth() == 0 || !entry.file_type().is_some_and(|kind| kind.is_dir()) {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if name == ".git" {
                return false;
            }
            if !use_ignore_files {
                return true;
            }
            let relative = entry
                .path()
                .strip_prefix(&root_path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if is_skipped_search_directory(&name, &relative) {
                return false;
            }
            if relative.split('/').any(|segment| segment.starts_with('.'))
                && entry.path().join(".git").exists()
            {
                // A nested repository inside a dot-folder: searched only when an include pattern
                // points into it.
                return include_prefixes.iter().any(|prefix| {
                    prefix == &relative
                        || prefix.starts_with(&format!("{relative}/"))
                        || relative.starts_with(&format!("{prefix}/"))
                });
            }
            true
        });

    let total = AtomicUsize::new(0);
    let files = AtomicUsize::new(0);
    let capped = AtomicBool::new(false);
    let on_file = &on_file;
    builder.build_parallel().run(|| {
        let matcher = &matcher;
        let (total, files, capped) = (&total, &files, &capped);
        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) || capped.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return WalkState::Continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                return WalkState::Continue;
            };
            let Some(mut matches) = search_bytes(&bytes, matcher) else {
                return WalkState::Continue;
            };
            if matches.is_empty() {
                return WalkState::Continue;
            }
            let before = total.fetch_add(matches.len(), Ordering::SeqCst);
            if before >= max_matches {
                capped.store(true, Ordering::SeqCst);
                return WalkState::Quit;
            }
            if before + matches.len() >= max_matches {
                matches.truncate(max_matches - before);
                capped.store(true, Ordering::SeqCst);
            }
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let path = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            files.fetch_add(1, Ordering::SeqCst);
            on_file(FileMatches { path, matches });
            if capped.load(Ordering::Relaxed) {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });
    let matches = total.load(Ordering::SeqCst).min(max_matches);
    Ok(ContentSearchSummary {
        files: files.load(Ordering::SeqCst),
        matches,
        capped: capped.load(Ordering::SeqCst),
        cancelled: cancel.load(Ordering::SeqCst),
    })
}

/// The options as the desktop sends them to a remote project's gxserver (`searchContent`).
pub fn options_to_json(options: &ContentSearchOptions) -> serde_json::Value {
    serde_json::json!({
        "query": options.query,
        "isRegex": options.is_regex,
        "matchCase": options.match_case,
        "wholeWord": options.whole_word,
        "include": options.include,
        "exclude": options.exclude,
        "useIgnoreFiles": options.use_ignore_files,
        "maxMatches": options.max_matches,
    })
}

pub fn options_from_json(value: &serde_json::Value) -> ContentSearchOptions {
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
    let flag = |key: &str| value[key].as_bool().unwrap_or(false);
    ContentSearchOptions {
        query: text("query"),
        is_regex: flag("isRegex"),
        match_case: flag("matchCase"),
        whole_word: flag("wholeWord"),
        include: text("include"),
        exclude: text("exclude"),
        use_ignore_files: value["useIgnoreFiles"].as_bool().unwrap_or(true),
        max_matches: value["maxMatches"]
            .as_u64()
            .map_or(DEFAULT_MAX_MATCHES, |max| {
                (max as usize).clamp(1, DEFAULT_MAX_MATCHES)
            }),
    }
}

pub fn file_matches_to_json(file: &FileMatches) -> serde_json::Value {
    serde_json::json!({
        "path": file.path,
        "matches": file.matches.iter().map(|found| serde_json::json!([
            found.line, found.start, found.end, found.before, found.matched, found.after,
        ])).collect::<Vec<_>>(),
    })
}

pub fn file_matches_from_json(value: &serde_json::Value) -> Option<FileMatches> {
    let matches = value["matches"]
        .as_array()?
        .iter()
        .filter_map(|found| {
            let number = |index: usize| found[index].as_u64().map(|value| value as usize);
            let text = |index: usize| found[index].as_str().map(str::to_string);
            Some(LineMatch {
                line: number(0)?,
                start: number(1)?,
                end: number(2)?,
                before: text(3)?,
                matched: text(4)?,
                after: text(5)?,
            })
        })
        .collect();
    Some(FileMatches {
        path: value["path"].as_str()?.to_string(),
        matches,
    })
}

/// Searches `root` and answers in one piece: every file with matches, sorted by path, and the
/// summary. gxserver answers a remote project's `searchContent` with it.
pub fn search_content_json(
    root: &Path,
    options: &ContentSearchOptions,
) -> Result<serde_json::Value, String> {
    let found = std::sync::Mutex::new(Vec::new());
    let summary = search_content(root, options, &AtomicBool::new(false), |file| {
        found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(file);
    })?;
    let mut found = found
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(serde_json::json!({
        "files": found.iter().map(file_matches_to_json).collect::<Vec<_>>(),
        "fileCount": summary.files,
        "matchCount": summary.matches,
        "capped": summary.capped,
    }))
}
