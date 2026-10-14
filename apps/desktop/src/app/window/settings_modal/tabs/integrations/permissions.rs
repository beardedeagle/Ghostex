//! The Accessibility and Screen Recording rows of SpaceO and Fast Computer & Browser Use in the
//! Desktop control section, and the re-checks that keep their status live while the page is
//! open (the grant flow itself is in helpers/os_cli/tool_permissions.rs).

use super::*;
use gpui::Subscription;
use std::time::Instant;

const ICON_ACCESSIBILITY: &str = "modals/settings/shield.svg";
const ICON_SCREEN_RECORDING: &str = "modals/settings/eye.svg";

/// While a Grant is pending the page re-checks every 2 seconds for at most 10 minutes;
/// otherwise every 10 seconds.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);
const WATCH_LIMIT: Duration = Duration::from_secs(10 * 60);
const IDLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PermissionTool {
    Spaceo,
    CuaDriver,
}

impl PermissionTool {
    fn id(self) -> &'static str {
        match self {
            Self::Spaceo => "spaceo",
            Self::CuaDriver => "cuaDriver",
        }
    }

    fn name(self) -> String {
        match self {
            Self::Spaceo => spaceo::spaceo_name(),
            Self::CuaDriver => trycua_name(),
        }
    }

    /// The entry macOS lists in System Settings for this tool's grants.
    fn grantee(self) -> &'static str {
        match self {
            Self::Spaceo => "spaceo",
            Self::CuaDriver => "CuaDriver",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Permission {
    Accessibility,
    ScreenRecording,
}

impl Permission {
    const ALL: [Self; 2] = [Self::Accessibility, Self::ScreenRecording];

    fn id(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ScreenRecording => "screenRecording",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::ScreenRecording => "Screen Recording",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionState {
    Checking,
    NotInstalled,
    Granted,
    NotGranted,
    NeedsRestart,
    Restarting,
    Unknown,
}

/// The state the host computed (`GpuiToolPermissionState`): SpaceO's per-grant strings, Fast
/// Computer & Browser Use's booleans from CuaDriver's own check.
fn permission_state(
    status: Option<&Value>,
    tool: PermissionTool,
    permission: Permission,
) -> PermissionState {
    if status.is_none() {
        return PermissionState::Checking;
    }
    match tool {
        PermissionTool::Spaceo => {
            if flag(status, "spaceoInstalled") != Some(true) {
                return PermissionState::NotInstalled;
            }
            let key = match permission {
                Permission::Accessibility => "spaceoAccessibilityPermission",
                Permission::ScreenRecording => "spaceoScreenRecordingPermission",
            };
            match text(status, key) {
                Some("granted") => PermissionState::Granted,
                Some("notGranted") => PermissionState::NotGranted,
                Some("needsRestart") if flag(status, "spaceoRestarting") == Some(true) => {
                    PermissionState::Restarting
                }
                Some("needsRestart") => PermissionState::NeedsRestart,
                _ => PermissionState::Unknown,
            }
        }
        PermissionTool::CuaDriver => {
            if flag(status, "cuaDriverInstalled") != Some(true) {
                return PermissionState::NotInstalled;
            }
            let key = match permission {
                Permission::Accessibility => "cuaDriverAccessibilityPermissionGranted",
                Permission::ScreenRecording => "cuaDriverScreenRecordingPermissionGranted",
            };
            match flag(status, key) {
                Some(true) => PermissionState::Granted,
                Some(false) => PermissionState::NotGranted,
                None => PermissionState::Unknown,
            }
        }
    }
}

/// A Grant click: the tool's grants are re-checked every 2 seconds until they land.
struct PermissionWatch {
    tool: PermissionTool,
    started: Instant,
    /// SpaceO's restart was asked for once the grant landed, so its daemon picks the grant up.
    restart_requested: bool,
}

#[derive(Default)]
pub(super) struct PermissionPolling {
    watch: Option<PermissionWatch>,
    last_check: Option<Instant>,
    ticker: Option<Task<()>>,
    activation: Option<Subscription>,
}

impl IntegrationsTab {
    /// Starts the once-a-second tick that decides when to re-check (`permission_tick`).
    pub(super) fn start_permission_polling(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") {
            return;
        }
        self.permissions.ticker = Some(cx.spawn(async move |page, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if page
                    .update(cx, |page, cx| page.permission_tick(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// Coming back from System Settings re-checks at the next tick instead of waiting.
    pub(super) fn observe_permission_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.permissions.activation.is_some() || !cfg!(target_os = "macos") {
            return;
        }
        self.permissions.activation = Some(cx.observe_window_activation(
            window,
            |page: &mut Self, window, _cx| {
                if window.is_window_active() {
                    page.permissions.last_check = None;
                }
            },
        ));
    }

    fn permission_tick(&mut self, cx: &mut Context<Self>) {
        let (active, status) = {
            let store = self.store.read(cx);
            (
                store.active_tab() == SettingsTabId::Integrations,
                store.host_payload("ghostexCliStatus").cloned(),
            )
        };
        let Some(status) = status else {
            return;
        };
        let status = Some(&status);
        if !active
            || (flag(status, "spaceoInstalled") != Some(true)
                && flag(status, "cuaDriverInstalled") != Some(true))
        {
            return;
        }
        if let Some(watch) = self.permissions.watch.as_mut() {
            let states =
                Permission::ALL.map(|permission| permission_state(status, watch.tool, permission));
            if states
                .iter()
                .all(|state| *state == PermissionState::Granted)
            {
                let name = watch.tool.name();
                self.permissions.watch = None;
                let description = format!("{name} has Accessibility and Screen Recording.");
                self.store.update(cx, |store, cx| {
                    store.toast("success", "Permissions granted", &description, cx)
                });
                cx.notify();
            } else if watch.started.elapsed() >= WATCH_LIMIT {
                self.permissions.watch = None;
                cx.notify();
            } else if watch.tool == PermissionTool::Spaceo
                && !watch.restart_requested
                && states.contains(&PermissionState::NeedsRestart)
            {
                watch.restart_requested = true;
                post_store_message(&self.store, json!({ "type": "restartSpaceo" }), cx);
            }
        }
        let interval = if self.permissions.watch.is_some() {
            WATCH_INTERVAL
        } else {
            IDLE_INTERVAL
        };
        if self
            .permissions
            .last_check
            .is_none_or(|checked| checked.elapsed() >= interval)
        {
            self.permissions.last_check = Some(Instant::now());
            post_store_message(
                &self.store,
                json!({ "type": "requestToolPermissionStatus" }),
                cx,
            );
        }
    }

    fn start_permission_grant(
        &mut self,
        tool: PermissionTool,
        permission: Permission,
        cx: &mut Context<Self>,
    ) {
        post_store_message(
            &self.store,
            json!({ "type": "grantToolPermission", "tool": tool.id(), "permission": permission.id() }),
            cx,
        );
        self.permissions.watch = Some(PermissionWatch {
            tool,
            started: Instant::now(),
            restart_requested: false,
        });
        cx.notify();
    }

    /// One row per grant. Every row always shows its status, granted or not.
    pub(super) fn permission_rows(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        tool: PermissionTool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        Permission::ALL
            .into_iter()
            .map(|permission| self.permission_row(p, status, tool, permission, cx))
            .collect()
    }

    fn permission_row(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        tool: PermissionTool,
        permission: Permission,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = tool.name();
        let state = permission_state(status, tool, permission);
        let watching = self
            .permissions
            .watch
            .as_ref()
            .is_some_and(|watch| watch.tool == tool);
        let why = match (tool, permission) {
            (PermissionTool::Spaceo, Permission::Accessibility) => {
                format!("Lets {name} click and type in the apps it runs for agents.")
            }
            (PermissionTool::Spaceo, Permission::ScreenRecording) => {
                format!("Lets {name} take screenshots of the apps it runs for agents.")
            }
            (PermissionTool::CuaDriver, Permission::Accessibility) => {
                format!("Lets {name} click and type in apps.")
            }
            (PermissionTool::CuaDriver, Permission::ScreenRecording) => {
                format!("Lets {name} see what is on the desktop.")
            }
        };
        let (status_label, tone) = match state {
            PermissionState::Checking => ("Checking", ListItemStatus::Neutral),
            PermissionState::NotInstalled => ("Not installed", ListItemStatus::Neutral),
            PermissionState::Granted => ("Granted", ListItemStatus::Success),
            PermissionState::NotGranted => ("Not granted", ListItemStatus::Warning),
            PermissionState::NeedsRestart => ("Needs restart", ListItemStatus::Warning),
            PermissionState::Restarting => ("Restarting", ListItemStatus::Warning),
            PermissionState::Unknown => ("Unknown", ListItemStatus::Neutral),
        };
        let grantee = tool.grantee();
        let pane = permission.label();
        let detail = match state {
            PermissionState::NotGranted | PermissionState::Unknown if watching => format!(
                "Turn on {grantee} in System Settings > Privacy & Security > {pane}. This page updates as soon as you do."
            ),
            PermissionState::NotGranted => format!(
                "{why} Grant asks macOS for it and opens System Settings, where you turn on {grantee}."
            ),
            PermissionState::Unknown => format!("{why} Ghostex could not read this permission."),
            PermissionState::NeedsRestart => format!(
                "macOS allowed it after {name} started, so {name} needs a restart to use it. The restart waits for agents using {name} to finish."
            ),
            PermissionState::Restarting => {
                format!("{name} restarts as soon as the agents using it finish.")
            }
            PermissionState::NotInstalled => format!("{why} Install {name} first."),
            PermissionState::Granted | PermissionState::Checking => why,
        };
        let row_id = format!("{}-{}", tool.id(), permission.id());
        let mut controls = vec![permission_status_label(p, status_label, tone)];
        match state {
            PermissionState::NotGranted | PermissionState::Unknown => {
                controls.push(settings_button(
                    p,
                    ElementId::Name(format!("integrations-grant-{row_id}").into()),
                    "Grant",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.start_permission_grant(tool, permission, cx)
                    },
                    cx,
                ));
            }
            PermissionState::NeedsRestart | PermissionState::Restarting => {
                let restarting = state == PermissionState::Restarting;
                controls.push(settings_button(
                    p,
                    ElementId::Name(format!("integrations-restart-{row_id}").into()),
                    if restarting {
                        format!("Restarting {name}…")
                    } else {
                        format!("Restart {name}")
                    },
                    restarting.then_some(ICON_LOADER),
                    ButtonVariant::Outline,
                    restarting,
                    Some(format!("{name} restarts once the agents using it finish.").into()),
                    |page: &mut Self, _window, cx| {
                        post_store_message(&page.store, json!({ "type": "restartSpaceo" }), cx);
                    },
                    cx,
                ));
            }
            _ => {}
        }
        permission_row_layout(
            p,
            &row_id,
            tone,
            match permission {
                Permission::Accessibility => ICON_ACCESSIBILITY,
                Permission::ScreenRecording => ICON_SCREEN_RECORDING,
            },
            format!("{name}: {pane}"),
            detail,
            controls,
        )
    }
}

/// The status word with its dot, always visible at the row's end.
///
/// CDXC:Settings 2026-10-14 DECISION:
/// User: "we should always show the status of each permission after it's done." Permission rows spell their state out (Granted, Not granted, Needs restart) next to the dot, an exception to the 2026-09-09 dot-only rule the other Integrations rows keep.
fn permission_status_label(p: &SettingsPalette, label: &str, tone: ListItemStatus) -> AnyElement {
    let (dot, color) = match tone {
        ListItemStatus::Success => (modal_rgba(0x34d399, 0.8), modal_rgba(0x34d399, 1.0)),
        ListItemStatus::Warning => (modal_rgba(0xfbbf24, 0.85), modal_rgba(0xfbbf24, 1.0)),
        ListItemStatus::Neutral => (modal_rgba(0xffffff, 0.2), p.muted),
    };
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .child(div().size(px(6.0)).rounded_full().bg(hsla(dot)))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(if p.light { p.foreground } else { color }))
                .child(label.to_string()),
        )
        .into_any_element()
}

/// `integration_row`'s layout with the detail written under the label instead of behind the
/// info icon, so what to do next is always readable.
fn permission_row_layout(
    p: &SettingsPalette,
    id: &str,
    tone: ListItemStatus,
    icon: &'static str,
    label: String,
    detail: String,
    controls: Vec<AnyElement>,
) -> AnyElement {
    let dot = match tone {
        ListItemStatus::Success => modal_rgba(0x34d399, 0.8),
        ListItemStatus::Warning => modal_rgba(0xfbbf24, 0.85),
        ListItemStatus::Neutral => modal_rgba(0xffffff, 0.2),
    };
    h_flex()
        .id(ElementId::Name(format!("integration-row-{id}").into()))
        .w_full()
        .min_h(px(52.0))
        .px(px(20.0))
        .py(px(10.0))
        .gap(px(24.0))
        .items_center()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(dot)),
        )
        .child(
            div()
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .child(settings_icon(icon, 18.0, p.muted)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.9))
                        .text_color(hsla(p.foreground))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(detail),
                ),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .gap(px(12.0))
                .children(controls),
        )
        .into_any_element()
}
