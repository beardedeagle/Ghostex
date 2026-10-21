// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds project board image request/clipboard handling: saving
// clipboard images, building preview data URLs, and path/extension/MIME
// helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use gpui::ImageFormat;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn project_board_image_path_from_reference(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    if trimmed.starts_with("file://") {
        let parsed = gpui::http_client::Url::parse(trimmed).ok()?;
        if parsed.scheme() == "file" {
            return parsed.to_file_path().ok();
        }
        return None;
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/.ghostex/") {
        return Some(
            shared_settings::ghostex_storage_paths()
                .data_dir
                .join(relative_path),
        );
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/") {
        return Some(home_dir().join(relative_path));
    }
    if trimmed.starts_with('/') {
        return Some(PathBuf::from(trimmed));
    }
    None
}

pub(crate) fn is_project_board_image_file_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    project_board_image_extension_is_allowed(&extension)
        && fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

pub(crate) fn project_board_image_extension_is_allowed(extension: &str) -> bool {
    matches!(
        extension,
        "avif"
            | "bmp"
            | "gif"
            | "heic"
            | "heif"
            | "ico"
            | "jpg"
            | "jpeg"
            | "png"
            | "svg"
            | "tif"
            | "tiff"
            | "webp"
    )
}

pub(crate) fn project_board_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tif",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

pub(crate) fn unique_project_board_image_path(extension: &str) -> Result<PathBuf, String> {
    let directory = shared_settings::ghostex_storage_paths().images_dir();
    fs::create_dir_all(&directory).map_err(|_| "Could not create image directory.".to_string())?;
    let base_name = system_time_epoch_millis_string(std::time::SystemTime::now());
    let first = directory.join(format!("{base_name}.{extension}"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..100 {
        let candidate = directory.join(format!("{base_name}-{index}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(directory.join(format!(
        "{}-{}.{}",
        base_name,
        std::process::id(),
        extension
    )))
}

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn project_board_base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}
