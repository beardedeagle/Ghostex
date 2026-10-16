use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::domain::DomainStateError;

use crate::agent_hooks::config::{HookPaths, OPENCODE_PLUGIN_MARKER};
use crate::agent_hooks::plugin_sources::build_opencode_plugin_source;
use crate::agent_hooks::probing::{io_error, path_string, push_unique_path, read_file_text};
use crate::agent_hooks::resolution::{
    is_opencode_session_plugin_registration, provider_hook_paths,
    text_contains_ghostex_owned_hook_command,
};

use super::*;

/// CDXC:AgentHooks 2026-10-16 WHY:
/// Every OpenCode 1.x (0.15 through 1.19) loads `*.js` from `plugin/` in its global config folder, while `plugins/` is only scanned from about 1.1, which is why Ghostex used to list `./plugins/ghostex-session.js` under `plugin` in opencode.json. OpenCode 2 reads that list as `plugins`, warns that the file "must be a directory", and auto-loads every `*.js` in `plugin/` and `plugins/` as a 2.x server plugin, which rejects the 1.x shape (GitHub #244). So the 1.x plugin lives in `plugin/` with no opencode.json entry, and the 2.x install (a TUI plugin in `plugins/ghostex-v2/`) removes every 1.x file and entry, which also repairs a computer that upgraded OpenCode after the 1.x install.
pub(crate) fn opencode_v1_plugin_path(config_dir: &Path) -> PathBuf {
    config_dir.join("plugin").join("ghostex-session.js")
}

fn opencode_v1_legacy_plugin_path(config_dir: &Path) -> PathBuf {
    config_dir.join("plugins").join("ghostex-session.js")
}

fn is_ghostex_opencode_plugin(path: &Path) -> bool {
    let text = read_file_text(path);
    text.contains(OPENCODE_PLUGIN_MARKER) || text_contains_ghostex_owned_hook_command(&text)
}

fn opencode_config_registers_ghostex(config_path: &Path) -> bool {
    read_json_object(&read_file_text(config_path))
        .get("plugin")
        .and_then(Value::as_array)
        .is_some_and(|plugins| plugins.iter().any(is_opencode_session_plugin_registration))
}

/// Whether an earlier install left what this one no longer writes: the `plugin` entry in
/// opencode.json, the 1.x plugin in `plugins/`, and, unless `keep_v1_plugin`, the 1.x plugin in
/// `plugin/` too.
pub(crate) fn opencode_v1_leftovers_present(config_dir: &Path, keep_v1_plugin: bool) -> bool {
    opencode_config_registers_ghostex(&config_dir.join("opencode.json"))
        || is_ghostex_opencode_plugin(&opencode_v1_legacy_plugin_path(config_dir))
        || (!keep_v1_plugin && is_ghostex_opencode_plugin(&opencode_v1_plugin_path(config_dir)))
}

/// Removes what `opencode_v1_leftovers_present` reports and returns the files it changed.
pub(crate) fn remove_opencode_v1_leftovers(
    config_dir: &Path,
    keep_v1_plugin: bool,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed = uninstall_opencode_hook_paths(
        &opencode_v1_legacy_plugin_path(config_dir),
        &config_dir.join("opencode.json"),
    )?;
    let plugin_path = opencode_v1_plugin_path(config_dir);
    if !keep_v1_plugin
        && is_ghostex_opencode_plugin(&plugin_path)
        && remove_file_if_exists(&plugin_path)?
    {
        push_unique_path(&mut removed, path_string(&plugin_path));
    }
    Ok(removed)
}

pub(super) fn uninstall_opencode_hook(
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    let directory = config_path.parent().unwrap();
    let mut removed = super::opencode_v2::uninstall(directory)?;
    for path in remove_opencode_v1_leftovers(directory, false)? {
        push_unique_path(&mut removed, path);
    }
    Ok(removed)
}

pub(crate) fn uninstall_opencode_hook_paths(
    plugin_path: &Path,
    config_path: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    let config_text = read_file_text(config_path);
    if !config_text.trim().is_empty() {
        let mut data = read_json_object(&config_text);
        if let Some(object) = data.as_object_mut() {
            if let Some(plugins) = object.get_mut("plugin").and_then(Value::as_array_mut) {
                let next_plugins = plugins
                    .iter()
                    .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
                    .cloned()
                    .collect::<Vec<_>>();
                if next_plugins.len() != plugins.len() {
                    *plugins = next_plugins;
                    write_json_file(config_path, &data)?;
                    push_unique_path(&mut removed_paths, path_string(config_path));
                }
            }
        }
    }
    if is_ghostex_opencode_plugin(plugin_path) && remove_file_if_exists(plugin_path)? {
        push_unique_path(&mut removed_paths, path_string(plugin_path));
    }
    Ok(removed_paths)
}

pub(super) fn install_opencode_hook(
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    let config_dir = config_path.parent().unwrap();
    // `provider_hook_paths` already chose 2.x (`cli.json`) or 1.x (`opencode.json`).
    if config_path
        .file_name()
        .is_some_and(|name| name == "cli.json")
    {
        return super::opencode_v2::install(hook_paths, config_dir);
    }
    if let Some(parent) = plugin_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(
        plugin_path,
        build_opencode_plugin_source(&hook_paths.notify_hook_path),
    )
    .map_err(io_error)?;
    remove_opencode_v1_leftovers(config_dir, true)?;
    Ok(vec![path_string(plugin_path)])
}
