//! The Changes view's colours and sizes, resolved from the app's modal palette so the view follows
//! the theme, light appearance and window glass the other native views use.

use crate::app::window::native_modal_kit::{ModalPalette, css_mix, modal_rgba, rgba_of};
use gpui::{Rgba, rgb};

use crate::ui_fonts::{FILES_LINE_NUMBER_SIZE, FILES_NAME_SIZE, FILES_TEXT_SIZE};

/// CDXC:Changes 2026-10-25 DECISION: User: "please make the font for everything here bigger to match font in files tab". Diff lines use the Files editor's text size, line numbers the size of its line numbers, and file headers, paths, counts and the toolbar the size of its file list's names; every row height, gutter and control below is measured from those three.
pub(crate) const CODE_SIZE: f32 = FILES_TEXT_SIZE;
pub(crate) const NUMBER_SIZE: f32 = FILES_LINE_NUMBER_SIZE;
pub(crate) const UI_SIZE: f32 = FILES_NAME_SIZE;
/// Secondary text: branch names, hints, comment labels, small buttons.
pub(crate) const SMALL_SIZE: f32 = CODE_SIZE - 1.0;
pub(crate) const ICON_SIZE: f32 = CODE_SIZE + 1.0;
pub(crate) const ROW_HEIGHT: f32 = CODE_SIZE * 5.0 / 3.0;
pub(crate) const FILE_HEADER_HEIGHT: f32 = UI_SIZE * 2.4;
pub(crate) const HUNK_HEIGHT: f32 = ROW_HEIGHT + 4.0;
pub(crate) const NUMBER_WIDTH: f32 = NUMBER_SIZE * 3.75;
pub(crate) const MARKER_WIDTH: f32 = CODE_SIZE * 4.0 / 3.0;
/// The toolbar's segmented control and icon toggles.
pub(crate) const CONTROL_HEIGHT: f32 = UI_SIZE * 2.0;
/// Text buttons inside rows (Viewed, Revert, Copy, Comment).
pub(crate) const BUTTON_HEIGHT: f32 = SMALL_SIZE * 2.0;
/// The commit bar's agent picker and Commit button.
pub(crate) const COMMIT_BUTTON_HEIGHT: f32 = 32.0;

#[derive(Clone, Copy)]
pub(crate) struct ChangesStyle {
    pub(crate) p: ModalPalette,
    pub(crate) text: Rgba,
    pub(crate) muted: Rgba,
    pub(crate) faint: Rgba,
    pub(crate) hairline: Rgba,
    pub(crate) panel: Rgba,
    pub(crate) code_bg: Rgba,
    pub(crate) header_bg: Rgba,
    /// What floats over the diff (the pinned file header, the resolved-comments pill): always
    /// opaque, so the lines scrolling under it never show through its text.
    pub(crate) sticky_bg: Rgba,
    pub(crate) sticky_hover: Rgba,
    pub(crate) hover: Rgba,
    pub(crate) selected: Rgba,
    pub(crate) accent: Rgba,
    pub(crate) added_row: Rgba,
    pub(crate) added_gutter: Rgba,
    pub(crate) added_word: Rgba,
    pub(crate) removed_row: Rgba,
    pub(crate) removed_gutter: Rgba,
    pub(crate) removed_word: Rgba,
    pub(crate) hunk_row: Rgba,
    pub(crate) hunk_text: Rgba,
    pub(crate) plus: Rgba,
    pub(crate) minus: Rgba,
    pub(crate) badge_added: Rgba,
    pub(crate) badge_modified: Rgba,
    pub(crate) badge_deleted: Rgba,
    pub(crate) keyword: Rgba,
    pub(crate) type_name: Rgba,
    pub(crate) function: Rgba,
    pub(crate) string: Rgba,
    pub(crate) number: Rgba,
    pub(crate) comment: Rgba,
    pub(crate) danger: Rgba,
    pub(crate) resolved: Rgba,
}

impl ChangesStyle {
    pub(crate) fn resolve(p: ModalPalette) -> Self {
        let ink = if p.light { 0x000000 } else { 0xffffff };
        let pick = |dark: u32, light: u32| rgb(if p.light { light } else { dark });
        let code_bg = if p.glass {
            modal_rgba(ink, 0.03)
        } else {
            css_mix(rgb(ink), 0.02, p.solid_surface)
        };
        // CDXC:Changes 2026-10-25 WHY: under window glass every header fill is a thin ink wash,
        // so the pinned file header let the scrolling diff show through and its text overlapped
        // the code. Floating rows mix the same ink into the surface and drop its alpha instead.
        let opaque = |amount: f32| Rgba {
            a: 1.0,
            ..css_mix(rgb(ink), amount, p.solid_surface)
        };
        Self {
            p,
            text: p.foreground,
            muted: p.muted,
            faint: rgba_of(p.muted, 0.7),
            hairline: p.hairline,
            panel: p.panel,
            code_bg,
            header_bg: if p.glass {
                modal_rgba(ink, 0.05)
            } else {
                css_mix(rgb(ink), 0.04, p.solid_surface)
            },
            sticky_bg: opaque(if p.glass { 0.08 } else { 0.04 }),
            sticky_hover: opaque(if p.glass { 0.12 } else { 0.09 }),
            hover: modal_rgba(ink, 0.05),
            selected: modal_rgba(ink, 0.09),
            accent: pick(0x9bbce0, 0x315d88),
            added_row: modal_rgba(0x16a34a, if p.light { 0.12 } else { 0.14 }),
            added_gutter: modal_rgba(0x16a34a, if p.light { 0.2 } else { 0.22 }),
            added_word: modal_rgba(0x22c55e, if p.light { 0.3 } else { 0.34 }),
            removed_row: modal_rgba(0xef4444, if p.light { 0.1 } else { 0.13 }),
            removed_gutter: modal_rgba(0xef4444, if p.light { 0.18 } else { 0.2 }),
            removed_word: modal_rgba(0xf87171, if p.light { 0.3 } else { 0.34 }),
            hunk_row: modal_rgba(0x3b82f6, if p.light { 0.08 } else { 0.1 }),
            hunk_text: pick(0x7f9cbc, 0x4a6a8c),
            plus: pick(0x26b36a, 0x15803d),
            minus: pick(0xe05656, 0xbe123c),
            badge_added: pick(0x86efac, 0x15803d),
            badge_modified: pick(0xe2c08d, 0xa16207),
            badge_deleted: pick(0xfda4af, 0xbe123c),
            keyword: pick(0xff7b72, 0xcf222e),
            type_name: pick(0x79c0ff, 0x0550ae),
            function: pick(0xd2a8ff, 0x8250df),
            string: pick(0xa5d6ff, 0x0a3069),
            number: pick(0x79c0ff, 0x0550ae),
            comment: pick(0x8b949e, 0x6e7781),
            danger: p.destructive,
            resolved: pick(0x86efac, 0x15803d),
        }
    }
}
