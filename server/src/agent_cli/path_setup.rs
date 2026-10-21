//! Putting an installed agent CLI's folder on the user's PATH.

use std::path::Path;

/// Whether a new terminal will find commands in `dir`. Windows reads the live registry PATH; elsewhere the
/// login shell's PATH (sessions run a login shell). An unreadable login PATH counts as "on PATH" so Ghostex
/// never edits a profile on a guess.
/// CDXC:ManagedTools 2026-10-21 WHY: Windows reads only the registry, not gxserver's own PATH: gxserver appends Ghostex's tool folders to that at startup, so the old check always answered "on PATH" and a Settings install never added the tools folder to the user `Path` (bd "not recognized" in every terminal, issue #269).
pub(crate) fn on_path(dir: &Path, home: &Path) -> bool {
    let same = |entry: &Path| {
        let key = |path: &Path| {
            let text = path.to_string_lossy();
            let text = text.trim_end_matches(['\\', '/']).to_string();
            if cfg!(windows) {
                text.to_lowercase()
            } else {
                text
            }
        };
        key(entry) == key(dir)
    };
    #[cfg(windows)]
    {
        let _ = home;
        crate::platform::live_path::new_program_directories()
            .iter()
            .any(|entry| same(entry))
    }
    #[cfg(not(windows))]
    {
        let entries = crate::agent_hooks::probing::login_shell_path(home);
        entries.is_empty() || entries.iter().any(|entry| same(Path::new(entry)))
    }
}

/// CDXC:AgentProviders 2026-09-28 WHY:
/// Claude's official installer (and Cursor's on macOS) leaves the CLI in a folder it never adds to PATH and only prints instructions, so an install from Ghostex ended with `claude` "not recognized" in every new terminal. After a successful install Ghostex adds that folder itself, the way the Codex and Grok installers add theirs: to the user `Path` in the registry on Windows, and as one marked `export PATH` line in the login shell's profile elsewhere. Only folders the catalog lists for the agent are ever added.
/// Returns what was changed, for the job output.
pub(crate) fn ensure_on_path(dir: &Path, home: &Path) -> Result<Option<String>, String> {
    ensure_on_path_at(dir, home, false, "agent CLIs")
}

/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User (1A): the Node.js and uv Ghostex installs itself go at the END of PATH, so commands they provide run in every terminal while the user's own tools keep winning. Windows already appends to the user `Path`.
pub(crate) fn ensure_on_path_end(dir: &Path, home: &Path) -> Result<Option<String>, String> {
    ensure_on_path_at(dir, home, true, "tools")
}

fn ensure_on_path_at(
    dir: &Path,
    home: &Path,
    at_end: bool,
    what: &str,
) -> Result<Option<String>, String> {
    if on_path(dir, home) {
        return Ok(None);
    }
    #[cfg(windows)]
    {
        let _ = (home, at_end, what);
        crate::platform::live_path::add_user_path_directory(dir)?;
        Ok(Some(format!(
            "Added {} to your user PATH so new terminals find it.",
            dir.display()
        )))
    }
    #[cfg(not(windows))]
    {
        let entry = match dir.strip_prefix(home) {
            Ok(rest) => format!("$HOME/{}", rest.display()),
            Err(_) => dir.display().to_string(),
        };
        let line = if at_end {
            format!("export PATH=\"$PATH:{entry}\"")
        } else {
            format!("export PATH=\"{entry}:$PATH\"")
        };
        let profile = append_to_login_profile(
            home,
            &format!("Added by Ghostex so {what} installed in {entry} run in new terminals."),
            &line,
        )
        .map_err(|shell_name| {
            format!(
                "Add {} to PATH in your {shell_name} configuration so new terminals find it.",
                dir.display()
            )
        })?;
        Ok(Some(format!(
            "Added {entry} to PATH in {} so new terminals find it.",
            profile.display()
        )))
    }
}

/// Windows has no login-shell profile; its PATH lives in the registry (`live_path`).
#[cfg(windows)]
pub(crate) fn append_to_login_profile(
    _home: &Path,
    _comment: &str,
    _line: &str,
) -> Result<std::path::PathBuf, String> {
    Err("Windows".into())
}

/// Appends `line` (with a `# comment` above it) to the login shell's profile unless it is already
/// there, and returns the profile. `Err` carries the shell's name when Ghostex does not know its
/// profile (fish, nu, …).
#[cfg(not(windows))]
pub(crate) fn append_to_login_profile(
    home: &Path,
    comment: &str,
    line: &str,
) -> Result<std::path::PathBuf, String> {
    let shell = crate::platform::shell::user_login_shell_path();
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let profile = match shell_name.as_str() {
        "zsh" => home.join(".zprofile"),
        "bash" if cfg!(target_os = "macos") || home.join(".bash_profile").exists() => {
            home.join(".bash_profile")
        }
        "bash" | "sh" | "dash" | "ksh" => home.join(".profile"),
        _ => return Err(shell_name),
    };
    let existing = std::fs::read_to_string(&profile).unwrap_or_default();
    if !existing.lines().any(|current| current.trim() == line) {
        let separator = if existing.is_empty() || existing.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let block = format!("{separator}\n# {comment}\n{line}\n");
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&profile)
            .and_then(|mut file| file.write_all(block.as_bytes()))
            .map_err(|error| format!("Could not update {}: {error}", profile.display()))
            .map_err(|_| shell_name.clone())?;
    }
    crate::agent_hooks::probing::refresh_cli_environment(home);
    Ok(profile)
}
