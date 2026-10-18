//! A machine switch's memory and its focus restore inside the app (the rule is gx-core
//! `sidebar_view/machine_switch.rs`).
//!
//! Every `SelectMachine` that moves the selected machine passes through the store's intent
//! applier, which first remembers the focused session under its own computer
//! ([`GhostexGpuiApp::gx_store_remember_machine_switch_session`]). Only a switch made with the
//! sidebar's machine picker restores: a reveal or a notification click switches machines on its
//! way to a session it focuses itself, and a tab correction (a machine removed or outside the
//! window's workspace) is not the user going back to a computer.
//!
//! The restore goes out as the same `focusSession` with `keepView` a Space switch posts
//! (space_switch.rs), so a remote session opens on its computer the way a sidebar click opens it.

use ghostex_gx_core::{
    SidebarUiIntent, SpaceSwitchFocus, machine_switch_memory, plan_machine_switch_restore,
};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// The machine a `selectMachine` from the picker moves to, read before the intent moves it.
pub(crate) struct MachineSwitchBefore {
    next_machine_id: String,
}

impl GhostexGpuiApp {
    /// Before a `SelectMachine` is applied: remembers the focused session under its own computer.
    /// Does nothing for any other intent, or when the selection would not move.
    pub(super) fn gx_store_remember_machine_switch_session(
        &mut self,
        intent: &SidebarUiIntent,
        cx: &mut gpui::Context<Self>,
    ) {
        let SidebarUiIntent::SelectMachine { machine_id } = intent else {
            return;
        };
        let memory = machine_switch_memory(
            self.gx_store.sidebar_ui.state(),
            self.gx_store.core.focus().focused_session.as_ref(),
            machine_id,
        );
        if let Some(memory) = memory {
            self.gx_store_apply_sidebar_ui_intent_unbuilt(memory, cx);
        }
    }

    /// The picker's `selectMachine` that will really move the selected machine; `None` for any
    /// other command.
    pub(crate) fn gx_store_machine_switch_before(
        &self,
        command: &Value,
    ) -> Option<MachineSwitchBefore> {
        if command.get("type").and_then(Value::as_str) != Some("selectMachine") {
            return None;
        }
        let next_machine_id = command.get("machineId").and_then(Value::as_str)?;
        (self.gx_store.sidebar_ui.selected_machine_id() != next_machine_id).then(|| {
            MachineSwitchBefore {
                next_machine_id: next_machine_id.to_string(),
            }
        })
    }

    /// Reopens the session the computer the picker switched to was last left on. Called after the
    /// intent has been applied and the list rebuilt for that computer, so the rows it picks from
    /// are the ones it now draws. Nothing remembered, nothing drawn, or an offline computer leaves
    /// the focus where it is.
    pub(crate) fn gx_store_restore_machine_switch_focus(
        &mut self,
        before: MachineSwitchBefore,
        cx: &mut gpui::Context<Self>,
    ) {
        // The switch did not land (a tab correction moved it straight back, say).
        if self.gx_store.sidebar_ui.selected_machine_id() != before.next_machine_id {
            return;
        }
        let plan = plan_machine_switch_restore(
            &self.gx_store.core,
            self.gx_store.sidebar_ui.state(),
            self.gx_store.sidebar_list.view(),
            &before.next_machine_id,
        );
        if let Some(SpaceSwitchFocus::Session { sidebar_session_id }) = plan {
            self.dispatch_native_sidebar_command(
                json!({
                    "type": "focusSession",
                    "sessionId": sidebar_session_id,
                    "keepView": true,
                }),
                cx,
            );
        }
    }
}
