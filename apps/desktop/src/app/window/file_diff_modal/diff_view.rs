//! The patch surface of `GitFileDiffPanel` and the three `GitFileDiffControls`, drawn natively.
//! Rows are virtualized: a uniform list while lines do not wrap (with the horizontal scroll the
//! React surface's `min-width: max-content` gives), a measured list while they wrap. The rows'
//! text is selectable through GPUI-Kit (diff_select.rs) and wraps where Chrome wraps it
//! (diff_wrap.rs).
use super::super::native_modal_kit::*;
use super::diff::*;
use super::diff_select::{
    DiffCell, DiffScroll, DiffSelection, DiffSelectionState, DiffSurfaceHost, DiffText,
    connect_selection, with_selection,
};
use super::diff_wrap::{CellText, WrapMetrics, lay_out_cell};
use super::focus::{focus_ring_shadow, focus_visible};
use super::model::GitFileStat;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, FontStyle, FontWeight, HighlightStyle,
    InteractiveElement as _, IntoElement, ListAlignment, ListHorizontalSizingBehavior,
    ListSizingBehavior, ListState, ParentElement as _, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, UniformListScrollHandle, Window,
    div, list, px, rgb, uniform_list,
};
use gpui_base::TextSelectionHandle;
use gpui_component::h_flex;
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::tooltip::Tooltip;
use std::cell::RefCell;
use std::rc::Rc;

/// `.git-file-diff-line { min-height: 22px }`.
pub(crate) const DIFF_ROW_HEIGHT: f32 = 22.0;
/// The gutter column of `grid-template-columns: 56px minmax(0, 1fr)`.
const NUMBER_COLUMN_WIDTH: f32 = 56.0;
/// `.git-file-diff-control-button`: 28px square icon buttons, 6px apart, 14px icons.
const CONTROL_SIZE: f32 = 28.0;

const ICON_COLUMNS: &str = "modals/git-commit/columns-2.svg";
const ICON_ROWS: &str = "modals/git-commit/layout-rows.svg";
const ICON_WRAP: &str = "modals/git-commit/text-wrap.svg";
const ICON_PILCROW: &str = "modals/git-commit/pilcrow.svg";

/// Where the panel is drawn: the File diff dialog (the `.ghostex-settings-shadcn` skin, whose text
/// is all 400). The commit review's skin went with that window on 2026-10-25.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitDiffSkin {
    FileDiff,
}

/// The diff's colours, from packages/core-ui/styles/modals.css (`.git-file-diff-*`) and the light
/// token overrides in modals-light.css, resolved over the modal palette.
#[derive(Clone, Copy)]
pub(crate) struct GitDiffPalette {
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    pub(crate) text_weight: FontWeight,
    surface_background: Rgba,
    surface_border: Rgba,
    number_border: Rgba,
    split_cell_border: Rgba,
    number_text: Rgba,
    addition_row: Rgba,
    deletion_row: Rgba,
    addition_split_row: Rgba,
    deletion_split_row: Rgba,
    hunk_row: Rgba,
    hunk_text: Rgba,
    metadata_row: Rgba,
    metadata_text: Rgba,
    hover_row: Rgba,
    marker: Rgba,
    keyword: Rgba,
    type_name: Rgba,
    function: Rgba,
    string: Rgba,
    number: Rgba,
    comment: Rgba,
    operator: Rgba,
    pub(crate) stat_addition: Rgba,
    pub(crate) stat_deletion: Rgba,
    pub(crate) stat_divider: Rgba,
    /// `.git-file-diff-modal-path`: `color-mix(in srgb, var(--foreground) 84%, var(--muted-foreground) 16%)`.
    pub(crate) path_text: Rgba,
    control_border: Rgba,
    control_text: Rgba,
    control_hover: Rgba,
    control_radius: f32,
    /// The hover-only 5px scrollbar thumb.
    pub(crate) scrollbar_thumb: Rgba,
    pub(crate) scrollbar_thumb_hover: Rgba,
}

impl GitDiffPalette {
    pub(crate) fn resolve(p: &ModalPalette, skin: GitDiffSkin) -> Self {
        let ink = if p.light { 0x000000 } else { 0xffffff };
        let fg = p.foreground;
        let muted = p.muted;
        // `--border` and `--input` as the two skins resolve them (the React computed styles).
        let (border_alpha, input_alpha) = match (skin, p.light) {
            (GitDiffSkin::FileDiff, false) => (0.0784, 0.0784),
            (GitDiffSkin::FileDiff, true) => (0.1412, 0.1608),
        };
        let border = modal_rgba(ink, border_alpha);
        // `color-mix(in srgb, var(--card) 84%, var(--background) 16%)`: the File diff's dark tokens
        // resolve to #151515 over #0d0d0d; light is white. Under window glass it becomes an ink
        // wash at the same step, like every other native modal surface.
        let surface_background = if p.light && !p.glass {
            rgb(0xffffff)
        } else {
            let surface_step = match skin {
                GitDiffSkin::FileDiff => 0.03,
            };
            if p.glass {
                modal_rgba(ink, surface_step)
            } else {
                css_mix(rgb(ink), surface_step, p.solid_surface)
            }
        };
        let app_muted = if p.light { rgb(0x717171) } else { muted };
        let pick = |dark: u32, light: u32| rgb(if p.light { light } else { dark });
        Self {
            foreground: fg,
            muted,
            text_weight: match skin {
                GitDiffSkin::FileDiff => FontWeight::NORMAL,
            },
            surface_background,
            surface_border: css_fade(border, 0.86),
            number_border: css_fade(border, 0.72),
            split_cell_border: css_fade(border, 0.54),
            number_text: rgba_of(muted, 0.76),
            addition_row: modal_rgba(0x16a34a, 0.16),
            deletion_row: modal_rgba(0xef4444, 0.16),
            addition_split_row: modal_rgba(0x16a34a, 0.10),
            deletion_split_row: modal_rgba(0xef4444, 0.10),
            hunk_row: modal_rgba(0x3b82f6, 0.16),
            hunk_text: css_mix(rgb(0x93c5fd), 0.88, fg),
            metadata_row: rgba_of(fg, 0.05),
            metadata_text: css_mix(fg, 0.62, muted),
            hover_row: rgba_of(fg, 0.06),
            marker: rgba_of(muted, 0.74),
            keyword: pick(0xc792ea, 0x8250df),
            type_name: pick(0x82aaff, 0x0550ae),
            function: pick(0xaddb67, 0x6639ba),
            string: pick(0xc3e88d, 0x0a5b35),
            number: pick(0xf78c6c, 0x953800),
            comment: if p.light {
                rgb(0x626262)
            } else {
                css_mix(rgb(0x7f8ea3), 0.82, muted)
            },
            operator: if p.light {
                rgb(0x0550ae)
            } else {
                css_mix(rgb(0x89ddff), 0.78, fg)
            },
            stat_addition: css_mix(rgb(0x26b36a), 0.88, fg),
            stat_deletion: css_mix(rgb(0xe05656), 0.88, fg),
            stat_divider: css_mix(app_muted, 0.82, fg),
            path_text: css_mix(fg, 0.84, muted),
            control_border: css_fade(modal_rgba(ink, input_alpha), 0.88),
            control_text: css_mix(fg, 0.72, muted),
            control_hover: css_fade(modal_rgba(ink, input_alpha), 0.46),
            control_radius: match skin {
                GitDiffSkin::FileDiff => 8.0,
            },
            scrollbar_thumb: if p.light {
                modal_rgba(0x000000, 0.3)
            } else {
                modal_rgba(0xffffff, 0.18)
            },
            scrollbar_thumb_hover: if p.light {
                modal_rgba(0x000000, 0.3)
            } else {
                modal_rgba(0xffffff, 0.24)
            },
        }
    }

    fn token_color(&self, kind: DiffTokenKind) -> Option<Rgba> {
        match kind {
            DiffTokenKind::Plain => None,
            DiffTokenKind::Marker => Some(self.marker),
            DiffTokenKind::Keyword => Some(self.keyword),
            DiffTokenKind::Type | DiffTokenKind::Attribute => Some(self.type_name),
            DiffTokenKind::Function => Some(self.function),
            DiffTokenKind::String => Some(self.string),
            DiffTokenKind::Number | DiffTokenKind::Literal => Some(self.number),
            DiffTokenKind::Comment => Some(self.comment),
            DiffTokenKind::Operator => Some(self.operator),
        }
    }
}

/// The React panel's scrollbars: 5px, no track, a thumb that shows only while the pane is hovered.
pub(crate) fn hover_scrollbar(scrollbar: Scrollbar, dp: &GitDiffPalette) -> Scrollbar {
    let (thumb, thumb_hover) = (hsla(dp.scrollbar_thumb), hsla(dp.scrollbar_thumb_hover));
    scrollbar
        .mode(ScrollbarMode::Hover)
        .thickness(px(5.0))
        .styles(|styles| {
            styles
                .track(|track| track.bg(transparent()).border_color(transparent()))
                .track_hover(|track| track.bg(transparent()).border_color(transparent()))
                .thumb(|t| t.bg(thumb))
                .thumb_hover(|t| t.bg(thumb_hover))
                .thumb_active(|t| t.bg(thumb_hover))
        })
}

/// The `+N / -N` of a file or a selection: mono, the additions and deletions colours, 4px apart.
pub(crate) fn render_diff_stat(
    dp: &GitDiffPalette,
    stat: GitFileStat,
    size: f32,
    line_height: f32,
    weight: FontWeight,
    gap: f32,
) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .gap(px(gap))
        .font_family(MODAL_MONO_FONT)
        .text_size(px(size))
        .line_height(px(line_height))
        .font_weight(weight)
        .whitespace_nowrap()
        .child(
            div()
                .text_color(hsla(dp.stat_addition))
                .child(format!("+{}", stat.additions)),
        )
        .child(div().text_color(hsla(dp.stat_divider)).child("/"))
        .child(
            div()
                .text_color(hsla(dp.stat_deletion))
                .child(format!("-{}", stat.deletions)),
        )
        .into_any_element()
}

/// The parsed patch the panel draws, its whitespace filter, and the two scroll states.
pub(crate) struct GitDiffView {
    patch: Option<String>,
    lines: Rc<Vec<DiffLine>>,
    visible: Rc<Vec<usize>>,
    /// Index into `visible` of the widest row, measured for the horizontal scroll.
    widest: usize,
    hide_whitespace: bool,
    wrap: bool,
    split: bool,
    list: ListState,
    uniform: UniformListScrollHandle,
    /// The rows' GPUI-Kit selection participant and what it shares with the rows.
    selection: TextSelectionHandle,
    selection_state: Rc<RefCell<DiffSelectionState>>,
    _selection_events: Subscription,
}

impl GitDiffView {
    pub(crate) fn new(window: &Window, cx: &mut App) -> Self {
        let selection = TextSelectionHandle::new("", cx);
        let selection_state = Rc::new(RefCell::new(DiffSelectionState::default()));
        let events = connect_selection(&selection, &selection_state, window, cx);
        Self {
            patch: None,
            lines: Rc::new(Vec::new()),
            visible: Rc::new(Vec::new()),
            widest: 0,
            hide_whitespace: false,
            wrap: false,
            split: false,
            list: ListState::new(0, ListAlignment::Top, px(400.0)),
            uniform: UniformListScrollHandle::new(),
            selection,
            selection_state,
            _selection_events: events,
        }
    }

    /// A press on the diff text takes focus off the message editor and the controls, the way a
    /// press on non-focusable page text blurs the focused element: `focus` is the dialog's own.
    pub(crate) fn focus_on_select(&self, focus: FocusHandle, cx: &mut App) {
        self.selection
            .focus_with(move |window, cx| focus.focus(window, cx), cx);
    }

    /// Shows `patch`, or nothing. The same patch again keeps the scroll position.
    pub(crate) fn set_patch(&mut self, patch: Option<&str>, prefs: GitDiffPrefs) {
        let changed = self.patch.as_deref() != patch;
        if changed {
            self.patch = patch.map(str::to_string);
            self.lines = Rc::new(patch.map(parse_diff_lines).unwrap_or_default());
            self.uniform = UniformListScrollHandle::new();
        }
        let hide_changed = self.hide_whitespace != prefs.hide_whitespace;
        let wrap_changed = self.wrap != prefs.effective_line_wrap();
        let split = prefs.view_mode == GitDiffViewMode::Split;
        let split_changed = self.split != split;
        if changed || hide_changed || wrap_changed {
            self.hide_whitespace = prefs.hide_whitespace;
            self.wrap = prefs.effective_line_wrap();
            self.refresh_visible();
        } else if split_changed {
            self.list.remeasure();
        }
        if changed || hide_changed || wrap_changed || split_changed {
            self.split = split;
            // Line wrap alone re-wraps the same text nodes in React, keeping the selection.
            self.selection_state.borrow_mut().set_rows(
                self.lines.clone(),
                self.visible.clone(),
                split,
                changed || hide_changed || split_changed,
            );
        }
    }

    fn refresh_visible(&mut self) {
        let visible: Vec<usize> = self
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !(self.hide_whitespace && is_whitespace_only_change(line)))
            .map(|(index, _)| index)
            .collect();
        self.widest = visible
            .iter()
            .enumerate()
            .max_by_key(|(_, index)| {
                let content = &self.lines[**index].content;
                expand_tabs(content).0.chars().count()
            })
            .map(|(position, _)| position)
            .unwrap_or(0);
        self.list
            .reset_with_uniform_height(visible.len(), px(DIFF_ROW_HEIGHT));
        self.visible = Rc::new(visible);
    }
}

struct DiffRows {
    lines: Rc<Vec<DiffLine>>,
    visible: Rc<Vec<usize>>,
    palette: GitDiffPalette,
    split: bool,
    wrap: bool,
    /// Where a unified (or full-width) cell and a split half break, once the rows' width is known.
    wrap_full: Option<WrapMetrics>,
    wrap_half: Option<WrapMetrics>,
    selection: Option<DiffSelection>,
    state: Rc<RefCell<DiffSelectionState>>,
}

impl DiffRows {
    fn line(&self, index: usize) -> Option<&DiffLine> {
        self.visible
            .get(index)
            .and_then(|line| self.lines.get(*line))
    }

    fn number_cell(&self, number: usize) -> gpui::Div {
        let dp = &self.palette;
        div()
            .flex_shrink_0()
            .w(px(NUMBER_COLUMN_WIDTH))
            .when(!self.wrap, |this| this.h_full())
            .when(self.wrap, |this| this.self_stretch())
            .pt(px(3.0))
            .pb(px(3.0))
            .pl(px(8.0))
            .pr(px(10.0))
            .border_r_1()
            .border_color(hsla(dp.number_border))
            .text_right()
            .text_size(px(11.0))
            .line_height(px(15.714))
            .font_weight(dp.text_weight)
            .text_color(hsla(dp.number_text))
            .child(number.to_string())
    }

    /// `renderDiffLineContent`: a code row's tokens in their colours, other rows as plain text,
    /// an empty cell as one space; wrapped where Chrome wraps it, and the selected part white on
    /// the selection blue.
    fn content_text(&self, kind: DiffLineKind, content: &str, cell: DiffCell) -> AnyElement {
        let wrap = if !self.wrap {
            None
        } else if self.split && kind.is_code() {
            self.wrap_half
        } else {
            self.wrap_full
        };
        let mut text = if content.is_empty() {
            CellText::blank()
        } else {
            lay_out_cell(content, wrap)
        };
        let dp = self.palette;
        let mut highlights: Vec<_> = if kind.is_code() {
            tokenize_diff_line(content)
                .into_iter()
                .filter_map(|(range, kind)| {
                    let color = dp.token_color(kind)?;
                    Some((
                        text.display_of(range.start)..text.display_of(range.end),
                        HighlightStyle {
                            color: Some(hsla(color)),
                            font_style: (kind == DiffTokenKind::Comment)
                                .then_some(FontStyle::Italic),
                            ..HighlightStyle::default()
                        },
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };
        if let Some((range, past_end)) = self
            .selection
            .and_then(|selection| selection.in_cell(cell, content.len()))
        {
            let start = text.display_of(range.start);
            let end = if !past_end {
                text.display_of(range.end)
            } else if content.is_empty() {
                text.display.len()
            } else {
                text.push_line_end();
                text.display.len()
            };
            if end > start {
                highlights = with_selection(highlights, start..end, text.display.len());
            }
        }
        DiffText::new(text, highlights, cell, self.state.clone()).into_any_element()
    }

    fn content_cell(
        &self,
        kind: DiffLineKind,
        content: &str,
        split_cell: bool,
        cell: DiffCell,
    ) -> gpui::Div {
        let dp = &self.palette;
        div()
            .pt(px(2.0))
            .pb(px(2.0))
            .px(px(12.0))
            .text_size(px(11.0))
            .line_height(px(17.05))
            .font_weight(dp.text_weight)
            // Wrapped rows carry their own breaks (diff_wrap.rs); gpui's wrapper never runs.
            .whitespace_nowrap()
            .map(|this| {
                if self.wrap {
                    this.flex_1().flex_basis(px(0.0)).min_w_0()
                } else {
                    this.flex_1()
                }
            })
            .when(split_cell, |this| {
                this.self_stretch()
                    .border_r_1()
                    .border_color(hsla(dp.split_cell_border))
            })
            .child(self.content_text(kind, content, cell))
    }

    fn row(&self, index: usize) -> AnyElement {
        let Some(line) = self.line(index) else {
            return div().into_any_element();
        };
        let dp = &self.palette;
        if self.split && line.kind.is_code() {
            let left = matches!(line.kind, DiffLineKind::Deletion | DiffLineKind::Context);
            let right = matches!(line.kind, DiffLineKind::Addition | DiffLineKind::Context);
            let background = match line.kind {
                DiffLineKind::Addition => Some(dp.addition_split_row),
                DiffLineKind::Deletion => Some(dp.deletion_split_row),
                _ => None,
            };
            return h_flex()
                .w_full()
                .min_h(px(DIFF_ROW_HEIGHT))
                .items_start()
                .text_color(hsla(dp.foreground))
                .when_some(background, |this, bg| this.bg(hsla(bg)))
                .child(self.number_cell(line.number))
                .child(self.content_cell(
                    line.kind,
                    if left { &line.content } else { "" },
                    true,
                    DiffCell {
                        row: index,
                        side: 0,
                    },
                ))
                .child(self.number_cell(line.number))
                .child(self.content_cell(
                    line.kind,
                    if right { &line.content } else { "" },
                    true,
                    DiffCell {
                        row: index,
                        side: 1,
                    },
                ))
                .into_any_element();
        }
        let (background, color) = match line.kind {
            DiffLineKind::Addition => (Some(dp.addition_row), dp.foreground),
            DiffLineKind::Deletion => (Some(dp.deletion_row), dp.foreground),
            DiffLineKind::Hunk => (Some(dp.hunk_row), dp.hunk_text),
            DiffLineKind::Metadata => (Some(dp.metadata_row), dp.metadata_text),
            DiffLineKind::Raw => (None, dp.muted),
            DiffLineKind::Context => (None, dp.foreground),
        };
        let hover = dp.hover_row;
        h_flex()
            .w_full()
            .map(|this| {
                if self.wrap {
                    this.min_h(px(DIFF_ROW_HEIGHT)).items_start()
                } else {
                    this.h(px(DIFF_ROW_HEIGHT))
                }
            })
            .text_color(hsla(color))
            .when_some(background, |this, bg| this.bg(hsla(bg)))
            // `.git-file-diff-line:hover` loses to the kind backgrounds declared after it.
            .when(background.is_none(), |this| {
                this.hover(move |this| this.bg(hsla(hover)))
            })
            .child(self.number_cell(line.number))
            .child(self.content_cell(
                line.kind,
                &line.content,
                false,
                DiffCell {
                    row: index,
                    side: 0,
                },
            ))
            .into_any_element()
    }
}

/// Rows past which a wrapped surface stops sizing itself to its rows: they would not fit the
/// File diff window anyway, and inferring the size lays every row out.
const INFER_WRAPPED_ROWS_LIMIT: usize = 200;

/// The surface as tall as its rows and no taller than the space it is given, for the File diff
/// dialog, whose patch body ends where the patch ends.
pub(crate) fn render_diff_surface_sized(
    view: &GitDiffView,
    dp: &GitDiffPalette,
    prefs: GitDiffPrefs,
    id: &'static str,
    window: &Window,
) -> AnyElement {
    let fill = prefs.effective_line_wrap() && view.visible.len() > INFER_WRAPPED_ROWS_LIMIT;
    diff_surface(view, dp, prefs, id, fill, window)
}

fn diff_surface(
    view: &GitDiffView,
    dp: &GitDiffPalette,
    prefs: GitDiffPrefs,
    id: &'static str,
    fill: bool,
    window: &Window,
) -> AnyElement {
    let sizing = if fill {
        ListSizingBehavior::Auto
    } else {
        ListSizingBehavior::Infer
    };
    let (selection, wrap_full, wrap_half) = {
        let state = view.selection_state.borrow();
        (
            state.selection(),
            state.wrap_metrics(false),
            state.wrap_metrics(true),
        )
    };
    let rows = Rc::new(DiffRows {
        lines: view.lines.clone(),
        visible: view.visible.clone(),
        palette: *dp,
        split: prefs.view_mode == GitDiffViewMode::Split,
        wrap: prefs.effective_line_wrap(),
        wrap_full,
        wrap_half,
        selection,
        state: view.selection_state.clone(),
    });
    let count = view.visible.len();
    // The padding is the surface border's room: the host paints the border on the scrolled box,
    // so it scrolls away with the rows as the React surface's does. It is one device pixel, the
    // width the 1px border is drawn at, so rows start on a whole pixel the way Chrome lays them
    // out; a fractional start rounds row edges apart and leaves hairline gaps between rows.
    let inset = px(1.0 / window.scale_factor());
    let body = if rows.wrap {
        let rows = rows.clone();
        list(view.list.clone(), move |index, _window, _cx| {
            rows.row(index)
        })
        .with_sizing_behavior(sizing)
        .w_full()
        .p(inset)
        .when(fill, |this| this.h_full())
        .into_any_element()
    } else {
        let rows = rows.clone();
        uniform_list(id, count, move |range, _window, _cx| {
            range.map(|index| rows.row(index)).collect::<Vec<_>>()
        })
        .with_sizing_behavior(sizing)
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(view.widest))
        .track_scroll(&view.uniform)
        .w_full()
        .p(inset)
        .when(fill, |this| this.h_full())
        .into_any_element()
    };
    let (scroll, scrollbar) = if rows.wrap {
        (
            DiffScroll::Wrapped(view.list.clone()),
            hover_scrollbar(Scrollbar::vertical(&view.list), dp),
        )
    } else {
        (
            DiffScroll::Rows(view.uniform.clone()),
            hover_scrollbar(Scrollbar::new(&view.uniform), dp),
        )
    };
    let host = DiffSurfaceHost {
        rows: body,
        scroll,
        selection: view.selection.clone(),
        state: view.selection_state.clone(),
        border: hsla(dp.surface_border),
        // `box-shadow: inset 0 1px 0 rgb(255 255 255 / 4%)`.
        highlight: hsla(modal_rgba(0xffffff, 0.04)),
        weight: dp.text_weight,
        inset,
    };
    div()
        .when(fill, |this| this.size_full())
        .when(!fill, |this| this.w_full().min_h_0().flex_shrink(1.0))
        .flex()
        .flex_col()
        .relative()
        .overflow_hidden()
        .bg(hsla(dp.surface_background))
        .font_family(MODAL_MONO_FONT)
        .child(host)
        .child(scrollbar)
        .into_any_element()
}

/// `GitFileDiffControls`: unified/split, line wrap (forced on and disabled in split) and hide
/// whitespace, each an outline icon button with its tooltip.
///
/// CDXC:Git 2026-06-08-04:07:
/// Diff display controls use a direct icon-control pattern: one tooltip button toggles unified/split, one toggles line wrapping, and one toggles whitespace-only changes. The commit modal hosts those controls in its file header while the standalone file diff keeps the same controls above its patch surface.
///
/// CDXC:Git 2026-06-08-04:47:
/// Side-by-side diff mode must force line wrapping so each half stays readable in the commit and standalone diff panes. Keep the wrap button disabled in split view and explain the forced state in its tooltip.
pub(crate) fn render_diff_controls<V: 'static>(
    dp: &GitDiffPalette,
    prefs: GitDiffPrefs,
    focus: [&FocusHandle; 3],
    window: &Window,
    on_change: impl Fn(&mut V, GitDiffPrefs, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let split = prefs.view_mode == GitDiffViewMode::Split;
    let wrap_forced = split;
    let effective_wrap = wrap_forced || prefs.line_wrap;
    let next_mode = if split {
        GitDiffViewMode::Unified
    } else {
        GitDiffViewMode::Split
    };
    let view_label = if split {
        "Switch to unified diff"
    } else {
        "Switch to split diff"
    };
    let wrap_label = if wrap_forced {
        "Line wrapping is forced on when side by side is used."
    } else if effective_wrap {
        "Disable line wrapping"
    } else {
        "Enable line wrapping"
    };
    let whitespace_label = if prefs.hide_whitespace {
        "Show whitespace changes"
    } else {
        "Hide whitespace changes"
    };
    let view_button = control_button(
        dp,
        "git-diff-control-view",
        if split { ICON_ROWS } else { ICON_COLUMNS },
        view_label,
        split,
        false,
        focus[0],
        window,
        {
            let on_change = on_change.clone();
            move |this: &mut V, window, cx| {
                on_change(
                    this,
                    GitDiffPrefs {
                        view_mode: next_mode,
                        ..prefs
                    },
                    window,
                    cx,
                )
            }
        },
        cx,
    );
    let wrap_button = control_button(
        dp,
        "git-diff-control-wrap",
        ICON_WRAP,
        wrap_label,
        effective_wrap,
        wrap_forced,
        focus[1],
        window,
        {
            let on_change = on_change.clone();
            move |this: &mut V, window, cx| {
                on_change(
                    this,
                    GitDiffPrefs {
                        line_wrap: !prefs.line_wrap,
                        ..prefs
                    },
                    window,
                    cx,
                )
            }
        },
        cx,
    );
    let whitespace_button = control_button(
        dp,
        "git-diff-control-whitespace",
        ICON_PILCROW,
        whitespace_label,
        prefs.hide_whitespace,
        false,
        focus[2],
        window,
        move |this: &mut V, window, cx| {
            on_change(
                this,
                GitDiffPrefs {
                    hide_whitespace: !prefs.hide_whitespace,
                    ..prefs
                },
                window,
                cx,
            )
        },
        cx,
    );
    h_flex()
        .flex_shrink_0()
        .items_center()
        .justify_end()
        .gap(px(6.0))
        .child(view_button)
        .child(wrap_button)
        .child(whitespace_button)
        .into_any_element()
}

/// One `.git-file-diff-control-button`: shadcn outline `icon-xs` restyled to 28px, a quiet border,
/// the active state as a 12% foreground fill, hover (and keyboard focus, with shadcn's 3px ring) on
/// 46% `--input`. The disabled wrap button is not a tab stop.
#[allow(clippy::too_many_arguments)]
fn control_button<V: 'static>(
    dp: &GitDiffPalette,
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    active: bool,
    disabled: bool,
    focus: &FocusHandle,
    window: &Window,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let fg = dp.foreground;
    let hover = dp.control_hover;
    let focused = !disabled && focus_visible(focus, window);
    let (background, border, color) = if focused {
        (Some(hover), dp.control_border, fg)
    } else if active {
        (Some(rgba_of(fg, 0.12)), rgba_of(fg, 0.28), fg)
    } else {
        (None, dp.control_border, dp.control_text)
    };
    let group: SharedString = format!("{id}-group").into();
    div()
        .id(id)
        .group(group.clone())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded(px(dp.control_radius))
        .border_1()
        .border_color(hsla(border))
        .when_some(background, |this, bg| this.bg(hsla(bg)))
        .when(focused, |this| this.shadow(focus_ring_shadow()))
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .when(disabled, |this| this.opacity(0.5))
        .when(!disabled, |this| {
            this.track_focus(focus)
                .cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_click(this, window, cx);
                }))
        })
        .child(modal_icon(icon, 14.0, color).when(!disabled, |svg| {
            svg.group_hover(group, move |style| style.text_color(hsla(fg)))
        }))
        .into_any_element()
}
