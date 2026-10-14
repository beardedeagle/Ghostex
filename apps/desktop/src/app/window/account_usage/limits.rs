use super::{data::*, panel::AccountUsagePanel, style::*};
use crate::*;
use serde_json::Value;

impl AccountUsagePanel {
    /// CDXC:AgentProviders 2026-09-12 DECISION:
    /// User: Claude's Fable limit must stay visible beside the five-hour and weekly limits; additional models start collapsed.
    /// SEE-ALSO: packages/shared/account-usage-windows.ts (deleted 2026-10-01) and apps/desktop/src/app/titlebar/account_usage.rs select the two tightest headline limits.
    pub(super) fn render_limits(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let p = self.palette;
        let windows = array(&self.account, "usage");
        let main: Vec<_> = windows
            .iter()
            .filter(|w| w["model"].is_null() && w["id"] != "spend")
            .collect();
        let session = main
            .iter()
            .copied()
            .find(|w| w["id"] == "fiveHour" || w["limitWindowSeconds"] == 18000);
        let weekly = main.iter().copied().find(|w| {
            w["id"] == "sevenDay" || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604800
        });
        let scoped: Vec<_> = windows
            .iter()
            .filter(|w| !text(w, "model").is_empty())
            .collect();
        let fable = (!p.codex)
            .then(|| {
                scoped
                    .iter()
                    .copied()
                    .find(|w| text(w, "model").to_lowercase().contains("fable"))
                    .or_else(|| scoped.first().copied())
            })
            .flatten();
        let other: Vec<_> = windows
            .iter()
            .filter(|w| {
                w["id"] != "spend"
                    && ![session, weekly, fable]
                        .into_iter()
                        .flatten()
                        .any(|selected| std::ptr::eq(*w, selected))
            })
            .collect();
        let mut bars = v_flex()
            .w_full()
            .gap(px(13.0))
            .child(self.render_bar(session, "5-hour limit".into(), 12.0))
            .child(self.render_bar(weekly, "Weekly limit".into(), 12.0));
        if let Some(fable) = fable {
            bars = bars.child(self.render_bar(
                Some(fable),
                format!("{} weekly limit", text(fable, "model")),
                12.0,
            ));
        }
        // CDXC:AgentProviders 2026-10-11 DECISION:
        // User: the usage card must not contradict itself. When gxserver marks the reading stale (refreshing has failed since it was taken), the pill says "Last known" in amber and the header says how long ago it was updated; "Live limits" shows only for a fresh reading.
        let (dot, pill) = if self.account["usageStale"] == true {
            (
                gpui::Hsla::from(chrome_color(0xf0a94f, 0x986009)),
                "Last known",
            )
        } else {
            (p.accent, "Live limits")
        };
        let mut tag = p.tag(true);
        if self.account["usageStale"] == true {
            tag = tag.bg(dot.opacity(0.12)).text_color(dot);
        }
        let mut card = p
            .card()
            .child(
                section_heading().child(heading("This account")).child(
                    tag.child(div().size(px(6.0)).rounded_full().bg(dot).shadow(vec![
                        shadow(0.0, 0.0, 0.0, dot.opacity(0.25)).spread_radius(px(2.0)),
                        shadow(0.0, 0.0, 8.0, dot),
                    ]))
                    .child(pill),
                ),
            )
            .child(bars);
        if !other.is_empty() {
            let mut models = v_flex().w_full().mt(px(12.0));
            models = models.child(
                h_flex()
                    .id("account-usage-models-toggle")
                    .track_focus(&self.model_focus)
                    .focus_visible(move |this| this.shadow(focus_ring(p)))
                    .items_center()
                    .self_start()
                    .gap(px(6.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(15.95))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|this| this.text_color(chrome_color(0xe4e4e2, 0x292524)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_models(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            this.toggle_models(cx);
                        }
                    }))
                    .child(chevron(self.models_open, false, p.muted))
                    .child(format!("More model limits ({})", other.len())),
            );
            if self.models_open {
                models =
                    models.child(
                        v_flex()
                            .w_full()
                            .mt(px(12.0))
                            .pl(px(14.0))
                            .border_l_2()
                            .border_color(p.strong)
                            .gap(px(13.0))
                            .children(other.into_iter().map(|w| {
                                self.render_bar(Some(w), text(w, "label").to_string(), 11.0)
                            })),
                    );
            }
            card = card.child(models);
        }
        let footer = |top: bool, title: &'static str| {
            h_flex()
                .w_full()
                .mt(px(if top { 13.0 } else { 9.0 }))
                .when(top, |this| {
                    this.pt(px(11.0)).border_t_1().border_color(p.line)
                })
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .text_size(px(11.0))
                .line_height(px(15.95))
                .text_color(p.muted)
                .child(title)
        };
        if !p.codex {
            let extra = windows
                .iter()
                .find(|w| w["id"] == "spend")
                .and_then(|w| w["usedPercent"].as_f64())
                .map(|n| format!("{}% used", n.round()))
                .unwrap_or_else(|| "Not reported".into());
            card = card.child(
                footer(true, "Extra usage").child(
                    label(extra, 11.0, chrome_color(0xe4e4e2, 0x292524).into())
                        .font_weight(FontWeight::MEDIUM),
                ),
            );
        }
        // CDXC:AgentProviders 2026-09-24 DECISION:
        // User: Claude resets must be shown and usable like Codex resets. Claude keeps its Extra usage row and gains the same Rate limit resets row beneath it once Anthropic reports the reset program for the account.
        let reports_resets = p.codex
            || self.account["resetCredits"].is_u64()
            || !text(&self.account, "resetCreditsError").is_empty();
        if reports_resets {
            let resets_text = self.account["resetCredits"]
                .as_u64()
                .map(|n| format!("{n} available"))
                .unwrap_or_else(|| "Not reported".into());
            card = card.child(
                footer(p.codex, "Rate limit resets").child(
                    h_flex()
                        .id("account-usage-reset-toggle")
                        .track_focus(&self.reset_focus)
                        .focus_visible(move |this| this.shadow(focus_ring(p)))
                        .h(px(22.0))
                        .pl(px(9.0))
                        .pr(px(4.0))
                        .items_center()
                        .gap(px(6.0))
                        .rounded_full()
                        .border_1()
                        .border_color(p.accent_line)
                        .bg(p.soft)
                        .text_color(p.accent_ink)
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(move |this| this.bg(p.accent.opacity(0.22)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_resets(cx);
                        }))
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.toggle_resets(cx);
                            }
                        }))
                        .child(resets_text)
                        .child(chevron(true, self.resets_open, p.accent_ink)),
                ),
            );
            if self.resets_open {
                card = card.child(self.render_resets(cx));
            }
        }
        card.into_any_element()
    }

    fn render_bar(&self, bar: Option<&Value>, title: String, label_size: f32) -> AnyElement {
        let p = self.palette;
        let bar = bar.unwrap_or(&Value::Null);
        let percent = bar["usedPercent"].as_f64();
        let warning = if p.codex {
            pace_warning(bar, self.now)
        } else {
            String::new()
        };
        let reset = text(bar, "resetsAt");
        let reset = if reset.is_empty() {
            String::new()
        } else {
            reset_text(reset, self.now)
        };
        let mut meta = h_flex()
            .flex_shrink_0()
            .items_baseline()
            .whitespace_nowrap()
            .font_features(tabular_numbers())
            .text_size(px(10.5))
            .line_height(px(15.225))
            .text_color(p.muted);
        if let Some(percent) = percent {
            meta = meta
                .child(
                    label(
                        format!("{}%", percent.round()),
                        11.5,
                        chrome_color(0xf2f2f0, 0x1c1917).into(),
                    )
                    .font_weight(FontWeight::SEMIBOLD)
                    .mr(px(1.0)),
                )
                .when(!reset.is_empty(), |this| this.child(format!(" · {reset}")));
        } else {
            meta = meta.child(if reset.is_empty() {
                "Not reported".into()
            } else {
                reset
            });
        }
        let (from, to, glow) = if percent.unwrap_or(0.0) >= 95.0 {
            (
                rgb(0xd9463c).into(),
                rgb(0xf5817a).into(),
                rgb(0xee6b62).opacity(0.45).into(),
            )
        } else if !warning.is_empty() || percent.unwrap_or(0.0) >= 80.0 {
            (
                rgb(0xd98a2b).into(),
                rgb(0xf5b95c).into(),
                chrome_color(0xf0a94f, 0x986009).opacity(0.45).into(),
            )
        } else {
            (p.deep, p.light, p.accent.opacity(0.45))
        };
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                h_flex()
                    .w_full()
                    .items_baseline()
                    .justify_between()
                    .gap(px(10.0))
                    .mb(px(6.0))
                    .child(
                        label(title, label_size, chrome_color(0xe4e4e2, 0x292524).into())
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis(),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(mix(p.accent, 0.07, chrome_color(0x2c2b2a, 0xe7e5e4).into()))
                    // A hard black inset on a light track reads as a dent rather than depth.
                    .shadow(vec![
                        shadow(
                            0.0,
                            1.0,
                            1.0,
                            rgb(0)
                                .opacity(if chrome_uses_light_appearance() {
                                    0.10
                                } else {
                                    0.35
                                })
                                .into(),
                        )
                        .inset(),
                    ])
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(
                                (percent.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0) as f32,
                            ))
                            .rounded_full()
                            .bg(gradient(90.0, from, to))
                            .shadow(vec![shadow(0.0, 0.0, 8.0, glow)]),
                    ),
            )
            .when(!warning.is_empty(), |this| {
                this.child(
                    h_flex()
                        .mt(px(6.0))
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(10.5))
                        .line_height(px(15.225))
                        .text_color(chrome_color(0xf0a94f, 0x986009))
                        .child(
                            div()
                                .size(px(6.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(chrome_color(0xf0a94f, 0x986009)),
                        )
                        .child(warning),
                )
            })
            .into_any_element()
    }
}
