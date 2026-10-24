//! What a session's transcript says about its links: the PRs and issues the agent opened, the
//! GitHub PR and issue URLs the chat only mentioned, and the Linear tickets it mentioned.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User picked which links join a session by themselves: the branch's, earlier branches', "PRs and issues the agent opened", what the agent reports with `ghostex link add`, and the issues a linked PR closes. A PR or issue URL the chat only mentioned is offered under Suggested instead ("suggest"). Linear tickets join when mentioned: "and several linear tickets too (if the user added linear api key and the linear issue id or link is mentioned in the thread)"; the pass keeps only IDs of the Linear teams that key sees.
//!
//! CDXC:SessionLinks 2026-10-24 WHY:
//! A transcript is read once in full and then only from where the last read stopped (rows at or past the length it had), so a long session costs a full read once per gxserver run and a page or two per pass after that.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde_json::Value;

use super::{parse_github_url, parse_linear_url, SessionLinkRef};

/// Transcript rows per page, the detail pager's own cap.
const PAGE_ROWS: usize = 200;
/// Pages read at most per transcript on its first read (20,000 rows).
const MAX_PAGES: usize = 100;
/// Transcripts whose findings are kept.
const CACHE_ENTRIES: usize = 512;
/// Mentioned GitHub URLs kept per transcript, newest last.
const MAX_MENTIONS: usize = 40;

/// What one transcript said, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptLinks {
    /// PRs and issues a tool call created.
    pub(crate) opened: Vec<SessionLinkRef>,
    /// GitHub PR and issue URLs the user or the agent wrote.
    pub(crate) github_mentions: Vec<SessionLinkRef>,
    /// Linear IDs (from URLs and bare `GHX-142` words) the user or the agent wrote.
    pub(crate) linear_mentions: Vec<String>,
}

impl TranscriptLinks {
    fn push_unique<T: PartialEq>(list: &mut Vec<T>, value: T) {
        if !list.contains(&value) {
            list.push(value);
        }
    }

    fn mention_github(&mut self, link: SessionLinkRef) {
        self.github_mentions.retain(|known| known != &link);
        self.github_mentions.push(link);
        if self.github_mentions.len() > MAX_MENTIONS {
            self.github_mentions.remove(0);
        }
    }
}

/// The scan's state for one transcript, so the next pass continues where this one stopped.
#[derive(Default)]
struct Scanner {
    links: TranscriptLinks,
    /// Tool calls that create a PR or issue, waiting for their result.
    creating_calls: HashSet<String>,
    /// The last tool call created something and recorded no call id (agents that pair a result
    /// with the call right before it).
    creating_without_id: bool,
}

/// The tool input of a call that creates a PR or an issue: `gh pr create`, `gh issue create`, or a
/// GitHub tool server's `create_pull_request` / `create_issue`.
fn creates_github_item(name: &str, input: &Value) -> bool {
    let input = match input {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let name = name.to_ascii_lowercase();
    input.contains("gh pr create")
        || input.contains("gh issue create")
        || name.contains("create_pull_request")
        || name.contains("create_issue")
}

impl Scanner {
    fn scan_message(&mut self, message: &Value) {
        let role = crate::ghostex_cli::session_chat_transcript::effective_role(message);
        let blocks = message
            .get("blocks")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool-call") => {
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let creates =
                        creates_github_item(name, block.get("input").unwrap_or(&Value::Null));
                    match block.get("callId").and_then(Value::as_str) {
                        Some(call_id) if creates => {
                            self.creating_calls.insert(call_id.to_string());
                        }
                        Some(_) => {}
                        None => self.creating_without_id = creates,
                    }
                }
                Some("tool-result") => {
                    let created = match block.get("callId").and_then(Value::as_str) {
                        Some(call_id) => self.creating_calls.remove(call_id),
                        None => std::mem::take(&mut self.creating_without_id),
                    };
                    let failed = block.get("isError").and_then(Value::as_bool) == Some(true);
                    if created && !failed {
                        let output = block.get("output").and_then(Value::as_str).unwrap_or("");
                        for link in urls_in(output, "https://github.com/")
                            .filter_map(|url| parse_github_url(&url))
                        {
                            TranscriptLinks::push_unique(&mut self.links.opened, link);
                        }
                    }
                }
                Some("text") if matches!(role, "user" | "assistant") => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                    self.scan_text(text);
                }
                _ => {}
            }
        }
    }

    fn scan_text(&mut self, text: &str) {
        for link in urls_in(text, "https://github.com/").filter_map(|url| parse_github_url(&url)) {
            self.links.mention_github(link);
        }
        for link in urls_in(text, "https://linear.app/").filter_map(|url| parse_linear_url(&url)) {
            if let SessionLinkRef::Linear { identifier } = link {
                TranscriptLinks::push_unique(&mut self.links.linear_mentions, identifier);
            }
        }
        for identifier in bare_linear_identifiers(text) {
            TranscriptLinks::push_unique(&mut self.links.linear_mentions, identifier);
        }
    }
}

/// Every address in `text` that starts with `prefix`, cut where the address ends.
fn urls_in<'a>(text: &'a str, prefix: &'a str) -> impl Iterator<Item = String> + 'a {
    text.match_indices(prefix).map(move |(start, _)| {
        text[start..]
            .chars()
            .take_while(|c| {
                !c.is_whitespace() && !matches!(c, ')' | ']' | '>' | '<' | '"' | '\'' | '`' | ',')
            })
            .collect::<String>()
            .trim_end_matches(['.', ':', ';'])
            .to_string()
    })
}

/// `GHX-142`-shaped words: an uppercase key that starts with a letter, a dash, digits. Lowercase
/// spellings are left out (`utf-8`, a branch's `spx-12`); the pass keeps only real team keys.
fn bare_linear_identifiers(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter_map(|word| {
            let (key, number) = word.split_once('-')?;
            let valid = (1..=10).contains(&key.len())
                && key.starts_with(|c: char| c.is_ascii_uppercase())
                && key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && (1..=7).contains(&number.len())
                && number.chars().all(|c| c.is_ascii_digit());
            valid.then(|| format!("{key}-{number}"))
        })
}

struct CachedScan {
    len: u64,
    modified: Option<SystemTime>,
    scanner: Scanner,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, CachedScan>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedScan>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Whether reading this transcript now would read it in full (it was never read, or it shrank),
/// which the pass rations.
pub(crate) fn needs_full_scan(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).map(|cached| metadata.len() < cached.len))
        .unwrap_or(true)
}

/// What the transcript said so far, reading only what was added since the last call.
pub(crate) fn scan_transcript(
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &Path,
    project_id: &str,
    session_id: &str,
) -> Option<TranscriptLinks> {
    let metadata = std::fs::metadata(path).ok()?;
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    let mut previous = cache().lock().ok()?.remove(path);
    if let Some(cached) = &previous {
        if cached.len == len && cached.modified == modified {
            let links = cached.scanner.links.clone();
            if let Ok(mut cache) = cache().lock() {
                cache.insert(path.to_path_buf(), previous.take()?);
            }
            return Some(links);
        }
    }
    // A shrunk file was rewritten: read it again from the start.
    let (mut scanner, from) = match previous {
        Some(cached) if cached.len <= len => (cached.scanner, cached.len),
        _ => (Scanner::default(), 0),
    };
    for message in read_rows_since(agent, path, len, from, project_id, session_id) {
        scanner.scan_message(&message);
    }
    let links = scanner.links.clone();
    if let Ok(mut cache) = cache().lock() {
        if cache.len() >= CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(
            path.to_path_buf(),
            CachedScan {
                len,
                modified,
                scanner,
            },
        );
    }
    Some(links)
}

/// The rows at or past byte `from`, oldest first.
fn read_rows_since(
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: &Path,
    len: u64,
    from: u64,
    project_id: &str,
    session_id: &str,
) -> Vec<Value> {
    let mut before = len;
    let mut rows: Vec<Value> = Vec::new();
    for _ in 0..MAX_PAGES {
        let Ok(page) = crate::session_chat_history::read_history(
            agent, path, before, PAGE_ROWS, true, false, project_id, session_id,
        ) else {
            break;
        };
        let mut older: Vec<Value> = page
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let next = page
            .get("beforeOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let reached_old_rows = next <= from;
        if from > 0 {
            older.retain(|row| {
                row.get("byteOffset")
                    .and_then(Value::as_u64)
                    .is_none_or(|offset| offset >= from)
            });
        }
        older.extend(rows);
        rows = older;
        if reached_old_rows
            || page.get("hasMore").and_then(Value::as_bool) != Some(true)
            || next >= before
        {
            break;
        }
        before = next;
    }
    rows
}
