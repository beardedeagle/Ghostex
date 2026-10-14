//! Leaving the Ghostty app's config behind: the one-time carry-over into Ghostex's own terminal
//! config, and Settings > Terminal's Import from Ghostty. Both only read the Ghostty app's files.

use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use super::*;
use serde_json::Value;

/// One Ghostty config file as read from disk.
struct GhosttyConfigSource {
    path: PathBuf,
    text: String,
}

/// CDXC:Terminal 2026-10-14 WHY:
/// Ghostex used to load the Ghostty app's config (plus everything it pulls in with `config-file`) for its own terminals, so an existing user's look lives in that whole file, not only in the old Ghostex block. Users who had run Ghostex (the block is there, or Ghostex's settings file exists) get all of it copied once, includes inlined and relative shader/image paths made absolute, so nothing changes for them. Anyone else starts from Ghostex's defaults and can use Import from Ghostty, which brings fonts but never the theme. The marker file keeps this from ever running twice, even if the user deletes the copied file.
pub fn migrate_terminal_config_from_ghostty_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.terminal_ghostty_config_migration_marker_file();
    if marker.exists() {
        return;
    }
    let ours = paths.terminal_ghostty_config_file();
    let outcome = if ours.exists() {
        "kept existing Ghostex terminal config"
    } else {
        match carried_over_terminal_config(&shared_sidebar_settings_path()) {
            Some(config) => {
                let written = ours
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|_| atomic_write_file(&ours, config.as_bytes()));
                if let Err(error) = written {
                    eprintln!("Ghostex could not create its terminal config: {error}");
                    return;
                }
                "copied from the Ghostty app's config"
            }
            None => "nothing to copy",
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}

fn carried_over_terminal_config(settings_file: &Path) -> Option<String> {
    let ghostty_path = selected_ghostty_config_path().ok()?;
    let sources = read_ghostty_config_with_includes(&ghostty_path);
    let main = sources.first()?;
    let had_ghostex_block = main
        .text
        .lines()
        .any(|line| line.trim() == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START);
    if !had_ghostex_block && !settings_file.exists() {
        return None;
    }

    let mut lines = vec![
        String::new(),
        "# Copied once from the Ghostty app's config when Ghostex stopped sharing it,".to_string(),
        "# so these terminals look the same as before. Ghostty's file was left as it was:"
            .to_string(),
        format!("# {}", ghostty_path.display()),
    ];
    let mut block_lines = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
            lines.push(format!("# From {}:", source.path.display()));
        }
        let mut inside_ghostex_block = false;
        for line in source.text.lines() {
            let marker = line.trim();
            if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_START {
                inside_ghostex_block = true;
                continue;
            }
            if inside_ghostex_block {
                if marker == GHOSTEX_GHOSTTY_CONFIG_BLOCK_END {
                    inside_ghostex_block = false;
                } else if !marker.is_empty() && !marker.starts_with('#') {
                    block_lines.push(line.to_string());
                }
                continue;
            }
            if marker.starts_with('#') {
                continue;
            }
            if marker.is_empty() {
                if lines.last().is_some_and(|last| !last.trim().is_empty()) {
                    lines.push(String::new());
                }
                continue;
            }
            let key = read_ghostty_config_key(line);
            if key == "config-file" {
                continue;
            }
            lines.push(line_with_absolute_path(&source.path, line, &key));
        }
    }
    let config = format!("{GHOSTEX_TERMINAL_CONFIG_HEADER}{}\n", lines.join("\n"));
    Some(merge_ghostty_managed_config_block_entries(
        &config,
        &[],
        block_lines,
    ))
}

/// Reads `path` and every `config-file` it pulls in, in Ghostty's load order: the file itself,
/// then its includes in order, then theirs. An empty `config-file =` drops the includes above it.
fn read_ghostty_config_with_includes(path: &Path) -> Vec<GhosttyConfigSource> {
    let mut sources = Vec::new();
    let mut queue = VecDeque::from([path.to_path_buf()]);
    let mut visited = HashSet::new();
    while let Some(path) = queue.pop_front() {
        let Ok(canonical) = fs::canonicalize(&path) else {
            continue;
        };
        if !visited.insert(canonical) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let mut includes = Vec::new();
        for line in text.lines() {
            if read_ghostty_config_key(line) != "config-file" {
                continue;
            }
            let value = ghostty_config_value(line);
            if value.is_empty() {
                includes.clear();
                continue;
            }
            let value = value.strip_prefix('?').unwrap_or(value);
            if let Some(include) = resolve_ghostty_config_path(&path, value) {
                includes.push(include);
            }
        }
        queue.extend(includes);
        sources.push(GhosttyConfigSource { path, text });
    }
    sources
}

/// The right-hand side of a `key = value` line, trimmed and without surrounding quotes.
fn ghostty_config_value(line: &str) -> &str {
    let value = line.split_once('=').map_or("", |(_, value)| value.trim());
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn resolve_ghostty_config_path(containing_file: &Path, value: &str) -> Option<PathBuf> {
    if let Some(relative) = value.strip_prefix("~/") {
        return Some(PathBuf::from(user_home_dir()?).join(relative));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(containing_file.parent()?.join(path))
}

/// Ghostty resolves these paths against the file that names them, which is no longer the same
/// folder once the line moves into Ghostex's config.
fn line_with_absolute_path(containing_file: &Path, line: &str, key: &str) -> String {
    if !matches!(key, "custom-shader" | "background-image") {
        return line.to_string();
    }
    let value = ghostty_config_value(line);
    let (optional, path) = match value.strip_prefix('?') {
        Some(path) => ("?", path),
        None => ("", value),
    };
    if path.is_empty() || path.starts_with("~/") || Path::new(path).is_absolute() {
        return line.to_string();
    }
    match resolve_ghostty_config_path(containing_file, path) {
        Some(absolute) => format!("{key} = {optional}{}", absolute.display()),
        None => line.to_string(),
    }
}

/// What Import from Ghostty found: Settings values to save, extra config lines for keys Settings
/// has no row for, and one human line per imported value.
pub(crate) struct GhosttyConfigImport {
    pub(crate) settings: Vec<(&'static str, Value)>,
    pub(crate) extra_keys: Vec<&'static str>,
    pub(crate) extra_lines: Vec<String>,
    pub(crate) summary: Vec<String>,
}

/// Font keys copied as config lines because Settings has no row for them.
const IMPORTED_REPEATABLE_FONT_KEYS: &[&str] = &[
    "font-family-bold",
    "font-family-italic",
    "font-family-bold-italic",
    "font-feature",
    "font-codepoint-map",
];
const IMPORTED_SINGLE_FONT_KEYS: &[&str] = &[
    "font-thicken",
    "font-thicken-strength",
    "font-synthetic-style",
    "adjust-font-baseline",
];

/// CDXC:Terminal 2026-10-14 DECISION:
/// User: "add 'Import from Ghostty' into Ghostex" that brings "fonts and a few things like that from Ghostty itself, but not the theme". Import reads fonts (family with its fallbacks, size, weight, bold/italic families, features, thicken, codepoint maps), line height and letter spacing, and cursor style and blink. Colors, theme, palette and every other key stay out.
pub(crate) fn read_ghostty_config_import()
-> Result<GhosttyConfigImport, SharedGhosttyConfigFileError> {
    let path = prepare_ghostty_config_file_for_open()?;
    let entries: Vec<(String, String)> = read_ghostty_config_with_includes(&path)
        .iter()
        .flat_map(|source| source.text.lines().map(str::to_string).collect::<Vec<_>>())
        .filter_map(|line| {
            let key = read_ghostty_config_key(&line);
            (!key.is_empty()).then(|| {
                (
                    key,
                    line.split_once('=')
                        .map_or("", |(_, v)| v.trim())
                        .to_string(),
                )
            })
        })
        .collect();
    let repeated = |key: &str| {
        let mut values: Vec<&str> = Vec::new();
        for (entry_key, value) in &entries {
            if entry_key == key {
                if value.is_empty() {
                    values.clear();
                } else {
                    values.push(value);
                }
            }
        }
        values
    };
    let single = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
    };
    let unquote = |value: &str| {
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value)
            .to_string()
    };

    let mut import = GhosttyConfigImport {
        settings: Vec::new(),
        extra_keys: Vec::new(),
        extra_lines: Vec::new(),
        summary: Vec::new(),
    };
    let extra = |import: &mut GhosttyConfigImport, key: &'static str, values: &[&str]| {
        import.extra_keys.push(key);
        for value in values {
            import.extra_lines.push(format!("{key} = {value}"));
        }
    };

    let families = repeated("font-family");
    if let Some(primary) = families.first() {
        import
            .settings
            .push(("terminalFontFamily", Value::String(unquote(primary))));
        import
            .summary
            .push(format!("Font family {}", unquote(primary)));
        if families.len() > 1 {
            extra(&mut import, "font-family", &families);
            let fallbacks: Vec<String> = families[1..].iter().map(|f| unquote(f)).collect();
            import
                .summary
                .push(format!("Fallback fonts {}", fallbacks.join(", ")));
        }
    }
    if let Some(size) = single("font-size").and_then(|value| value.parse::<f64>().ok()) {
        import.settings.push(("terminalFontSize", size.into()));
        import.summary.push(format!("Font size {size}"));
    }
    if let Some(weight) = repeated("font-variation")
        .iter()
        .filter_map(|value| {
            unquote(value)
                .strip_prefix("wght=")?
                .trim()
                .parse::<f64>()
                .ok()
        })
        .next_back()
    {
        import.settings.push(("terminalFontWeight", weight.into()));
        import.summary.push(format!("Font weight {weight}"));
    }
    for key in IMPORTED_REPEATABLE_FONT_KEYS {
        let values = repeated(key);
        if !values.is_empty() {
            import.summary.push(format!("{key} {}", values.join(", ")));
            extra(&mut import, key, &values);
        }
    }
    for key in IMPORTED_SINGLE_FONT_KEYS {
        if let Some(value) = single(key) {
            import.summary.push(format!("{key} {value}"));
            extra(&mut import, key, &[value]);
        }
    }
    if let Some(value) = single("adjust-cell-height") {
        match value
            .strip_suffix('%')
            .and_then(|p| p.trim().parse::<f64>().ok())
        {
            Some(percent) => {
                let line_height = ((1.0 + percent / 100.0) * 100.0).round() / 100.0;
                import
                    .settings
                    .push(("terminalLineHeight", line_height.into()));
                import.summary.push(format!("Line height {line_height}"));
            }
            None => {
                import.summary.push(format!("adjust-cell-height {value}"));
                extra(&mut import, "adjust-cell-height", &[value]);
            }
        }
    }
    if let Some(value) = single("adjust-cell-width") {
        match value.parse::<f64>() {
            Ok(spacing) => {
                import
                    .settings
                    .push(("terminalLetterSpacing", spacing.into()));
                import.summary.push(format!("Letter spacing {spacing}"));
            }
            Err(_) => {
                import.summary.push(format!("adjust-cell-width {value}"));
                extra(&mut import, "adjust-cell-width", &[value]);
            }
        }
    }
    if let Some(style) = single("cursor-style") {
        let style = match style {
            "block_hollow" => "block",
            style => style,
        };
        if matches!(style, "block" | "bar" | "underline") {
            import
                .settings
                .push(("terminalCursorStyle", Value::String(style.to_string())));
            import.summary.push(format!("Cursor {style}"));
        }
    }
    if let Some(blink) = single("cursor-style-blink").and_then(|value| value.parse::<bool>().ok()) {
        import
            .settings
            .push(("terminalCursorStyleBlink", Value::Bool(blink)));
        import
            .summary
            .push(format!("Cursor blink {}", if blink { "on" } else { "off" }));
    }
    Ok(import)
}
