//! Settings > Terminal's own-config actions: open Ghostex's terminal config, and Import from
//! Ghostty (fonts and cursor, never the theme) into it.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_ghostex_terminal_config_file(&mut self, cx: &mut gpui::Context<Self>) {
        let opened = shared_settings::prepare_ghostex_terminal_config_file_for_open()
            .map_err(|_| "Ghostex could not create its terminal config file.".to_string())
            .and_then(|path| gpui_open_path(&path));
        if let Err(message) = opened {
            self.dispatch_gpui_settings_action_status(
                "openGhostexTerminalConfigFile",
                false,
                &message,
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open terminal config",
                &message,
                cx,
            );
            return;
        }
        self.dispatch_gpui_settings_action_status(
            "openGhostexTerminalConfigFile",
            true,
            "Ghostex terminal config opened with the OS file handler.",
            cx,
        );
    }

    /// Saves the imported values through the same path as a Settings save (Settings file, then
    /// the managed block of Ghostex's terminal config), adds the font lines Settings has no row
    /// for, then reloads the terminals and the open Settings window.
    pub(crate) fn import_gpui_ghostty_config(&mut self, cx: &mut gpui::Context<Self>) {
        let import = match shared_settings::ghostty_config_migration::read_ghostty_config_import() {
            Ok(import) => import,
            Err(_) => {
                let message = "The Ghostty app has no config file on this computer, so there is nothing to import.";
                self.dispatch_gpui_settings_action_status(
                    "importGhosttyConfig",
                    false,
                    message,
                    cx,
                );
                self.dispatch_gpui_app_modal_toast("warning", "Nothing to import", message, cx);
                return;
            }
        };
        if import.summary.is_empty() {
            let message = "Your Ghostty config sets no fonts, line height or cursor settings.";
            self.dispatch_gpui_settings_action_status("importGhosttyConfig", true, message, cx);
            self.dispatch_gpui_app_modal_toast("info", "Nothing to import", message, cx);
            return;
        }

        let previous_settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        let mut settings_object = previous_settings_object.clone();
        for (key, value) in &import.settings {
            settings_object.insert((*key).to_string(), value.clone());
        }
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            let message = "Ghostex could not save the imported terminal settings.";
            self.dispatch_gpui_settings_action_status("importGhosttyConfig", false, message, cx);
            self.dispatch_gpui_app_modal_toast("warning", "Import failed", message, cx);
            return;
        };
        let changed_keys = shared_settings::ghostty_terminal_config_backed_setting_keys_changed(
            &previous_settings_object,
            write_result.snapshot.object(),
        );
        self.sync_gpui_ghostty_config_file_after_settings_save(
            &changed_keys,
            &write_result.snapshot,
            cx,
        );
        if !import.extra_keys.is_empty()
            && shared_settings::merge_ghostex_terminal_config_lines(
                &import.extra_keys,
                import.extra_lines.clone(),
            )
            .is_err()
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not update terminal config",
                "Settings were imported, but Ghostex could not add the extra font lines to its terminal config.",
                cx,
            );
        }
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);

        let message = format!(
            "{}. The theme and colors were not imported.",
            import.summary.join(", ")
        );
        self.dispatch_gpui_settings_action_status("importGhosttyConfig", true, &message, cx);
        self.dispatch_gpui_app_modal_toast("success", "Imported from Ghostty", &message, cx);
    }
}
