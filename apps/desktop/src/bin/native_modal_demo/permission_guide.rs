//! Floating permission guide preview: the edge tag beside a System Settings stand-in, placed the
//! way the app places it. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (waiting for the
//! drop into Screen Recording), `left` (the same, flipped to System Settings' left side because
//! the right has no room), `dropped` (it landed, Accessibility opens next), `next` (waiting for
//! the drop into Accessibility), `restarting` (SpaceO restarts by itself), `restarting-busy` (it
//! waits for two agents using SpaceO), `ready` (All set, closing), `flow` (steps through
//! waiting, dropped, next, restarting and ready every 1.5 seconds, the way a real grant goes).
use super::permission_guide_tag::*;
use gpui::prelude::*;
use gpui::{App, Context, IntoElement, Render, Window, div, px, rgb, rgba};

/// The stand-in System Settings window inside the demo, and where the app puts the tag beside it.
const SETTINGS_X: f32 = 40.0;
const SETTINGS_Y: f32 = 50.0;
const SETTINGS_W: f32 = 600.0;
const SETTINGS_H: f32 = 440.0;
const LIST_END_FROM_TOP: f32 = 236.0;
const EDGE_GAP: f32 = 14.0;

struct PermissionGuideDemo {
    state: String,
    /// `flow`'s step.
    step: usize,
}

const FLOW: [&str; 5] = ["waiting", "dropped", "next", "restarting", "ready"];

fn switch(on: bool) -> impl IntoElement {
    div()
        .w(px(30.0))
        .h(px(18.0))
        .rounded(px(9.0))
        .bg(if on { rgb(0x0a84ff) } else { rgb(0x4a4a4e) })
        .flex()
        .items_center()
        .when(on, |this| this.justify_end())
        .px(px(2.0))
        .child(div().size(px(14.0)).rounded_full().bg(rgb(0xffffff)))
}

fn app_row(name: &str, icon: u32, on: bool) -> impl IntoElement {
    div()
        .h(px(38.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .border_t_1()
        .border_color(rgba(0xffffff14))
        .child(div().size(px(22.0)).rounded(px(5.0)).bg(rgb(icon)))
        .child(div().flex_1().text_size(px(13.0)).child(name.to_string()))
        .child(switch(on))
}

fn system_settings(left: f32, screen: bool, spaceo_in: bool) -> impl IntoElement {
    let (pane, apps): (&str, [(&str, u32, bool); 3]) = if screen {
        (
            "Screen & System Audio Recording",
            [
                ("Ghostex", 0x3b6fd8, true),
                ("QuickTime Player", 0x8a8d93, false),
                ("Terminal", 0x1b1b1d, true),
            ],
        )
    } else {
        (
            "Accessibility",
            [
                ("Ghostex", 0x3b6fd8, true),
                ("Script Editor", 0xb9bcc3, false),
                ("Terminal", 0x1b1b1d, true),
            ],
        )
    };
    let sidebar = [
        "Wi-Fi",
        "Bluetooth",
        "Network",
        "General",
        "Accessibility",
        "Appearance",
        "Displays",
        "Notifications",
        "Privacy & Security",
    ];
    div()
        .absolute()
        .left(px(left))
        .top(px(SETTINGS_Y))
        .w(px(SETTINGS_W))
        .h(px(SETTINGS_H))
        .rounded(px(12.0))
        .bg(rgb(0x1e1e1e))
        .border_1()
        .border_color(rgba(0xffffff29))
        .overflow_hidden()
        .flex()
        .font_family(".SystemUIFont")
        .text_color(rgb(0xe6e6e8))
        .child(
            div()
                .w(px(180.0))
                .h_full()
                .bg(rgb(0x2a2a2a))
                .pt(px(44.0))
                .px(px(10.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(sidebar.map(|item| {
                    let selected = item == "Privacy & Security";
                    div()
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .text_size(px(12.5))
                        .when(selected, |this| this.bg(rgb(0x2463d5)))
                        .child(item)
                })),
        )
        .child(
            div()
                .flex_1()
                .p(px(16.0))
                .child(
                    div()
                        .h(px(36.0))
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(pane),
                )
                .child(
                    div()
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(0xffffff14))
                        .bg(rgba(0xffffff08))
                        .child(
                            div()
                                .p(px(12.0))
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(pane),
                                )
                                .child(
                                    div()
                                        .mt(px(3.0))
                                        .text_size(px(11.0))
                                        .text_color(rgb(0xa0a0a4))
                                        .child("Allow the applications below to use this."),
                                ),
                        )
                        .children(apps.map(|(name, icon, on)| app_row(name, icon, on)))
                        .when(spaceo_in, |this| {
                            this.child(app_row("spaceo", 0x050505, true))
                        })
                        .child(
                            div()
                                .h(px(30.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(14.0))
                                .border_t_1()
                                .border_color(rgba(0xffffff14))
                                .text_color(rgb(0xa0a0a4))
                                .child("+")
                                .child("−"),
                        ),
                ),
        )
}

impl Render for PermissionGuideDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let off = TagGrant::default();
        let on = TagGrant {
            has: true,
            needs_restart: false,
        };
        let restart = TagGrant {
            has: true,
            needs_restart: true,
        };
        let state = if self.state == "flow" {
            FLOW[self.step % FLOW.len()]
        } else {
            self.state.as_str()
        };
        let (stage, current, screen, access) = match state {
            "dropped" => (TagStage::Dropped, TagPermission::ScreenRecording, on, off),
            "next" => (TagStage::Waiting, TagPermission::Accessibility, on, off),
            "restarting" => (
                TagStage::Restarting { agents: 0 },
                TagPermission::Accessibility,
                restart,
                on,
            ),
            "restarting-busy" => (
                TagStage::Restarting { agents: 2 },
                TagPermission::Accessibility,
                restart,
                on,
            ),
            "ready" => (TagStage::Ready, TagPermission::Accessibility, on, on),
            _ => (TagStage::Waiting, TagPermission::ScreenRecording, off, off),
        };
        // `left`: System Settings sits against the screen's right edge, so the tag flips left.
        let side = if state == "left" {
            TagSide::Left
        } else {
            TagSide::Right
        };
        let (settings_x, tag_x) = match side {
            TagSide::Right => (SETTINGS_X, SETTINGS_X + SETTINGS_W + EDGE_GAP - NOTCH_ROOM),
            TagSide::Left => {
                let settings_x = DEMO_WIDTH - SETTINGS_W - SETTINGS_X;
                (settings_x, settings_x - EDGE_GAP - TAG_WIDTH)
            }
        };
        let content = tag_content(stage, current, screen, access, "spaceo", side);
        let height = tag_height(&content);
        let screen_pane = current == TagPermission::ScreenRecording;
        let spaceo_in = if screen_pane { screen.has } else { access.has };
        div()
            .size_full()
            .relative()
            .bg(gpui::linear_gradient(
                160.0,
                gpui::linear_color_stop(rgb(0x1a1d33), 0.0),
                gpui::linear_color_stop(rgb(0x0b0d18), 1.0),
            ))
            .child(system_settings(settings_x, screen_pane, spaceo_in))
            .child(
                div()
                    .absolute()
                    .left(px(tag_x))
                    .top(px(SETTINGS_Y + LIST_END_FROM_TOP - NOTCH_Y))
                    .w(px(WINDOW_WIDTH))
                    .h(px(height))
                    .child(render_tag(
                        content,
                        TagActions {
                            drag: Box::new(|_: &mut Self, _, _| eprintln!("drag spaceo")),
                            close: Box::new(|_, _, cx| cx.quit()),
                        },
                        cx,
                    )),
            )
    }
}

const DEMO_WIDTH: f32 = SETTINGS_X + SETTINGS_W + EDGE_GAP + TAG_WIDTH + 40.0;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let _ = super::open_modal_window(
        DEMO_WIDTH,
        SETTINGS_Y + SETTINGS_H + 40.0,
        move |_, cx| {
            cx.new(|cx| {
                if state == "flow" {
                    cx.spawn(async move |demo, cx| {
                        loop {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1500))
                                .await;
                            if demo
                                .update(cx, |demo: &mut PermissionGuideDemo, cx| {
                                    demo.step += 1;
                                    cx.notify();
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                PermissionGuideDemo { state, step: 0 }
            })
        },
        cx,
    );
}
