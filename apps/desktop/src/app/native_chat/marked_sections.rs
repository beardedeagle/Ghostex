//! Marked sections in the transcript: the look (`marked_look.rs`) in this chat's tones.
//!
//! The core finds the sections (`packages/gx-chat-core/src/transcript/marked_sections.rs`) and sends
//! them as marks around the section's own lines; `rich_markdown.rs` splits on the marks.
use super::appearance::ChatAppearance;

pub(crate) use super::marked_look::{
    MarkedLook, MarkerStyle, SECTION_CLOSE, SECTION_OPEN, SectionMeta, section,
};

/// The look of marked sections in this chat's tones.
pub(super) fn look(p: &ChatAppearance) -> MarkedLook {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    MarkedLook {
        emoji: super::marked_look::emoji_from_settings(settings.object()),
        strength: p.marked_heading_strength,
        light: p.light,
        scale: p.scale,
        foreground: p.foreground,
        prose: p.prose,
        border: p.border,
        line_height: super::rich_markdown::PROSE_LINE_HEIGHT,
        block_gap: super::markdown_style::paragraph_gap(p),
    }
}
