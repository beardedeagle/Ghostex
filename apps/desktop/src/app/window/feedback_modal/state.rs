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
/// relay 170 seconds. Starting the feedback agent sends the same images.
const SEND_TIMEOUT: Duration = Duration::from_secs(180);

/// What the pop-up asks its host to do. The pop-up removes its own window before sending it.
pub(crate) enum FeedbackModalCommand {
    Closed,
    /// gxserver started the feedback agent in this session; the host opens it.
    OpenSession {
        project_id: String,
        session_id: String,
        agent_id: String,
    },
}

pub(crate) type FeedbackModalHost = Rc<dyn Fn(FeedbackModalCommand, &mut App)>;

pub(crate) struct FeedbackModalConfig {
    /// The relay's `client.app`: `desktop` or `web`.
    pub(crate) app: &'static str,
    pub(crate) palette: ModalPalette,
    /// The GitHub username the last feedback named, to fill the field with.
    pub(crate) github_username: String,
    /// The agent the feedback agent runs as: the sidebar's default agent for new sessions, else
    /// gxserver picks the Default Prompt Agent.
    pub(crate) agent_id: Option<String>,
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
/// CDXC:Feedback 2026-10-16 DECISION:
/// User: "add a new field above the text field for the feedback that says "github username" and make it required", "add text that says you can paste images in the input text area", "should show the github user name mentioned at the top of the issue", "implement agent for gathering info (agent launches with the agents description and image attached and can debug and then create the issue him self after collecting useful info about the user's system" and "rename the option to Send Bug / Feedback". The pop-up is titled Send Bug / Feedback; a required GitHub username field ("name" or "@name", checked for a GitHub username's shape, remembered for the next feedback) sits above the message box, whose hint says screenshots can be pasted into it; gxserver puts "Reported by @name" at the top of the issue so GitHub notifies them. With "Collect more data with an agent" on, Start Agent hands the message, username and screenshots to gxserver, which starts the feedback agent (server/src/feedback/agent.rs) and the pop-up opens its session; the agent files the issue with `ghostex feedback send`. This supersedes the 2026-10-14 rule that hid the switch and made the username optional.
/// SEE-ALSO: server/src/feedback/ (the draft, the relay post and the feedback agent), server/src/ghostex_cli/feedback.rs (`ghostex feedback send`), packages/gx-core/src/sidebar_menu/navigation.rs (the More options row), packages/gx-core/src/sidebar_actions/open.rs (the `feedback` action), apps/gpui-web/src/app/web_host/modals.rs (the web build opens it too).
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
    /// "Collect more data with an agent": the primary button starts the feedback agent instead of
    /// the Review step.
    pub(super) collect_with_agent: bool,
    pub(super) agent_id: Option<String>,
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
        // The username comes first; once it is remembered, writing the message is what is left.
        if github_username(&github).ok().flatten().is_some() {
            message_input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            github_input.update(cx, |input, cx| input.focus(window, cx));
        }
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
            collect_with_agent: false,
            agent_id: config.agent_id,
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

    /// The valid username in the field; the field is required.
    pub(super) fn username(&self) -> Option<&str> {
        github_username(&self.github).ok().flatten()
    }

    /// Whether the primary Compose button (Review, or Start Agent) can go.
    pub(super) fn can_review(&self) -> bool {
        !self.busy
            && self.pending_images == 0
            && !self.message.trim().is_empty()
            && self.username().is_some()
    }

    pub(super) fn toggle_collect_with_agent(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.collect_with_agent = !self.collect_with_agent;
            cx.notify();
        }
    }

    /// Stores the username for the next feedback, off the UI thread.
    fn remember_username(&self, cx: &mut Context<Self>) {
        let username = self.username().map(str::to_string);
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    FEEDBACK_GITHUB_USERNAME_STORAGE_KEY,
                    username.as_deref(),
                );
            })
            .detach();
    }

    /// The pasted images as the `[{ name, dataBase64 }]` list gxserver takes, encoded off the UI
    /// thread.
    fn encoded_images(&self, cx: &mut Context<Self>) -> gpui::Task<Vec<Value>> {
        let images = self.images.clone();
        cx.background_executor().spawn(async move {
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
        })
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
        self.remember_username(cx);
        let params = json!({
            "app": self.app,
            "message": self.message.trim(),
            "githubUsername": self.username(),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/draftFeedback", params).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(draft) => this.show_review(&draft, window, cx),
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// gxserver's draft already starts with the reporter line, so the Review step shows it, the
    /// user can edit it, and the relay posts it unchanged.
    fn show_review(&mut self, draft: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let text = |key: &str| draft[key].as_str().unwrap_or_default().to_string();
        self.title = text("title");
        self.body = text("body");
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
        let encode = self.encoded_images(cx);
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

    /// Compose → the feedback agent: gxserver saves the screenshots, starts the agent with the
    /// message, and the pop-up closes onto the agent's session.
    pub(super) fn start_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_review() {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        self.remember_username(cx);
        let mut params = json!({
            "app": self.app,
            "message": self.message.trim(),
            "githubUsername": self.username(),
        });
        if let Some(agent_id) = &self.agent_id {
            params["agentId"] = json!(agent_id);
        }
        let encode = self.encoded_images(cx);
        cx.spawn_in(window, async move |this, cx| {
            params["images"] = Value::Array(encode.await);
            let result =
                gx_rpc_with_timeout(None, "/api/startFeedbackAgent", params, SEND_TIMEOUT).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let text = |value: &Value, key: &str| value[key].as_str().map(str::to_string);
                match result {
                    Ok(started) => match (text(&started, "projectId"), text(&started, "sessionId"))
                    {
                        (Some(project_id), Some(session_id)) => {
                            let agent_id = text(&started, "agentId").unwrap_or_default();
                            window.remove_window();
                            (this.host)(
                                FeedbackModalCommand::OpenSession {
                                    project_id,
                                    session_id,
                                    agent_id,
                                },
                                cx,
                            );
                            return;
                        }
                        _ => this.error = Some(
                            "The agent started, but Ghostex did not say which session it is in."
                                .to_string(),
                        ),
                    },
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

    /// Cmd/Ctrl+Enter moves on: Review (or Start Agent) from the message, Send from the review,
    /// Close when sent.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            FeedbackStep::Compose if self.collect_with_agent => self.start_agent(window, cx),
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
