//! JSON in the Files view: Format and Minify for strict JSON, and the Tree view.
//!
//! CDXC:Docs 2026-10-17 DECISION:
//! User (Q220): JSON gets "format and tree". Format and Minify in the header re-space strict JSON keeping its key order and its numbers exactly as written; the result is an unsaved, undoable edit. They are offered for `.json` only, because JSONC comments would be lost. A Tree / Raw switch shows the file as a tree: fold objects and arrays, search keys and values, copy a value's path (`$.scripts.build`) or the value. The tree lists only what is unfolded and draws only the rows on screen, so a large file stays quick. `.jsonl` and `.ndjson` are coloured as JSON (`editor_language` in state.rs).

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, UniformListScrollHandle, Window, div, px, svg, uniform_list,
};
use gpui_component::Sizable as _;
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::Value;

use super::palette::DocsPalette;

/// The files Format and Minify are offered for: JSON with no comments or trailing commas.
pub(crate) fn is_strict_json(path: &str) -> bool {
    matches!(
        super::state::file_extension(path).as_str(),
        "json" | "geojson" | "webmanifest"
    )
}

/// The files the Tree view is offered for.
pub(crate) fn has_tree(path: &str) -> bool {
    is_strict_json(path)
}

/// Why `text` is not valid JSON, with the line and column serde reports.
fn parse_error(error: &serde_json::Error) -> String {
    let message = error.to_string();
    // serde appends " at line L column C"; say it the way the editor counts.
    let message = message
        .split(" at line ")
        .next()
        .unwrap_or(&message)
        .to_string();
    format!(
        "Line {}, column {}: {message}",
        error.line(),
        error.column()
    )
}

/// `text` re-spaced: two-space indents when `pretty`, no whitespace at all otherwise. Keys keep
/// their order and strings and numbers their exact text, because only the whitespace between
/// tokens changes. A trailing newline stays when the file had one.
pub(crate) fn reformat(text: &str, pretty: bool) -> Result<String, String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).map_err(|error| parse_error(&error))?;
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut depth = 0usize;
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    };
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' => {
                // Every byte that ends a string is ASCII, so the slice stays on char boundaries.
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                out.push_str(&text[start..index.min(bytes.len())]);
                continue;
            }
            b'{' | b'[' => {
                let close = if byte == b'{' { b'}' } else { b']' };
                let mut next = index + 1;
                while next < bytes.len() && is_space(bytes[next]) {
                    next += 1;
                }
                out.push(byte as char);
                if bytes.get(next) == Some(&close) {
                    out.push(close as char);
                    index = next + 1;
                    continue;
                }
                depth += 1;
                if pretty {
                    newline(&mut out, depth);
                }
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if pretty {
                    newline(&mut out, depth);
                }
                out.push(byte as char);
            }
            b',' => {
                out.push(',');
                if pretty {
                    newline(&mut out, depth);
                }
            }
            b':' => {
                out.push(':');
                if pretty {
                    out.push(' ');
                }
            }
            _ if is_space(byte) => {}
            _ => {
                let start = index;
                while index < bytes.len()
                    && !is_space(bytes[index])
                    && !matches!(bytes[index], b',' | b':' | b'{' | b'}' | b'[' | b']' | b'"')
                {
                    index += 1;
                }
                out.push_str(&text[start..index]);
                continue;
            }
        }
        index += 1;
    }
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// What a tree row stands for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonKind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

#[derive(Clone)]
struct TreeRow {
    depth: usize,
    /// `$.scripts.build`.
    path: SharedString,
    /// The key, `[3]` for an array item, or `$` for the root.
    label: SharedString,
    summary: SharedString,
    kind: JsonKind,
    expanded: bool,
    /// The value's JSON Pointer, for Copy value.
    pointer: Rc<str>,
}

const ROW_HEIGHT: f32 = 22.0;
const INDENT: f32 = 14.0;
/// A string longer than this shows its start.
const SUMMARY_CHARS: usize = 240;

fn child_path(parent: &str, key: &str) -> String {
    let identifier = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if identifier {
        format!("{parent}.{key}")
    } else {
        format!(
            "{parent}[{}]",
            serde_json::to_string(key).unwrap_or_default()
        )
    }
}

fn kind_and_summary(value: &Value) -> (JsonKind, String) {
    match value {
        Value::Object(map) => (
            JsonKind::Object,
            match map.len() {
                1 => "{1 key}".to_string(),
                count => format!("{{{count} keys}}"),
            },
        ),
        Value::Array(items) => (
            JsonKind::Array,
            match items.len() {
                1 => "[1 item]".to_string(),
                count => format!("[{count} items]"),
            },
        ),
        Value::String(text) => {
            let mut shown: String = text.chars().take(SUMMARY_CHARS).collect();
            if shown.len() < text.len() {
                shown.push('…');
            }
            (
                JsonKind::String,
                serde_json::to_string(&shown).unwrap_or_default(),
            )
        }
        Value::Number(number) => (JsonKind::Number, number.to_string()),
        Value::Bool(flag) => (JsonKind::Bool, flag.to_string()),
        Value::Null => (JsonKind::Null, "null".to_string()),
    }
}

/// The open file's JSON as a tree.
pub(crate) struct DocsJsonTree {
    root: Result<Rc<Value>, String>,
    /// Unfolded containers, by path.
    expanded: HashSet<String>,
    rows: Rc<Vec<TreeRow>>,
    query: String,
    search: Entity<InputState>,
    scroll: UniformListScrollHandle,
    /// Set by the Files view before each draw.
    pub(crate) palette: DocsPalette,
    copy: Rc<dyn Fn(String, &mut App)>,
    _search_subscription: Subscription,
}

impl DocsJsonTree {
    pub(crate) fn new(
        text: &str,
        palette: DocsPalette,
        copy: Rc<dyn Fn(String, &mut App)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search keys and values"));
        let subscription = cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = input.read(cx).value().trim().to_lowercase();
                this.rebuild();
                cx.notify();
            }
        });
        let root = serde_json::from_str::<Value>(text)
            .map(Rc::new)
            .map_err(|error| parse_error(&error));
        let mut tree = Self {
            root,
            expanded: HashSet::from(["$".to_string()]),
            rows: Rc::new(Vec::new()),
            query: String::new(),
            search,
            scroll: UniformListScrollHandle::new(),
            palette,
            copy,
            _search_subscription: subscription,
        };
        // A small root opens one level further, so the first look shows its shape.
        if let Ok(root) = &tree.root {
            let children: Vec<String> = match root.as_ref() {
                Value::Object(map) if map.len() <= 40 => {
                    map.keys().map(|key| child_path("$", key)).collect()
                }
                Value::Array(items) if items.len() <= 40 => {
                    (0..items.len()).map(|ix| format!("$[{ix}]")).collect()
                }
                _ => Vec::new(),
            };
            tree.expanded.extend(children);
        }
        tree.rebuild();
        tree
    }

    /// Lists the rows on show: every unfolded container's children or, while searching, every
    /// match with the containers above it.
    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        if let Ok(root) = self.root.clone() {
            self.walk(
                &root,
                "$".to_string(),
                String::new(),
                "$".into(),
                0,
                &mut rows,
            );
        }
        self.rows = Rc::new(rows);
    }

    /// Adds `value`'s row and, when open, its children's; returns whether it or anything under
    /// it matched the search (always true without one).
    fn walk(
        &self,
        value: &Value,
        path: String,
        pointer: String,
        label: SharedString,
        depth: usize,
        rows: &mut Vec<TreeRow>,
    ) -> bool {
        let searching = !self.query.is_empty();
        let (kind, summary) = kind_and_summary(value);
        let container = matches!(kind, JsonKind::Object | JsonKind::Array);
        let own_match = searching
            && (label.to_lowercase().contains(&self.query)
                || (!container && summary.to_lowercase().contains(&self.query)));
        let start = rows.len();
        let open = container && (searching || self.expanded.contains(&path));
        rows.push(TreeRow {
            depth,
            path: path.clone().into(),
            label,
            summary: summary.into(),
            kind,
            expanded: open,
            pointer: pointer.clone().into(),
        });
        let mut child_match = false;
        if open {
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        let escaped = key.replace('~', "~0").replace('/', "~1");
                        child_match |= self.walk(
                            child,
                            child_path(&path, key),
                            format!("{pointer}/{escaped}"),
                            key.clone().into(),
                            depth + 1,
                            rows,
                        );
                    }
                }
                Value::Array(items) => {
                    for (ix, child) in items.iter().enumerate() {
                        child_match |= self.walk(
                            child,
                            format!("{path}[{ix}]"),
                            format!("{pointer}/{ix}"),
                            format!("[{ix}]").into(),
                            depth + 1,
                            rows,
                        );
                    }
                }
                _ => {}
            }
        }
        if searching {
            if !own_match && !child_match {
                rows.truncate(start);
                return false;
            }
            // Only matching children were kept; a container that matched by name alone shows
            // folded.
            rows[start].expanded = child_match;
        }
        true
    }

    /// Puts `query` in the search box, as typing it would.
    #[allow(dead_code)] // The Files preview binary (src/bin/native_docs_demo) searches with it.
    pub(crate) fn search_for(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            search.set_value(query.to_string(), window, cx)
        });
        self.query = query.trim().to_lowercase();
        self.rebuild();
        cx.notify();
    }

    fn toggle(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.query.is_empty() {
            return;
        }
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_string());
        }
        self.rebuild();
        cx.notify();
    }

    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded = HashSet::from(["$".to_string()]);
        self.rebuild();
        cx.notify();
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let p = self.palette.clone();
        let rows = self.rows.clone();
        range
            .filter_map(|ix| rows.get(ix).cloned().map(|row| (ix, row)))
            .map(|(ix, row)| {
                let container = matches!(row.kind, JsonKind::Object | JsonKind::Array);
                let value_color = match row.kind {
                    JsonKind::String => p.green,
                    JsonKind::Number => p.accent,
                    JsonKind::Bool | JsonKind::Null => p.amber,
                    JsonKind::Object | JsonKind::Array => p.subtle,
                };
                let toggle_path = row.path.clone();
                let (copy_path, pointer) = (row.path.to_string(), row.pointer.clone());
                let root = self.root.clone().ok();
                let (copy_a, copy_b) = (self.copy.clone(), self.copy.clone());
                div()
                    .id(("json-row", ix))
                    .group("json-row")
                    .flex()
                    .items_center()
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .pl(px(10.0 + row.depth as f32 * INDENT))
                    .pr(px(8.0))
                    .gap(px(4.0))
                    .whitespace_nowrap()
                    .hover(|style| style.bg(p.row_hover))
                    .when(container, |this| {
                        this.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle(&toggle_path, cx);
                            }))
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(px(12.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(container, |this| {
                                this.child(
                                    svg()
                                        .path(if row.expanded {
                                            "titlebar/chevron-down.svg"
                                        } else {
                                            "titlebar/chevron-right.svg"
                                        })
                                        .size(px(11.0))
                                        .text_color(p.muted),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .child(
                                div()
                                    .text_color(p.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(row.label.clone()),
                            )
                            .child(div().text_color(p.subtle).child(":")),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(value_color)
                            .child(row.summary.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .gap(px(2.0))
                            .invisible()
                            .group_hover("json-row", |style| style.visible())
                            .child(
                                row_button(("json-copy-path", ix), "Copy path", &p).on_click(
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        copy_a(copy_path.clone(), cx);
                                    },
                                ),
                            )
                            .child(
                                row_button(("json-copy-value", ix), "Copy value", &p).on_click(
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        let Some(value) =
                                            root.as_ref().and_then(|root| root.pointer(&pointer))
                                        else {
                                            return;
                                        };
                                        let text = match value {
                                            Value::String(text) => text.clone(),
                                            value => serde_json::to_string_pretty(value)
                                                .unwrap_or_default(),
                                        };
                                        copy_b(text, cx);
                                    },
                                ),
                            ),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

fn row_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    p: &DocsPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .px(px(6.0))
        .h(px(18.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(p.border)
        .bg(p.chrome)
        .text_size(px(11.0))
        .text_color(p.muted)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
}

impl Render for DocsJsonTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette.clone();
        let body = match &self.root {
            Err(error) => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .text_size(px(13.0))
                .text_color(p.muted)
                .child("This JSON can't be shown as a tree.")
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.subtle)
                        .child(error.clone()),
                )
                .into_any_element(),
            Ok(_) if self.rows.is_empty() => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(p.muted)
                .child("Nothing matches that search.")
                .into_any_element(),
            Ok(_) => uniform_list(
                "native-docs-json-tree",
                self.rows.len(),
                cx.processor(Self::render_rows),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .font_family(p.mono_font.clone())
            .text_size(px(12.0))
            .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(34.0))
                    .px(px(10.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        svg()
                            .path("titlebar/search.svg")
                            .size(px(14.0))
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .child(Input::new(&self.search).appearance(false).small()),
                    )
                    .child(
                        row_button("json-collapse-all", "Collapse all", &p)
                            .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
                    ),
            )
            .child(body)
    }
}
