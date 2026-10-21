// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: hover-link checks, GPUI-engine terminal search actions, and the queued-prompts chip plus per-surface search bar renderers.

use std::collections::HashMap;

use gpui::AnyElement;
use gpui::App;
use gpui::AppContext as _;
use gpui::Entity;
use gpui::Focusable as _;
use gpui::Window;
use gpui_component::input::InputEvent;
use gpui_component::input::InputState;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn command_terminal_slot_hovers_link(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.command_terminal_runtime_osc_states
            .get(&command_terminal_runtime_session_id(slot_id))
            .is_some_and(|state| state.hovered_link_url.is_some())
    }

    /// Cmd+F on a focused GPUI-engine terminal opens the search bar, driving the element's
    /// viewport find.
    pub(crate) fn start_search_in_focused_gpui_engine_terminal(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let target = match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(target) => target,
            None => return false,
        };
        let (record, osc_states) = match target {
            FocusedTerminalTextTarget::Agents => {
                let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    &self.agents_workspace,
                ) else {
                    return false;
                };
                let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.agents_terminal_runtime_osc_states)
            }
            FocusedTerminalTextTarget::Command => {
                let Some(slot_id) = focused_command_terminal_surface_mount_slot(
                    self.shell_focus,
                    &self.command_pane,
                ) else {
                    return false;
                };
                let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.command_terminal_runtime_osc_states)
            }
        };
        let runtime_session_id = record.runtime_session_id;
        let view = record.view.clone();
        let state = osc_states.entry(runtime_session_id).or_default();
        if state.search.is_none() {
            state.search = Some(GpuiTerminalSearchState::default());
        }
        view.update(cx, |view, cx| view.set_search_needle("", cx));
        self.terminal_search_focus_pending = Some(runtime_session_id);
        cx.notify();
        true
    }

    /// Drive a GPUI-engine terminal's find from the shared search-bar action
    /// vocabulary (`search:<needle>`, `navigate_search:*`, `end_search`).
    pub(crate) fn perform_gpui_engine_terminal_search_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<bool> {
        let record = self.gpui_engine_record_for_runtime_session_id(runtime_session_id)?;
        let view = record.view.clone();
        Some(match action {
            "navigate_search:next" => {
                view.update(cx, |view, cx| view.navigate_search(true, cx));
                true
            }
            "navigate_search:previous" => {
                view.update(cx, |view, cx| view.navigate_search(false, cx));
                true
            }
            "end_search" => {
                view.update(cx, |view, cx| view.clear_search(cx));
                true
            }
            _ => {
                if let Some(needle) = action.strip_prefix("search:") {
                    let needle = needle.to_string();
                    view.update(cx, |view, cx| view.set_search_needle(&needle, cx));
                    true
                } else {
                    false
                }
            }
        })
    }

    pub(crate) fn start_search_in_focused_terminal_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.start_search_in_focused_gpui_engine_terminal(cx)
    }

    /// Off macOS every terminal is a GPUI-engine terminal, so the shared
    /// search-bar actions drive only the element's viewport find.
    pub(crate) fn perform_terminal_search_binding_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.perform_gpui_engine_terminal_search_action(runtime_session_id, action, cx)
            .unwrap_or(false)
    }

    /// Typing in the GPUI search bar mirrors macOS ownership: the local search
    /// state's needle is the source of truth updated from the field, then the
    /// needle is pushed into Ghostty via the `search:<needle>` keybind action.
    pub(crate) fn update_terminal_search_needle(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        needle: &str,
    ) -> bool {
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(search) = osc_states
                .get_mut(&runtime_session_id)
                .and_then(|state| state.search.as_mut())
            {
                search.needle = needle.to_string();
                return true;
            }
        }
        false
    }

    pub(crate) fn handle_terminal_search_input_event(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        input: &Entity<InputState>,
        event: &InputEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let needle = input.read(cx).value().to_string();
                if self.update_terminal_search_needle(runtime_session_id, &needle) {
                    let _ = self.perform_terminal_search_binding_action(
                        runtime_session_id,
                        &format!("search:{needle}"),
                        cx,
                    );
                }
            }
            InputEvent::PressEnter { shift, .. } => {
                let action = if *shift {
                    "navigate_search:previous"
                } else {
                    "navigate_search:next"
                };
                let _ = self.perform_terminal_search_binding_action(runtime_session_id, action, cx);
            }
            InputEvent::Focus => self.reclaim_gpui_root_for_chrome_input_focus(),
            InputEvent::Blur => {}
        }
    }

    pub(crate) fn close_terminal_search(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.perform_terminal_search_binding_action(runtime_session_id, "end_search", cx);
        let mut closed = false;
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(state) = osc_states.get_mut(&runtime_session_id) {
                closed |= state.search.take().is_some();
            }
        }
        if !closed {
            return;
        }
        if let Some(slot_id) = self.agents_terminal_search_owner_slot(runtime_session_id) {
            self.focus_agents_terminal_mount_slot(slot_id, window, cx);
        } else if let Some(slot_id) = self.command_terminal_search_owner_slot(runtime_session_id) {
            self.focus_command_terminal_mount_slot(slot_id, window, cx);
        }
        cx.notify();
    }

    /// The Agents mount slot whose GPUI-engine terminal runs `runtime_session_id`.
    fn agents_terminal_search_owner_slot(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<AgentsTerminalBodyMountSlotId> {
        self.agents_gpui_engine_terminals
            .iter()
            .find(|(_, record)| record.runtime_session_id == runtime_session_id)
            .and_then(|(session_id, _)| {
                let pane_id = self.agents_workspace.pane_id_for_session(*session_id)?;
                Some(AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: *session_id,
                })
            })
    }

    /// The command-pane mount slot whose GPUI-engine terminal runs `runtime_session_id`.
    fn command_terminal_search_owner_slot(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<CommandTerminalBodyMountSlotId> {
        self.command_gpui_engine_terminals
            .iter()
            .find(|(_, record)| record.runtime_session_id == runtime_session_id)
            .and_then(|(session_id, _)| {
                self.command_pane
                    .flat_tab_ids()
                    .into_iter()
                    .find(|(_, tab_session_id)| tab_session_id == session_id)
                    .map(|(group_id, session_id)| CommandTerminalBodyMountSlotId {
                        group_id,
                        session_id,
                    })
            })
    }

    /// Whether any live terminal search input currently holds GPUI keyboard
    /// focus. Shell focus stays on the terminal pane while the search bar is
    /// open, so terminal key routing that is derived from shell focus must
    /// consult this before treating a keystroke as terminal input.
    pub(crate) fn terminal_search_input_owns_keyboard_focus(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.terminal_search_inputs
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Keeps one live search input per terminal with an active Ghostty search
    /// state, mirrors Ghostty-provided needles into the field, and applies a
    /// pending open-focus so Cmd+F immediately types into the bar like macOS.
    pub(crate) fn sync_terminal_search_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut active_needles: HashMap<AgentsTerminalRuntimeSessionId, String> = HashMap::new();
        for (runtime_session_id, state) in self
            .agents_terminal_runtime_osc_states
            .iter()
            .chain(self.command_terminal_runtime_osc_states.iter())
        {
            if let Some(search) = &state.search {
                active_needles.insert(*runtime_session_id, search.needle.clone());
            }
        }
        self.terminal_search_inputs
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        self.terminal_search_input_subscriptions
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        for (runtime_session_id, needle) in active_needles {
            let input = match self.terminal_search_inputs.get(&runtime_session_id) {
                Some(input) => input.clone(),
                None => {
                    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
                    let subscription = cx.subscribe(
                        &input,
                        move |this: &mut Self, input, event: &InputEvent, cx| {
                            this.handle_terminal_search_input_event(
                                runtime_session_id,
                                &input,
                                event,
                                cx,
                            );
                        },
                    );
                    self.terminal_search_inputs
                        .insert(runtime_session_id, input.clone());
                    self.terminal_search_input_subscriptions
                        .insert(runtime_session_id, subscription);
                    input
                }
            };
            if input.read(cx).value().as_ref() != needle {
                input.update(cx, |input, cx| input.set_value(needle, window, cx));
            }
        }
        if let Some(runtime_session_id) = self.terminal_search_focus_pending.take()
            && let Some(input) = self
                .terminal_search_inputs
                .get(&runtime_session_id)
                .cloned()
        {
            #[cfg(target_os = "macos")]
            self.begin_programmatic_focus();
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            cef::focus_gpui_root_view(self.parent_ns_view);
            input.update(cx, |input, cx| input.focus(window, cx));
            #[cfg(target_os = "macos")]
            self.end_programmatic_focus();
        }
    }

    pub(crate) fn render_agents_terminal_search_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let slot_id = self
            .agents_workspace
            .terminal_body_mount_candidate(leaf)
            .mount_slot_id()?;
        let runtime_session_id = self.agents_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .agents_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("agents-{}-{}", slot_id.pane_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn agents_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        self.agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.runtime_session_id)
    }

    pub(crate) fn render_command_terminal_search_bar(
        &self,
        leaf: &CommandPaneLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id: leaf.group_id,
            session_id,
        };
        let runtime_session_id = self.command_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .command_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("command-{}-{}", slot_id.group_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn command_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        self.command_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.runtime_session_id)
    }
}
