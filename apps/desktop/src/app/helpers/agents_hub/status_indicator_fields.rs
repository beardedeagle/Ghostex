// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the sidebar status-indicator JSON field
// parsing and validation helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) fn reject_unexpected_gpui_status_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), ()> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(());
    }
    Ok(())
}

pub(crate) fn gpui_status_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or(())
}

pub(crate) fn gpui_status_count_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<u64, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value <= u64::from(u16::MAX))
        .ok_or(())
}

pub(crate) fn gpui_status_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<GpuiStatusIndicatorStatus, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiStatusIndicatorStatus::from_str)
        .ok_or(())
}

pub(crate) fn gpui_status_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_bridge_id_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_title_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_optional_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(_) => gpui_status_title_field(object, key).map(Some),
    }
}

pub(crate) fn gpui_status_optional_icon_data_url_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    /*
    CDXC:Notifications 2026-06-26-07:22:
    GPUI session-attention icons may enter Rust only as bounded image data URLs on sanitized status project rows. Reject paths, http(s) URLs, control characters, oversized strings, malformed headers, and unexpected keys so native notification attachment creation never receives a fallback file/URL source or raw renderer payload.
    */
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_status_icon_data_url_allowed(value))
            .map(str::to_string)
            .map(Some)
            .ok_or(()),
    }
}

pub(crate) fn gpui_status_bridge_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_status_icon_data_url_allowed(value: &str) -> bool {
    if value.is_empty()
        || value.len() > GPUI_STATUS_INDICATOR_ICON_DATA_URL_MAX_CHARS
        || !value.is_ascii()
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return false;
    }
    let Some(comma_index) = value.find(',') else {
        return false;
    };
    let header = value[..comma_index].to_ascii_lowercase();
    let payload = &value[comma_index + 1..];
    header.starts_with("data:image/")
        && header.contains(";base64")
        && gpui_status_icon_base64_payload_allowed(payload)
}

pub(crate) fn gpui_status_icon_base64_payload_allowed(payload: &str) -> bool {
    if payload.is_empty() || !payload.len().is_multiple_of(4) {
        return false;
    }
    let padding_index = payload.find('=').unwrap_or(payload.len());
    let padding_len = payload.len().saturating_sub(padding_index);
    padding_len <= 2
        && payload[padding_index..].bytes().all(|byte| byte == b'=')
        && payload[..padding_index]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
}

pub(crate) fn gpui_status_title_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}
