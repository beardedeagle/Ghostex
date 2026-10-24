//! The small headset on the sidebar row of the session an open voice call is about.
//!
//! Part of the user's focus strip pick (CDXC:VoiceMode 2026-10-23 DECISION in `panel_focus_strip.rs`): the row of the call's session carries a green headset while the call is connecting or live, so two look-alike rows are told apart; it goes when the call widens to all sessions or ends.

use gpui::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, rgba};

use crate::app::helpers::*;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::*;

const ICON_HEADSET: &str = "titlebar/headset.svg";
/// A little under the coordinator badge's glyph (13.55), as the user asked for a small headset.
const BADGE_SIZE: f32 = 12.0;

impl GhostexGpuiApp {
    /// The headset for the row with this sidebar session id, while a call is about it.
    pub(crate) fn render_voice_call_badge(
        &self,
        sidebar_session_id: &str,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        if !self.voice_call_is_about(sidebar_session_id) {
            return None;
        }
        let scale = appearance.scale;
        Some(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(titlebar_svg_icon(
                    ICON_HEADSET,
                    BADGE_SIZE * scale,
                    rgba(0x4ade80ff).into(),
                ))
                .into_any_element(),
        )
    }
}
