//! The tools Ghostex can install for the user, and what every one of them reports.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ToolId {
    Node,
    Uv,
    Homebrew,
    SystemTools,
    Beads,
    Gh,
    Glab,
    PowerShell,
}

impl ToolId {
    pub(crate) const ALL: [ToolId; 8] = [
        ToolId::Node,
        ToolId::Uv,
        ToolId::Homebrew,
        ToolId::SystemTools,
        ToolId::Beads,
        ToolId::Gh,
        ToolId::Glab,
        ToolId::PowerShell,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            ToolId::Node => "node",
            ToolId::Uv => "uv",
            ToolId::Homebrew => "homebrew",
            ToolId::SystemTools => "systemTools",
            ToolId::Beads => "beads",
            ToolId::Gh => "gh",
            ToolId::Glab => "glab",
            ToolId::PowerShell => "powershell",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.id() == value)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ToolId::Node => "Node.js and npm",
            ToolId::Uv => "uv",
            ToolId::Homebrew => "Homebrew",
            ToolId::SystemTools => "System tools",
            ToolId::Beads => "Beads",
            ToolId::Gh => "GitHub CLI",
            ToolId::Glab => "GitLab CLI",
            ToolId::PowerShell => "PowerShell 7",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            ToolId::Node => "Runs agent CLIs that install with npm, such as Gemini, Pi and Qoder.",
            ToolId::Uv => {
                "Python package installer from Astral. Ghostex uses it to install Claude Swap for switching Claude accounts, and it downloads the Python Claude Swap needs. uv and uvx also work in your terminals."
            }
            ToolId::Homebrew => {
                "The Mac package manager. Ghostex installs it when an install you choose runs through Homebrew."
            }
            ToolId::SystemTools => {
                "curl, certificates, unzip and git, which agent installers need on Linux."
            }
            ToolId::Beads => "The bd command behind the Project board.",
            ToolId::Gh => "Lets Add Project clone and list your GitHub repositories.",
            ToolId::Glab => "Lets Add Project clone and list your GitLab repositories.",
            ToolId::PowerShell => {
                "The current PowerShell. New terminals on Windows use it instead of Windows PowerShell 5.1."
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Source {
    Ghostex,
    System,
}

impl Source {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Source::Ghostex => "ghostex",
            Source::System => "system",
        }
    }
}

/// What one tool looks like on this computer right now.
pub(crate) struct Status {
    /// `Err(reason)` when this platform cannot have the tool at all.
    pub supported: Result<(), String>,
    pub executable: Option<PathBuf>,
    pub source: Option<Source>,
    pub version: Option<String>,
    /// Why an install cannot run here even though the platform is supported.
    pub install_blocker: Option<String>,
    /// The install shows a password or administrator prompt.
    pub needs_password: bool,
    /// Tooltip text: exactly how Ghostex installs it.
    pub plan: String,
    /// Run in a terminal tab instead of a background job (a `sudo` that needs a terminal).
    pub terminal_command: Option<String>,
    /// Extra status line (for example which system tools are missing).
    pub detail: Option<String>,
    /// Where to get the tool by hand when Ghostex cannot install it here.
    pub download_url: Option<String>,
    /// Operations that make sense for this install (before blockers are applied).
    pub operations: Vec<&'static str>,
    /// Ghostex's copy exists but a new terminal or agent session can't find it by name.
    pub off_path: bool,
}

impl Status {
    pub(crate) fn unsupported(reason: String) -> Self {
        Self {
            supported: Err(reason),
            executable: None,
            source: None,
            version: None,
            install_blocker: None,
            needs_password: false,
            plan: String::new(),
            terminal_command: None,
            detail: None,
            download_url: None,
            operations: Vec::new(),
            off_path: false,
        }
    }

    pub(crate) fn new(plan: String) -> Self {
        Self {
            supported: Ok(()),
            plan,
            ..Self::unsupported(String::new())
        }
    }
}

/// Resolves `binary` the way sessions will (login-shell PATH, the usual tool folders, Ghostex's
/// own tools), and says whether Ghostex installed that copy.
pub(crate) fn locate(binary: &str, home: &Path) -> Option<(PathBuf, Source)> {
    let path = crate::agent_hooks::probing::resolve_cli_command(binary, home)
        .map(PathBuf::from)
        .or_else(|| {
            super::paths::path_dirs()
                .into_iter()
                .map(|dir| dir.join(super::paths::executable_name(binary)))
                .find(|path| path.is_file())
        })?;
    let source = if super::paths::is_ghostex_owned(&path) {
        Source::Ghostex
    } else {
        Source::System
    };
    Some((path, source))
}

/// Whether `binary` runs by bare name in a new terminal or agent session, the way the board skills call
/// `bd`. Unlike `locate` it leaves out the folders only gxserver searches. An unreadable login PATH
/// counts as reachable, the same as `path_setup::on_path`.
pub(crate) fn reachable(binary: &str, home: &Path) -> bool {
    #[cfg(windows)]
    let directories = {
        let _ = home;
        crate::platform::live_path::new_program_directories()
    };
    #[cfg(not(windows))]
    let directories: Vec<PathBuf> = {
        let entries = crate::agent_hooks::probing::login_shell_path(home);
        if entries.is_empty() {
            return true;
        }
        entries.into_iter().map(PathBuf::from).collect()
    };
    crate::platform::live_path::find_in(binary, &directories).is_some()
}

/// `<program> --version`, reduced to its first dotted version number.
pub(crate) fn version_of(executable: &Path, args: &[&str]) -> Option<String> {
    let mut command = crate::platform::process::background_command(executable);
    command.args(args).stdin(Stdio::null());
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            &ghostex_paths::GhostexPaths::resolve().home_dir,
        ),
    );
    let output = command.output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    version_in(&text)
}

pub(crate) fn version_in(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
        .map(|token| token.trim_start_matches('v'))
        .find(|token| {
            token.starts_with(|c: char| c.is_ascii_digit())
                && token.contains('.')
                && token
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        })
        .map(str::to_string)
}

pub(crate) fn newer(latest: &str, current: &str) -> bool {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split(['.', '-', '+'])
            .map_while(|part| part.parse::<u64>().ok())
            .collect()
    };
    parts(latest) > parts(current)
}

const LATEST_TTL: Duration = Duration::from_secs(6 * 60 * 60);
static LATEST: LazyLock<Mutex<HashMap<ToolId, (Instant, Result<String, String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The newest release of `tool`, cached for six hours; `fresh` is a "check again" click.
pub(crate) fn latest_version(tool: ToolId, fresh: bool) -> Option<Result<String, String>> {
    if !fresh {
        if let Some((at, answer)) = LATEST.lock().ok()?.get(&tool) {
            if at.elapsed() < LATEST_TTL {
                return Some(answer.clone());
            }
        }
    }
    let answer = match tool {
        ToolId::Node => super::node::latest_lts().map(|(version, _)| version),
        ToolId::Uv => super::uv::latest_version(),
        ToolId::Beads | ToolId::Gh | ToolId::Glab => super::binaries::latest_version(tool),
        ToolId::Homebrew => super::homebrew::latest_homebrew_version(),
        ToolId::SystemTools | ToolId::PowerShell => return None,
    };
    if let Ok(mut cache) = LATEST.lock() {
        cache.insert(tool, (Instant::now(), answer.clone()));
    }
    Some(answer)
}

pub(crate) fn forget_latest(tool: ToolId) {
    if let Ok(mut cache) = LATEST.lock() {
        cache.remove(&tool);
    }
}
