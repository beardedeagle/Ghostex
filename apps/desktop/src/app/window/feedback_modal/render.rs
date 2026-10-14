use super::images::FEEDBACK_MAX_IMAGES;
use super::state::{FeedbackStep, GpuiFeedbackModalWindow, github_username};
use crate::app::window::native_modal_kit::*;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::v_flex;
use std::rc::Rc;

/// Whether the "Collect more data with an agent" switch is drawn (CDXC:Feedback 2026-10-14 in
/// state.rs): off until its agent flow is built.
const COLLECT_WITH_AGENT_READY: bool = false;

impl GpuiFeedbackModalWindow {
    fn render_compose(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let paste: ModalPasteHandler = Rc::new(Self::paste_handler(cx.entity().downgrade()));
        let paste_hint = format!(
            "Paste screenshots with {}: up to {FEEDBACK_MAX_IMAGES} PNG, JPEG or WebP images.",
            crate::hotkey_label::terminal_overlay_hotkey_chord_label("cmd+v")
        );
        let message = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "Feedback"))
            .child(modal_text_area_with_paste(
                &p,
                &self.message_input,
                None,
                self.busy,
                Some(paste),
                window,
                cx,
            ))
            .children(self.render_images(cx))
            .child(modal_hint(&p, paste_hint))
            .children(self.image_error.clone().map(|error| modal_error(&p, error)))
            .into_any_element();
        let github = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "GitHub username (optional)"))
            .child(modal_text_input(
                &p,
                &self.github_input,
                self.busy,
                window,
                cx,
            ))
            .child(match github_username(&self.github) {
                Ok(_) => modal_hint(&p, "So we can let you know on GitHub when it's fixed.")
                    .into_any_element(),
                Err(()) => modal_error(
                    &p,
                    "That isn't a GitHub username: letters, digits and single hyphens, up to 39 \
                     characters.",
                ),
            })
            .into_any_element();
        let mut content = vec![
            modal_header(
                &p,
                "Send Feedback",
                Some(
                    "Tell us what is broken, confusing or missing. You review the issue before it \
                     is posted publicly on the Ghostex GitHub.",
                ),
            ),
            message,
            github,
        ];
        if COLLECT_WITH_AGENT_READY {
            content.push(
                modal_panel(&p)
                    .child(modal_panel_row(
                        &p,
                        "Collect more data with an agent",
                        Some("An agent gathers the logs and details that help us fix it."),
                        modal_switch(&p, false, false),
                        false,
                    ))
                    .into_any_element(),
            );
        }
        content
    }

    fn render_review(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let screenshots = match self.images.len() {
            0 => String::new(),
            1 => "1 screenshot and ".to_string(),
            count => format!("{count} screenshots and "),
        };
        let added = format!(
            "Added when it is posted: {screenshots}\u{201c}{}\u{201d}.",
            self.footer
        );
        let fields = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "Title"))
            .child(modal_text_input(
                &p,
                &self.title_input,
                self.busy,
                window,
                cx,
            ))
            .child(div().h(px(4.0)))
            .child(modal_section_title(&p, "Description"))
            .child(modal_text_area(
                &p,
                &self.body_input,
                None,
                self.busy,
                window,
                cx,
            ))
            .child(modal_hint(&p, added))
            .children(self.error.clone().map(|error| modal_error(&p, error)))
            .into_any_element();
        vec![
            modal_header(
                &p,
                "Review Issue",
                Some(
                    "This is exactly what will be posted publicly on the Ghostex GitHub. Edit \
                     anything before you send it.",
                ),
            ),
            fields,
        ]
    }

    fn render_sent(&self, number: u64, url: &str) -> Vec<AnyElement> {
        let p = self.palette;
        vec![
            modal_header(
                &p,
                "Thanks for the feedback",
                Some(format!(
                    "Issue #{number} is on GitHub now. You can follow it there."
                )),
            ),
            modal_hint(&p, url.to_string()).into_any_element(),
        ]
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let spinner = || self.busy.then(|| modal_spinner(p.primary_foreground));
        let buttons = match &self.step {
            FeedbackStep::Compose => vec![
                modal_action_button(
                    &p,
                    "feedback-cancel",
                    "Cancel",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.close(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "feedback-review",
                    "Review",
                    spinner(),
                    ModalButtonTone::Primary,
                    !self.can_review(),
                    |this, window, cx| this.review(window, cx),
                    cx,
                ),
            ],
            FeedbackStep::Review => vec![
                modal_action_button(
                    &p,
                    "feedback-back",
                    "Back",
                    None,
                    ModalButtonTone::Neutral,
                    self.busy,
                    |this, window, cx| this.back(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "feedback-send",
                    "Send",
                    spinner(),
                    ModalButtonTone::Primary,
                    !self.can_send(),
                    |this, window, cx| this.send(window, cx),
                    cx,
                ),
            ],
            FeedbackStep::Sent { .. } => vec![
                modal_action_button(
                    &p,
                    "feedback-done",
                    "Done",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.close(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "feedback-open-issue",
                    "Open Issue",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this, _window, cx| this.open_issue(cx),
                    cx,
                ),
            ],
        };
        modal_footer(buttons)
    }
}

impl Render for GpuiFeedbackModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = match &self.step {
            FeedbackStep::Compose => {
                let mut content = self.render_compose(window, cx);
                if let Some(error) = self.error.clone() {
                    content.push(modal_error(&p, error));
                }
                content
            }
            FeedbackStep::Review => self.render_review(window, cx),
            FeedbackStep::Sent { number, url } => self.render_sent(*number, url),
        };
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-feedback-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}
