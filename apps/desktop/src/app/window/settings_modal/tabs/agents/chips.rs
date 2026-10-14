//! The bottom of the Agents list: the "More agents" disclosure with its compact grid of agents
//! that are off and were never used (a click turns one on), and the one-time offer to turn off
//! agents that are on but were never used.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_icon, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::icons;
use super::logos::agent_icon;
use super::model::AgentButton;
use super::status::emerald;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Transformation, div, px, radians,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// The offer shows only once; this setting remembers it was answered.
const TIDY_UP_DISMISSED: &str = "agentsTidyUpOfferDismissed";
/// The offer needs at least this many unused agents to be worth showing.
const TIDY_UP_MIN: usize = 3;

impl AgentsTab {
    /// The "More agents" row and, while it is open, the grid.
    pub(super) fn render_more_agents(
        &mut self,
        p: &SettingsPalette,
        agents: &[AgentButton],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if agents.is_empty() {
            return None;
        }
        let open = self.more_agents_open;
        let muted = p.muted;
        let hover = p.raised_hover;
        let chevron = settings_icon(icons::CHEVRON_DOWN, 15.0, muted)
            .when(!open, |icon| {
                icon.with_transformation(Transformation::rotate(radians(
                    -std::f32::consts::FRAC_PI_2,
                )))
            })
            .flex_shrink_0();
        let peek = (!open).then(|| {
            h_flex().gap(px(6.0)).items_center().opacity(0.75).children(
                agents
                    .iter()
                    .take(6)
                    .map(|agent| agent_icon(agent.icon.as_deref(), p)),
            )
        });
        let toggle = h_flex()
            .id("agents-more-toggle")
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("More agents, {}", agents.len())))
            .aria_expanded(open)
            .w_full()
            .px(px(20.0))
            .py(px(12.0))
            .gap(px(10.0))
            .items_center()
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(hover)))
            .on_press(cx, |page, _window, cx| {
                page.more_agents_open = !page.more_agents_open;
                cx.notify();
            })
            .child(chevron)
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(hsla(p.foreground))
                    .child("More agents"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(muted))
                    .child(agents.len().to_string()),
            )
            .children(peek)
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(muted))
                    .child(if open {
                        "Click an agent to turn it on"
                    } else {
                        "Off and never used"
                    }),
            );
        let mut block = v_flex().w_full().child(toggle);
        if open {
            let chips: Vec<AnyElement> = agents
                .iter()
                .map(|agent| self.render_agent_chip(p, agent, cx))
                .collect();
            block = block.child(
                div()
                    .w_full()
                    .px(px(16.0))
                    .pb(px(16.0))
                    .pt(px(2.0))
                    .grid()
                    .grid_cols(4)
                    .gap(px(8.0))
                    .children(chips),
            );
        }
        Some(block.into_any_element())
    }

    fn render_agent_chip(
        &mut self,
        p: &SettingsPalette,
        agent: &AgentButton,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let found = self.cli_found(&self.cli_agent_id(agent));
        let (_, dot) = emerald(p);
        let fills = neutral_button_fills(p.light);
        let target = agent.clone();
        h_flex()
            .id(SharedString::from(format!("agent-chip-{}", agent.agent_id)))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("Turn on {}", agent.name)))
            .min_w_0()
            .h(px(36.0))
            .px(px(10.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(9.0))
            .border_1()
            .border_color(transparent())
            .bg(hsla(fills.rest))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(fills.hover)))
            .active(move |this| this.bg(hsla(fills.pressed)))
            .tooltip(tooltip_text(if found {
                format!("Turn on {} (already on this computer)", agent.name)
            } else {
                format!("Turn on {}", agent.name)
            }))
            .on_press(cx, move |page, _window, cx| {
                page.turn_on_agent(&target, cx);
            })
            .child(agent_icon(agent.icon.as_deref(), p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .text_color(hsla(p.foreground))
                    .child(agent.name.clone()),
            )
            .when(found, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(hsla(dot)),
                )
            })
            .child(settings_icon(icons::PLUS, 14.0, p.muted).flex_shrink_0())
            .into_any_element()
    }

    /// The one-time offer: shown when some agent was used here and at least a few built-in
    /// agents that are on never were. Nothing turns off unless the user says so.
    pub(super) fn render_tidy_up(
        &mut self,
        p: &SettingsPalette,
        agents: &[AgentButton],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.roster.is_none() || self.store.read(cx).values().bool(TIDY_UP_DISMISSED) {
            return None;
        }
        if !agents.iter().any(AgentButton::used_before) {
            return None;
        }
        // The agents the Defaults card points at stay on.
        let (prompt_agent, title_agent) = {
            let values = self.store.read(cx).values();
            (
                values.string("defaultPromptAgentId"),
                values.string("sessionTitleGenerationAgent"),
            )
        };
        let unused: Vec<String> = agents
            .iter()
            .filter(|agent| agent.enabled && agent.is_default && !agent.used_before())
            .filter(|agent| agent.agent_id != prompt_agent && agent.agent_id != title_agent)
            .map(|agent| agent.agent_id.clone())
            .collect();
        if unused.len() < TIDY_UP_MIN {
            return None;
        }
        let on = agents.iter().filter(|agent| agent.enabled).count();
        let count = unused.len();
        let turn_off = settings_button_sized(
            p,
            "agents-tidy-turn-off",
            format!("Turn off {count} unused agents"),
            None,
            ButtonVariant::Default,
            ButtonSize::Sm,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.set_agents_enabled(unused.clone(), false, cx);
                page.save(TIDY_UP_DISMISSED, json!(true), cx);
            },
            cx,
        );
        let keep = settings_button_sized(
            p,
            "agents-tidy-keep",
            "Keep all",
            None,
            ButtonVariant::Ghost,
            ButtonSize::Sm,
            false,
            None,
            |page: &mut Self, _window, cx| page.save(TIDY_UP_DISMISSED, json!(true), cx),
            cx,
        );
        Some(
            h_flex()
                .id("agents-tidy-up")
                .m(px(12.0))
                .p(px(12.0))
                .gap(px(12.0))
                .items_start()
                .rounded(px(10.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(css_fade(p.raised_hover, 0.6)))
                .child(settings_icon(icons::SPARKLES, 16.0, p.settings_accent).flex_shrink_0())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(13.5))
                                .text_color(hsla(p.foreground))
                                .child(format!(
                                    "You have {on} agents turned on, and {count} of them were never used here"
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(hsla(p.muted))
                                .child("Turning them off keeps the New session menu short. They stay one click away under More agents."),
                        )
                        .child(
                            h_flex()
                                .pt(px(8.0))
                                .gap(px(8.0))
                                .child(turn_off)
                                .child(keep),
                        ),
                )
                .into_any_element(),
        )
    }
}
