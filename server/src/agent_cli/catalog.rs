use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Definition {
    pub agent_id: String,
    pub binary: String,
    pub mise_tool: Option<String>,
    pub npm_package: Option<String>,
    #[serde(default)]
    pub npm_flags: Vec<String>,
    pub package_managers: Option<Vec<String>>,
    pub brew_formula: Option<String>,
    #[serde(default)]
    pub brew_cask: bool,
    pub winget_id: Option<String>,
    pub native: Option<Native>,
    pub version_args: Option<Vec<String>>,
    #[serde(default)]
    pub install_dirs: InstallDirs,
    pub latest_version: Option<LatestVersion>,
    /// A sentence every install method's tooltip adds (Grok and Cursor both install `agent`).
    pub install_note: Option<String>,
    /// The update kills running processes of the CLI (Droid's installer runs `pkill droid`).
    #[serde(default)]
    pub update_stops_sessions: bool,
    /// The CLI's own updater for an agent without an official installer entry (`copilot update`).
    pub self_update: Option<String>,
    /// Arguments of the CLI's own "is there an update" check, for an agent whose version number does not
    /// move with its releases (Hermes updates by git commit).
    pub update_check: Option<Vec<String>>,
    /// The oldest Bun the bun-installed package runs on (OMP).
    pub minimum_bun_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Native {
    install: String,
    update: Option<String>,
    windows_install: Option<String>,
    /// CDXC:AgentProviders 2026-10-25 WHY:
    /// The Windows update when the CLI's own updater breaks there: OpenCode 2.0.16's `opencode upgrade` hands bash a Windows path with its backslashes stripped (`/bin/bash: C:Usersmadda.cacheopencode…install: No such file`), so on Windows OpenCode updates by running its install script again, which installs the latest release over `~/.opencode/bin`.
    windows_update: Option<String>,
    /// Variables the official installer and updater need, such as `CODEX_NON_INTERACTIVE`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Lowercase, `/`-separated path fragments that only this agent's official installer produces.
    #[serde(default)]
    path_markers: Vec<String>,
    /// Commands the official installer needs besides curl, per platform family.
    #[serde(default)]
    requires: InstallDirs,
    /// The macOS installer copies an app into /Applications (Kiro).
    #[serde(default)]
    needs_applications_folder: bool,
    /// Tooltip wording for an inline script that is not worth showing verbatim (Rovo Dev's download).
    plan: Option<String>,
}

/// Folders the official installer puts the binary in, `~` and `%VAR%` expanded. An installer that does not
/// add its folder to PATH (Claude's) still leaves a CLI Ghostex can find, and put on PATH.
#[derive(Default, Deserialize)]
pub(crate) struct InstallDirs {
    #[serde(default)]
    windows: Vec<String>,
    #[serde(default)]
    unix: Vec<String>,
}

impl Native {
    /// The update command on this platform: `windowsUpdate` on Windows when the catalog has one.
    fn update_command(&self) -> Option<&str> {
        cfg!(windows)
            .then_some(self.windows_update.as_deref())
            .flatten()
            .or(self.update.as_deref())
    }
}

/// Where the newest published version is read: a plain-text version, or one field of a JSON reply.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestVersion {
    pub url: String,
    pub json_field: Option<String>,
}

impl Definition {
    pub(crate) fn install_dirs(&self, home: &Path) -> Vec<PathBuf> {
        let dirs = if cfg!(windows) {
            &self.install_dirs.windows
        } else {
            &self.install_dirs.unix
        };
        dirs.iter()
            .filter_map(|dir| expand_dir(dir, home))
            .collect()
    }

    /// The command the CLI updates itself with, whichever way it was installed.
    pub(crate) fn self_updater(&self) -> Option<&str> {
        self.native
            .as_ref()
            .and_then(Native::update_command)
            .or(self.self_update.as_deref())
    }

    pub(crate) fn native_env(&self) -> BTreeMap<String, String> {
        self.native
            .as_ref()
            .map(|native| native.env.clone())
            .unwrap_or_default()
    }
}

fn expand_dir(template: &str, home: &Path) -> Option<PathBuf> {
    let mut text = template.to_string();
    if let Some(rest) = text.strip_prefix("~/") {
        text = format!("{}/{rest}", home.to_string_lossy());
    }
    while let Some(start) = text.find('%') {
        let end = start + 1 + text[start + 1..].find('%')?;
        let value = std::env::var(&text[start + 1..end]).ok()?;
        text.replace_range(start..=end, &value);
    }
    let path = if cfg!(windows) {
        PathBuf::from(text.replace('/', "\\"))
    } else {
        PathBuf::from(text)
    };
    path.is_absolute().then_some(path)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Method {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Tooltip text: exactly what one click does, including anything Ghostex installs first.
    pub plan: String,
    /// A tool Ghostex installs before running `command` ("node", "homebrew" or "systemTools").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite: Option<String>,
    /// Commands `systemTools` must provide first (Linux).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub system_tools: Vec<String>,
}

impl Method {
    fn new(id: &str, label: &str, command: String, plan: String) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            command,
            unavailable_reason: None,
            plan,
            prerequisite: None,
            system_tools: Vec::new(),
        }
    }
}

pub(crate) static CATALOG: LazyLock<Vec<Definition>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../packages/shared/agent-cli-catalog.json"
    ))
    .expect("valid bundled CLI catalog")
});

/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User: "when they click on something, we help them install it on windows/macos/linux automatically (show a button with a tooltip explaining how we'll install) but 1 click installs it for them as much as possible". Every method carries its tooltip `plan`, and a method whose tool is missing but that Ghostex can install (npm through its own Node.js, Homebrew on a Mac, curl/unzip/git on Linux) stays available with that tool as its `prerequisite` instead of asking the user to install it first.
pub(crate) fn methods(
    definition: &Definition,
    executable: Option<&str>,
    home: &Path,
    mise_installation: Option<&super::mise::Installation>,
) -> Vec<Method> {
    let installed = executable.is_some();
    let mut methods = Vec::new();
    let note = |plan: String| -> String {
        let mut plan = plan;
        if let Some(note) = &definition.install_note {
            plan.push(' ');
            plan.push_str(note);
        }
        if installed && definition.update_stops_sessions {
            plan.push_str(" Updating stops sessions that are running it.");
        }
        plan
    };
    // A package published for bun only (omp's starts with `#!/usr/bin/env bun`) does not run from mise's npm install.
    let npm_runnable = definition
        .package_managers
        .as_ref()
        .is_none_or(|managers| managers.iter().any(|manager| manager == "npm"));
    let mise_tool = mise_installation
        .and_then(|installation| installation.tool.clone())
        .or_else(|| definition.mise_tool.clone())
        .or_else(|| {
            definition
                .npm_package
                .as_ref()
                .filter(|_| npm_runnable)
                .map(|package| format!("npm:{package}"))
        });
    if let Some(tool) = mise_tool {
        let managed = mise_installation.is_some_and(|installation| installation.tool.is_some());
        let command = super::mise::command(&tool, managed);
        let mut method = Method::new(
            "mise",
            "mise",
            command.clone(),
            note(format!("Runs {command} with your mise.")),
        );
        method.unavailable_reason = missing_command("mise", home).or_else(|| {
            (!managed)
                .then(|| super::mise::unavailable_reason(&tool, home))
                .flatten()
        });
        methods.push(method);
    }
    if let Some(native) = &definition.native {
        let install = if cfg!(windows) {
            native.windows_install.as_deref()
        } else {
            Some(native.install.as_str())
        };
        if let Some(install) = install {
            let command = if installed {
                native.update_command().unwrap_or(install)
            } else {
                install
            };
            let mut method = Method::new(
                "native",
                "Official installer",
                command.into(),
                note(match &native.plan {
                    Some(plan) => plan.clone(),
                    None => format!("Runs the official installer: {command}. No password needed."),
                }),
            );
            if installed
                && command != install
                && !command.starts_with("curl ")
                && !command.contains("irm ")
            {
                // The CLI's own updater.
                method.unavailable_reason = None;
            } else {
                apply_native_requirements(&mut method, native, command, home);
            }
            methods.push(method);
        }
    }
    if let Some(package) = &definition.npm_package {
        let managers = definition
            .package_managers
            .clone()
            .unwrap_or_else(|| vec!["npm".into(), "bun".into(), "pnpm".into()]);
        for manager in managers {
            let verb = if manager == "pnpm" { "add" } else { "install" };
            let flags = if manager == "npm" && !definition.npm_flags.is_empty() {
                format!("{} ", definition.npm_flags.join(" "))
            } else {
                String::new()
            };
            let command = format!("{manager} {verb} -g {flags}{package}@latest");
            let mut method = Method::new(
                &manager,
                &manager,
                command.clone(),
                note(format!("Runs {command}.")),
            );
            let missing = missing_command(&manager, home);
            if manager == "npm" && missing.is_some() {
                match crate::managed_tools::node::supported() {
                    Ok(()) => {
                        method.prerequisite = Some("node".into());
                        method.plan = note(format!(
                            "Ghostex first downloads Node.js (LTS) from nodejs.org into its tools folder and adds it to the end of your PATH, then runs {command}. No password needed."
                        ));
                    }
                    Err(reason) => method.unavailable_reason = Some(reason),
                }
            } else {
                method.unavailable_reason = missing;
            }
            if manager == "bun" && method.unavailable_reason.is_none() {
                method.unavailable_reason = definition
                    .minimum_bun_version
                    .as_deref()
                    .and_then(|minimum| bun_too_old(&definition.binary, minimum, home));
            }
            methods.push(method);
        }
    }
    if !cfg!(windows) {
        if let Some(formula) = &definition.brew_formula {
            let installed_formula = executable.and_then(brew_package);
            let formula = installed_formula
                .as_deref()
                .filter(|name| {
                    name.starts_with(&format!(
                        "{}@",
                        formula.rsplit('/').next().unwrap_or(formula)
                    ))
                })
                .unwrap_or(formula);
            let verb = if installed { "upgrade" } else { "install" };
            let cask = if definition.brew_cask { "--cask " } else { "" };
            let command = format!("brew {verb} {cask}{formula}");
            let mut method = Method::new(
                "brew",
                "Homebrew",
                command.clone(),
                note(format!("Runs {command} with Homebrew.")),
            );
            if let Some(missing) = missing_command("brew", home) {
                match crate::managed_tools::homebrew::can_install_homebrew() {
                    Ok(()) if !installed => {
                        method.prerequisite = Some("homebrew".into());
                        method.plan = note(format!(
                            "Ghostex first installs Homebrew with its official installer (macOS asks for your password once, and Apple's Command Line Tools are installed if missing), then runs {command}."
                        ));
                    }
                    Ok(()) => method.unavailable_reason = Some(missing),
                    Err(reason) => {
                        method.unavailable_reason = Some(if cfg!(target_os = "macos") {
                            reason
                        } else {
                            missing
                        })
                    }
                }
            }
            methods.push(method);
        }
    }
    if cfg!(windows) {
        if let Some(package) = &definition.winget_id {
            let verb = if installed { "upgrade" } else { "install" };
            let command = format!("winget {verb} --id {package} --exact --disable-interactivity");
            let mut method = Method::new(
                "winget",
                "WinGet",
                command.clone(),
                note(format!("Runs {command}.")),
            );
            method.unavailable_reason = missing_command("winget", home);
            methods.push(method);
        }
    }
    methods
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// OMP runs on Bun and refuses a Bun older than 1.3.14 (phase 0 hit 1.3.12 on Windows). Bun is not a tool Ghostex installs for the user, so like git outside Linux the bun method names what to update and how instead of failing halfway; the official installer always downloads OMP's own binary (`--binary`), which needs no Bun.
fn bun_too_old(binary: &str, minimum: &str, home: &Path) -> Option<String> {
    let bun = super::process::resolve("bun", home)?;
    let mut command = crate::platform::process::background_command(&bun);
    command.arg("--version").current_dir(home);
    let current = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        std::time::Duration::from_secs(3),
    )
    .and_then(|text| super::latest::version_in(&text))?;
    super::latest::is_newer(minimum, &current).then(|| {
        format!("{binary} needs Bun {minimum} or newer, and this computer has Bun {current}. Run bun upgrade in a terminal, then try again, or use the official installer, which needs no Bun.")
    })
}

/// The CLI's own updater, offered when Ghostex cannot tell how the CLI was installed.
pub(crate) fn self_method(definition: &Definition) -> Option<Method> {
    let command = definition.self_updater()?;
    Some(Method::new(
        "self",
        "Built-in updater",
        command.into(),
        format!("Runs {command}, the CLI's own updater, which updates it the way it was installed. No password needed."),
    ))
}

/// What the official installer needs on this computer, and what Ghostex can install for it.
fn apply_native_requirements(method: &mut Method, native: &Native, command: &str, home: &Path) {
    let mut wanted: Vec<&str> = Vec::new();
    if command.starts_with("curl ") {
        wanted.extend(["curl", "ca-certificates"]);
    }
    let extra = if cfg!(windows) {
        &native.requires.windows
    } else {
        &native.requires.unix
    };
    wanted.extend(extra.iter().map(String::as_str));
    if native.needs_applications_folder && cfg!(target_os = "macos") && !applications_writable() {
        method.unavailable_reason = Some("This installer copies an app into /Applications, which this account can't change. Sign in as an administrator, or ask one to install it.".into());
        return;
    }
    if cfg!(target_os = "linux") {
        let missing = crate::managed_tools::system_tools::missing(home, &wanted);
        if missing.is_empty() {
            return;
        }
        let list = missing
            .iter()
            .map(|name| {
                if *name == "ca-certificates" {
                    "certificates"
                } else {
                    name
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        if crate::managed_tools::system_tools::can_install_in_background() {
            method.prerequisite = Some("systemTools".into());
            method.system_tools = missing.iter().map(|name| name.to_string()).collect();
            method.plan = format!(
                "Ghostex first installs {list} with your package manager (your computer asks for your password once), then {}",
                lowercase_first(&method.plan)
            );
        } else {
            method.unavailable_reason = Some(format!(
                "This installer needs {list}. Use Install system tools in Settings > Integrations > Tools (it opens a terminal that asks for your password), then try again."
            ));
        }
        return;
    }
    for &tool in &wanted {
        if tool == "ca-certificates" || crate::agent_hooks::probing::command_exists(tool, home) {
            continue;
        }
        // CDXC:ManagedTools 2026-09-29 DECISION:
        // User (8B): when git is missing outside Linux (Hermes on a Mac without Apple's Command Line Tools, Claude Code on Windows without Git for Windows), show a clear "install git first" message instead of installing it.
        method.unavailable_reason = Some(match (tool, cfg!(target_os = "macos")) {
            ("git", true) => "This agent needs git. Install Apple's Command Line Tools first (run xcode-select --install in a terminal), then try again.".into(),
            ("git", false) => "This agent needs Git for Windows. Install it from git-scm.com/downloads/win, then try again.".into(),
            _ => format!("This installer needs {tool}. Install it first, then try again."),
        });
        return;
    }
    if cfg!(target_os = "macos") && wanted.contains(&"git") && !mac_git_usable() {
        method.unavailable_reason = Some("This agent needs git. Install Apple's Command Line Tools first (run xcode-select --install in a terminal), then try again.".into());
    }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(unix)]
fn applications_writable() -> bool {
    let path = std::ffi::CString::new("/Applications").expect("static path");
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn applications_writable() -> bool {
    true
}

/// `/usr/bin/git` on a Mac is a stub that opens Apple's installer dialog until the Command Line
/// Tools (or Xcode) are installed; a git from Homebrew works either way.
fn mac_git_usable() -> bool {
    std::process::Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        || ["/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .iter()
            .any(|path| Path::new(path).is_file())
}

/// Prerequisites (npm, curl, brew, winget, mise) come from the shared 60 second probe cache: a list of every
/// agent asks for the same few tools many times over.
fn missing_command(command: &str, home: &Path) -> Option<String> {
    (!crate::agent_hooks::probing::command_exists(command, home))
        .then(|| format!("Install {command} on this computer first."))
}

fn brew_package(path: &str) -> Option<String> {
    let path = std::fs::canonicalize(path)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let (_, rest) = path
        .split_once("/caskroom/")
        .or_else(|| path.split_once("/cellar/"))?;
    let name = rest.split('/').next()?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._-+".contains(&byte)))
    .then(|| name.to_string())
}

/// CDXC:AgentProviders 2026-09-14 WHY:
/// A global npm install can leave a second CLI behind a Homebrew or native binary on PATH.
/// Resolve symlinks before choosing an updater. An unknown installation never gets a package manager's updater (that installs a second copy); since 2026-10-17 it updates through the CLI's own updater when it has one (`copilot update`, `pi update self`, see `Definition::self_updater`), which knows how it was installed, and only a CLI without one needs an explicit method selection.
pub(crate) fn detected_method(path: &str, definition: &Definition) -> Option<String> {
    if super::mise::looks_managed(path) {
        // Unresolved shims must not offer a native self-updater that would modify mise's installation.
        return Some("manual".into());
    }
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    let normalized = real.to_string_lossy().replace('\\', "/").to_lowercase();
    let original = path.replace('\\', "/").to_lowercase();
    if normalized.contains("/winget/") || original.contains("/winget/") {
        return Some("winget".into());
    }
    if normalized.contains("/scoop/")
        || original.contains("/scoop/")
        || normalized.contains("/nix/store/")
    {
        return Some("manual".into());
    }
    if normalized.contains("/cellar/") || normalized.contains("/caskroom/") {
        return definition.brew_formula.as_ref().map(|_| "brew".into());
    }
    if definition.npm_package.is_some() {
        // On Windows npm leaves only a `.cmd` shim beside Ghostex's node.exe, which never resolves
        // into node_modules.
        if Path::new(path).starts_with(crate::managed_tools::paths::node_dir()) {
            return Some("npm".into());
        }
        if original.contains("/.bun/") || normalized.contains("/.bun/") {
            return Some("bun".into());
        }
        if original.contains("/pnpm/") || normalized.contains("/pnpm/") {
            return Some("pnpm".into());
        }
        if normalized.contains("/node_modules/")
            || (cfg!(windows) && original.contains("/appdata/roaming/npm/"))
        {
            return Some("npm".into());
        }
    }
    if let Some(native) = &definition.native {
        if native.path_markers.iter().any(|marker| {
            normalized.contains(marker.as_str()) || original.contains(marker.as_str())
        }) {
            return Some("native".into());
        }
    }
    if definition.native.is_some()
        && [
            "/.local/",
            "/.claude/",
            "/.cursor/",
            "/.grok/",
            "/.factory/",
            "/.amp/",
            "/.opencode/",
            "/.hermes/",
            "/.kiro/",
            "/agy/",
        ]
        .iter()
        .any(|part| normalized.contains(part) || original.contains(part))
    {
        return Some("native".into());
    }
    None
}
