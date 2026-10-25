//! The standalone File diff dialog (`GitFileDiffModal`), shown only when a file's diff arrives
//! while no commit review is open: the path and its counts over the patch, with the display
//! options above it. Its options are the panel's own state and are not remembered.
use super::super::native_modal_kit::*;
use super::diff::{GitDiffPrefs, GitDiffViewMode};
use super::diff_view::{
    GitDiffPalette, GitDiffSkin, GitDiffView, render_diff_controls, render_diff_stat,
    render_diff_surface_sized,
};
use super::model::{GitFileDiffDraft, GitFileStat};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, Styled as _, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `APP_MODAL_HOST_WINDOW_WIDTH` / `_HEIGHT`: the dialog fills this child window.
pub(crate) const GIT_FILE_DIFF_MODAL_WIDTH: f32 = 1080.0;
pub(crate) const GIT_FILE_DIFF_MODAL_HEIGHT: f32 = 760.0;

pub(crate) enum GitFileDiffModalCommand {
    Close,
}

pub(crate) type GitFileDiffModalHost = Rc<dyn Fn(GitFileDiffModalCommand, &mut App)>;

pub(crate) struct GpuiGitFileDiffModalWindow {
    host: GitFileDiffModalHost,
    palette: ModalPalette,
    diff_palette: GitDiffPalette,
    draft: GitFileDiffDraft,
    prefs: GitDiffPrefs,
    diff: GitDiffView,
    focus_handle: FocusHandle,
    /// The three display controls, which are tab stops as in the React dialog.
    control_focus: [FocusHandle; 3],
    _click_away: Vec<gpui::Subscription>,
}

impl GpuiGitFileDiffModalWindow {
    pub(crate) fn new(
        draft: GitFileDiffDraft,
        palette: ModalPalette,
        host: GitFileDiffModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let prefs = GitDiffPrefs::default();
        let mut diff = GitDiffView::new(window, cx);
        diff.set_patch(Some(&draft.patch), prefs);
        diff.focus_on_select(focus_handle.clone(), cx);
        Self {
            host,
            palette,
            diff_palette: GitDiffPalette::resolve(&palette, GitDiffSkin::FileDiff),
            draft,
            prefs,
            diff,
            focus_handle,
            control_focus: [
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(true),
            ],
            _click_away: super::super::popup_dismissal::close_app_modal_on_click_away(window, cx),
        }
    }

    /// A newer `gitFileDiff` for the open dialog replaces what it shows.
    pub(crate) fn set_draft(&mut self, draft: GitFileDiffDraft, cx: &mut Context<Self>) {
        self.diff.set_patch(Some(&draft.patch), self.prefs);
        self.draft = draft;
        cx.notify();
    }

    fn set_prefs(&mut self, prefs: GitDiffPrefs, cx: &mut Context<Self>) {
        self.prefs = prefs;
        self.diff.set_patch(Some(&self.draft.patch), prefs);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(GitFileDiffModalCommand::Close, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            cx.stop_propagation();
            self.close(window, cx);
            return;
        }
        if !matches!(key, "enter" | "space") {
            return;
        }
        // Enter and Space press the focused display control.
        let prefs = self.prefs;
        let Some(index) = self
            .control_focus
            .iter()
            .position(|handle| handle.is_focused(window))
        else {
            return;
        };
        let next = match index {
            0 => GitDiffPrefs {
                view_mode: match prefs.view_mode {
                    GitDiffViewMode::Split => GitDiffViewMode::Unified,
                    GitDiffViewMode::Unified => GitDiffViewMode::Split,
                },
                ..prefs
            },
            1 if prefs.view_mode == GitDiffViewMode::Split => return,
            1 => GitDiffPrefs {
                line_wrap: !prefs.line_wrap,
                ..prefs
            },
            _ => GitDiffPrefs {
                hide_whitespace: !prefs.hide_whitespace,
                ..prefs
            },
        };
        cx.stop_propagation();
        self.set_prefs(next, cx);
    }

    /// Preview hook for the standalone demo binary.
    #[allow(dead_code)] // used by src/bin/native_modal_demo/git_commit.rs only
    pub(crate) fn preview_prefs(&mut self, prefs: GitDiffPrefs, cx: &mut Context<Self>) {
        self.set_prefs(prefs, cx);
    }
}

impl Render for GpuiGitFileDiffModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let lp = ModalLegacyPalette::resolve(&p);
        let dp = self.diff_palette;
        let has_stats = self.draft.additions.is_some() || self.draft.deletions.is_some();
        let stat = GitFileStat {
            additions: self.draft.additions.unwrap_or(0),
            deletions: self.draft.deletions.unwrap_or(0),
        };
        // CDXC:Git 2026-05-25-10:16:
        // The standalone file diff modal uses a sticky file header, monospaced patch rows, and addition/deletion coloring while staying inside Ghostex's existing modal host.
        let header = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap(px(8.0))
            .pt(px(18.0))
            .px(px(20.0))
            .pb(px(14.0))
            .border_b_1()
            .border_color(hsla(lp.divider()))
            .child(
                div()
                    .text_size(px(20.0))
                    .line_height(px(24.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child("File diff"),
            )
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(12.0))
                    .text_color(hsla(p.muted))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(17.14))
                            .text_color(hsla(dp.path_text))
                            .child(self.draft.file_path.clone()),
                    )
                    .when(has_stats, |this| {
                        this.child(render_diff_stat(
                            &dp,
                            stat,
                            11.0,
                            15.714,
                            FontWeight::NORMAL,
                            4.0,
                        ))
                    }),
            );
        let body_background = if p.light && !p.glass {
            rgb(0xf0f0f0)
        } else {
            // `color-mix(in srgb, var(--background) 94%, black 6%)`.
            css_mix(lp.surface, 0.94, modal_rgba(0x000000, 1.0))
        };
        let body = v_flex()
            .w_full()
            .min_h_0()
            .flex_shrink(1.0)
            .gap(px(10.0))
            .p(px(12.0))
            .bg(hsla(body_background))
            .child(render_diff_controls(
                &dp,
                self.prefs,
                [
                    &self.control_focus[0],
                    &self.control_focus[1],
                    &self.control_focus[2],
                ],
                window,
                |this: &mut Self, prefs, _window, cx| this.set_prefs(prefs, cx),
                cx,
            ))
            .child(render_diff_surface_sized(
                &self.diff,
                &dp,
                self.prefs,
                "git-file-diff-rows",
                window,
            ));
        div()
            .id("ghostex-gpui-git-file-diff-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(lp.window))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(v_flex().size_full().child(header).child(body))
    }
}

impl ModalCornerClose for GpuiGitFileDiffModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
