//! The Update / Up to date / Check for updates icon button Fast Computer & Browser Use and SpaceO
//! share, read from their `{prefix}Version`, `{prefix}LatestVersion` and `{prefix}UpdateAvailable`
//! status fields (helpers/os_cli/tool_updates.rs has the matching toast).

use super::*;

pub(super) struct UpdateButton {
    pub(super) id: &'static str,
    pub(super) name: String,
    /// The status fields' prefix: `cuaDriver` or `spaceo`.
    pub(super) prefix: &'static str,
    /// What the Update click posts (`installCuaDriver`, `installSpaceo`).
    pub(super) update_message: &'static str,
    /// What the other clicks post (`checkCuaDriverUpdate`, `checkSpaceoUpdate`).
    pub(super) check_message: &'static str,
    /// A sentence after the Update tooltip, such as SpaceO's "Agents using SpaceO finish first."
    pub(super) update_note: Option<String>,
    /// The tool's update job is running.
    pub(super) updating: bool,
    pub(super) disabled: bool,
    pub(super) disabled_reason: String,
}

impl IntegrationsTab {
    pub(super) fn update_button(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        button: UpdateButton,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = &button.name;
        let field = |key: &str| format!("{}{key}", button.prefix);
        let current = text(status, &field("Version"));
        let latest = text(status, &field("LatestVersion"));
        let installed_suffix = current
            .map(|current| format!(" (installed v{current})"))
            .unwrap_or_default();
        let note = button
            .update_note
            .map(|note| format!(". {note}"))
            .unwrap_or_default();
        let (icon, color, tooltip, message) = match flag(status, &field("UpdateAvailable")) {
            Some(true) => (
                ICON_CIRCLE_ARROW_UP,
                rgb(SKY_400),
                match latest {
                    Some(latest) => format!("Update {name} to v{latest}{installed_suffix}{note}"),
                    None => format!("Update {name}{installed_suffix}{note}"),
                },
                button.update_message,
            ),
            Some(false) => (
                ICON_CIRCLE_CHECK,
                p.muted,
                format!(
                    "{name}{} is up to date. Click to check again.",
                    current
                        .or(latest)
                        .map(|version| format!(" v{version}"))
                        .unwrap_or_default()
                ),
                button.check_message,
            ),
            None => (
                ICON_CLOUD_SEARCH,
                p.foreground,
                format!("Check for {name} updates{installed_suffix}"),
                button.check_message,
            ),
        };
        ghost_icon_button(
            p,
            button.id,
            if button.updating { ICON_LOADER } else { icon },
            color,
            tooltip,
            button.disabled,
            button.disabled_reason,
            move |page, _window, cx| page.post(message, cx),
            cx,
        )
    }
}
