//! Isolated app variants: a second Ghostex built from another checkout that runs next to the user's normal one (`cargo xtask start --isolated[=<variant>]`, `cargo xtask gx-isolated`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::bail;
use crate::util::{env_trimmed, home_dir, status_code, Res};

pub const DEFAULT_VARIANT: &str = "ghostex-3";

struct Variant {
    name: &'static str,
    app_name: &'static str,
    bundle_id: &'static str,
    storage_directory_name: &'static str,
    gxserver_dev_port: &'static str,
    code_server_port: &'static str,
    cef_remote_debugging_port: &'static str,
}

/// CDXC:Build 2026-09-20 WHY:
/// A variant is one complete alternative app identity: its own app name, bundle id, storage root and the three ports a running instance owns (gxserver, code-server, CEF remote debugging). Keeping them in one table is what lets a second checkout build and run its own copy next to the user's normal Ghostex without sharing state or fighting over a port, and every value here must stay clear of the main instance (58744, 3777, and 3779 for the Code editor a remote computer starts here; SOURCE_CODE_SERVER_REMOTE_PORT) and of every other row. Add a row to introduce a variant; never branch the launcher on which one is selected.
const VARIANTS: &[Variant] = &[
    // CDXC:Build 2026-09-13 DECISION:
    // User: ghostex-3 must start separately from the main Ghostex with separate configuration, sharing only the existing hooks via a symlink.
    // The bundle retains this environment so reopening it from Finder uses the same isolated instance.
    Variant {
        name: "ghostex-3",
        app_name: "Ghostex-3",
        bundle_id: "com.madda.ghostex.gpui.ghostex-3",
        storage_directory_name: "ghostex-3",
        gxserver_dev_port: "58747",
        code_server_port: "3778",
        cef_remote_debugging_port: "9337",
    },
    // CDXC:Build 2026-09-20 DECISION:
    // User: the UI revamp clone must launch its own app instance alongside the normal Ghostex.
    Variant {
        name: "ui-revamp",
        app_name: "Ghostex-UI-Revamp",
        bundle_id: "com.madda.ghostex.gpui.ui-revamp",
        storage_directory_name: "ghostex-ui-revamp",
        gxserver_dev_port: "58751",
        code_server_port: "3781",
        cef_remote_debugging_port: "9341",
    },
];

pub struct Isolated {
    pub variant: String,
    pub app_name: String,
    pub bundle_id: String,
    pub install_dir: PathBuf,
    /// Applied to the start's own environment, so the build, the launch and the bundle all see it.
    pub environment: Vec<(&'static str, String)>,
}

impl Isolated {
    pub fn env(&self, key: &str) -> Option<&str> {
        self.environment
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
    }
}

fn variant_names() -> String {
    VARIANTS
        .iter()
        .map(|v| v.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reads `--isolated` / `--isolated=<variant>`; None for any other argument.
pub fn parse_argument(argument: &str) -> Res<Option<String>> {
    if argument == "--isolated" {
        return Ok(Some(DEFAULT_VARIANT.to_string()));
    }
    let Some(variant) = argument.strip_prefix("--isolated=") else {
        return Ok(None);
    };
    let variant = variant.trim();
    if variant.is_empty() {
        bail!(
            "Name the variant: --isolated=<{}>.",
            VARIANTS
                .iter()
                .map(|v| v.name)
                .collect::<Vec<_>>()
                .join("|")
        );
    }
    Ok(Some(variant.to_string()))
}

pub fn start_command(variant: &str) -> String {
    if variant == DEFAULT_VARIANT {
        "cargo xtask start --isolated".into()
    } else {
        format!("cargo xtask start --isolated={variant}")
    }
}

pub fn configuration(variant_name: &str) -> Res<Isolated> {
    if !cfg!(target_os = "macos") {
        bail!("The isolated Ghostex launcher currently supports macOS.");
    }
    let Some(variant) = VARIANTS.iter().find(|v| v.name == variant_name) else {
        bail!(
            "Unknown isolated Ghostex variant: {variant_name}. Known variants: {}.",
            variant_names()
        );
    };
    let home = home_dir()
        .join(".local")
        .join("share")
        .join(variant.storage_directory_name);
    let install_dir = home_dir().join("Applications");
    let gxserver = install_dir
        .join(format!("{}.app", variant.app_name))
        .join("Contents/Resources/Web/gxserver/bin/gxserver")
        .display()
        .to_string();
    Ok(Isolated {
        variant: variant_name.to_string(),
        app_name: variant.app_name.into(),
        bundle_id: variant.bundle_id.into(),
        install_dir,
        environment: vec![
            ("GHOSTEX_HOME", home.display().to_string()),
            (
                "GHOSTEX_GXSERVER_DEV_PORT",
                variant.gxserver_dev_port.into(),
            ),
            ("GHOSTEX_CODE_SERVER_PORT", variant.code_server_port.into()),
            (
                "CODE_SERVER_CONFIG",
                home.join("code-server-runtime-gpui/config.yaml")
                    .display()
                    .to_string(),
            ),
            (
                "GHOSTEX_GPUI_CEF_REMOTE_DEBUGGING_PORT",
                variant.cef_remote_debugging_port.into(),
            ),
            ("GHOSTEX_GXSERVER_CLI", gxserver.clone()),
            ("GHOSTEX_GXSERVER_BIN", gxserver),
        ],
    })
}

pub fn prepare(configuration: &Isolated) -> Res {
    let original_data = match env_trimmed("XDG_DATA_HOME") {
        Some(value) if Path::new(&value).is_absolute() => PathBuf::from(value).join("ghostex"),
        _ => home_dir().join(".local").join("share").join("ghostex"),
    };
    let original_hooks = original_data.join("hooks");
    if !original_hooks.exists() {
        bail!(
            "Original Ghostex hooks are missing: {}",
            original_hooks.display()
        );
    }
    let home = PathBuf::from(
        configuration
            .env("GHOSTEX_HOME")
            .expect("isolated environment sets GHOSTEX_HOME"),
    );
    create_private_dir(&home)?;
    fs::create_dir_all(&configuration.install_dir)?;
    let hooks = home.join("hooks");
    let is_symlink = |path: &Path| {
        fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    };
    if is_symlink(&home) || is_symlink(&hooks) {
        bail!("The isolated storage root and hooks directory must be private directories.");
    }
    detach_vscode_settings(&home)?;
    create_private_dir(&hooks)?;
    // Link files individually: hook upgrades use atomic rename, so they can replace a local link without writing through a shared directory.
    for entry in fs::read_dir(&original_hooks)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        let source = entry.path();
        let target = hooks.join(entry.file_name());
        match fs::symlink_metadata(&target) {
            Ok(existing) => {
                if existing.file_type().is_symlink()
                    && fs::canonicalize(&target)? != fs::canonicalize(&source)?
                {
                    bail!(
                        "The existing hook link points elsewhere: {}",
                        target.display()
                    );
                }
            }
            Err(_) => symlink(&source, &target)?,
        }
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Res {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    Ok(())
}

fn symlink(source: &Path, target: &Path) -> Res {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, target)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(source, target)?;
    Ok(())
}

/// The first isolated build linked these entries back to the user's VS Code. Preserve their current contents as private copies before starting the editor.
fn detach_vscode_settings(home: &Path) -> Res {
    let user = home.join("code-server-runtime-gpui/user-data/User");
    for name in [
        "settings.json",
        "keybindings.json",
        "snippets",
        "mcp.json",
        "tasks.json",
    ] {
        let target = user.join(name);
        if !fs::symlink_metadata(&target)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let resolved = fs::canonicalize(&target)?;
        let temporary = PathBuf::from(format!(
            "{}.isolated-{}",
            target.display(),
            std::process::id()
        ));
        if temporary.exists() {
            bail!("{} already exists.", temporary.display());
        }
        copy_recursive(&resolved, &temporary)?;
        // POSIX rename replaces a file symlink directly. A copied snippets directory needs its old directory symlink unlinked first.
        if temporary.is_dir() {
            fs::remove_file(&target)?;
        }
        fs::rename(&temporary, &target)?;
    }
    Ok(())
}

fn copy_recursive(source: &Path, destination: &Path) -> Res {
    let metadata = fs::metadata(source)?;
    if metadata.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(
                &fs::canonicalize(entry.path())?,
                &destination.join(entry.file_name()),
            )?;
        }
    } else {
        fs::copy(source, destination)?;
    }
    Ok(())
}

/// `cargo xtask gx-isolated [--isolated=<variant>] [--print | <ghostex CLI arguments>]`: runs the isolated app's own `ghostex` CLI against its private storage.
pub fn run_cli(args: &[String]) -> Res<i32> {
    let mut variant = DEFAULT_VARIANT.to_string();
    let mut forwarded = Vec::new();
    for argument in args {
        match parse_argument(argument)? {
            Some(selected) => variant = selected,
            None => forwarded.push(argument.clone()),
        }
    }
    let configuration = configuration(&variant)?;
    if forwarded.first().map(String::as_str) == Some("--print") {
        let environment: serde_json::Map<String, serde_json::Value> = configuration
            .environment
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::from(v.as_str())))
            .collect();
        let printed = serde_json::json!({
            "variant": configuration.variant,
            "appName": configuration.app_name,
            "bundleId": configuration.bundle_id,
            "installDir": configuration.install_dir.display().to_string(),
            "environment": environment,
        });
        println!("{}", serde_json::to_string_pretty(&printed)?);
        return Ok(0);
    }
    prepare(&configuration)?;
    let cli = configuration
        .install_dir
        .join(format!("{}.app", configuration.app_name))
        .join("Contents/Resources/CLI/ghostex");
    if !cli.exists() {
        bail!(
            "Build {} first with {}.",
            configuration.app_name,
            start_command(&variant)
        );
    }
    let mut command = Command::new(cli);
    command.args(&forwarded).envs(
        configuration
            .environment
            .iter()
            .map(|(k, v)| (*k, v.as_str())),
    );
    status_code(&mut command)
}
