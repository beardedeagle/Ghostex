//! Drawing the Changes view: the toolbar, the file list, the stream with its pinned file header,
//! the file menu and the keys.

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, canvas, div, list, px,
};
use gpui_component::{h_flex, v_flex};

use super::model::{ChangesScope, Row};
use super::rows::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, checkbox, stat, status_badge};
use super::style::ChangesStyle;
use super::view::{ChangesRequest, ChangesView, Confirm};
use crate::app::window::native_modal_kit::{
    MODAL_UI_FONT, hsla, modal_icon, modal_spinner, rgba_of,
};

const ICON_UNIFIED: &str = "modals/git-commit/layout-rows.svg";
const ICON_SPLIT: &str = "modals/git-commit/columns-2.svg";
const ICON_WRAP: &str = "modals/git-commit/text-wrap.svg";
const ICON_WHITESPACE: &str = "modals/git-commit/pilcrow.svg";
const ICON_REFRESH: &str = "titlebar/refresh.svg";
const ICON_FOLD: &str = "titlebar/arrows-diagonal-minimize.svg";
const ICON_UNFOLD: &str = "titlebar/arrows-diagonal-2.svg";
const ICON_BRANCH: &str = "titlebar/git-branch.svg";

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(s) = self.style else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let body = self.render_body(&s, window, cx);
        v_flex()
            .id("changes-view")
            .key_context("ChangesView")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .min_h_0()
            .relative()
            .font_family(MODAL_UI_FONT)
            .text_color(hsla(s.text))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.end_selection();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.file_menu.take().is_some()
                        || this.commit_menu_open
                        || this.agent_select.open
                    {
                        this.commit_menu_open = false;
                        this.agent_select.close();
                        cx.notify();
                    }
                }),
            )
            // The view's own origin, so menus asked for at a pointer position land under it.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |this, _| this.view_origin = bounds.origin);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(self.render_toolbar(&s, cx))
            .child(body)
            .children(self.render_commit_bar(&s, window, cx))
            .children(self.render_file_menu(&s, cx))
            .into_any_element()
    }
}

impl ChangesView {
    fn render_toolbar(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let data = self.data.as_ref();
        let refs = data.map(|data| {
            let branch = data.branch.clone().unwrap_or_else(|| "HEAD".to_string());
            match self.scope {
                ChangesScope::AgainstMain if data.on_default_branch => {
                    format!("{branch} (uncommitted)")
                }
                ChangesScope::AgainstMain => format!("{branch} \u{2192} {}", data.base_label),
                ChangesScope::Uncommitted => branch,
            }
        });
        let file_count = data.map_or(0, |data| data.files.len());
        let (additions, deletions) =
            data.map_or((0, 0), |data| (data.additions(), data.deletions()));
        let scope_button =
            |scope: ChangesScope, label: &'static str, id: &'static str, cx: &mut Context<Self>| {
                let on = self.scope == scope;
                div()
                    .id(id)
                    .h_full()
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .text_color(hsla(if on { s.text } else { s.muted }))
                    .when(on, |this| {
                        this.bg(hsla(s.selected)).font_weight(FontWeight::MEDIUM)
                    })
                    .hover(|this| this.bg(hsla(s.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_scope(scope, cx)))
                    .child(label)
            };
        let toggle = |id: &'static str, icon: &'static str, on: bool, tip: &'static str| {
            div()
                .id(id)
                .flex_none()
                .size(px(26.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(on, |this| this.bg(hsla(s.selected)))
                .hover(|this| this.bg(hsla(s.hover)))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .child(modal_icon(icon, 14.0, if on { s.text } else { s.muted }))
        };
        let all_collapsed = self.all_collapsed();
        // A narrow panel wraps the switches onto a second row rather than clipping them.
        h_flex()
            .flex_none()
            .flex_wrap()
            .w_full()
            .min_h(px(38.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(6.0))
            .border_b_1()
            .border_color(hsla(s.hairline))
            .whitespace_nowrap()
            .child(
                h_flex()
                    .flex_none()
                    .h(px(26.0))
                    .rounded(px(6.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(hsla(s.hairline))
                    .child(scope_button(
                        ChangesScope::AgainstMain,
                        "Against main",
                        "changes-scope-main",
                        cx,
                    ))
                    .child(div().w(px(1.0)).h_full().bg(hsla(s.hairline)))
                    .child(scope_button(
                        ChangesScope::Uncommitted,
                        "Uncommitted",
                        "changes-scope-uncommitted",
                        cx,
                    )),
            )
            .when_some(refs, |this, refs| {
                this.child(
                    h_flex()
                        .min_w_0()
                        .max_w(px(240.0))
                        .flex_shrink(1.0)
                        .gap(px(4.0))
                        .overflow_hidden()
                        .text_size(px(11.5))
                        .text_color(hsla(s.muted))
                        .child(modal_icon(ICON_BRANCH, 12.0, s.muted))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(refs),
                        ),
                )
            })
            .when(file_count > 0, |this| {
                this.child(stat(s, additions, deletions))
            })
            .child(div().flex_1().min_w(px(0.0)))
            .when(self.loading, |this| {
                this.child(div().flex_none().child(modal_spinner(s.muted)))
            })
            .child(
                h_flex()
                    .flex_none()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(hsla(s.hairline))
                    .child(
                        toggle("changes-unified", ICON_UNIFIED, !self.split, "Unified")
                            .on_click(cx.listener(|this, _, _, cx| this.set_split(false, cx))),
                    )
                    .child(
                        toggle("changes-split", ICON_SPLIT, self.split, "Split")
                            .on_click(cx.listener(|this, _, _, cx| this.set_split(true, cx))),
                    ),
            )
            .child(
                toggle(
                    "changes-wrap",
                    ICON_WRAP,
                    self.wrap || self.split,
                    if self.split {
                        "Split view always wraps"
                    } else {
                        "Wrap long lines"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wrap = !this.wrap;
                    cx.notify();
                })),
            )
            .child(
                toggle(
                    "changes-whitespace",
                    ICON_WHITESPACE,
                    self.hide_whitespace,
                    "Hide whitespace changes",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let hide = !this.hide_whitespace;
                    this.set_hide_whitespace(hide, cx);
                })),
            )
            .child(
                toggle(
                    "changes-fold-all",
                    if all_collapsed {
                        ICON_UNFOLD
                    } else {
                        ICON_FOLD
                    },
                    false,
                    if all_collapsed {
                        "Expand all files"
                    } else {
                        "Collapse all files"
                    },
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.set_all_collapsed(!all_collapsed, cx)),
                ),
            )
            .child(
                toggle("changes-refresh", ICON_REFRESH, false, "Refresh")
                    .on_click(cx.listener(|this, _, _, cx| this.load(false, cx))),
            )
            .when(file_count > 0, |this| {
                this.child(
                    h_flex()
                        .id("changes-list-toggle")
                        .flex_none()
                        .h(px(26.0))
                        .px(px(8.0))
                        .gap(px(4.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .text_color(hsla(s.muted))
                        .hover(|this| this.bg(hsla(s.hover)))
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Show or hide the file list")
                                .build(window, cx)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.list_open = !this.list_open;
                            cx.notify();
                        }))
                        .child(format!(
                            "{file_count} {}",
                            if file_count == 1 { "file" } else { "files" }
                        ))
                        .child(modal_icon(
                            if self.list_open {
                                ICON_CHEVRON_DOWN
                            } else {
                                ICON_CHEVRON_RIGHT
                            },
                            11.0,
                            s.muted,
                        )),
                )
            })
            .into_any_element()
    }

    fn render_body(
        &mut self,
        s: &ChangesStyle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.target.is_none() {
            return message(s, "Open a project to see its changes.", None);
        }
        if let Some(error) = &self.error
            && self.data.is_none()
        {
            return message(s, "Could not read the changes", Some(error.clone()));
        }
        let Some(data) = &self.data else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(modal_spinner(s.muted))
                .into_any_element();
        };
        if data.files.is_empty() {
            let text = match self.scope {
                ChangesScope::AgainstMain if data.on_default_branch => "No uncommitted changes.",
                ChangesScope::AgainstMain => "This branch has no changes against its base.",
                ChangesScope::Uncommitted => "No uncommitted changes.",
            };
            return message(s, text, None);
        }
        let truncated = data.truncated;
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(self.list_open, |this| {
                this.child(self.render_file_list(s, cx))
            })
            .when(truncated, |this| {
                this.child(
                    div()
                        .px(px(12.0))
                        .py(px(4.0))
                        .text_size(px(11.5))
                        .text_color(hsla(s.badge_modified))
                        .child(
                            "The diff is too large to show completely; some files are left out.",
                        ),
                )
            })
            .child(self.render_stream(s, cx))
            .into_any_element()
    }

    fn render_file_list(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = &self.data else {
            return div().into_any_element();
        };
        let current = self.current_file();
        let files: Vec<_> = data.files.iter().cloned().collect();
        let rows = files.into_iter().map(|file| {
            let path = file.path.clone();
            let scroll_path = path.clone();
            let check_path = path.clone();
            let included = file.uncommitted && !self.excluded.contains(&path);
            let viewed = self.viewed.contains_key(&path);
            h_flex()
                .id(SharedString::from(format!("changes-list-{path}")))
                .w_full()
                .h(px(24.0))
                .px(px(10.0))
                .gap(px(6.0))
                .flex_none()
                .text_size(px(12.0))
                .cursor_pointer()
                .whitespace_nowrap()
                .when(current.as_deref() == Some(path.as_str()), |this| {
                    this.bg(hsla(s.selected))
                })
                .hover(|this| this.bg(hsla(s.hover)))
                .when(viewed, |this| this.opacity(0.55))
                .on_click(cx.listener(move |this, _, _, cx| this.scroll_to_file(&scroll_path, cx)))
                .when(file.uncommitted, |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!("changes-list-check-{path}")))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_included(&check_path, cx);
                            }))
                            .child(checkbox(s, included)),
                    )
                })
                .when(!file.uncommitted, |this| {
                    this.child(div().flex_none().w(px(13.0)))
                })
                .child(status_badge(s, file.status))
                .child(
                    div()
                        .flex_none()
                        .text_color(hsla(s.text))
                        .child(file.file_name().to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_shrink(1.0)
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(hsla(s.muted))
                        .child(file.folder().trim_end_matches('/').to_string()),
                )
                .child(div().flex_1())
                .child(stat(s, file.additions, file.deletions))
        });
        v_flex()
            .id("changes-file-list")
            .flex_none()
            .w_full()
            .max_h(px(180.0))
            .overflow_y_scroll()
            .py(px(3.0))
            .border_b_1()
            .border_color(hsla(s.hairline))
            .bg(hsla(s.panel))
            .children(rows)
            .into_any_element()
    }

    fn render_stream(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        // The file the top of the view is in, pinned over the stream once its own header has
        // scrolled away.
        let top = self.list.logical_scroll_top();
        let pinned = (0..=top.item_ix.min(self.rows.len().saturating_sub(1)))
            .rev()
            .find_map(|index| match self.rows.get(index) {
                Some(Row::File(file)) => Some((index, *file)),
                _ => None,
            })
            .filter(|(index, _)| *index < top.item_ix || top.offset_in_item > px(0.0));
        let header = pinned.map(|(_, file)| self.render_file_header(s, file, true, cx));
        let resolved = self.resolved_count();
        let view = cx.entity().downgrade();
        let list_state = self.list.clone();
        list_state.set_scroll_handler(move |_, _, cx| {
            let _ = view.update(cx, |_, cx| cx.notify());
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(hsla(s.code_bg))
            .child(
                list(
                    self.list.clone(),
                    cx.processor(|this, index, window, cx| this.render_row(index, window, cx)),
                )
                .size_full(),
            )
            .when_some(header, |this, header| {
                this.child(div().absolute().top_0().left_0().w_full().child(header))
            })
            .when(resolved > 0, |this| {
                let label = if self.show_resolved {
                    format!("Hide resolved comments ({resolved})")
                } else {
                    format!("Show resolved comments ({resolved})")
                };
                this.child(
                    div()
                        .id("changes-show-resolved")
                        .absolute()
                        .bottom(px(8.0))
                        .right(px(12.0))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(hsla(s.hairline))
                        .bg(hsla(s.header_bg))
                        .text_size(px(11.0))
                        .text_color(hsla(s.muted))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_show_resolved(cx)))
                        .child(label),
                )
            })
            .into_any_element()
    }

    fn render_file_menu(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (path, position) = self.file_menu.clone()?;
        let item = |id: &'static str, label: &'static str, danger: bool| {
            div()
                .id(id)
                .h(px(26.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .text_color(hsla(if danger { s.danger } else { s.text }))
                .hover(|this| this.bg(hsla(s.hover)))
                .child(label)
        };
        let (copy, open, reveal, revert) = (path.clone(), path.clone(), path.clone(), path.clone());
        Some(
            v_flex()
                .id("changes-file-menu")
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(220.0))
                .p(px(4.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(s.p.menu_border))
                .bg(hsla(s.p.menu_background))
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    item("changes-menu-copy", "Copy Path", false).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.file_menu = None;
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()));
                            (this.host)(ChangesRequest::Copied, window, cx);
                        },
                    )),
                )
                .child(
                    item("changes-menu-open", "Open in Files", false).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.file_menu = None;
                            this.open_in_files(&open, None, window, cx);
                        },
                    )),
                )
                .child(
                    item("changes-menu-reveal", "Open File/Folder Location", false).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.file_menu = None;
                            if let Some(target) = this.target.clone() {
                                (this.host)(
                                    ChangesRequest::RevealFile {
                                        project_key: target.project_key,
                                        path: reveal.clone(),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                    ),
                )
                .child(div().h(px(1.0)).my(px(4.0)).bg(hsla(s.hairline)))
                .child(
                    item("changes-menu-revert", "Revert file\u{2026}", true).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.file_menu = None;
                            this.confirm = Some(Confirm::RevertFile(revert.clone()));
                            this.scroll_to_file(&revert, cx);
                        },
                    )),
                )
                .into_any_element(),
        )
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Keys typed into the commit message or a comment belong to that field.
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window))
            || self
                .message
                .as_ref()
                .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
        {
            return;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.control {
            if keystroke.key == "c" && self.copy_selection(window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let handled = match keystroke.key.as_str() {
            "j" => {
                self.step(true, |row| matches!(row, Row::Hunk(..)), cx);
                true
            }
            "k" => {
                self.step(false, |row| matches!(row, Row::Hunk(..)), cx);
                true
            }
            "]" => {
                self.step(true, |row| matches!(row, Row::File(..)), cx);
                true
            }
            "[" => {
                self.step(false, |row| matches!(row, Row::File(..)), cx);
                true
            }
            "c" => {
                self.comment_at_cursor(window, cx);
                true
            }
            "v" => {
                if let Some(path) = self.current_file() {
                    self.toggle_viewed(&path, cx);
                    self.step(true, |row| matches!(row, Row::File(..)), cx);
                }
                true
            }
            "o" => {
                if let Some(path) = self.current_file() {
                    self.open_in_files(&path, None, window, cx);
                }
                true
            }
            "escape" => {
                let had = self.selection.take().is_some()
                    || self.file_menu.take().is_some()
                    || self.confirm.take().is_some()
                    || std::mem::take(&mut self.commit_menu_open);
                cx.notify();
                had
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }
}

fn message(s: &ChangesStyle, title: &str, detail: Option<String>) -> AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .p(px(24.0))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(hsla(s.text))
                .child(title.to_string()),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(rgba_of(s.muted, 0.9)))
                    .child(detail),
            )
        })
        .into_any_element()
}
