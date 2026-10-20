//! The voice panel: a small native child window under the work area header's top right that shows
//! the open conversation's state, live captions and tool lines, with Mute, Stop talking and
//! Hang up, and says why a conversation stopped.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the conversation keeps going while the user moves between projects and chats, so the panel belongs to the main window, not to a chat or a view: it is an owned pop-up window attached as an AppKit child of the main window (the app toast window's pattern), which moves with the window, never takes focus, and overlaps nothing interactive of its own. A size change of the main window places it again.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, px, rgba, size,
};
use gpui_component::{h_flex, v_flex};

use super::conversation::{VoicePhase, VoiceProblem, VoiceRole, VoiceRow};
use crate::app::helpers::*;
use crate::app::render::workarea_header::workarea_header_bottom_y;
use crate::app::window::*;
use crate::*;

const PANEL_WIDTH: f32 = 340.0;
const PANEL_HEIGHT: f32 = 292.0;
/// From the window's trailing edge and below the header.
const PANEL_MARGIN: f32 = 12.0;
const PANEL_RADIUS: f32 = 12.0;
/// How long the main window's size must hold before the panel is placed again.
const REPOSITION_DELAY: std::time::Duration = std::time::Duration::from_millis(160);

const ICON_MIC: &str = "titlebar/microphone.svg";
const ICON_MIC_OFF: &str = "titlebar/microphone-off.svg";
const ICON_HANG_UP: &str = "titlebar/phone-off.svg";
const ICON_STOP: &str = "titlebar/player-stop.svg";
const ICON_TOOL: &str = "titlebar/tool.svg";
const ICON_CHECK: &str = "titlebar/check.svg";
const ICON_ALERT: &str = "titlebar/alert-triangle.svg";
const ICON_CLOSE: &str = "titlebar/x.svg";
const ICON_SETTINGS: &str = "titlebar/settings.svg";
const ICON_HEADSET: &str = "titlebar/headset.svg";

/// The panel's colours for the chrome's appearance.
struct PanelColors {
    background: Hsla,
    border: Hsla,
    ink: Hsla,
    text: Hsla,
    muted: Hsla,
}

impl PanelColors {
    fn resolve() -> Self {
        let light = chrome_uses_light_appearance();
        let ink: Hsla = if light { gpui::black() } else { gpui::white() };
        Self {
            background: if light {
                rgba(0xfafafaf7).into()
            } else {
                rgba(0x1e1e21f7).into()
            },
            border: ink.opacity(if light { 0.10 } else { 0.12 }),
            ink,
            text: ink.opacity(0.90),
            muted: ink.opacity(0.55),
        }
    }
}

/// The state's colour dot: green while it listens, blue while you talk, amber while it works,
/// violet while it speaks.
fn state_color(state: &str) -> Hsla {
    match state {
        "listening" => rgba(0x4ade80ff),
        "userSpeaking" => rgba(0x38bdf8ff),
        "thinking" | "toolRunning" => rgba(0xfbbf24ff),
        "speaking" => rgba(0xa78bfaff),
        _ => rgba(0x9ca3afff),
    }
    .into()
}

/// What the panel draws, copied out of the app and pushed into the window.
///
/// CDXC:VoiceMode 2026-10-20 WHY: the panel must never read the app entity while rendering: `open_window` renders the new window at once, inside the app's own update (the Voice button's click), and reading the app there panics with "already being updated" and aborts the app.
struct PanelSnapshot {
    phase: VoicePhase,
    state: String,
    tool_label: Option<String>,
    rows: Vec<VoiceRow>,
    muted: bool,
    problem: Option<VoiceProblem>,
    voice_label: Option<String>,
}

pub(crate) struct VoicePanelWindow {
    app: WeakEntity<GhostexGpuiApp>,
    snapshot: PanelSnapshot,
}

impl GhostexGpuiApp {
    /// Opens, refreshes or closes the panel to match the conversation, and redraws the Voice
    /// button.
    pub(crate) fn sync_voice_agent_panel(&mut self, cx: &mut gpui::Context<Self>) {
        cx.notify();
        if self.voice_agent.phase == VoicePhase::Idle {
            self.voice_agent.panel_reposition = None;
            self.voice_agent.panel_placed_for = None;
            if let Some(panel) = self.voice_agent.panel.take() {
                let _ = panel.update(cx, |_, window, _| window.remove_window());
            }
            return;
        }
        if let Some(panel) = self.voice_agent.panel {
            let snapshot = self.voice_panel_snapshot();
            let updated = panel.update(cx, |panel, _, cx| {
                panel.snapshot = snapshot;
                cx.notify();
            });
            if updated.is_ok() {
                return;
            }
            self.voice_agent.panel = None;
        }
        self.open_voice_agent_panel_window(cx);
    }

    fn open_voice_agent_panel_window(&mut self, cx: &mut gpui::Context<Self>) {
        let main = self.main_window_bounds;
        let bounds = Bounds {
            origin: point(
                main.origin.x + main.size.width - px(PANEL_WIDTH + PANEL_MARGIN),
                main.origin.y + px(workarea_header_bottom_y() + PANEL_MARGIN / 2.0),
            ),
            size: size(px(PANEL_WIDTH), px(PANEL_HEIGHT)),
        };
        let app = cx.entity().downgrade();
        let snapshot = self.voice_panel_snapshot();
        let main_window_native_view = self.parent_ns_view;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: self.main_window_popup_owner().display_for(bounds, cx),
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        self.voice_agent.panel = cx
            .open_window(options, move |panel_window, cx| {
                panel_window.set_background_corner_radius(px(PANEL_RADIUS));
                remove_gpui_app_toast_popup_window_chrome(panel_window, main_window_native_view);
                cx.new(|_| VoicePanelWindow { app, snapshot })
            })
            .ok();
        self.voice_agent.panel_placed_for = Some(main.size);
    }

    /// Places the panel again after the main window changed size (it follows moves by itself, as
    /// an AppKit child window). Called from the main window's render.
    pub(crate) fn voice_agent_follow_main_window(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(placed_for) = self.voice_agent.panel_placed_for else {
            return;
        };
        if self.voice_agent.panel.is_none()
            || placed_for == self.main_window_bounds.size
            || self.voice_agent.panel_reposition.is_some()
        {
            return;
        }
        self.voice_agent.panel_reposition = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REPOSITION_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.voice_agent.panel_reposition = None;
                if this.voice_agent.panel_placed_for == Some(this.main_window_bounds.size) {
                    return;
                }
                if let Some(panel) = this.voice_agent.panel.take() {
                    let _ = panel.update(cx, |_, window, _| window.remove_window());
                }
                if this.voice_agent.phase != VoicePhase::Idle {
                    this.open_voice_agent_panel_window(cx);
                }
            });
        }));
    }

    fn voice_panel_snapshot(&self) -> PanelSnapshot {
        let agent = &self.voice_agent;
        PanelSnapshot {
            phase: agent.phase,
            state: agent.state.clone(),
            tool_label: agent.tool_label.clone(),
            rows: agent.rows.iter().rev().take(12).rev().cloned().collect(),
            muted: agent.muted,
            problem: agent.problem.clone(),
            voice_label: agent.voice_label.clone(),
        }
    }
}

/// What a panel button asks the app to do.
#[derive(Clone, Copy)]
enum PanelAction {
    Mute,
    StopTalking,
    HangUp,
    Close,
    StartAgain,
    OpenSettings,
}

impl VoicePanelWindow {
    fn run(&mut self, action: PanelAction, cx: &mut Context<Self>) {
        let app = self.app.clone();
        // Deferred: the app's answer updates (or removes) this very window.
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| match action {
                PanelAction::Mute => app.toggle_voice_agent_mute(cx),
                PanelAction::StopTalking => app.interrupt_voice_agent(cx),
                PanelAction::HangUp => app.hang_up_voice_agent(cx),
                PanelAction::Close => app.close_voice_agent_panel(cx),
                PanelAction::StartAgain => {
                    app.close_voice_agent_panel(cx);
                    app.start_voice_agent_conversation(cx);
                }
                PanelAction::OpenSettings => {
                    app.close_voice_agent_panel(cx);
                    app.open_gpui_settings_voice_agent_page(cx);
                }
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: Option<&'static str>,
        action: PanelAction,
        colors: &PanelColors,
        danger: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (fill, hover, foreground): (Hsla, Hsla, Hsla) = if danger {
            (
                rgba(0xdc2626ff).into(),
                rgba(0xef4444ff).into(),
                gpui::white(),
            )
        } else {
            (
                colors.ink.opacity(0.07),
                colors.ink.opacity(0.13),
                colors.text,
            )
        };
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(SharedString::new_static(label.unwrap_or(id)))
            .h(px(28.0))
            .px(px(if label.is_some() { 10.0 } else { 7.0 }))
            .gap(px(6.0))
            .items_center()
            .rounded(px(8.0))
            .bg(fill)
            .text_color(foreground)
            .text_size(px(12.5))
            .cursor_default()
            .hover(move |this| this.bg(hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.run(action, cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let panel = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| this.run(action, cx));
                }
            })
            .child(titlebar_svg_icon(icon, 14.0, foreground))
            .when_some(label, |this, label| this.child(label))
            .into_any_element()
    }

    fn header(&self, snapshot: &PanelSnapshot, colors: &PanelColors) -> AnyElement {
        let (label, busy): (String, bool) = match snapshot.phase {
            VoicePhase::Starting => ("Connecting…".to_string(), true),
            VoicePhase::Ending => ("Hanging up…".to_string(), true),
            VoicePhase::Ended => ("Voice agent stopped".to_string(), false),
            VoicePhase::Idle | VoicePhase::Live => match snapshot.state.as_str() {
                "listening" if snapshot.muted => ("Muted".to_string(), false),
                "listening" => ("Listening".to_string(), false),
                "userSpeaking" => ("Hearing you".to_string(), false),
                "thinking" => ("Thinking…".to_string(), true),
                "toolRunning" => (
                    snapshot
                        .tool_label
                        .clone()
                        .unwrap_or_else(|| "Working…".to_string()),
                    true,
                ),
                "speaking" => ("Speaking".to_string(), false),
                _ => ("Connecting…".to_string(), true),
            },
        };
        let indicator: AnyElement = if snapshot.phase == VoicePhase::Ended {
            titlebar_svg_icon(ICON_ALERT, 14.0, rgba(0xfbbf24ff).into()).into_any_element()
        } else if busy {
            canvas(
                |_, _, _| {},
                |bounds, _, window, _| {
                    window.request_animation_frame();
                    paint_agent_gui_loading_spinner(bounds, window);
                },
            )
            .size(px(12.0))
            .into_any_element()
        } else {
            let color = if snapshot.muted {
                colors.muted
            } else {
                state_color(&snapshot.state)
            };
            div()
                .size(px(9.0))
                .rounded_full()
                .bg(color)
                .into_any_element()
        };
        h_flex()
            .h(px(36.0))
            .px(px(12.0))
            .gap(px(8.0))
            .items_center()
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex()
                    .size(px(14.0))
                    .items_center()
                    .justify_center()
                    .child(indicator),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(colors.text)
                    .child(label),
            )
            .child(
                h_flex()
                    .gap(px(5.0))
                    .items_center()
                    .text_size(px(11.5))
                    .text_color(colors.muted)
                    .child(titlebar_svg_icon(ICON_HEADSET, 12.0, colors.muted))
                    .child(
                        snapshot
                            .voice_label
                            .clone()
                            .unwrap_or_else(|| "Voice agent".to_string()),
                    ),
            )
            .into_any_element()
    }

    fn row(row: &VoiceRow, colors: &PanelColors) -> AnyElement {
        match row {
            VoiceRow::Caption {
                role, text, cut, ..
            } => {
                let you = *role == VoiceRole::User;
                v_flex()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colors.muted)
                            .child(if you { "You" } else { "Agent" }),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(if you {
                                colors.text.opacity(0.78)
                            } else {
                                colors.text
                            })
                            .child(if *cut {
                                format!("{text}…")
                            } else {
                                text.clone()
                            }),
                    )
                    .into_any_element()
            }
            VoiceRow::Tool { label, ok, .. } => h_flex()
                .gap(px(6.0))
                .items_center()
                .text_size(px(11.5))
                .text_color(colors.muted)
                .child(titlebar_svg_icon(ICON_TOOL, 12.0, colors.muted))
                .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                .when(*ok == Some(true), |this| {
                    this.child(titlebar_svg_icon(ICON_CHECK, 12.0, rgba(0x4ade80ff).into()))
                })
                .when(*ok == Some(false), |this| {
                    this.child(titlebar_svg_icon(ICON_CLOSE, 12.0, rgba(0xf87171ff).into()))
                })
                .into_any_element(),
            VoiceRow::Announcement { text } => div()
                .text_size(px(11.5))
                .italic()
                .text_color(colors.muted)
                .child(text.clone())
                .into_any_element(),
        }
    }

    fn body(&self, snapshot: &PanelSnapshot, colors: &PanelColors) -> AnyElement {
        let rows = v_flex()
            .id("voice-panel-rows")
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .justify_end()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(10.0))
            .children(snapshot.rows.iter().map(|row| Self::row(row, colors)));
        let empty = snapshot.rows.is_empty() && snapshot.problem.is_none();
        v_flex()
            .flex_1()
            .min_h_0()
            .when(empty, |this| {
                this.child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap(px(4.0))
                        .px(px(24.0))
                        .text_center()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(colors.text)
                                .child(match snapshot.phase {
                                    VoicePhase::Live => "Go ahead, I'm listening.",
                                    _ => "Starting the voice agent…",
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .line_height(px(16.0))
                                .text_color(colors.muted)
                                .child("Ask what your agents are doing, send one a prompt, answer a question, or start a new agent."),
                        ),
                )
            })
            .when(!empty, |this| this.child(rows))
            .when_some(snapshot.problem.clone(), |this, problem| {
                this.child(
                    h_flex()
                        .mx(px(12.0))
                        .mb(px(8.0))
                        .p(px(10.0))
                        .gap(px(8.0))
                        .items_start()
                        .rounded(px(8.0))
                        .bg(rgba(0xfbbf2418))
                        .border_1()
                        .border_color(rgba(0xfbbf2440))
                        .child(
                            div()
                                .pt(px(1.0))
                                .child(titlebar_svg_icon(ICON_ALERT, 14.0, rgba(0xfbbf24ff).into())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(colors.text)
                                .child(problem.message),
                        ),
                )
            })
            .into_any_element()
    }

    fn footer(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut left: Vec<AnyElement> = Vec::new();
        let mut right: Vec<AnyElement> = Vec::new();
        match snapshot.phase {
            VoicePhase::Starting | VoicePhase::Live => {
                left.push(self.button(
                    "voice-panel-mute",
                    if snapshot.muted {
                        ICON_MIC_OFF
                    } else {
                        ICON_MIC
                    },
                    Some(if snapshot.muted { "Unmute" } else { "Mute" }),
                    PanelAction::Mute,
                    colors,
                    false,
                    cx,
                ));
                if snapshot.state == "speaking" {
                    left.push(self.button(
                        "voice-panel-stop-talking",
                        ICON_STOP,
                        Some("Stop talking"),
                        PanelAction::StopTalking,
                        colors,
                        false,
                        cx,
                    ));
                }
                right.push(self.button(
                    "voice-panel-hang-up",
                    ICON_HANG_UP,
                    Some("Hang up"),
                    PanelAction::HangUp,
                    colors,
                    true,
                    cx,
                ));
            }
            VoicePhase::Ending => {}
            VoicePhase::Ended | VoicePhase::Idle => {
                if snapshot
                    .problem
                    .as_ref()
                    .is_some_and(VoiceProblem::wants_settings)
                {
                    left.push(self.button(
                        "voice-panel-open-settings",
                        ICON_SETTINGS,
                        Some("Open Settings"),
                        PanelAction::OpenSettings,
                        colors,
                        false,
                        cx,
                    ));
                } else {
                    left.push(self.button(
                        "voice-panel-start-again",
                        ICON_MIC,
                        Some("Start again"),
                        PanelAction::StartAgain,
                        colors,
                        false,
                        cx,
                    ));
                }
                right.push(self.button(
                    "voice-panel-close",
                    ICON_CLOSE,
                    Some("Close"),
                    PanelAction::Close,
                    colors,
                    false,
                    cx,
                ));
            }
        }
        h_flex()
            .h(px(44.0))
            .px(px(10.0))
            .gap(px(6.0))
            .items_center()
            .border_t_1()
            .border_color(colors.border)
            .children(left)
            .child(div().flex_1())
            .children(right)
            .into_any_element()
    }
}

impl Render for VoicePanelWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = &self.snapshot;
        let colors = PanelColors::resolve();
        v_flex()
            .id("voice-panel")
            .role(gpui::Role::Group)
            .aria_label(SharedString::new_static("Voice agent"))
            .size_full()
            .overflow_hidden()
            .rounded(px(PANEL_RADIUS))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .font_family(crate::ui_fonts::UI_FONT)
            .child(self.header(&snapshot, &colors))
            .child(self.body(&snapshot, &colors))
            .child(self.footer(&snapshot, &colors, cx))
            .into_any_element()
    }
}
