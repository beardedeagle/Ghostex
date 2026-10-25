use super::rpc::{CliError, CliResult};
use crate::accounts::helpers;
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::process::Stdio;
use std::{path::Path, process::Command};

const HELP: &str = "Usage: ghostex account-login claude [--account NUMBER] [--email EMAIL] [--json]\n\nSign in with a separate Claude profile and save it with cswap.\nWithout --account, saves the account you choose in the browser; --email pre-fills it and requires that account.\nWith --account, reconnects only that saved account.";

/// CDXC:AgentProviders 2026-09-07 WHY:
/// Running `claude auth login && cswap add` against the active profile refreshed the last-used account when browser login reused its identity. New logins need a fresh profile and an identity check before cswap captures anything; reconnect must match the selected saved identity.
/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User picked "Add account without typing the email" for Settings > Accounts. A new account no longer asks for an email: the browser sign-in decides which account is saved, and an optional --email only pre-fills it and requires that account. Reconnect always pre-fills and requires the saved account's own email.
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{HELP}");
        return Ok(());
    }
    let json_output = args.iter().any(|arg| arg == "--json");
    let email_index = args.iter().position(|arg| arg == "--email");
    let supplied_email = email_index.and_then(|index| args.get(index + 1)).cloned();
    if email_index.is_some() && supplied_email.is_none() {
        return Err(failure("--email requires an address"));
    }
    let filtered: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| {
            arg.as_str() != "--json"
                && email_index
                    .is_none_or(|email_index| *index != email_index && *index != email_index + 1)
        })
        .map(|(_, arg)| arg.clone())
        .collect();
    let account = match filtered.as_slice() {
        [provider] if provider == "claude" => None,
        [provider, flag, slot]
            if provider == "claude"
                && flag == "--account"
                && slot.parse::<u64>().is_ok_and(|number| number > 0) =>
        {
            Some(slot.as_str())
        }
        _ => return Err(failure(HELP)),
    };
    // Windows has no HOME; USERPROFILE is the home the account helpers themselves resolve.
    let home = ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .find(|path| path.is_absolute())
        .ok_or_else(|| failure("The home directory is unavailable."))?;
    let claude = helpers::executable(&home, "claude")
        .ok_or_else(|| failure("Install Claude Code first."))?;
    let cswap =
        helpers::executable(&home, "cswap").ok_or_else(|| failure("Install claude-swap first."))?;
    println!("Reading saved Claude accounts...");
    let roster = helpers::json_command(&home, "cswap", &["list", "--json"]).map_err(failure)?;
    let rows = roster["accounts"].as_array().ok_or_else(|| {
        failure("cswap returned no account list. Update claude-swap and try again.")
    })?;
    let target = account
        .map(|slot| {
            rows.iter()
                .find(|row| row["number"].as_u64().map(|n| n.to_string()).as_deref() == Some(slot))
                .ok_or_else(|| {
                    failure("The selected saved account no longer exists. Refresh Accounts first.")
                })
        })
        .transpose()?;
    let email: Option<String> = if let Some(target) = target {
        Some(
            target["email"]
                .as_str()
                .filter(|email| !email.is_empty())
                .ok_or_else(|| {
                    failure("The saved account has no email identity. Refresh Accounts first.")
                })?
                .to_string(),
        )
    } else {
        supplied_email.map(|email| email.trim().to_string())
    };
    if email
        .as_deref()
        .is_some_and(|email| !email.contains('@') || email.chars().any(char::is_whitespace))
    {
        return Err(failure(
            "Enter the email address of the Claude account you want to connect.",
        ));
    }
    // Use a private ASCII path on macOS so the profile's Keychain service hash is unambiguous.
    let mut builder = tempfile::Builder::new();
    builder.prefix("ghostex-claude-login-");
    #[cfg(target_os = "macos")]
    let directory = builder.tempdir_in("/private/tmp");
    #[cfg(not(target_os = "macos"))]
    let directory = builder.tempdir();
    let profile = LoginProfile(directory.map_err(|e| failure(e.to_string()))?);
    println!(
        "Choose the requested account in the browser. If another account appears, choose a different account before authorizing."
    );
    let mut login = profile.command(&claude);
    login.args(["auth", "login", "--claudeai"]);
    if let Some(email) = &email {
        login.args(["--email", email]);
    }
    let status = login
        .status()
        .map_err(|e| failure(format!("Could not start Claude login: {e}")))?;
    if !status.success() {
        return Err(failure(
            "Claude login was cancelled or failed. No saved account was changed.",
        ));
    }
    let config: Value = serde_json::from_slice(
        &std::fs::read(profile.0.path().join(".claude.json")).map_err(|_| {
            failure("Claude did not save the new profile's identity. No saved account was changed.")
        })?,
    )
    .map_err(|_| failure("Claude's new profile identity could not be read."))?;
    let identity = &config["oauthAccount"];
    let signed_in_email = identity["emailAddress"].as_str().unwrap_or_default();
    let organization = identity["organizationUuid"].as_str().unwrap_or_default();
    if signed_in_email.is_empty() {
        return Err(failure(
            "Claude did not report which account signed in. No saved account was changed.",
        ));
    }
    // SEE-ALSO: `wrong_account` in server/src/accounts/setup.rs reads this sentence.
    if let Some(email) = email
        .as_deref()
        .filter(|email| !signed_in_email.eq_ignore_ascii_case(email))
    {
        return Err(failure(format!(
            "The browser signed in to {signed_in_email} instead of {email}. Run login again and choose the requested account. No saved account was changed."
        )));
    }
    let matches_identity = |row: &Value| {
        row["email"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(signed_in_email))
            && row["organizationUuid"].as_str().unwrap_or_default() == organization
    };
    // Login is interactive; another account may have been saved or moved while the browser was open.
    let latest_roster =
        helpers::json_command(&home, "cswap", &["list", "--json"]).map_err(failure)?;
    let latest_rows = latest_roster["accounts"].as_array().ok_or_else(|| {
        failure("cswap's account list could not be checked. No saved account was changed.")
    })?;
    if let Some(target) = target {
        let still_selected = latest_rows
            .iter()
            .any(|row| row["number"] == target["number"] && matches_identity(row));
        if !matches_identity(target) || !still_selected {
            return Err(failure(
                "The login belongs to a different organization than the selected account. No saved account was changed.",
            ));
        }
    } else if latest_rows.iter().any(&matches_identity) {
        return Err(failure(
            "This account is already saved. Use its Reconnect action in Settings, or run login again with a different account. No saved account was changed.",
        ));
    }
    let status = profile
        .command(&cswap)
        .arg("add")
        .status()
        .map_err(|e| failure(format!("Could not save the login with cswap: {e}")))?;
    if !status.success() {
        return Err(failure(
            "cswap could not save this login. Check the message above and try again.",
        ));
    }
    if json_output {
        let saved = helpers::json_command(&home, "cswap", &["list", "--json"]).map_err(failure)?;
        let saved = saved["accounts"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| matches_identity(row)))
            .ok_or_else(|| failure("The saved login could not be located."))?;
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"account":{"number":saved["number"],"email":saved["email"]}})
        );
    } else {
        println!(
            "Login saved. Ghostex will finish connecting this account automatically when login was started in Settings."
        );
    }
    Ok(())
}

struct LoginProfile(tempfile::TempDir);
impl LoginProfile {
    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
        ] {
            command.env_remove(key);
        }
        command
            .env("CLAUDE_CONFIG_DIR", self.0.path())
            .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", self.0.path());
        command
    }
}
impl Drop for LoginProfile {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use sha2::{Digest, Sha256};
            let hash = hex::encode(Sha256::digest(self.0.path().as_os_str().as_encoded_bytes()));
            let _ = Command::new("/usr/bin/security")
                .args([
                    "delete-generic-password",
                    "-s",
                    &format!("Claude Code-credentials-{}", &hash[..8]),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}
fn failure(message: impl Into<String>) -> CliError {
    CliError::Other(message.into())
}
