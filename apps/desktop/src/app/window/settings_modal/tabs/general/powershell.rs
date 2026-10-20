//! The PowerShell 7 row of General > Terminal (Windows only): says PowerShell 7 is recommended while
//! only Windows PowerShell 5.1 is present, and installs it with winget through gxserver's
//! `/api/managedTools` (`powershell` tool, server/src/managed_tools/powershell.rs). The job runs in
//! gxserver, so closing Settings does not stop it; the row re-reads every 1.5 s while it runs.
//!
//! CDXC:ManagedTools 2026-10-08 DECISION:
//! User: "offer a one-click 'Install PowerShell 7' (via winget) in Settings › Terminal when only 5.1 is present. Sessions keep using 5.1 until 7 is installed, so nothing breaks." The row appears only while 7 is missing (and stays after an install started from it, to show the result); without winget it links to Microsoft's download page instead.

use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_icon, settings_list_item, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{post_store_message, store_gxserver_rpc};
use super::{GeneralCx, GeneralTab};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Task, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// A read runs `pwsh --version` and looks for winget.
const TIMEOUT: Duration = Duration::from_secs(30);
const SPINNER: &str = "modals/settings/loader-2.svg";

#[derive(Default)]
pub(super) struct PowerShellState {
    /// gxserver's `powershell` tool state (`ManagedToolState`).
    tool: Option<Value>,
    /// A start request that has not answered yet.
    starting: bool,
    /// The install ran from this page, so an installed tool still shows its result.
    touched: bool,
    poll: Option<Task<()>>,
}

fn text<'a>(tool: &'a Value, key: &str) -> Option<&'a str> {
    tool.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn active_job(tool: &Value) -> Option<&Value> {
    tool.get("job")
        .filter(|job| matches!(job["status"].as_str(), Some("queued" | "running")))
}

/// The last line of the installer's output that is not a progress bar or a blank.
fn progress_line(job: &Value) -> Option<String> {
    job["output"].as_str().and_then(|output| {
        output
            .lines()
            .flat_map(|line| line.split('\r'))
            .map(|line| line.trim().trim_matches(['-', '\\', '|', '/']).trim())
            .rfind(|line| !line.is_empty() && line.chars().any(char::is_alphanumeric))
            .map(str::to_string)
    })
}

impl GeneralTab {
    fn powershell_connected(&self, cx: &gpui::App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    /// Reads the tool's state; also the 1.5 s poll while its job runs.
    pub(super) fn load_powershell(&mut self, cx: &mut Context<Self>) {
        if !self.powershell_connected(cx) {
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/managedTools",
            json!({ "action": "read", "tool": "powershell" }),
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(tool) = result {
                        page.apply_powershell(tool, cx);
                    }
                });
            },
            cx,
        );
    }

    fn apply_powershell(&mut self, tool: Value, cx: &mut Context<Self>) {
        let was_running = self
            .powershell
            .tool
            .as_ref()
            .is_some_and(|previous| active_job(previous).is_some());
        let running = active_job(&tool).is_some();
        if was_running && !running {
            let (level, title, description) = match tool["job"]["status"].as_str() {
                Some("succeeded") => (
                    "success",
                    "PowerShell 7 installed",
                    "New terminals use it; terminals that are already open keep their shell."
                        .to_string(),
                ),
                _ => (
                    "error",
                    "PowerShell 7 install failed",
                    text(&tool["job"], "error")
                        .unwrap_or("The install did not finish.")
                        .to_string(),
                ),
            };
            self.store
                .update(cx, |store, cx| store.toast(level, title, &description, cx));
        }
        self.powershell.touched |= running || was_running;
        self.powershell.tool = Some(tool);
        if !running {
            self.powershell.poll = None;
        } else if self.powershell.poll.is_none() {
            self.powershell.poll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL_INTERVAL).await;
                    if this
                        .update(cx, |page, cx| page.load_powershell(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    fn install_powershell(&mut self, cx: &mut Context<Self>) {
        if !self.powershell_connected(cx) {
            return;
        }
        self.powershell.starting = true;
        self.powershell.touched = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/managedTools",
            json!({ "action": "start", "tool": "powershell", "operation": "install" }),
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.powershell.starting = false;
                    match result {
                        Ok(tool) => page.apply_powershell(tool, cx),
                        Err(error) => {
                            page.store.update(cx, |store, cx| {
                                store.toast(
                                    "error",
                                    "PowerShell 7 install failed",
                                    if error.is_empty() {
                                        "The install could not start."
                                    } else {
                                        &error
                                    },
                                    cx,
                                )
                            });
                            cx.notify();
                        }
                    }
                });
            },
            cx,
        );
    }

    /// The row, or `None` when PowerShell 7 is already there (and nothing was installed from here),
    /// the computer is not Windows, or gxserver has not answered.
    pub(super) fn powershell_row(
        &mut self,
        g: &GeneralCx,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !g.visible("terminal", "windowsPowerShell7") {
            return None;
        }
        let tool = self.powershell.tool.clone()?;
        if tool["supported"] != true {
            return None;
        }
        let installed = tool["installed"] == true;
        if installed && !self.powershell.touched {
            return None;
        }
        let p = &g.p;
        let job = active_job(&tool).cloned();
        let busy = job.is_some() || self.powershell.starting;
        let (status, detail) = if busy {
            let progress = job.as_ref().and_then(progress_line);
            (
                ListItemStatus::Neutral,
                match progress {
                    Some(progress) => format!("Installing PowerShell 7… {progress}"),
                    None => "Installing PowerShell 7…".to_string(),
                },
            )
        } else if installed {
            (
                ListItemStatus::Success,
                match text(&tool, "version") {
                    Some(version) => format!(
                        "PowerShell {version} is installed. New terminals use it; terminals that are already open keep their shell."
                    ),
                    None => "PowerShell 7 is installed. New terminals use it; terminals that are already open keep their shell.".to_string(),
                },
            )
        } else {
            let failure = tool["job"]["status"]
                .as_str()
                .filter(|status| *status == "failed")
                .and_then(|_| text(&tool["job"], "error"))
                .map(|error| format!(" The last install failed: {error}"))
                .unwrap_or_default();
            let blocker = text(&tool, "unavailableReason")
                .map(|reason| format!(" {reason}"))
                .unwrap_or_default();
            (
                ListItemStatus::Warning,
                format!(
                    "Recommended. Your terminals use Windows PowerShell 5.1 until PowerShell 7 is installed; nothing breaks in the meantime. New terminals switch to it right after the install, and terminals that are already open keep their shell.{blocker}{failure}"
                ),
            )
        };

        let mut controls: Vec<AnyElement> = Vec::new();
        if !installed {
            if let (Some(url), Some(_)) = (
                text(&tool, "downloadUrl").map(str::to_string),
                text(&tool, "unavailableReason"),
            ) {
                controls.push(settings_button(
                    p,
                    "powershell7-download",
                    "Download PowerShell 7",
                    Some("modals/settings/download.svg"),
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut GeneralTab, _window, cx| {
                        let store = page.store.clone();
                        post_store_message(
                            &store,
                            json!({ "type": "openExternalUrl", "url": url.clone() }),
                            cx,
                        );
                    },
                    cx,
                ));
            } else {
                let button = settings_button(
                    p,
                    "powershell7-install",
                    if busy {
                        "Installing…"
                    } else {
                        "Install PowerShell 7"
                    },
                    Some(if busy {
                        SPINNER
                    } else {
                        "modals/settings/download.svg"
                    }),
                    ButtonVariant::Outline,
                    busy,
                    Some("PowerShell 7 is being installed.".into()),
                    |page: &mut GeneralTab, _window, cx| page.install_powershell(cx),
                    cx,
                );
                controls.push(if busy {
                    button
                } else {
                    div()
                        .id("powershell7-install-tip")
                        .tooltip(tooltip_text(
                            text(&tool, "installPlan").unwrap_or_default().to_string(),
                        ))
                        .child(button)
                        .into_any_element()
                });
            }
        }
        Some(settings_list_item(
            p,
            Some(status),
            Some(icon_for(p)),
            "PowerShell 7",
            Some(div().whitespace_normal().child(detail).into_any_element()),
            (!controls.is_empty()).then(|| {
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(controls)
                    .into_any_element()
            }),
        ))
    }
}

fn icon_for(p: &SettingsPalette) -> AnyElement {
    settings_icon("modals/settings/terminal-2.svg", 17.0, p.muted).into_any_element()
}
