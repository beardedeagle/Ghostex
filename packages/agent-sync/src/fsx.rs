//! Filesystem helpers shared by scan, plan, and apply: symlink inspection,
//! relative link creation, backup names, and content hashing.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The raw symlink target when `path` is a symlink.
pub fn link_target(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_symlink() {
        return None;
    }
    fs::read_link(path).ok()
}

pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// A symlink whose target cannot be resolved (missing, or a loop such as a
/// link pointing at itself).
pub fn is_dangling_link(path: &Path) -> bool {
    is_symlink(path) && fs::metadata(path).is_err()
}

/// True when both paths resolve to the same filesystem entry.
pub fn same_entry(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Lexically normalize `path` (collapse `.` and `..`) without touching disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The absolute path a link points at, resolving relative targets against the
/// link's parent folder (lexically, so dangling links still get a path).
pub fn absolute_link_target(link: &Path) -> Option<PathBuf> {
    let target = link_target(link)?;
    if target.is_absolute() {
        return Some(normalize(&target));
    }
    Some(normalize(&link.parent()?.join(target)))
}

/// A relative path from `from_dir` to `to`, both absolute. Falls back to the
/// absolute target when they share no root.
pub fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = normalize(from_dir)
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let to_parts: Vec<_> = normalize(to)
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let mut common = 0;
    while common < from.len() && common < to_parts.len() && from[common] == to_parts[common] {
        common += 1;
    }
    if common == 0 {
        return to.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to_parts[common..] {
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// The link target to write for a per-skill link at `link` pointing at
/// `target`: relative to the *real* parent folder of the link, so a link inside
/// a symlinked folder still resolves (the `skills` CLI's `resolveParentSymlinks`
/// rule). Windows keeps absolute targets because directory symlinks there do
/// not resolve relative targets reliably.
pub fn link_target_for(link: &Path, target: &Path) -> PathBuf {
    if cfg!(windows) {
        return target.to_path_buf();
    }
    let parent = link.parent().map(Path::to_path_buf).unwrap_or_default();
    let real_parent = fs::canonicalize(&parent).unwrap_or(parent);
    let real_target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    relative_path(&real_parent, &real_target)
}

#[cfg(unix)]
pub fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
pub fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(unix)]
pub fn create_file_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
pub fn create_file_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

/// Remove a symlink itself, never what it points at.
pub fn remove_link(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a symlink"));
    }
    #[cfg(windows)]
    {
        if fs::remove_dir(path).is_ok() {
            return Ok(());
        }
    }
    fs::remove_file(path)
}

/// Where Agent Sync moves anything in its way: Ghostex's state folder, never the
/// agent's own folders, so a backed-up skill is not picked up as a second skill.
pub fn default_backup_root() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("agent-sync-backups")
}

/// `<backup_root>/<stamp>/<path relative to home>`, so every backup can be put
/// back by moving it to the same place under HOME.
pub fn backup_path(path: &Path, home: &Path, backup_root: &Path, stamp: &str) -> PathBuf {
    let relative = path.strip_prefix(home).unwrap_or(path);
    let mut target = backup_root.join(stamp);
    for component in relative.components() {
        if let std::path::Component::Normal(part) = component {
            target.push(part);
        }
    }
    target
}

/// SHA-256 over every regular file in `dir` (sorted relative path plus bytes),
/// following symlinks and skipping `.DS_Store`. `None` when unreadable.
pub fn dir_content_hash(dir: &Path) -> Option<String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files).ok()?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(dir.join(&relative)).ok()?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Some(hex::encode(hasher.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".DS_Store" {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

/// Sorted child names of a folder, skipping `.DS_Store`.
pub fn child_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name != ".DS_Store")
        .collect();
    names.sort();
    names
}

/// Display form with HOME shortened to `~`.
pub fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        // `/` between every part, so a Windows path built with `join` does not show as `~/.claude\hooks`.
        Ok(rest) => format!(
            "~/{}",
            rest.components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        ),
        Err(_) => path.display().to_string(),
    }
}
