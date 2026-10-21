//! CDXC:AgentProviders 2026-10-22 WHY:
//! `cswap run <N>` (claude-swap 0.27.2) starts plain `claude` on the default login, skipping the slot's session profile, whenever `~/.claude.json` names slot N's account. It never checks that the default login still holds a credential. On 2026-10-22 `~/.claude.json` named yahia@shortpoint.com while the default login was signed out, so every session on account 2 (S90:P5vkl:G2zg8 among them) started with "Not logged in · Please run /login". Reconnecting, reloading and re-seeding the slot's profile changed nothing, because cswap never used that profile. Claude Swap's own escape from that shortcut is a `CLAUDE_CONFIG_DIR` already set when it starts: it then validates the slot's profile, re-seeds it from its stored login when needed and runs Claude there. So a start whose slot names a signed-out default login gets that variable. A default login that still holds a credential keeps cswap's shortcut, which exists so the default login and a session profile never hold two copies of one token family.
//! SEE-ALSO: server/src/zmx/provider.rs (session starts and wakes), server/src/accounts/restart.rs (in-place account switch), server/src/agents/drafts.rs (draft agent switch), server/src/accounts/launch.rs (one-shot jobs).
use super::claude_resets::ClaudeSlot;
use std::{collections::HashMap, path::Path};

/// `text` with `CLAUDE_CONFIG_DIR=<slot profile>` in front of each line that starts `cswap run <slot>` for a slot whose account is the signed-out default login, so cswap starts that slot's own profile instead.
pub(crate) fn route_around_signed_out_default(text: &str) -> String {
    if cfg!(windows) || !text.contains("cswap") {
        return text.to_string();
    }
    let Ok(home) = super::launch::home() else {
        return text.to_string();
    };
    let mut profiles: HashMap<String, Option<String>> = HashMap::new();
    let mut routed = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let Some(slot) = cswap_run_slot(line, indent) else {
            routed.push_str(line);
            continue;
        };
        let profile = profiles
            .entry(slot.clone())
            .or_insert_with(|| signed_out_default_profile(&home, &slot));
        match profile {
            Some(profile) => {
                routed.push_str(&line[..indent]);
                routed.push_str("CLAUDE_CONFIG_DIR=");
                routed.push_str(&crate::agents::quote_shell_arg(profile));
                routed.push(' ');
                routed.push_str(&line[indent..]);
            }
            None => routed.push_str(line),
        }
    }
    routed
}

/// The slot of a line whose first word runs cswap as `cswap run <slot>`.
fn cswap_run_slot(line: &str, from: usize) -> Option<String> {
    let (_, end, word) = crate::agents::command_word(line, from)?;
    if Path::new(&word).file_name()?.to_str()? != "cswap" {
        return None;
    }
    let (_, end, run) = crate::agents::command_word(line, end)?;
    if run != "run" {
        return None;
    }
    let (_, _, slot) = crate::agents::command_word(line, end)?;
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
