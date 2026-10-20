//! Where PowerShell 7 lives on a Windows computer, and the Windows PowerShell 5.1 every Windows has.
//!
//! CDXC:PlatformSupport 2026-10-08 DECISION:
//! User: "Find PowerShell 7 wherever it's installed (PATH, Store, preview), and offer a one-click 'Install PowerShell 7' (via winget) in Settings › Terminal when only 5.1 is present. Sessions keep using 5.1 until 7 is installed, so nothing breaks." One finder serves gxserver (sessions, helpers) and the desktop app (its own terminals), so both pick the same shell: stable PowerShell 7 from Program Files, a per-user install, the Microsoft Store alias, then `pwsh.exe` on PATH (Scoop, mise and the like); a preview build only when no stable one exists; Windows PowerShell 5.1 otherwise.
//! SEE-ALSO: server/src/platform/shell.rs (`powershell_executable`), apps/desktop/src/windows_terminal_backend/native.rs, server/src/managed_tools/powershell.rs (the Install PowerShell 7 job).

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The folders and environment the search reads. `from_process` fills it from this process's
/// environment; a caller (or a scratch-folder check) can fill it from anywhere.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    /// `ProgramW6432`, `ProgramFiles` and `ProgramFiles(x86)`, in that order, without duplicates.
    pub program_files: Vec<PathBuf>,
    /// `%LOCALAPPDATA%`: per-user installs and the Microsoft Store app execution aliases.
    pub local_app_data: Option<PathBuf>,
    /// `%SystemRoot%`, for Windows PowerShell 5.1.
    pub system_root: Option<PathBuf>,
}

impl Environment {
    pub fn from_process() -> Self {
        let mut program_files: Vec<PathBuf> = Vec::new();
        for name in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(path) = std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
            {
                if !program_files.contains(&path) {
                    program_files.push(path);
                }
            }
        }
        Self {
            program_files,
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            system_root: std::env::var_os("SystemRoot").map(PathBuf::from),
        }
    }
}

/// A PowerShell 7 (or newer) that can be started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerShell7 {
    pub executable: PathBuf,
    /// A preview build, used only because no stable one is installed.
    pub preview: bool,
}

/// Whether `path` names something Windows can start. The Microsoft Store's `pwsh.exe` is an app
/// execution alias: a reparse point that launches fine but fails a normal `stat` (os error 1920),
/// so the check must not follow it.
pub fn launchable(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.file_type().is_dir())
}

/// The install folders under `<base>\PowerShell`, newest major version first: `7`, `8`, … for
/// stable and `7-preview`, … for previews.
fn install_folders(base: &Path, preview: bool) -> Vec<PathBuf> {
    let root = base.join("PowerShell");
    let mut found: Vec<(u32, PathBuf)> = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (major, is_preview) = match name.strip_suffix("-preview") {
                Some(major) => (major.to_string(), true),
                None => (name, false),
            };
            let major: u32 = major.parse().ok().filter(|major| *major >= 7)?;
            (is_preview == preview).then(|| (major, entry.path()))
        })
        .collect();
    found.sort_by_key(|(major, _)| std::cmp::Reverse(*major));
    let mut folders: Vec<PathBuf> = found.into_iter().map(|(_, path)| path).collect();
    // A folder the scan could not list (permissions) still gets the well-known name.
    let fallback = root.join(if preview { "7-preview" } else { "7" });
    if !folders.contains(&fallback) {
        folders.push(fallback);
    }
    folders
}

/// The first PowerShell 7 that exists: stable before preview, and within each, Program Files, a
/// per-user install, the Store alias, then `path_directories` (called only when nothing else
/// matched, since it can read the registry).
pub fn find_powershell7(
    environment: &Environment,
    path_directories: impl Fn() -> Vec<PathBuf>,
) -> Option<PowerShell7> {
    let mut cached: Option<Vec<PathBuf>> = None;
    for preview in [false, true] {
        let mut candidates: Vec<PathBuf> = Vec::new();
        for program_files in &environment.program_files {
            candidates.extend(
                install_folders(program_files, preview)
                    .into_iter()
                    .map(|folder| folder.join("pwsh.exe")),
            );
        }
        if let Some(local) = &environment.local_app_data {
            // `winget install --scope user` and the per-user MSI.
            candidates.extend(
                install_folders(&local.join("Microsoft"), preview)
                    .into_iter()
                    .map(|folder| folder.join("pwsh.exe")),
            );
            candidates.push(local.join("Microsoft/WindowsApps").join(if preview {
                "pwsh-preview.exe"
            } else {
                "pwsh.exe"
            }));
        }
        if let Some(executable) = candidates.into_iter().find(|path| launchable(path)) {
            return Some(PowerShell7 {
                executable,
                preview,
            });
        }
        let directories = cached.get_or_insert_with(&path_directories);
        let name = if preview {
            "pwsh-preview.exe"
        } else {
            "pwsh.exe"
        };
        if let Some(executable) = directories
            .iter()
            .map(|directory| directory.join(name))
            .find(|path| launchable(path))
        {
            return Some(PowerShell7 {
                executable,
                preview,
            });
        }
    }
    None
}

/// `name` (for example `winget.exe`) in the Store app execution alias folder, then in
/// `path_directories`. The aliases do not pass a normal `stat`, which is why `winget` is not found
/// by a plain PATH search that checks file metadata.
pub fn find_app_execution_alias(
    name: &str,
    environment: &Environment,
    path_directories: impl Fn() -> Vec<PathBuf>,
) -> Option<PathBuf> {
    environment
        .local_app_data
        .iter()
        .map(|local| local.join("Microsoft/WindowsApps").join(name))
        .find(|path| launchable(path))
        .or_else(|| {
            path_directories()
                .into_iter()
                .map(|directory| directory.join(name))
                .find(|path| launchable(path))
        })
}

/// Windows PowerShell 5.1, which every Windows has.
pub fn windows_powershell(environment: &Environment) -> PathBuf {
    environment
        .system_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("C:/Windows"))
        .join("System32/WindowsPowerShell/v1.0/powershell.exe")
}

/// PowerShell 7 when it is installed anywhere, otherwise Windows PowerShell 5.1.
pub fn preferred_powershell(
    environment: &Environment,
    path_directories: impl Fn() -> Vec<PathBuf>,
) -> PathBuf {
    find_powershell7(environment, path_directories)
        .map(|found| found.executable)
        .unwrap_or_else(|| windows_powershell(environment))
}
