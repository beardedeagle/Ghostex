use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use super::*;
use crate::app::helpers::*;

pub(crate) fn manage_files_bridge_error_response(
    action: &str,
    request_id: &str,
    error: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "error": error,
        "requestId": request_id,
    })
}

/*
CDXC:Docs 2026-08-09:
The project's own entries come first and are discovered exactly as they have
always been, so setting a Docs directory can never take the repo's README.md,
CLAUDE.md, or docs/ away. The mounted Docs directory is appended after them.
*/
pub(crate) fn manage_project_file_entries(
    context: ManageDocsContext<'_>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut entries =
        manage_project_root_file_entries(context.roots.project.as_path(), context, true)?;
    if let Some(mount) = context.roots.extra.as_ref() {
        manage_append_docs_extra_root_entries(&mut entries, mount);
    }
    Ok(entries)
}

pub(crate) fn manage_project_root_file_entries(
    root: &Path,
    context: ManageDocsContext<'_>,
    recursive: bool,
) -> Result<Vec<serde_json::Value>, String> {
    /*
    macOS `manageProjectFileEntries` parity: docs/ and each configured Docs
    folder render as their own top-level directory entries, direct repo-root
    Markdown/HTML/Excalidraw artifacts join them, and only the scan roots are
    walked (bounded), so the Docs sidebar never becomes a broad repo browser.
    */
    let mut entries = Vec::new();
    let mut scanned_directory_entries = 0;
    let scan_roots =
        manage_docs_project_scan_root_relative_paths(root, context.additional_docs_folders_text);
    for relative_path in &scan_roots {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            return Err(manage_docs_scan_cap_error());
        }
        let Some(directory) = manage_project_directory(root, relative_path) else {
            continue;
        };
        let modified_at = fs::metadata(&directory)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(gpui_iso8601_utc);
        entries.push(serde_json::json!({
            "depth": 0,
            "kind": "directory",
            "modifiedAt": modified_at,
            "name": relative_path,
            "path": relative_path,
            "size": serde_json::Value::Null,
        }));
    }
    manage_append_project_root_artifact_file_entries(
        &mut entries,
        root,
        &mut scanned_directory_entries,
    )?;
    if !recursive {
        return Ok(entries);
    }
    for relative_path in &scan_roots {
        let Some(directory) = manage_project_directory(root, relative_path) else {
            continue;
        };
        manage_append_project_file_entries(
            &mut entries,
            root,
            &directory,
            relative_path,
            1,
            &mut scanned_directory_entries,
            true,
        )?;
    }
    Ok(entries)
}

pub(crate) fn manage_project_directory(root: &Path, relative_path: &str) -> Option<PathBuf> {
    let directory = root.join(PathBuf::from(relative_path));
    let metadata = fs::metadata(&directory).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let resolved = fs::canonicalize(&directory).ok()?;
    path_is_inside_or_equal(&resolved, root).then_some(resolved)
}

/*
CDXC:Docs 2026-08-09:
Mirrors `append_extra_root_entries` in `server/src/project_docs/tree.rs`, so the
local Docs pane and a remote project's Docs pane list the same tree. The mounted
Docs directory is walked to the bottom and files are narrowed to the extensions
Docs renders.

CDXC:Docs 2026-08-09: every failure lands on the mount node's label
instead of on the listing — an unopenable directory, and the entry and depth caps
alike. Losing the whole panel, including the project's own README.md, because a
vault is too deep is the one thing this must not do, and a tree that silently
stopped at 20,000 entries reads exactly like a vault that only has that many.
*/
pub(crate) fn manage_append_docs_extra_root_entries(
    entries: &mut Vec<serde_json::Value>,
    mount: &ManageDocsExtraMount,
) {
    let root = match mount.location.as_deref() {
        Ok(root) => root,
        Err(error) => {
            entries.push(manage_unavailable_docs_extra_root_entry(&mount.name, error));
            return;
        }
    };
    let mut tree = Vec::new();
    let mut scanned_directory_entries = 0;
    if let Err(error) = manage_append_docs_tree_entries(
        &mut tree,
        root,
        root,
        MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        1,
        &mut scanned_directory_entries,
        true,
    ) {
        entries.push(manage_unavailable_docs_extra_root_entry(
            &mount.name,
            &error,
        ));
        return;
    }
    let modified_at = fs::metadata(root)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(gpui_iso8601_utc);
    entries.push(serde_json::json!({
        "depth": 0,
        "displayPath": mount.name,
        "kind": "directory",
        "modifiedAt": modified_at,
        "name": mount.name,
        "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        "size": serde_json::Value::Null,
    }));
    manage_name_docs_extra_root_tree_entries(&mut tree, &mount.name);
    entries.append(&mut tree);
}

/*
CDXC:Docs 2026-08-10:
Mirrors `name_extra_root_tree_entries` in server/src/project_docs/tree.rs: every
mounted entry carries the name the tree shows it under beside the routing
address it answers to, so the reserved segment never reaches Copy Path or text
pasted into a terminal.
*/
pub(crate) fn manage_name_docs_extra_root_tree_entries(
    tree: &mut [serde_json::Value],
    mount_name: &str,
) {
    for entry in tree {
        let Some(relative_path) = entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .and_then(|path| path.strip_prefix(MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT))
        else {
            continue;
        };
        let display_path = format!("{mount_name}{relative_path}");
        if let Some(entry) = entry.as_object_mut() {
            entry.insert(
                "displayPath".to_string(),
                serde_json::Value::String(display_path),
            );
        }
    }
}

/// The mount still shows when its folder does not, carrying the reason in the
/// only field the Docs tree renders. A missing vault must look missing, not
/// look empty.
pub(crate) fn manage_unavailable_docs_extra_root_entry(
    name: &str,
    error: &str,
) -> serde_json::Value {
    /*
    CDXC:Copy 2026-09-03:
    User decision: Ghostex-owned user-facing copy in the desktop, web, and mobile apps uses no em dashes; use punctuation that preserves the sentence's natural reading instead.
    */
    serde_json::json!({
        "depth": 0,
        "kind": "directory",
        "displayPath": name,
        "modifiedAt": serde_json::Value::Null,
        "name": format!("{name}: {error}"),
        "path": MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT,
        "size": serde_json::Value::Null,
    })
}

/// The scan budget the gxserver walk enforces, so a folder full of files Docs
/// does not render costs the same on both sides instead of being free here.
pub(crate) fn manage_bounded_docs_children(
    directory: &Path,
    scanned_directory_entries: &mut usize,
    limit: usize,
    limit_error: fn() -> String,
) -> Result<Vec<ghostex_docs::directory::Entry>, String> {
    ghostex_docs::directory::children(directory, scanned_directory_entries, limit, limit_error)
}

pub(crate) fn manage_append_docs_tree_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
    directory: &Path,
    relative_directory_path: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
    recursive: bool,
) -> Result<(), String> {
    if depth > MANAGE_DOCS_TREE_MAX_DEPTH {
        return Err(manage_docs_tree_depth_cap_error());
    }
    let mut children = manage_bounded_docs_children(
        directory,
        scanned_directory_entries,
        MANAGE_DOCS_TREE_MAX_ENTRIES,
        manage_docs_tree_entry_cap_error,
    )?;
    children.sort_by(|left, right| {
        let left_is_dir = left
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let right_is_dir = right
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= MANAGE_DOCS_TREE_MAX_ENTRIES {
            return Err(manage_docs_tree_entry_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        let metadata = match child.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let is_directory = metadata.is_dir();
        if is_directory {
            if name.starts_with('.') || MANAGE_IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }
        } else if !manage_has_docs_artifact_extension(&name) {
            continue;
        }
        // Confinement, and the reason an outward symlink never joins the tree.
        let resolved = match fs::canonicalize(child.path()) {
            Ok(resolved) => resolved,
            Err(_) => continue,
        };
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        let relative_path = format!("{relative_directory_path}/{name}");
        entries.push(serde_json::json!({
            "depth": depth,
            "kind": if is_directory { "directory" } else { "file" },
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": relative_path,
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
        if is_directory
            && child.file_type().is_ok_and(|kind| kind.is_symlink())
            && let Some(entry) = entries
                .last_mut()
                .and_then(serde_json::Value::as_object_mut)
        {
            entry.insert("childrenLoaded".to_string(), serde_json::Value::Bool(true));
        }
        if is_directory
            && !child
                .file_type()
                .map(|file_type| file_type.is_symlink())
                .unwrap_or(false)
        {
            directories.push((child.path(), relative_path));
        }
    }

    if !recursive {
        return Ok(());
    }
    for (directory, relative_path) in directories {
        manage_append_docs_tree_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
            true,
        )?;
    }
    Ok(())
}

pub(crate) fn manage_docs_tree_entry_cap_error() -> String {
    format!(
        "Docs directory holds more than {MANAGE_DOCS_TREE_MAX_ENTRIES} files and folders. Point it at a smaller folder in Settings > Projects."
    )
}

pub(crate) fn manage_docs_tree_depth_cap_error() -> String {
    format!(
        "Docs directory nests deeper than {MANAGE_DOCS_TREE_MAX_DEPTH} folders. Point it at a smaller folder in Settings > Projects."
    )
}

pub(crate) fn manage_docs_scan_cap_error() -> String {
    format!(
        "Project Docs exceeds the scan limit of {MANAGE_FILE_LIST_MAX_ENTRIES} files and folders."
    )
}

pub(crate) fn manage_docs_scan_depth_cap_error() -> String {
    format!("Project Docs nests deeper than {MANAGE_FILE_LIST_MAX_DEPTH} folders.")
}

pub(crate) fn manage_append_project_root_artifact_file_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
    scanned_directory_entries: &mut usize,
) -> Result<(), String> {
    let mut children = manage_bounded_docs_children(
        root,
        scanned_directory_entries,
        MANAGE_FILE_LIST_MAX_ENTRIES,
        manage_docs_scan_cap_error,
    )?;
    children.sort_by_key(|child| child.file_name());
    for child in children {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            return Err(manage_docs_scan_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" || !manage_is_root_artifact_file_relative_path(&name) {
            continue;
        }
        let Ok(metadata) = child.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            continue;
        }
        let Ok(resolved) = fs::canonicalize(child.path()) else {
            continue;
        };
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        entries.push(serde_json::json!({
            "depth": 0,
            "kind": "file",
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": name,
            "size": metadata.len(),
        }));
    }
    Ok(())
}

pub(crate) fn manage_append_project_file_entries(
    entries: &mut Vec<serde_json::Value>,
    root: &Path,
    directory: &Path,
    relative_directory_path: &str,
    depth: usize,
    scanned_directory_entries: &mut usize,
    recursive: bool,
) -> Result<(), String> {
    let mut children = manage_bounded_docs_children(
        directory,
        scanned_directory_entries,
        MANAGE_FILE_LIST_MAX_ENTRIES,
        manage_docs_scan_cap_error,
    )?;
    if depth > MANAGE_FILE_LIST_MAX_DEPTH && !children.is_empty() {
        return Err(manage_docs_scan_depth_cap_error());
    }
    children.sort_by(|left, right| {
        let left_is_dir = left
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let right_is_dir = right
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    let mut directories = Vec::new();
    for child in children {
        if entries.len() >= MANAGE_FILE_LIST_MAX_ENTRIES {
            return Err(manage_docs_scan_cap_error());
        }
        let name = child.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" {
            continue;
        }
        let metadata = match child.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let is_directory = metadata.is_dir();
        if is_directory && MANAGE_IGNORED_DIRECTORY_NAMES.contains(&name.as_str()) {
            continue;
        }
        let resolved = match fs::canonicalize(child.path()) {
            Ok(resolved) => resolved,
            Err(_) => continue,
        };
        if !path_is_inside_or_equal(&resolved, root) {
            continue;
        }
        let relative_path = if relative_directory_path.is_empty() {
            name.clone()
        } else {
            format!("{relative_directory_path}/{name}")
        };
        entries.push(serde_json::json!({
            "depth": depth,
            "kind": if is_directory { "directory" } else { "file" },
            "modifiedAt": metadata.modified().ok().map(gpui_iso8601_utc),
            "name": name,
            "path": relative_path,
            "size": if is_directory { None } else { Some(metadata.len()) },
        }));
        if is_directory
            && child.file_type().is_ok_and(|kind| kind.is_symlink())
            && let Some(entry) = entries
                .last_mut()
                .and_then(serde_json::Value::as_object_mut)
        {
            entry.insert("childrenLoaded".to_string(), serde_json::Value::Bool(true));
        }
        if is_directory
            && !child
                .file_type()
                .map(|file_type| file_type.is_symlink())
                .unwrap_or(false)
        {
            directories.push((child.path(), relative_path));
        }
    }

    if !recursive {
        return Ok(());
    }
    for (directory, relative_path) in directories {
        manage_append_project_file_entries(
            entries,
            root,
            &directory,
            &relative_path,
            depth + 1,
            scanned_directory_entries,
            true,
        )?;
    }
    Ok(())
}
