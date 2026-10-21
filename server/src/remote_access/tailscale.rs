use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/*
CDXC:RemotePairing 2026-09-03:
Tailscale is the second phone path (SSH over the tailnet). gxserver only READS
the local client's state through `tailscale status --json`; it never installs,
signs in, or changes Tailscale settings. A missing binary is a reported status
the UI turns into "Install Tailscale", not an error.
*/
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub account: Option<String>,
    pub magic_dns_name: Option<String>,
    pub ip: Option<String>,
    pub ssh_enabled: Option<bool>,
    /// What the `tailscale status --json` probe printed when it could not report a status (exit code and output), so a "not connected" report can be diagnosed from `/api/remoteAccessStatus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_error: Option<String>,
}

impl TailscaleStatus {
    fn not_installed() -> Self {
        Self {
            installed: false,
            running: false,
            account: None,
            magic_dns_name: None,
            ip: None,
            ssh_enabled: None,
            probe_error: None,
        }
    }

    fn installed_not_running() -> Self {
        Self {
            installed: true,
            ..Self::not_installed()
        }
    }

    fn probe_failed(error: String) -> Self {
        Self {
            probe_error: Some(error),
            ..Self::installed_not_running()
        }
    }

    /// True when a phone can reach this computer over the tailnet by name or IP.
    pub fn reachable_address(&self) -> Option<(Option<String>, Option<String>)> {
        if !self.running || (self.magic_dns_name.is_none() && self.ip.is_none()) {
            return None;
        }
        Some((self.magic_dns_name.clone(), self.ip.clone()))
    }
}

/// CDXC:RemotePairing 2026-10-21 WHY:
/// The macOS Tailscale.app binary only behaves as a CLI when `TERM`, `SHLVL` or `TAILSCALE_BE_CLI` is set; otherwise it tries to start the GUI, prints "The Tailscale GUI failed to start ... (Tailscale.CLIError error 3.)" on stdout and exits 0. gxserver runs as a launchd daemon without a shell, so Macs without the `/usr/local/bin/tailscale` shell wrapper (which sets `SHLVL`) saw a connected Tailscale as "installed but not connected" (GitHub #268); `TAILSCALE_BE_CLI=1` is Tailscale's own switch for this, and the Homebrew, Linux and Windows CLIs run the same with or without it. Each installed CLI is asked in turn until one reports a running backend, because a Homebrew or standalone CLI earlier on PATH can talk to a different (stopped) daemon than the app the user signed in to.
pub fn read_tailscale_status() -> TailscaleStatus {
    let binaries = installed_tailscale_binaries();
    if binaries.is_empty() {
        return TailscaleStatus::not_installed();
    }
    let mut first_status = None;
    let mut errors = Vec::new();
    for binary in binaries {
        match probe_tailscale_binary(&binary) {
            Ok(status) if status.running => return status,
            Ok(status) => {
                first_status.get_or_insert(status);
            }
            Err(error) => errors.push(format!("{}: {error}", binary.display())),
        }
    }
    first_status.unwrap_or_else(|| TailscaleStatus::probe_failed(errors.join("; ")))
}

fn probe_tailscale_binary(binary: &Path) -> Result<TailscaleStatus, String> {
    let output = crate::platform::process::background_command(binary)
        .args(["status", "--json"])
        .env("TAILSCALE_BE_CLI", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("could not run: {error}"))?;
    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(status) => Ok(tailscale_status_from_json(&status)),
        Err(_) => {
            let printed = [&output.stderr, &output.stdout]
                .iter()
                .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" | ");
            let printed: String = printed.chars().take(400).collect();
            Err(format!("{}, printed: {printed}", output.status))
        }
    }
}

fn tailscale_status_from_json(status: &Value) -> TailscaleStatus {
    let running = status.get("BackendState").and_then(Value::as_str) == Some("Running");
    if !running {
        return TailscaleStatus::installed_not_running();
    }
    let node = status.get("Self");
    let magic_dns_name = node
        .and_then(|node| node.get("DNSName"))
        .and_then(Value::as_str)
        .map(|name| name.trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty());
    let ip = node
        .and_then(|node| node.get("TailscaleIPs"))
        .and_then(Value::as_array)
        .and_then(|ips| ips.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    let ssh_enabled = node
        .and_then(|node| node.get("SSH_HostKeys"))
        .and_then(Value::as_array)
        .map(|keys| !keys.is_empty())
        .unwrap_or(false);
    let login_name = node
        .and_then(|node| node.get("UserID"))
        .and_then(Value::as_i64)
        .and_then(|user_id| {
            status
                .get("User")
                .and_then(|users| users.get(user_id.to_string()))
                .and_then(|user| user.get("LoginName"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);
    let tailnet_name = status
        .get("CurrentTailnet")
        .and_then(|tailnet| tailnet.get("Name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    TailscaleStatus {
        installed: true,
        running: true,
        account: login_name.or(tailnet_name),
        magic_dns_name,
        ip,
        ssh_enabled: Some(ssh_enabled),
        probe_error: None,
    }
}

fn installed_tailscale_binaries() -> Vec<PathBuf> {
    let mut binaries: Vec<PathBuf> = Vec::new();
    for candidate in tailscale_binary_candidates() {
        if is_executable_file(&candidate) && !binaries.contains(&candidate) {
            binaries.push(candidate);
        }
    }
    binaries
}

fn tailscale_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates
            .extend(env::split_paths(&path).map(|directory| directory.join(executable_name())));
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ));
    }
    if cfg!(windows) {
        candidates.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
    } else {
        candidates.push(PathBuf::from("/usr/bin/tailscale"));
    }
    candidates
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "tailscale.exe"
    } else {
        "tailscale"
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
