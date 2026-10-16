use super::{
    config::HookPaths,
    probing::{io_error, path_string, read_file_text},
};
use crate::domain::DomainStateError;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};

/// CDXC:AgentHooks 2026-10-16 WHY:
/// Any version probe that failed read as 1.x, and a first `opencode --version` on Windows can wait for the antivirus scan longer than the 3 s this probe allowed (the Agents page allows 15 s for the same reason). OpenCode 2 then got the 1.x plugin, which it refuses to load, and never the 2.x one (GitHub #244). The probe waits as long as the Agents page's version check, and the answer is kept per executable until the file changes, so a later slow or failed probe cannot flip a computer back to the 1.x install.
pub(crate) fn installed(hooks: &HookPaths) -> bool {
    static MAJOR_TWO: LazyLock<Mutex<HashMap<String, (Option<(u64, SystemTime)>, bool)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let Some(executable) = super::probing::resolve_cli_command("opencode", &hooks.home_dir) else {
        return false;
    };
    let stamp = fs::metadata(&executable)
        .ok()
        .and_then(|metadata| Some((metadata.len(), metadata.modified().ok()?)));
    let known = MAJOR_TWO
        .lock()
        .ok()
        .and_then(|cache| cache.get(&executable).cloned());
    if let Some((known_stamp, major_two)) = known {
        if stamp.is_some() && known_stamp == stamp {
            return major_two;
        }
    }
    let mut command = std::process::Command::new(&executable);
    command.arg("--version");
    let Some(version) =
        super::probing::run_command_stdout_with_timeout(command, Duration::from_secs(15))
    else {
        return known.is_some_and(|(_, major_two)| major_two);
    };
    let major_two = version
        .trim()
        .trim_start_matches("opencode ")
        .trim_start_matches('v')
        .starts_with("2.");
    if let Ok(mut cache) = MAJOR_TWO.lock() {
        cache.insert(executable, (stamp, major_two));
    }
    major_two
}

fn source(hooks: &HookPaths) -> Result<String, DomainStateError> {
    let executable = std::env::current_exe().map_err(io_error)?;
    Ok(include_str!("opencode_v2_tui.js")
        .replace("__GXSERVER__", &json!(path_string(&executable)).to_string())
        .replace(
            "__NOTIFY__",
            &json!(path_string(&hooks.notify_hook_path)).to_string(),
        ))
}

pub(crate) fn inspect(hooks: &HookPaths, directory: &Path) -> super::install::HookInspection {
    let plugin = directory.join("plugins/ghostex-v2/tui.js");
    let text = read_file_text(&plugin);
    let config = read_file_text(&directory.join("cli.json"));
    let leftovers = super::install::opencode_v1_leftovers_present(directory, false);
    super::install::HookInspection {
        ghostex_hook_present: text.contains("ghostex-opencode-session-plugin-marker")
            || config.contains("plugins/ghostex-v2")
            || leftovers,
        current_hook_installed: source(hooks).is_ok_and(|expected| expected == text)
            && config.contains("plugins/ghostex-v2")
            && !leftovers,
    }
}

pub(crate) fn uninstall(directory: &Path) -> Result<Vec<String>, DomainStateError> {
    let config_path = directory.join("cli.json");
    let plugin_dir = directory.join("plugins/ghostex-v2");
    let spec = url::Url::from_directory_path(&plugin_dir)
        .map_err(|_| io_error(std::io::Error::other("Invalid OpenCode plugin path")))?
        .to_string();
    let mut removed = Vec::new();
    if config_path.is_file() {
        let mut config: Value =
            crate::session_chat_opencode::read_config(&read_file_text(&config_path))?;
        if let Some(plugins) = config.get_mut("plugins").and_then(Value::as_array_mut) {
            let before = plugins.len();
            plugins.retain(|p| p.as_str() != Some(&spec) && p["package"].as_str() != Some(&spec));
            if before != plugins.len() {
                crate::session_chat_opencode::patch_config(
                    &config_path,
                    "plugins",
                    &config["plugins"],
                )?;
                removed.push(path_string(&config_path));
            }
        }
    }
    let plugin = plugin_dir.join("tui.js");
    if read_file_text(&plugin).contains("ghostex-opencode-session-plugin-marker") {
        fs::remove_file(&plugin).map_err(io_error)?;
        removed.push(path_string(&plugin));
    }
    Ok(removed)
}

pub(crate) fn install(
    hooks: &HookPaths,
    config_dir: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let plugin_dir = config_dir.join("plugins/ghostex-v2");
    let source = source(hooks)?;
    let plugin_path = plugin_dir.join("tui.js");
    let config_path = config_dir.join("cli.json");
    let text = read_file_text(&config_path);
    let mut config: Value = if text.trim().is_empty() {
        json!({})
    } else {
        crate::session_chat_opencode::read_config(&text)?
    };
    let object = config.as_object_mut().ok_or_else(|| DomainStateError {
        code: "invalidParams",
        message: "OpenCode cli.json must contain an object.".into(),
    })?;
    let plugins = object
        .entry("plugins")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| DomainStateError {
            code: "invalidParams",
            message: "OpenCode's plugins setting must be a list.".into(),
        })?;
    let spec = url::Url::from_directory_path(&plugin_dir)
        .map_err(|_| io_error(std::io::Error::other("Invalid OpenCode plugin path")))?
        .to_string();
    if !plugins
        .iter()
        .any(|p| p.as_str() == Some(&spec) || p["package"].as_str() == Some(&spec))
    {
        plugins.push(json!(spec));
    }
    fs::create_dir_all(&plugin_dir).map_err(io_error)?;
    fs::write(&plugin_path, source).map_err(io_error)?;
    crate::session_chat_opencode::patch_config(&config_path, "plugins", &config["plugins"])?;
    super::install::remove_opencode_v1_leftovers(config_dir, false)?;
    Ok(vec![path_string(&plugin_path)])
}
