//! Automatic agents chat modes, GPUI engine terminal views and the terminal text-input focus handoff.

use std::collections::HashSet;

use gpui::Bounds;
use gpui::Entity;
use gpui::Focusable as _;
use gpui::Pixels;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// The agent action bar reads chat eligibility straight off the app when it
    /// renders (apps/desktop/src/app/render/terminal_agent_action_bar.rs), so
    /// nothing has to be pushed into the terminal views any more. What still
    /// belongs on this reconcile edge is the automatic Chat handoff, which
    /// fires the moment a session first becomes chat-eligible.
    pub(crate) fn sync_gpui_engine_agents_chat_eligibility(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let chat_view_session_ids = self
            .agents_workspace
            .terminal_sessions
            .iter()
            .map(|session| session.id)
            .filter(|session_id| self.agents_terminal_runtime_is_live_for_chat_launch(*session_id))
            .filter(|session_id| self.agents_session_chat_eligible(*session_id))
            .collect::<HashSet<_>>();
        self.reconcile_automatic_agents_chat_modes(&chat_view_session_ids, cx);
    }

    pub(crate) fn reconcile_automatic_agents_chat_modes(
        &mut self,
        eligible_session_ids: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The Default Agent View is resolved per session, not once for the whole
        sweep: a per-agent override can put one agent in Chat while the global
        preference keeps every other agent in the terminal. Recording the value
        each session was considered under is also what replaces the old global
        "preference just enabled" flag — a session is swept again exactly when
        its own effective value changes, so a global flip and an override flip
        are the same edge instead of two mechanisms.
        */
        let shared_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings_object = shared_settings_snapshot.object();
        // CDXC:SessionChat 2026-09-06 WHY:
        // Project restore can briefly lack eligible terminal runtimes or sidebar metadata; that gap must not make an existing session new to the Chat default again.
        self.agents_chat_auto_switch_observed_sessions
            .retain(|session_id, _| self.agents_workspace.has_session(*session_id));
        let mut newly_eligible = Vec::new();
        for session_id in eligible_session_ids.iter().copied() {
            let effective_interface = gpui_effective_preferred_agent_interface_for_agent_icon(
                settings_object,
                self.agents_workspace
                    .session(session_id)
                    .and_then(|session| session.agent_icon),
            );
            let previous_interface = self
                .agents_chat_auto_switch_observed_sessions
                .get(&session_id)
                .copied();
            if effective_interface == GpuiPreferredAgentInterface::Chat
                && previous_interface != Some(effective_interface)
            {
                newly_eligible.push(session_id);
            }
            self.agents_chat_auto_switch_observed_sessions
                .insert(session_id, effective_interface);
        }

        let mut switched = Vec::new();
        for session_id in newly_eligible {
            if !self.agents_chat_mode_sessions.insert(session_id) {
                continue;
            }
            switched.push(session_id);
            /*
            CDXC:Drafts 2026-08-18:
            This is the switch a user never asked for: they started an agent by
            typing into a terminal, and the moment it becomes chat-eligible the
            app moves them to Chat. Anything already typed into the CLI composer
            has to come with them, or it is simply gone from view behind the
            parked terminal.
            */
            self.request_session_chat_draft_transfer(session_id, cx);
        }
        if switched.is_empty() {
            return;
        }
        /*
        CDXC:SessionChat 2026-10-01 WHY:
        Only the focused session's own switch to Chat hands it the keyboard. The handoff used to fire for the focused session whenever any session in the sweep switched, so a coordinator starting a thread (a new session turning chat-eligible) pulled the keyboard into the focused chat's composer while the user was typing in its search field, a note or an answer.
        */
        if let Some(focused_session_id) = self.focused_agents_or_companion_shell_session_id()
            && switched.contains(&focused_session_id)
        {
            self.request_keyboard_handoff_for_session(focused_session_id);
        }
        self.reconcile_agents_pane_surfaces(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn gpui_engine_terminal_view_for_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<Entity<terminal_element::TerminalView>> {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
        }
    }

    /*
    CDXC:FocusRouting 2026-07-04-05:45:
    GPUI-engine terminals are GPUI-owned key surfaces inside a main window
    whose AppKit first responder can be parked on a CEF child view (sidebar/
    browser interaction) or a native Ghostty host view. gpui makes its
    content NSView first responder only once at window creation and never
    reclaims it on click, so focusing the engine element's FocusHandle alone
    leaves hardware key events flowing into Chromium and the terminal dead
    to typing. Every engine-view focus must therefore return first-responder
    ownership to the exact GPUI parent view first — the same handoff the
    GPUI address bar and terminal search input already perform — and then
    focus the element handle for GPUI-side dispatch.
    */
    pub(crate) fn focus_gpui_engine_terminal_view(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        view: &Entity<terminal_element::TerminalView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:Diagnostics 2026-08-24: this is the primitive
        // that yanks AppKit first responder off any CEF surface (the chat
        // composer included) onto the GPUI root, so record every execution
        // with the responder it is about to displace.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.engineTerminalViewFocused",
            serde_json::json!({
                "target": format!("{target:?}"),
                "shellFocus": format!("{:?}", self.shell_focus),
                "firstResponderTarget": format!("{:?}", self.first_responder_target),
            }),
        );
        #[cfg(target_os = "macos")]
        self.begin_programmatic_focus();
        // CDXC:FocusRouting 2026-09-18 WHY:
        // GPUI FocusHandle changes do not move X11 keyboard focus out of a CEF child. Linux needs the same native-root handoff as macOS and Windows or terminal clicks leave typing in Chromium until the window is reactivated.
        let focus_root = cef_parent_native_view(window).unwrap_or(self.parent_ns_view);
        cef::focus_gpui_root_view(focus_root);
        let focus_handle = view.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        self.composited_terminal_keyboard_owner = Some((focus_root as usize, target));
        #[cfg(target_os = "macos")]
        {
            /*
            CDXC:FocusRouting 2026-07-30:
            App-level shell focus and GPUI's FocusHandle already identify the
            exact composited terminal synchronously. Do not wait for the
            terminal's next rendered prepaint edge to update native keyboard
            ownership: hidden/remounted views can retain their cached focused
            bit, leaving the router on an old terminal or the generic GPUI
            responder while dictation events arrive. Claim the same exact
            terminal target as part of this canonical focus handoff.
            */
            update_gpui_keyboard_router_composited_terminal_focus(
                focus_root,
                target,
                true,
                self.first_responder_target,
            );
            self.end_programmatic_focus();
        }
    }

    /// Per-surface request helpers kept for their call sites; each files the one `PendingKeyboardHandoff` that `drain_pending_keyboard_handoff` executes.
    pub(crate) fn request_agents_terminal_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) {
        self.request_keyboard_handoff(PendingKeyboardHandoff {
            target: ShellFocusTarget::AgentsPane(slot_id.pane_id),
            session_id: Some(slot_id.session_id),
            command_session_id: None,
        });
    }

    pub(crate) fn request_command_terminal_text_focus_handoff(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) {
        self.request_keyboard_handoff(PendingKeyboardHandoff {
            target: ShellFocusTarget::CommandPane,
            session_id: None,
            command_session_id: Some(slot_id.session_id),
        });
    }

    /// The GPUI-engine view backing the focused target, resolved by `shell_keyboard_owner`: a chat-mode session's parked terminal is never returned, so root-forwarded text, paste, and zoom cannot reach a hidden PTY.
    pub(crate) fn focused_gpui_engine_terminal_view(
        &self,
    ) -> Option<Entity<terminal_element::TerminalView>> {
        match self.shell_keyboard_owner() {
            ShellKeyboardOwner::EngineTerminal { view, .. } => Some(view),
            _ => None,
        }
    }

    pub(crate) fn request_focused_command_terminal_text_focus_handoff(&mut self) {
        let Some((group_id, session_id)) = self.command_pane.focused_group_active_session_id()
        else {
            return;
        };
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
    }

    pub(crate) fn request_command_group_terminal_text_focus_handoff(
        &mut self,
        group_id: CommandPaneGroupId,
    ) {
        let Some(session_id) = self
            .command_pane
            .find_leaf(group_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
        else {
            return;
        };
        self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        });
    }

    pub(crate) fn focused_terminal_text_mount_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        match focused_terminal_text_target(self.active_mode, self.shell_focus)? {
            FocusedTerminalTextTarget::Agents => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .map(FocusedTerminalTextMountTarget::Agents),
            FocusedTerminalTextTarget::Command => {
                focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                    .map(FocusedTerminalTextMountTarget::Command)
            }
        }
    }

    pub(crate) fn terminal_text_input_should_track_agents_slot(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.focused_terminal_text_mount_target()
            == Some(FocusedTerminalTextMountTarget::Agents(slot_id))
    }

    pub(crate) fn register_agents_terminal_text_input_handler(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        _bounds: Bounds<Pixels>,
        _view: Entity<Self>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = slot_id;
        self.drain_pending_keyboard_handoff(window, cx);
    }

    pub(crate) fn register_command_terminal_text_input_handler(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        _bounds: Bounds<Pixels>,
        _view: Entity<Self>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = slot_id;
        self.drain_pending_keyboard_handoff(window, cx);
    }

    pub(crate) fn terminal_text_service_accepts_text_input(&self, window: &Window) -> bool {
        self.terminal_text_focus_handle.is_focused(window)
            && self.exact_focused_terminal_text_surface_target().is_some()
    }

    pub(crate) fn exact_focused_terminal_text_surface_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        None
    }
}
