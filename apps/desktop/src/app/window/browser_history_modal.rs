//! The native Browser History window: a search line with the All / Current Project scope at its
//! right edge, visits grouped by day, and a footer with what Return does. It is drawn in Quick
//! Access's language and palette (frosted under window glass) and reads the visit store directly.
//!
//! CDXC:Browser 2026-09-09 DECISION:
//! User: Browser history uses the Sessions list look and the same borderless GPUI popup, opens on All Projects, and offers Current Project; cards show the favicon and page title with the URL on a second line and full details on hover. When the page title equals its URL, show that text once.
//! Moved here from the React modal with the port to native GPUI (2026-09-27); Quick Access's palette and search line give it the Sessions look.
//! SEE-ALSO: apps/desktop/src/browser_history.rs (the visit store),
//! apps/desktop/src/app/browser_history_modal_lifecycle.rs (open, open a visit, close).
use super::quick_access::chrome::{
    quick_access_keycap, quick_access_search_bar, quick_access_tooltip,
};
use super::quick_access::palette::{
    QUICK_ACCESS_FILTER_HEIGHT, QUICK_ACCESS_FOOTER_HEIGHT, QUICK_ACCESS_GROUP_HEADING_HEIGHT,
    QUICK_ACCESS_ITEM_FONT_SIZE, QUICK_ACCESS_LIST_PADDING, QUICK_ACCESS_META_FONT_SIZE,
    QUICK_ACCESS_ROW_FONT_SIZE, QUICK_ACCESS_ROW_PADDING_X, QUICK_ACCESS_ROW_RADIUS,
    QuickAccessPalette, hsla,
};
use crate::app::consts::{BROWSER_ICON_HISTORY, BROWSER_ICON_WORLD};
use crate::app::model::BrowserFaviconFetchSource;
use crate::app::window::native_modal_kit::{MODAL_UI_FONT, neutral_button_fills, rgba_of};
use crate::browser_history::HistoryEntry;
use chrono::TimeZone as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    Rgba, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, img, px, svg,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::time::Duration;

/// A visit's two lines of text and its metadata column.
const BROWSER_HISTORY_ROW_MIN_HEIGHT: f32 = 52.0;
/// Typing settles before the store is asked again, as the React list did.
const BROWSER_HISTORY_QUERY_DEBOUNCE: Duration = Duration::from_millis(150);

pub(crate) enum BrowserHistoryModalCommand {
    Open { id: i64 },
    Close,
}

pub(crate) type BrowserHistoryModalHost = Rc<dyn Fn(BrowserHistoryModalCommand, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BrowserHistoryScope {
    All,
    Current,
}

pub(crate) struct GpuiBrowserHistoryModalWindow {
    host: BrowserHistoryModalHost,
    /// The project whose visits Current Project lists: the one showing in Browser view at open.
    current_project_id: String,
    search: Entity<InputState>,
    query: String,
    scope: BrowserHistoryScope,
    entries: Vec<HistoryEntry>,
    has_more: bool,
    loading: bool,
    error: Option<String>,
    selected: usize,
    /// Each request's number; a result that is not the newest request's is dropped.
    request_generation: u64,
    scroll: ScrollHandle,
    pending_scroll: Option<usize>,
    /// Window glass was on when the window opened, so its window blurs what is behind it
    /// (`open_native_app_modal`) and the palette is frosted to match.
    glass: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiBrowserHistoryModalWindow {
    pub(crate) fn new(
        host: BrowserHistoryModalHost,
        current_project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search titles, URLs, and projects...")
        });
        let change = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if value != this.query {
                        this.query = value;
                        this.request(false, cx);
                    }
                }
            },
        );
        search.update(cx, |input, cx| input.focus(window, cx));
        // Relative times ("5m ago") keep moving while the window stays open.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut this = Self {
            host,
            current_project_id,
            search,
            query: String::new(),
            scope: BrowserHistoryScope::All,
            entries: Vec::new(),
            has_more: false,
            loading: true,
            error: None,
            selected: 0,
            request_generation: 0,
            scroll: ScrollHandle::new(),
            pending_scroll: None,
            glass: crate::app::helpers::window_glass_active(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![change],
        };
        this._subscriptions
            .extend(super::popup_dismissal::close_app_modal_on_click_away(
                window, cx,
            ));
        this.request(false, cx);
        this
    }

    /// A failed open (the visit is gone, the store could not be read) stays in the window.
    pub(crate) fn show_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }

    fn post(&self, command: BrowserHistoryModalCommand, cx: &mut App) {
        (self.host)(command, cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.post(BrowserHistoryModalCommand::Close, cx);
    }

    /// Asks the store for the first page (a new query or scope) or the next one (Load more).
    fn request(&mut self, append: bool, cx: &mut Context<Self>) {
        self.request_generation += 1;
        let generation = self.request_generation;
        let project_id =
            (self.scope == BrowserHistoryScope::Current).then(|| self.current_project_id.clone());
        let query: String = self.query.trim().chars().take(1024).collect();
        let before = if append {
            self.entries
                .last()
                .map(|entry| (entry.visited_at, entry.id))
        } else {
            self.entries.clear();
            self.has_more = false;
            self.selected = 0;
            self.pending_scroll = Some(0);
            None
        };
        self.loading = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if !append {
                cx.background_executor()
                    .timer(BROWSER_HISTORY_QUERY_DEBOUNCE)
                    .await;
                let current = this
                    .read_with(cx, |this, _| this.request_generation == generation)
                    .unwrap_or(false);
                if !current {
                    return;
                }
            }
            let result = crate::browser_history::query(project_id, query, before).await;
            let _ = this.update(cx, |this, cx| {
                if this.request_generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(page) => {
                        this.has_more = page.has_more;
                        this.entries.extend(page.entries);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(id) = self.entries.get(index).map(|entry| entry.id) else {
            return;
        };
        self.error = None;
        self.post(BrowserHistoryModalCommand::Open { id }, cx);
        cx.notify();
    }

    fn set_scope(&mut self, scope: BrowserHistoryScope, cx: &mut Context<Self>) {
        if self.scope != scope {
            self.scope = scope;
            self.request(false, cx);
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, last) as usize;
        if next != self.selected {
            self.selected = next;
            self.pending_scroll = Some(self.flat_index(next));
            cx.notify();
        }
    }

    /// The list child that shows entry `index`, counting the day headings above it.
    fn flat_index(&self, index: usize) -> usize {
        let mut headings = 0;
        let mut previous: Option<String> = None;
        for entry in self.entries.iter().take(index + 1) {
            let label = day_label(entry.visited_at);
            if previous.as_deref() != Some(label.as_str()) {
                headings += 1;
                previous = Some(label);
            }
        }
        index + headings
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                self.close(cx);
            }
            "down" => {
                cx.stop_propagation();
                self.move_selection(1, cx);
            }
            "up" => {
                cx.stop_propagation();
                self.move_selection(-1, cx);
            }
            "enter"
                if !modifiers.alt
                    && !modifiers.control
                    && !modifiers.shift
                    && !modifiers.platform =>
            {
                if self.selected < self.entries.len() {
                    cx.stop_propagation();
                    self.open_entry(self.selected, cx);
                }
            }
            _ => {
                // Everything else is search text, wherever the focus was.
                let search = self.search.clone();
                if !search.read(cx).focus_handle(cx).is_focused(window) {
                    search.update(cx, |input, cx| input.focus(window, cx));
                }
            }
        }
    }
}

impl Render for GpuiBrowserHistoryModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = QuickAccessPalette::current(self.glass);
        if let Some(index) = self.pending_scroll.take() {
            self.scroll.scroll_to_item(index);
        }
        let scope = self.render_scope(&p, cx);
        div()
            .id("browser-history-window")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.window))
            .font_family(MODAL_UI_FONT)
            .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
            .line_height(px(18.0))
            .text_color(hsla(p.item))
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(
                v_flex()
                    .size_full()
                    .min_h_0()
                    .child(quick_access_search_bar(
                        &p,
                        &self.search,
                        !self.query.is_empty(),
                        vec![scope],
                        |this: &mut Self, window, cx| {
                            this.search.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                                input.focus(window, cx);
                            });
                            if !this.query.is_empty() {
                                this.query.clear();
                                this.request(false, cx);
                            }
                        },
                        cx,
                    ))
                    .child(self.render_list(&p, cx))
                    .child(self.render_footer(&p, cx)),
            )
    }
}

impl GpuiBrowserHistoryModalWindow {
    /// All Projects / Current Project, as two pills in the search line's filter slot.
    fn render_scope(&self, p: &QuickAccessPalette, cx: &mut Context<Self>) -> AnyElement {
        let p = *p;
        let pill = |id: &'static str, label: &'static str, scope: BrowserHistoryScope| {
            let active = self.scope == scope;
            div()
                .id(id)
                .h_full()
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .whitespace_nowrap()
                .cursor_pointer()
                .text_color(hsla(if active { p.foreground } else { p.muted }))
                .when(active, |this| this.bg(hsla(p.footer_active)))
                .when(!active, |this| {
                    this.hover(move |this| this.text_color(hsla(p.foreground)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.set_scope(scope, cx);
                }))
                .child(label)
        };
        h_flex()
            .flex_shrink_0()
            .h(px(QUICK_ACCESS_FILTER_HEIGHT))
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .text_size(px(12.5))
            .line_height(px(18.0))
            .child(pill(
                "browser-history-scope-all",
                "All Projects",
                BrowserHistoryScope::All,
            ))
            .child(pill(
                "browser-history-scope-current",
                "Current Project",
                BrowserHistoryScope::Current,
            ))
            .into_any_element()
    }

    fn render_list(&self, p: &QuickAccessPalette, cx: &mut Context<Self>) -> AnyElement {
        let p = *p;
        let now = chrono::Utc::now().timestamp_millis();
        let show_project = self.scope == BrowserHistoryScope::All;
        let mut children: Vec<AnyElement> = Vec::new();
        let mut previous_label: Option<String> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            let label = day_label(entry.visited_at);
            if previous_label.as_deref() != Some(label.as_str()) {
                children.push(
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .min_h(px(QUICK_ACCESS_GROUP_HEADING_HEIGHT))
                        .px(px(QUICK_ACCESS_ROW_PADDING_X))
                        .pt(px(if previous_label.is_some() { 10.0 } else { 5.0 }))
                        .pb(px(5.0))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(SharedString::from(label.clone()))
                        .into_any_element(),
                );
                previous_label = Some(label);
            }
            children.push(self.render_row(&p, index, entry, show_project, now, cx));
        }
        let status = if let Some(error) = self.error.as_ref() {
            Some((error.clone(), p.destructive))
        } else if self.loading {
            Some(("Loading history...".to_string(), p.muted))
        } else if self.entries.is_empty() {
            Some((
                if !self.query.trim().is_empty() {
                    "No matching history."
                } else if self.scope == BrowserHistoryScope::Current {
                    "No browser history for this project yet."
                } else {
                    "No browser history yet. Pages you visit will appear here."
                }
                .to_string(),
                p.muted,
            ))
        } else {
            None
        };
        if let Some((text, color)) = status {
            children.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .py(px(18.0))
                    .px(px(12.0))
                    .text_center()
                    .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                    .text_color(hsla(color))
                    .child(SharedString::from(text))
                    .into_any_element(),
            );
        }
        if self.has_more && !self.loading {
            // A neutral filled button (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
            let fills = neutral_button_fills(p.light);
            children.push(
                div()
                    .id("browser-history-load-more")
                    .flex_shrink_0()
                    .mx_auto()
                    .my(px(8.0))
                    .h(px(28.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(hsla(rgba_of(p.hairline, 0.0)))
                    .bg(hsla(fills.rest))
                    .text_size(px(12.5))
                    .text_color(hsla(p.item))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(fills.hover)))
                    .active(move |this| this.bg(hsla(fills.pressed)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.request(true, cx);
                    }))
                    .child("Load more")
                    .into_any_element(),
            );
        }
        v_flex()
            .id("browser-history-list")
            .flex_1()
            .min_h_0()
            .w_full()
            .p(px(QUICK_ACCESS_LIST_PADDING))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(children)
            .into_any_element()
    }

    /// One visit: favicon, the title with its URL on a second line (once when they are equal),
    /// the project and time, and every detail in the tooltip.
    fn render_row(
        &self,
        p: &QuickAccessPalette,
        index: usize,
        entry: &HistoryEntry,
        show_project: bool,
        now_ms: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let title = entry.title.trim();
        let url = entry.url.trim();
        let heading = if title.is_empty() { url } else { title };
        let second_line = (!title.is_empty() && title != url).then(|| url.to_string());
        let mut tooltip = vec![heading.to_string()];
        tooltip.extend(second_line.clone());
        if !entry.project_name.is_empty() {
            tooltip.push(entry.project_name.clone());
        }
        if entry.visited_at > 0 {
            tooltip.push(full_date_time(entry.visited_at));
        }
        let tooltip = tooltip.join("\n");
        let selected = index == self.selected;
        h_flex()
            .id(("browser-history-row", index))
            .w_full()
            .flex_shrink_0()
            .min_h(px(BROWSER_HISTORY_ROW_MIN_HEIGHT))
            .px(px(QUICK_ACCESS_ROW_PADDING_X))
            .py(px(7.0))
            .gap(px(11.0))
            .items_center()
            .rounded(px(QUICK_ACCESS_ROW_RADIUS))
            .cursor_default()
            .when(selected, |this| this.bg(hsla(p.row_selected)))
            .on_mouse_move(cx.listener(move |this, _, _window, cx| {
                if this.selected != index {
                    this.selected = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_entry(index, cx);
            }))
            .tooltip(move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(history_favicon(entry.favicon_url.as_deref(), p.muted)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(QUICK_ACCESS_ROW_FONT_SIZE))
                            .line_height(px(20.0))
                            .text_color(hsla(p.foreground))
                            .child(SharedString::from(heading.to_string())),
                    )
                    .children(second_line.map(|url| {
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(QUICK_ACCESS_META_FONT_SIZE))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .child(SharedString::from(url))
                    })),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .max_w(px(220.0))
                    .gap(px(10.0))
                    .items_center()
                    .text_size(px(QUICK_ACCESS_META_FONT_SIZE))
                    .line_height(px(20.0))
                    .text_color(hsla(p.muted))
                    .children((show_project && !entry.project_name.is_empty()).then(|| {
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(entry.project_name.clone()))
                    }))
                    .children((entry.visited_at > 0).then(|| {
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .child(SharedString::from(relative_time_label(
                                entry.visited_at,
                                now_ms,
                            )))
                    })),
            )
            .into_any_element()
    }

    fn render_footer(&self, p: &QuickAccessPalette, cx: &mut Context<Self>) -> AnyElement {
        let p = *p;
        let can_open = self.selected < self.entries.len();
        h_flex()
            .id("browser-history-footer")
            .flex_shrink_0()
            .w_full()
            .h(px(QUICK_ACCESS_FOOTER_HEIGHT))
            .px(px(14.0))
            .gap(px(8.0))
            .items_center()
            .border_t_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.footer))
            .child(
                svg()
                    .path(BROWSER_ICON_HISTORY)
                    .size(px(15.0))
                    .flex_shrink_0()
                    .text_color(hsla(p.muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .text_color(hsla(p.muted))
                    .child("Browser History"),
            )
            .children(can_open.then(|| {
                h_flex()
                    .id("browser-history-open")
                    .flex_shrink_0()
                    .h(px(28.0))
                    .pl(px(9.0))
                    .pr(px(6.0))
                    .gap(px(7.0))
                    .items_center()
                    .rounded(px(7.0))
                    .text_size(px(12.5))
                    .text_color(hsla(p.foreground))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.raised)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.open_entry(this.selected, cx);
                    }))
                    .child("Open in New Tab")
                    .child(quick_access_keycap(&p, "↵"))
            }))
            .into_any_element()
    }
}

impl Focusable for GpuiBrowserHistoryModalWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The page's icon: a stored `data:` image, a fetched `http(s)` icon, else the globe.
fn history_favicon(url: Option<&str>, color: Rgba) -> AnyElement {
    let globe = || {
        svg()
            .path(BROWSER_ICON_WORLD)
            .size(px(16.0))
            .text_color(hsla(color))
            .into_any_element()
    };
    match url.map(str::trim) {
        Some(url) if url.starts_with("data:") => {
            match crate::app::native_sidebar::images::sidebar_image(url) {
                Some(image) => img(image).size(px(16.0)).into_any_element(),
                None => globe(),
            }
        }
        Some(url) if url.starts_with("https://") || url.starts_with("http://") => {
            crate::app::helpers::browser_favicon_element(
                16.0,
                None,
                Some(&BrowserFaviconFetchSource {
                    url: url.to_string(),
                    cache_key: None,
                }),
            )
        }
        _ => globe(),
    }
}

/// The day heading, in local time ("Saturday, September 26, 2026"); pages imported from tab
/// history have no visit time.
fn day_label(visited_at_ms: i64) -> String {
    if visited_at_ms <= 0 {
        return "Earlier tab history".to_string();
    }
    chrono::Local
        .timestamp_millis_opt(visited_at_ms)
        .single()
        .map(|time| time.format("%A, %B %-d, %Y").to_string())
        .unwrap_or_else(|| "Earlier tab history".to_string())
}

fn full_date_time(visited_at_ms: i64) -> String {
    chrono::Local
        .timestamp_millis_opt(visited_at_ms)
        .single()
        .map(|time| time.format("%-m/%-d/%Y, %-I:%M:%S %p").to_string())
        .unwrap_or_default()
}

/// The sidebar's compact relative time (`formatRelativeTimeLabel`): "just now", "42s ago", "5m ago".
fn relative_time_label(visited_at_ms: i64, now_ms: i64) -> String {
    let seconds = (now_ms - visited_at_ms).max(0) / 1000;
    if seconds < 5 {
        return "just now".to_string();
    }
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

impl crate::app::window::native_modal_kit::ModalCornerClose for GpuiBrowserHistoryModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
    }
}
