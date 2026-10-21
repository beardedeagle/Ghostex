//! CDXC:AgentProviders 2026-10-22 WHY:
//! `cswap run <N>` (claude-swap 0.27.2) starts plain `claude` on the default login, skipping the slot's session profile, whenever `~/.claude.json` names slot N's account. It never checks that the default login still holds a credential. On 2026-10-22 `~/.claude.json` named yahia@shortpoint.com while the default login was signed out, so every session on account 2 (S90:P5vkl:G2zg8 among them) started with "Not logged in · Please run /login". Reconnecting, reloading and re-seeding the slot's profile changed nothing, because cswap never used that profile. Claude Swap's own escape from that shortcut is a `CLAUDE_CONFIG_DIR` already set when it starts: it then validates the slot's profile, re-seeds it from its stored login when needed and runs Claude there. So a start whose slot names a signed-out default login gets that variable. A default login that still holds a credential keeps cswap's shortcut, which exists so the default login and a session profile never hold two copies of one token family.
//! SEE-ALSO: server/src/zmx/provider.rs (session starts and wakes), server/src/accounts/restart.rs (in-place account switch), server/src/agents/drafts.rs (draft agent switch), server/src/accounts/launch.rs (one-shot jobs).
use super::claude_resets::ClaudeSlot;
use std::{collections::HashMap, path::Path};

/// `text` with `CLAUDE_CONFIG_DIR=<slot profile>` in front of each `cswap run <slot>` command for a slot whose account is the signed-out default login, so cswap starts that slot's own profile instead. Commands are found wherever the shell runs one: at a line start, after `;`, `&&`, `||`, `|`, `(`, `$(`, `{`, `then`, `else` or `do` (a wake runs `"$(cswap run 2 -- agents --json)"; if …; then cswap run 2 -- attach …; else cswap run 2 -- --resume …; fi`, accounts/claude_background.rs). The quoted copy a restore banner prints is never in such a position.
pub(crate) fn route_around_signed_out_default(text: &str) -> String {
    if cfg!(windows) || !text.contains("cswap") {
        return text.to_string();
    }
    let Ok(home) = super::launch::home() else {
        return text.to_string();
    };
    let mut profiles: HashMap<String, Option<String>> = HashMap::new();
    let mut inserts = Vec::new();
    for (found, _) in text.match_indices("cswap") {
        let start = text[..found]
            .rfind(|ch: char| ch.is_whitespace() || "(;|&{`".contains(ch))
            .map_or(0, |at| {
                at + text[at..].chars().next().map_or(1, char::len_utf8)
            });
        if !in_command_position(&text[..start]) {
            continue;
        }
        let Some(slot) = cswap_run_slot(text, start) else {
            continue;
        };
        if let Some(profile) = profiles
            .entry(slot.clone())
            .or_insert_with(|| signed_out_default_profile(&home, &slot))
        {
            inserts.push((
                start,
                format!(
                    "CLAUDE_CONFIG_DIR={} ",
                    crate::agents::quote_shell_arg(profile)
                ),
            ));
        }
    }
    inserts.dedup_by(|a, b| a.0 == b.0);
    let mut routed = text.to_string();
    for (at, prefix) in inserts.into_iter().rev() {
        routed.insert_str(at, &prefix);
    }
    routed
}

/// Whether a word that follows `before` is where the shell expects a command name.
fn in_command_position(before: &str) -> bool {
    let before = before.trim_end_matches([' ', '\t']);
    if before.is_empty() || before.ends_with(['\n', ';', '|', '&', '(', '{', '`']) {
        return true;
    }
    let word_start = before
        .rfind(|ch: char| ch.is_whitespace() || ";|&({`".contains(ch))
        .map_or(0, |at| at + 1);
    let word = &before[word_start..];
    // A custom agent's own variables (`FOO=1 claude …`) stay in front of the cswap wrapper (launch.rs `with_account_command`).
    let assignment = word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.bytes().all(|b| b == b'_' || b.is_ascii_alphanumeric())
    });
    (assignment || matches!(word, "then" | "else" | "do"))
        && in_command_position(&before[..word_start])
}

/// The slot of a command that starts at `from` and runs cswap as `cswap run <slot>`.
fn cswap_run_slot(text: &str, from: usize) -> Option<String> {
    let (_, end, word) = crate::agents::command_word(text, from)?;
    if Path::new(&word).file_name()?.to_str()? != "cswap" {
        return None;
    }
    let (_, end, run) = crate::agents::command_word(text, end)?;
    if run != "run" {
        return None;
    }
    let (_, _, slot) = crate::agents::command_word(text, end)?;
    (!slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit())).then_some(slot)
}

/// The slot's session profile when `cswap run <slot>` would start Claude on a default login that names the slot's account but holds no credential.
fn signed_out_default_profile(home: &Path, slot: &str) -> Option<String> {
    let slot = ClaudeSlot::read(home, slot).ok()?;
    (slot.names_default_login(home) && default_login_signed_out(home)).then(|| slot.session_dir())
}

/// Whether the default login certainly holds no credential, read the way Claude Swap reads a profile's credential material: no non-empty `~/.claude/.credentials.json` and, on macOS, a Keychain miss. A Keychain that cannot be read counts as signed in, which leaves cswap's own choice alone.
fn default_login_signed_out(home: &Path) -> bool {
    if std::fs::read_to_string(home.join(".claude/.credentials.json"))
        .is_ok_and(|text| !text.trim().is_empty())
    {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        keychain_item_missing("Claude Code-credentials")
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// `security find-generic-password` without `-w` reads only the item's attributes; exit status 44 is "no such item".
#[cfg(target_os = "macos")]
fn keychain_item_missing(service: &str) -> bool {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let Some(user) = std::env::var("USER").ok().filter(|user| !user.is_empty()) else {
        return false;
    };
    let Ok(mut child) = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-a", &user, "-s", service])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code() == Some(44),
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}
