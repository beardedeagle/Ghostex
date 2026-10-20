//! A SQLite database in Files: its tables and views on the left, one page of the chosen table's
//! rows (or of a query's answer) in the Files table (`data_table.rs`), a query box above it and
//! the paging bar below. The reading happens in `ghostex_docs::sqlite`, through the `sqlite`
//! Files action, which gxserver answers for a remote project.
//!
//! CDXC:Docs 2026-10-17 SEE-ALSO: the read-only rules (Q223) are on `packages/project-docs/src/sqlite.rs`; the desktop's own bridge (`helpers/manage_docs/files_bridge.rs`) and gxserver (`server/src/project_docs/actions.rs`) answer the same action.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClipboardItem, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement as _, ParentElement as _, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::table::TableState;
use serde_json::{Value, json};

use super::data_table::{DocsDataTable, new_data_table, render_data_table, render_table_notice};
use super::palette::DocsPalette;
use super::state::DocsDocumentLoad;
use crate::GhostexGpuiApp;

const TABLE_LIST_WIDTH: f32 = 200.0;
const PAGE_ROWS: u64 = ghostex_docs::sqlite::DEFAULT_PAGE_ROWS;

/// One open database's browser: what is chosen, the page showing, and its widgets.
#[derive(Default)]
pub(crate) struct DocsSqliteView {
    /// (name, "table" | "view").
    tables: Vec<(String, String)>,
    selected: Option<String>,
    /// The query whose answer is showing, instead of a table.
    sql: Option<String>,
    offset: u64,
    columns_shown: bool,
    has_more: bool,
    total_rows: Option<u64>,
    shown_rows: u64,
    /// A query's or a page's error, shown above the rows that were there before.
    error: Option<String>,
    loading: bool,
    /// Bumped per request; an answer for an older one is dropped.
    request: u64,
    /// Rows waiting for the next draw, which has the window a new table needs.
    pending_rows: Option<(Vec<String>, Vec<Vec<String>>)>,
    table: Option<Entity<TableState<DocsDataTable>>>,
    query_input: Option<Entity<InputState>>,
    _query_subscription: Option<Subscription>,
    list_scroll: ScrollHandle,
}

impl GhostexGpuiApp {
    /// Reads the database's tables and the page the view is on (the first table's first page
    /// for a database just opened). Also how Files reloads one that changed on disk.
    pub(crate) fn native_docs_sqlite_load(&mut self, path: &str, cx: &mut Context<Self>) {
        let view = self.native_docs.sqlite.entry(path.to_string()).or_default();
        view.request += 1;
        view.loading = true;
        let request_number = view.request;
        let mut fields = json!({
            "path": path,
            "offset": view.offset,
            "limit": PAGE_ROWS,
        });
        if let Some(sql) = &view.sql {
            fields["sql"] = json!(sql);
        } else if let Some(table) = &view.selected {
            fields["table"] = json!(table);
        }
        let generation = self.native_docs.generation;
        let request = self.native_docs_request("sqlite", fields);
        let path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            this.native_docs_sqlite_apply(&path, request_number, &response, cx);
        });
        self.native_docs_notify(cx);
    }

    fn native_docs_sqlite_apply(
        &mut self,
        path: &str,
        request_number: u64,
        response: &Value,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.native_docs.sqlite.get_mut(path) else {
            return;
        };
        if view.request != request_number {
            return;
        }
        view.loading = false;
        let error = response["error"].as_str().map(str::to_string);
        let opened = view.columns_shown || !view.tables.is_empty();
        if let Some(error) = error {
            // A query or page that failed keeps the database open, with the reason above; a file
            // that is not a database, or is unreadable, gets the notice with Open in system app.
            let load = if opened {
                view.error = Some(error);
                DocsDocumentLoad::Ready
            } else {
                DocsDocumentLoad::Unsupported(error)
            };
            if let Some(document) = self.native_docs.document_mut(path) {
                document.load = load;
            }
            self.native_docs_notify(cx);
            return;
        }
        view.error = None;
        view.tables = response["tables"]
            .as_array()
            .map(|tables| {
                tables
                    .iter()
                    .filter_map(|table| {
                        Some((
                            table["name"].as_str()?.to_string(),
                            table["kind"].as_str().unwrap_or("table").to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(document) = self.native_docs.document_mut(path) {
            document.load = DocsDocumentLoad::Ready;
        }
        let view = self
            .native_docs
            .sqlite
            .get_mut(path)
            .expect("the view was just read");
        // A table that is gone (dropped, or the file was replaced) falls back to the first.
        if view.sql.is_none()
            && view
                .selected
                .as_ref()
                .is_none_or(|selected| !view.tables.iter().any(|(name, _)| name == selected))
        {
            view.offset = 0;
            view.selected = view.tables.first().map(|(name, _)| name.clone());
            if view.selected.is_some() {
                self.native_docs_sqlite_load(path, cx);
                return;
            }
        }
        if let Some(columns) = response["columns"].as_array() {
            let headers = columns
                .iter()
                .map(|column| column.as_str().unwrap_or_default().to_string())
                .collect();
            let rows: Vec<Vec<String>> = response["rows"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| {
                                    cells
                                        .iter()
                                        .map(|cell| cell.as_str().unwrap_or_default().to_string())
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            view.shown_rows = rows.len() as u64;
            view.has_more = response["hasMore"].as_bool().unwrap_or(false);
            view.total_rows = response["totalRows"].as_u64();
            view.columns_shown = true;
            view.pending_rows = Some((headers, rows));
        }
        self.native_docs_notify(cx);
    }

    fn native_docs_sqlite_select_table(
        &mut self,
        path: &str,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.native_docs.sqlite.get_mut(path) else {
            return;
        };
        view.selected = Some(table);
        view.sql = None;
        view.offset = 0;
        self.native_docs_sqlite_load(path, cx);
    }

    fn native_docs_sqlite_run_query(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(view) = self.native_docs.sqlite.get_mut(path) else {
            return;
        };
        let Some(input) = view.query_input.clone() else {
            return;
        };
        let sql = input.read(cx).value().trim().to_string();
        let view = self
            .native_docs
            .sqlite
            .get_mut(path)
            .expect("the view was just read");
        view.sql = (!sql.is_empty()).then_some(sql);
        view.offset = 0;
        self.native_docs_sqlite_load(path, cx);
    }

    fn native_docs_sqlite_clear_query(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.native_docs.sqlite.get_mut(path) else {
            return;
        };
        view.sql = None;
        view.offset = 0;
        if let Some(input) = view.query_input.clone() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.native_docs_sqlite_load(path, cx);
    }

    fn native_docs_sqlite_page(&mut self, path: &str, forward: bool, cx: &mut Context<Self>) {
        let Some(view) = self.native_docs.sqlite.get_mut(path) else {
            return;
        };
        view.offset = if forward {
            view.offset + PAGE_ROWS
        } else {
            view.offset.saturating_sub(PAGE_ROWS)
        };
        self.native_docs_sqlite_load(path, cx);
    }

    /// The browser for the open database.
    pub(crate) fn render_native_docs_sqlite(
        &mut self,
        path: &str,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.native_docs.sqlite.contains_key(path) {
            self.native_docs_sqlite_load(path, cx);
        }
        let view = self
            .native_docs
            .sqlite
            .get_mut(path)
            .expect("the view was just made");
        if view.query_input.is_none() {
            let input = cx.new(|cx| {
                InputState::new(window, cx).placeholder("SELECT … (read-only; press Enter to run)")
            });
            let query_path = path.to_string();
            view._query_subscription = Some(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.native_docs_sqlite_run_query(&query_path, cx);
                    }
                },
            ));
            view.query_input = Some(input);
        }
        if let Some((headers, rows)) = view.pending_rows.take() {
            match &view.table {
                Some(table) => table.update(cx, |state, cx| {
                    state.delegate_mut().set_rows(headers, rows);
                    state.refresh(cx);
                    state.scroll_to_row(0, cx);
                }),
                None => view.table = Some(new_data_table(headers, rows, window, cx)),
            }
        }
        let view = &self.native_docs.sqlite[path];
        let table_list = self.render_native_docs_sqlite_tables(path, view, p, cx);
        let query_row = self.render_native_docs_sqlite_query(path, view, p, cx);
        let notice = if let Some(error) = &view.error {
            Some(
                div()
                    .flex_none()
                    .px(px(12.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_size(px(12.0))
                    .text_color(p.danger)
                    .child(error.clone())
                    .into_any_element(),
            )
        } else if view.columns_shown && view.shown_rows == 0 && view.offset == 0 {
            Some(render_table_notice(
                if view.sql.is_some() {
                    "The query returned no rows.".to_string()
                } else {
                    "This table is empty.".to_string()
                },
                p,
            ))
        } else {
            None
        };
        let copy: std::rc::Rc<dyn Fn(String, &mut gpui::App)> = std::rc::Rc::new(|text, cx| {
            crate::app::helpers::gpui_copy_to_clipboard(ClipboardItem::new_string(text), cx);
        });
        let rows = match view.table.clone() {
            Some(table) => render_data_table(&table, p, copy, cx),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.5))
                .text_color(p.muted)
                .child(if view.loading {
                    "Reading the database…"
                } else {
                    "This database has no tables."
                })
                .into_any_element(),
        };
        let pager = self.render_native_docs_sqlite_pager(path, p, cx);
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(table_list)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(query_row)
                    .children(notice)
                    .child(div().flex_1().min_h_0().child(rows))
                    .children(pager),
            )
            .into_any_element()
    }

    fn render_native_docs_sqlite_tables(
        &self,
        path: &str,
        view: &DocsSqliteView,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let querying = view.sql.is_some();
        div()
            .id("native-docs-sqlite-tables")
            .flex_none()
            .w(px(TABLE_LIST_WIDTH))
            .h_full()
            .border_r_1()
            .border_color(p.border)
            .overflow_y_scroll()
            .track_scroll(&view.list_scroll)
            .py(px(6.0))
            .child(
                div()
                    .px(px(12.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.subtle)
                    .child("TABLES"),
            )
            .children(view.tables.iter().enumerate().map(|(ix, (name, kind))| {
                let selected = !querying && view.selected.as_deref() == Some(name.as_str());
                let table = name.clone();
                let row_path = path.to_string();
                div()
                    .id(("native-docs-sqlite-table", ix))
                    .mx(px(6.0))
                    .px(px(6.0))
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(if selected {
                        p.row_text_strong
                    } else {
                        p.row_text
                    })
                    .when(selected, |row| row.bg(p.row_surface))
                    .hover(|row| row.bg(p.row_hover))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(name.clone()),
                    )
                    .when(kind == "view", |row| {
                        row.child(
                            div()
                                .flex_none()
                                .text_size(px(10.5))
                                .text_color(p.subtle)
                                .child("view"),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_sqlite_select_table(&row_path, table.clone(), cx);
                    }))
            }))
            .into_any_element()
    }

    fn render_native_docs_sqlite_query(
        &self,
        path: &str,
        view: &DocsSqliteView,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let run_path = path.to_string();
        let clear_path = path.to_string();
        div()
            .flex_none()
            .h(px(40.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(26.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .bg(p.text.opacity(0.05))
                    .font_family(p.mono_font.clone())
                    .text_size(px(12.0))
                    .children(
                        view.query_input
                            .as_ref()
                            .map(|input| Input::new(input).appearance(false)),
                    ),
            )
            .child(
                sqlite_button("native-docs-sqlite-run", "Run", true, p).on_click(cx.listener(
                    move |this, _, _, cx| this.native_docs_sqlite_run_query(&run_path, cx),
                )),
            )
            .when(view.sql.is_some(), |row| {
                row.child(
                    sqlite_button("native-docs-sqlite-clear", "Back to table", true, p).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.native_docs_sqlite_clear_query(&clear_path, window, cx)
                        }),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_native_docs_sqlite_pager(
        &self,
        path: &str,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = self.native_docs.sqlite.get(path)?;
        if !view.columns_shown || (view.offset == 0 && !view.has_more) {
            return None;
        }
        let first = view.offset + u64::from(view.shown_rows > 0);
        let last = view.offset + view.shown_rows;
        let label: SharedString = match view.total_rows {
            Some(total) => format!(
                "Rows {}–{} of {}",
                group_digits(first),
                group_digits(last),
                group_digits(total)
            ),
            None => format!("Rows {}–{}", group_digits(first), group_digits(last)),
        }
        .into();
        let (back_path, next_path) = (path.to_string(), path.to_string());
        let can_back = view.offset > 0 && !view.loading;
        let can_next = view.has_more && !view.loading;
        Some(
            div()
                .flex_none()
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .border_t_1()
                .border_color(p.border)
                .text_size(px(12.0))
                .text_color(p.muted)
                .child(div().flex_1().child(label))
                .child(
                    sqlite_button("native-docs-sqlite-previous", "Previous", can_back, p).when(
                        can_back,
                        |button| {
                            button.on_click(cx.listener(move |this, _, _, cx| {
                                this.native_docs_sqlite_page(&back_path, false, cx)
                            }))
                        },
                    ),
                )
                .child(
                    sqlite_button("native-docs-sqlite-next", "Next", can_next, p).when(
                        can_next,
                        |button| {
                            button.on_click(cx.listener(move |this, _, _, cx| {
                                this.native_docs_sqlite_page(&next_path, true, cx)
                            }))
                        },
                    ),
                )
                .into_any_element(),
        )
    }
}

fn sqlite_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    p: &DocsPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .flex_none()
        .h(px(26.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(5.0))
        .border_1()
        .border_color(p.border_strong)
        .text_size(px(12.0))
        .text_color(if enabled { p.text } else { p.toolbar_disabled })
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |button| button.bg(hover))
        })
        .child(label)
}

fn group_digits(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (ix, c) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
