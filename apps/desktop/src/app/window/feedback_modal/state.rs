use crate::app::gx_store::{gx_rpc, gx_rpc_with_timeout, write_client_document_value};
use crate::app::window::native_modal_kit::*;
use base64::Engine as _;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Image, KeyDownEvent, Subscription, Window,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use serde_json::{Value, json};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const FEEDBACK_MODAL_WIDTH: f32 = 600.0;
pub(crate) const FEEDBACK_MODAL_INITIAL_HEIGHT: f32 = 600.0;

/// Five images of 5 MiB in base64 can take minutes to upload on a slow line; gxserver gives the
/// relay 170 seconds.
const SEND_TIMEOUT: Duration = Duration::from_secs(180);

/// What the pop-up asks its host to do. The pop-up removes its own window before sending it.
pub(crate) enum FeedbackModalCommand {
    Closed,
}

pub(crate) type FeedbackModalHost = Rc<dyn Fn(FeedbackModalCommand, &mut App)>;

pub(crate) struct FeedbackModalConfig {
    /// The relay's `client.app`: `desktop` or `web`.
    pub(crate) app: &'static str,
    pub(crate) palette: ModalPalette,
    /// The GitHub username the last feedback named, to fill the field with.
    pub(crate) github_username: String,
}

/// Client storage's `feedbackGithubUser` store: the GitHub username the last feedback named.
pub(crate) const FEEDBACK_GITHUB_USERNAME_STORAGE_KEY: &str = "ghostex.feedback.githubUsername";

/// The username in the field, without its `@`: `Ok(None)` when the field is empty, `Err` when it
/// is not a GitHub username (letters, digits and single hyphens, no hyphen at either end, up to 39
/// characters).
pub(super) fn github_username(raw: &str) -> Result<Option<&str>, ()> {
    let name = raw.trim();
    let name = name.strip_prefix('@').unwrap_or(name);
    if name.is_empty() {
        return Ok(None);
    }
    let valid = name.len() <= 39
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid { Ok(Some(name)) } else { Err(()) }
}

pub(super) enum FeedbackStep {
    /// The message and its pasted images.
    Compose,
    /// The issue exactly as it will be posted, editable.
    Review,
    /// The relay created the issue.
    Sent { number: u64, url: String },
}

/// CDXC:Feedback 2026-10-04 DECISION:
/// User: "add a button at the top of the sidebar. It should be a chat bubble icon… When you click that button, it should show a pop-up made with GPUI. This pop-up should have a text area and the ability to paste images. When someone presses send, it's for sending feedback to us. It should … create an issue [on GitHub] with the required information from the user." Decided with them: the issue is posted through the Ghostex feedback relay, pasted images go with it, and the user ALWAYS sees and can edit the final issue text (the Review step) before it is posted.
/// CDXC:Feedback 2026-10-14 DECISION:
/// User: "let's hide the collect more data with agent toggle until we implement it" and "add a text field for the person to write their github username so they can get a message when it's fixed". The "Collect more data with an agent" switch is not drawn until its agent flow is built (`COLLECT_WITH_AGENT_READY` in render.rs) and every send says `collectedByAgent: false`; this supersedes the 2026-10-04 rule that drew it disabled with "Coming soon". An optional GitHub username field ("name" or "@name", checked for a GitHub username's shape) adds "Reported by @name" to the drafted issue, so GitHub notifies them and the Review step shows it, and the username is remembered for the next feedback.
/// SEE-ALSO: server/src/feedback/ (the draft and the relay post), packages/gx-core/src/sidebar_menu/navigation.rs (the More options row), packages/gx-core/src/sidebar_actions/open.rs (the `feedback` action), apps/gpui-web/src/app/web_host/modals.rs (the web build opens it too).
pub(crate) struct GpuiFeedbackModalWindow {
    pub(super) host: FeedbackModalHost,
    pub(super) palette: ModalPalette,
    pub(super) app: &'static str,
    pub(super) step: FeedbackStep,
    pub(super) message_input: Entity<TextareaState>,
    /// Mirror of the message field, refreshed on every change.
    pub(super) message: String,
    pub(super) images: Vec<Arc<Image>>,
    pub(super) github_input: Entity<InputState>,
    /// Mirror of the GitHub username field, refreshed on every change.
    pub(super) github: String,
    /// Pasted images still being checked (and shrunk when too large).
    pub(super) pending_images: usize,
    pub(super) image_error: Option<String>,
    pub(super) title_input: Entity<InputState>,
    pub(super) title: String,
    pub(super) body_input: Entity<TextareaState>,
    pub(super) body: String,
    /// The line the relay adds under the issue (`Sent from Ghostex feedback · Desktop … · macOS …`).
    pub(super) footer: String,
    /// A draft or a send is in flight.
    pub(super) busy: bool,
    pub(super) error: Option<String>,
    pub(super) fit: ModalFit,
    pub(super) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiFeedbackModalWindow {
    pub(crate) fn new(
        config: FeedbackModalConfig,
        host: FeedbackModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let message_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What happened, what did you expect, or what would you like to see?")
        });
        let github = config.github_username;
        let github_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("name or @name")
                .default_value(github.clone())
        });
        let title_input = cx.new(|cx| InputState::new(window, cx));
        let body_input = cx.new(|cx| TextareaState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &message_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.message = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &github_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.github = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &title_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.title = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &body_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.body = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
        ];
        message_input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            host,
            palette: config.palette,
            app: config.app,
            step: FeedbackStep::Compose,
            message_input,
            message: String::new(),
            images: Vec::new(),
            github_input,
            github,
            pending_images: 0,
            image_error: None,
            title_input,
            title: String::new(),
            body_input,
            body: String::new(),
            footer: String::new(),
            busy: false,
            error: None,
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn can_review(&self) -> bool {
        !self.busy
            && self.pending_images == 0
            && !self.message.trim().is_empty()
            && github_username(&self.github).is_ok()
    }

    pub(super) fn can_send(&self) -> bool {
        !self.busy && !self.title.trim().is_empty() && !self.body.trim().is_empty()
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(FeedbackModalCommand::Closed, cx);
    }

    /// Compose → Review: gxserver turns the message into the issue the relay would post, and the
    /// user sees and edits that before anything leaves the computer.
    pub(super) fn review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_review() {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let username = github_username(&self.github)
            .ok()
            .flatten()
            .map(str::to_string);
        let remembered = username.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    FEEDBACK_GITHUB_USERNAME_STORAGE_KEY,
                    remembered.as_deref(),
                );
            })
            .detach();
        let params = json!({ "app": self.app, "message": self.message.trim() });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/draftFeedback", params).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(draft) => this.show_review(&draft, username.as_deref(), window, cx),
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The reporter's mention goes into the body itself, so the Review step shows it, the user can
    /// edit it, and the relay posts it unchanged.
    fn show_review(
        &mut self,
        draft: &Value,
        username: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = |key: &str| draft[key].as_str().unwrap_or_default().to_string();
        self.title = text("title");
        self.body = match username {
            Some(name) => format!("{}\n\nReported by @{name}", text("body")),
            None => text("body"),
        };
        self.footer = text("footer");
        let (title, body) = (self.title.clone(), self.body.clone());
        self.title_input
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.body_input
            .update(cx, |input, cx| input.set_value(body, window, cx));
        self.title_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.step = FeedbackStep::Review;
    }

    /// Review → Compose, keeping the message and the images; edits made to the issue are dropped
    /// because the next Review drafts it again from the message.
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.step = FeedbackStep::Compose;
        self.error = None;
        self.message_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send() {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let app = self.app;
        let title = self.title.trim().to_string();
        let body = self.body.trim().to_string();
        let images = self.images.clone();
        let encode = cx.background_executor().spawn(async move {
            images
                .iter()
                .enumerate()
                .map(|(index, image)| {
                    json!({
                        "name": format!("screenshot-{}.{}", index + 1, image.format().extension()),
                        "dataBase64": base64::engine::general_purpose::STANDARD.encode(image.bytes()),
                    })
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let images = encode.await;
            let params = json!({ "app": app, "title": title, "body": body, "images": images });
            let result = gx_rpc_with_timeout(None, "/api/sendFeedback", params, SEND_TIMEOUT).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(issue) => {
                        match (issue["issueNumber"].as_u64(), issue["issueUrl"].as_str()) {
                            (Some(number), Some(url)) => {
                                this.step = FeedbackStep::Sent {
                                    number,
                                    url: url.to_string(),
                                };
                                // The fields are gone, so the frame takes the keys (Escape, Cmd+Enter).
                                this.focus_handle.focus(window, cx);
                            }
                            _ => {
                                this.error =
                                    Some("The issue was sent, but no link came back.".to_string())
                            }
                        }
                    }
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_issue(&mut self, cx: &mut Context<Self>) {
        if let FeedbackStep::Sent { url, .. } = &self.step {
            cx.open_url(url);
        }
    }

    /// Cmd/Ctrl+Enter moves on: Review from the message, Send from the review, Close when sent.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            FeedbackStep::Compose => self.review(window, cx),
            FeedbackStep::Review => self.send(window, cx),
            FeedbackStep::Sent { .. } => self.close(window, cx),
        }
    }

    /// Plain Enter types a newline; the platform's Cmd/Ctrl+Enter chord advances. A capture-phase
    /// action listener stops propagation itself, or the field's own handler runs too.
    pub(super) fn on_enter_action(
        &mut self,
        action: &Enter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !action.secondary {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.advance(window, cx);
    }

    pub(super) fn on_escape_action(
        &mut self,
        _: &Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.close(window, cx);
    }

    /// Keys while the frame itself holds focus.
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => self.close(window, cx),
            "enter" if event.keystroke.modifiers.secondary() && !event.is_held => {
                self.advance(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl ModalCornerClose for GpuiFeedbackModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
