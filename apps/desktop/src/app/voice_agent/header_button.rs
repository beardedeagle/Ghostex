//! The work area header's Voice button.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "the one in the chat box just pastes whatever i say into the chat box, while the one above is more general and for discussing". The header carries a global Voice button that starts and ends a spoken conversation with the manager voice agent, which keeps going while the user moves between projects and chats. It sits with the ⋯ menu, so a narrow column never scrolls it out of reach.

use gpui::prelude::FluentBuilder as _;
use gpui::{InteractiveElement as _, IntoElement, MouseButton, Styled as _, px, rgba};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::conversation::VoicePhase;
use crate::app::helpers::*;
use crate::app::render::workarea_header::toggles::header_panel_toggle_button;
use crate::*;

const ICON_HEADSET: &str = "titlebar/headset.svg";

impl GhostexGpuiApp {
    pub(crate) fn render_workarea_header_voice_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let agent = &self.voice_agent;
        let active = agent.active();
        let stopped = agent.phase == VoicePhase::Ended;
        let icon_color = if stopped {
            Some(rgba(0xfbbf24ff).into())
        } else if active {
            Some(rgba(0x4ade80ff).into())
        } else {
            None
        };
        let tooltip: gpui::SharedString = titlebar_tooltip_label(
            if active {
                "End the voice conversation"
            } else {
                "Talk to the voice agent"
            },
            "toggleVoiceAgent",
        );
        header_panel_toggle_button(
            "ghostex-gpui-workarea-header-voice",
            ICON_HEADSET,
            0.0,
            true,
            icon_color,
        )
        .role(gpui::Role::Button)
        .aria_label(tooltip.clone())
        .when(active, |this| this.bg(titlebar_active_segment_color()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.toggle_voice_agent_conversation(cx);
            }),
        )
        .on_a11y_action(gpui::AccessibleAction::Click, {
            let app = cx.entity().downgrade();
            move |_, _, cx| {
                let _ = app.update(cx, |this, cx| this.toggle_voice_agent_conversation(cx));
            }
        })
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Auto, move |window, cx| {
            titlebar_tooltip(tooltip.clone(), window, cx)
        })
        .mr(px(2.0))
    }
}
