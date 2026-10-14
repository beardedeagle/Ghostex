//! Native GPUI Portless Setup prompt, the desktop twin of the React
//! `PortlessSetupModal` in packages/core-ui/portless-setup-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:Portless 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and each native modal must be EXACTLY 1 to 1 with its React twin: the same layout, copy, colors, radii, spacing, states, keyboard behaviour and bridge messages in both appearances. The primary action (Install / Reconfigure) is the neutral filled button, with Disable as the destructive filled button in the middle (CDXC:AppModal 2026-10-14 on `neutral_button_fills` superseded the React footer's outline buttons).
//! SEE-ALSO: packages/core-ui/portless-setup-modal.tsx (deleted 2026-10-01) (the React twin and `PORTLESS_SETUP_MODAL_COPY`, mirrored below), apps/desktop/src/app/portless_setup_modal_lifecycle.rs (open, close, Portless admin actions), apps/desktop/src/app/os_integration/notifications_and_portless.rs (the opener and the action handlers).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, Context, FocusHandle, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_WIDTH`.
pub(crate) const PORTLESS_SETUP_MODAL_WIDTH: f32 = 640.0;
/// First-frame height only (`APP_MODAL_HOST_PORTLESS_SETUP_WINDOW_HEIGHT`); the window is resized to the measured layout on the first prepaint.
pub(crate) const PORTLESS_SETUP_MODAL_INITIAL_HEIGHT: f32 = 340.0;

const DISABLE: &str = "Disable";

/// The 13px description line box; every word item is this tall so wrapped rows keep the text rhythm.
const LINE_HEIGHT: f32 = 20.15;
/// The advance of a space in 13px system UI text, used as the gap between words.
const SPACE_WIDTH: f32 = 3.7;

/// `PortlessSetupModalMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortlessSetupModalMode {
    FirstSetup,
    StandaloneReconfigure,
}

impl PortlessSetupModalMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "firstSetup" => Some(Self::FirstSetup),
            "standaloneReconfigure" => Some(Self::StandaloneReconfigure),
            _ => None,
        }
    }
}

/// `NativePortlessProtocol`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortlessSetupProtocol {
    Https,
    Http,
}

impl PortlessSetupProtocol {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "https" => Some(Self::Https),
            "http" => Some(Self::Http),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }
}

/// The admin action the primary button runs (`PortlessSetupAdminAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortlessSetupAdminAction {
    Install,
    Reconfigure,
}

impl PortlessSetupAdminAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Reconfigure => "reconfigure",
        }
    }
}

/// One row of `PORTLESS_SETUP_MODAL_COPY`.
struct PortlessSetupCopy {
    title: &'static str,
    body: [&'static str; 2],
    dismiss_label: &'static str,
    primary_action: PortlessSetupAdminAction,
    primary_label: &'static str,
}

const FIRST_SETUP_COPY: PortlessSetupCopy = PortlessSetupCopy {
    title: "Set up Portless domains?",
    body: [
        "Ghostex found a running dev server. Portless gives it a stable local domain like https://ghostex.localhost, so you can run multiple apps and worktrees of the same project without conflicting ports.",
        "Installing the Portless background proxy requires admin permission once so it can listen on standard local web ports. You can disable Portless if you do not want Ghostex to show this again.",
    ],
    dismiss_label: "Postpone",
    primary_action: PortlessSetupAdminAction::Install,
    primary_label: "Install",
};

const STANDALONE_RECONFIGURE_COPY: PortlessSetupCopy = PortlessSetupCopy {
    title: "Reconfigure Portless for Ghostex?",
    body: [
        "Portless is already installed on this computer. Ghostex needs to manage the Portless background proxy so it can create stable domains for your projects and worktrees.",
        "Reconfiguring will point Portless at Ghostex's state directory. You can cancel, or disable Portless in Settings if you do not want Ghostex to show this again.",
    ],
    dismiss_label: "Cancel",
    primary_action: PortlessSetupAdminAction::Reconfigure,
    primary_label: "Reconfigure",
};

fn copy_for(mode: PortlessSetupModalMode) -> &'static PortlessSetupCopy {
    match mode {
        PortlessSetupModalMode::FirstSetup => &FIRST_SETUP_COPY,
        PortlessSetupModalMode::StandaloneReconfigure => &STANDALONE_RECONFIGURE_COPY,
    }
}

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum PortlessSetupModalCommand {
    /// Install / Reconfigure: `runPortlessSetupPromptAdminAction` with this action, the protocol and a fresh request id.
    AdminAction {
        action: PortlessSetupAdminAction,
        protocol: PortlessSetupProtocol,
        request_id: String,
    },
    /// "Disable": `setPortlessEnabled { enabled: false }`.
    Disable,
    /// Postpone (first setup): `postponePortlessSetupPrompt`.
    Postpone,
    /// Cancel (standalone reconfigure): `cancelPortlessSetupPrompt`.
    Cancel,
}

pub(crate) type PortlessSetupModalHost = Rc<dyn Fn(PortlessSetupModalCommand, &mut App)>;

pub(crate) struct PortlessSetupModalConfig {
    pub(crate) mode: PortlessSetupModalMode,
    pub(crate) protocol: PortlessSetupProtocol,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiPortlessSetupModalWindow {
    host: PortlessSetupModalHost,
    palette: ModalPalette,
    mode: PortlessSetupModalMode,
    protocol: PortlessSetupProtocol,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

/// `createPortlessSetupModalRequestId`: `portless-setup-{action}-{now base36}-{random base36}`.
pub(crate) fn create_portless_setup_modal_request_id(action: PortlessSetupAdminAction) -> String {
    fn base36(mut value: u64) -> String {
        const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        if value == 0 {
            return "0".to_string();
        }
        let mut out = Vec::new();
        while value > 0 {
            out.push(DIGITS[(value % 36) as usize]);
            value /= 36;
        }
        out.reverse();
        String::from_utf8(out).unwrap_or_default()
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    // A splitmix64 step over the clock nanos and a fresh heap address stands in for `Math.random()`.
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0)
        ^ (Box::into_raw(Box::new(0u8)) as u64).rotate_left(32);
    seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut random = seed;
    random = (random ^ (random >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    random = (random ^ (random >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    random ^= random >> 31;
    format!(
        "portless-setup-{}-{}-{}",
        action.as_str(),
        base36(now),
        base36(random)
    )
}

impl GpuiPortlessSetupModalWindow {
    pub(crate) fn new(
        config: PortlessSetupModalConfig,
        host: PortlessSetupModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            mode: config.mode,
            protocol: config.protocol,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: PortlessSetupModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    /// The React `dismiss`: Postpone on first setup, Cancel otherwise.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = match self.mode {
            PortlessSetupModalMode::FirstSetup => PortlessSetupModalCommand::Postpone,
            PortlessSetupModalMode::StandaloneReconfigure => PortlessSetupModalCommand::Cancel,
        };
        self.close_window_and_send(command, window, cx);
    }

    fn run_admin_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = copy_for(self.mode).primary_action;
        let command = PortlessSetupModalCommand::AdminAction {
            action,
            protocol: self.protocol,
            request_id: create_portless_setup_modal_request_id(action),
        };
        self.close_window_and_send(command, window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        self.dismiss(window, cx);
        cx.stop_propagation();
    }

    /// The dialog header. The description is laid out word by word because
    /// gpui's line wrapper may break inside `https://ghostex.localhost` (after
    /// a `/`), where the browser keeps the URL on one line.
    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let copy = copy_for(self.mode);
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .line_height(px(20.8))
                    .child(copy.title),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_x(px(SPACE_WIDTH))
                    .gap_y(px(0.0))
                    .text_size(px(13.0))
                    .line_height(px(LINE_HEIGHT))
                    .text_color(hsla(p.muted))
                    .children(copy.body[0].split(' ').filter(|word| !word.is_empty()).map(
                        |word| {
                            div()
                                .h(px(LINE_HEIGHT))
                                .whitespace_nowrap()
                                .child(word.to_string())
                        },
                    )),
            )
            .into_any_element()
    }

    /// `.portless-setup-modal-body`: one `Card size='sm'` whose header holds the second paragraph.
    fn render_body(&self) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                // `CardHeader` lays its lone description out in a two-row grid, so the empty second row's 8px gap lands under the text.
                modal_panel(&p)
                    .pt(px(16.0))
                    .px(px(16.0))
                    .pb(px(24.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(20.15))
                            .text_color(hsla(p.muted))
                            .child(copy_for(self.mode).body[1]),
                    ),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let copy = copy_for(self.mode);
        modal_footer(vec![
            modal_action_button(
                &p,
                "portless-setup-dismiss",
                copy.dismiss_label,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.dismiss(window, cx),
                cx,
            ),
            modal_action_button(
                &p,
                "portless-setup-disable",
                DISABLE,
                None,
                ModalButtonTone::Danger,
                false,
                |this, window, cx| {
                    this.close_window_and_send(PortlessSetupModalCommand::Disable, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "portless-setup-primary",
                copy.primary_label,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.run_admin_action(window, cx),
                cx,
            ),
        ])
    }
}

impl Render for GpuiPortlessSetupModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![self.render_header(), self.render_body()];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-portless-setup-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiPortlessSetupModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(window, cx);
    }
}
