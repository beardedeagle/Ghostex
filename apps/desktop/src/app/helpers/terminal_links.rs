use std::path::Path;

/// Files clicked in a terminal open in the system app when Settings > File opening says so or the
/// Files view cannot show them; the rest go through the chat file routing, which opens them where
/// File opening says. Extension-only so the decision is cross-platform; the caller separately
/// verifies that the resolved path is a real local file before launching it.
pub(crate) fn gpui_terminal_file_opens_with_os_default(path: &Path) -> bool {
    crate::app::native_docs::entry::file_open_target_for_path(path)
        == crate::shared_settings::SharedFileOpenTarget::SystemApp
}
