//! Selecting and copying the diff's text through GPUI-Kit's window text selection.
//!
//! The virtualized surface is one Kit participant: `DiffSurfaceHost` registers the rows' viewport
//! and hands the Kit the laid-out text of every painted cell (`DiffText`), so a drag, a double
//! click and Ctrl/Cmd+C (the Kit `Root` copy binding) go through the Kit. Endpoints are stored as
//! content keys naming a row, a side and a byte of the patch line, so a selection keeps its ends
//! when they scroll out of view and copies rows that are not drawn. Only the content cells are
//! text: the gutter numbers are `user-select: none` in React and are left out here too.
use super::super::native_modal_kit::MODAL_MONO_FONT;
use super::diff::{DiffLine, DiffLineKind};
use super::diff_wrap::{CellText, WrapMetrics};
use gpui::{
    AnyElement, AnyWindowHandle, App, BorderStyle, Bounds, ContentMask, CursorStyle, Element,
    ElementId, Font, FontWeight, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, ListState, MouseButton, MouseDownEvent, Pixels,
    Point, SharedString, StyledText, Subscription, TextLayout, TextRun, UniformListScrollHandle,
    Window, point, px, size,
};
use gpui_base::{
    TextSelectionContentKey, TextSelectionEvent, TextSelectionHandle, TextSelectionRegistration,
    TextSelectionRun, TextSelectionSnapshot,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Chrome's default `::selection` over the React diff, measured on both themes:
/// `rgb(0 65 197 / 80%)` behind white text.
fn selection_background() -> Hsla {
    gpui::Rgba {
        r: 0.0,
        g: 65.0 / 255.0,
        b: 197.0 / 255.0,
        a: 0.8,
    }
    .into()
}

/// One text cell of the diff: a visible row and, in split view, its side (0 left, 1 right).
/// The derived order is document order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DiffCell {
    pub(crate) row: usize,
    pub(crate) side: u8,
}

impl DiffCell {
    fn document_order(self) -> u64 {
        self.row as u64 * 2 + u64::from(self.side)
    }
}

/// A position in the patch: a cell and a byte of the line it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DiffPoint {
    cell: DiffCell,
    offset: usize,
}

const OFFSET_BITS: u32 = 31;
const ROW_SHIFT: u32 = 32;
const ROW_MASK: u64 = (1 << 24) - 1;
const GENERATION_SHIFT: u32 = 56;

/// Packs a point with the rows' generation, so a key taken before the patch or the view mode
/// changed resolves to nothing.
fn encode(generation: u8, point: DiffPoint) -> TextSelectionContentKey {
    TextSelectionContentKey::new(
        (u64::from(generation) << GENERATION_SHIFT)
            | ((point.cell.row as u64 & ROW_MASK) << ROW_SHIFT)
            | (u64::from(point.cell.side & 1) << OFFSET_BITS)
            | (point.offset as u64 & ((1 << OFFSET_BITS) - 1)),
    )
}

fn decode(key: TextSelectionContentKey, generation: u8) -> Option<DiffPoint> {
    let value = key.value();
    ((value >> GENERATION_SHIFT) as u8 == generation).then_some(DiffPoint {
        cell: DiffCell {
            row: ((value >> ROW_SHIFT) & ROW_MASK) as usize,
            side: ((value >> OFFSET_BITS) & 1) as u8,
        },
        offset: (value & ((1 << OFFSET_BITS) - 1)) as usize,
    })
}

/// What is selected: a stretch between two points, or one whole cell (a triple click on a line
/// that wraps, which Chrome selects as a paragraph).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffSelection {
    Range(DiffPoint, DiffPoint),
    Cell(DiffCell),
}

impl DiffSelection {
    /// The selected bytes of `cell`'s `len`-byte line and whether the selection runs on past its
    /// end (Chrome then highlights the line break too).
    pub(crate) fn in_cell(&self, cell: DiffCell, len: usize) -> Option<(Range<usize>, bool)> {
        match *self {
            Self::Cell(selected) => (selected == cell).then_some((0..len, true)),
            Self::Range(start, end) => {
                if cell < start.cell || cell > end.cell {
                    return None;
                }
                let from = if cell == start.cell {
                    start.offset.min(len)
                } else {
                    0
                };
                let to = if cell == end.cell {
                    end.offset.min(len)
                } else {
                    len
                };
                let past_end = cell != end.cell;
                (from < to || past_end).then_some((from..to.max(from), past_end))
            }
        }
    }
}

/// A cell as it was painted this frame.
struct PaintedCell {
    cell: DiffCell,
    text: SharedString,
    layout: TextLayout,
    bounds: Bounds<Pixels>,
    glyph_rows: Vec<Bounds<Pixels>>,
    source_map: Rc<Vec<usize>>,
    multi_row: bool,
}

/// The selection state the view, its rows and the Kit callbacks share.
#[derive(Default)]
pub(crate) struct DiffSelectionState {
    generation: u8,
    snapshot: Option<TextSelectionSnapshot>,
    whole_cell: Option<(u8, DiffCell)>,
    lines: Rc<Vec<DiffLine>>,
    visible: Rc<Vec<usize>>,
    split: bool,
    painted: Vec<PaintedCell>,
    origin: Point<Pixels>,
    scroll: Point<Pixels>,
    row_width: Option<Pixels>,
    advance: Option<Pixels>,
}

impl DiffSelectionState {
    /// The rows the surface draws; `reset` drops the selection (a new patch, the whitespace filter
    /// or the view mode changed, which re-renders React's rows too).
    pub(crate) fn set_rows(
        &mut self,
        lines: Rc<Vec<DiffLine>>,
        visible: Rc<Vec<usize>>,
        split: bool,
        reset: bool,
    ) {
        self.lines = lines;
        self.visible = visible;
        self.split = split;
        if reset {
            self.generation = self.generation.wrapping_add(1);
            self.whole_cell = None;
        }
    }

    pub(crate) fn selection(&self) -> Option<DiffSelection> {
        if let Some((generation, cell)) = self.whole_cell
            && generation == self.generation
        {
            return Some(DiffSelection::Cell(cell));
        }
        let snapshot = self.snapshot?;
        let anchor = decode(snapshot.anchor().content_key()?, self.generation)?;
        let cursor = decode(snapshot.cursor().content_key()?, self.generation)?;
        Some(DiffSelection::Range(anchor.min(cursor), anchor.max(cursor)))
    }

    /// Where wrapped text breaks, from the width the rows had last frame: a unified cell (or a
    /// full-width hunk row) spans the row less its 56px gutter, a split half shares what is left
    /// of two gutters with the other half and gives 1px to its divider. Both have 12px padding.
    pub(crate) fn wrap_metrics(&self, half: bool) -> Option<WrapMetrics> {
        let row = f32::from(self.row_width?);
        let advance = f32::from(self.advance?);
        let width = if half {
            (row - 112.0) / 2.0 - 25.0
        } else {
            row - 56.0 - 24.0
        };
        Some(WrapMetrics {
            advance,
            wide: 11.0,
            width: width.max(advance),
        })
    }

    fn sides(&self, row: usize) -> u8 {
        let code = self
            .visible
            .get(row)
            .and_then(|line| self.lines.get(*line))
            .is_some_and(|line| line.kind.is_code());
        if self.split && code { 2 } else { 1 }
    }

    /// The patch text a cell shows: split view leaves a deletion's right half and an addition's
    /// left half empty.
    fn cell_source(&self, cell: DiffCell) -> Option<&str> {
        let line = self.lines.get(*self.visible.get(cell.row)?)?;
        if self.split && line.kind.is_code() {
            let shown = matches!(
                (cell.side, line.kind),
                (0, DiffLineKind::Deletion | DiffLineKind::Context)
                    | (1, DiffLineKind::Addition | DiffLineKind::Context)
            );
            return Some(if shown { &line.content } else { "" });
        }
        (cell.side == 0).then_some(line.content.as_str())
    }

    /// The selection as the React DOM copies it: each selected cell's text (with its `+`/`-`
    /// marker, which React renders as selectable text) on its own line, no gutter numbers.
    fn copy_text(&self) -> String {
        let Some(selection) = self.selection() else {
            return String::new();
        };
        let (first, last) = match selection {
            DiffSelection::Cell(cell) => (cell.row, cell.row),
            DiffSelection::Range(start, end) => (start.cell.row, end.cell.row),
        };
        let mut pieces = Vec::new();
        for row in first..=last.min(self.visible.len().saturating_sub(1)) {
            for side in 0..self.sides(row) {
                let cell = DiffCell { row, side };
                let Some(source) = self.cell_source(cell) else {
                    continue;
                };
                if source.is_empty() {
                    continue;
                }
                if let Some((range, _)) = selection.in_cell(cell, source.len()) {
                    pieces.push(source.get(range).unwrap_or_default());
                }
            }
        }
        pieces.join("\n")
    }

    /// The content key of the point a Kit endpoint landed on: the nearest painted row, the cell
    /// under it by x, and the character boundary nearest to it.
    fn resolve(&self, content_point: Point<Pixels>) -> Option<TextSelectionContentKey> {
        let position = content_point + self.scroll + self.origin;
        let distance = |bounds: &Bounds<Pixels>| {
            if position.y < bounds.top() {
                bounds.top() - position.y
            } else if position.y > bounds.bottom() {
                position.y - bounds.bottom()
            } else {
                px(0.0)
            }
        };
        let nearest = self.painted.iter().min_by(|a, b| {
            distance(&a.bounds)
                .partial_cmp(&distance(&b.bounds))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cell.cmp(&b.cell))
        })?;
        let mut row: Vec<&PaintedCell> = self
            .painted
            .iter()
            .filter(|cell| cell.cell.row == nearest.cell.row)
            .collect();
        row.sort_by_key(|cell| cell.cell.side);
        let painted = row
            .iter()
            .rev()
            .find(|cell| cell.bounds.left() <= position.x)
            .or(row.first())
            .copied()?;
        let display = closest_index(&painted.layout, painted.bounds, position);
        let offset = painted
            .source_map
            .get(display)
            .or(painted.source_map.last())
            .copied()
            .unwrap_or(0);
        Some(encode(
            self.generation,
            DiffPoint {
                cell: painted.cell,
                offset,
            },
        ))
    }

    fn wrapped_cell_at(&self, position: Point<Pixels>) -> Option<DiffCell> {
        self.painted
            .iter()
            .find(|cell| cell.multi_row && cell.bounds.contains(&position))
            .map(|cell| cell.cell)
    }
}

/// The character boundary nearest to `position` in a laid-out cell; above it is its start,
/// below it its end.
fn closest_index(layout: &TextLayout, bounds: Bounds<Pixels>, position: Point<Pixels>) -> usize {
    let lines = layout.line_layouts();
    let line_height = layout.line_height();
    let y = position.y - bounds.top() + px(0.01);
    if lines.is_empty() || y < px(0.0) {
        return 0;
    }
    let line_ix = (y / line_height).floor() as usize;
    if line_ix >= lines.len() {
        return layout.len();
    }
    let start: usize = lines[..line_ix].iter().map(|line| line.len() + 1).sum();
    let local = point(position.x - bounds.left(), y - line_height * line_ix as f32);
    let (Ok(index) | Err(index)) = lines[line_ix].closest_index_for_position(local, line_height);
    start + index
}

/// Hooks the Kit participant up to the shared state: copy, clear, content keys, and a redraw of
/// `window` whenever the selection changes.
pub(crate) fn connect_selection(
    selection: &TextSelectionHandle,
    state: &Rc<RefCell<DiffSelectionState>>,
    window: &Window,
    cx: &mut App,
) -> Subscription {
    let copy_state = state.clone();
    selection.copy_with(move |_| copy_state.borrow().copy_text(), cx);
    let clear_state = state.clone();
    selection.clear_with(move |_| clear_state.borrow_mut().whole_cell = None, cx);
    let key_state = state.clone();
    selection.resolve_content_key_with(move |point, _| key_state.borrow().resolve(point), cx);
    let window: AnyWindowHandle = window.window_handle();
    let state = state.clone();
    selection.subscribe(
        move |event, cx| {
            if let TextSelectionEvent::SelectionChanged(snapshot) = event {
                state.borrow_mut().snapshot = *snapshot;
                let _ = window.update(cx, |_, window, _| window.refresh());
            }
        },
        cx,
    )
}

/// Merges the selection into a cell's token colours: selected text is white on Chrome's
/// selection blue, keeping its italics.
pub(crate) fn with_selection(
    tokens: Vec<(Range<usize>, HighlightStyle)>,
    selected: Range<usize>,
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let selected_style = |base: Option<&HighlightStyle>| HighlightStyle {
        color: Some(gpui::white()),
        background_color: Some(selection_background()),
        font_style: base.and_then(|style| style.font_style),
        ..HighlightStyle::default()
    };
    let mut out = Vec::with_capacity(tokens.len() + 2);
    let mut push = |range: Range<usize>, style: Option<&HighlightStyle>| {
        let before = range.start..range.end.min(selected.start);
        let inside = range.start.max(selected.start)..range.end.min(selected.end);
        let after = range.start.max(selected.end)..range.end;
        if let Some(style) = style
            && !before.is_empty()
        {
            out.push((before, *style));
        }
        if !inside.is_empty() {
            out.push((inside, selected_style(style)));
        }
        if let Some(style) = style
            && !after.is_empty()
        {
            out.push((after, *style));
        }
    };
    let mut cursor = 0;
    for (range, style) in &tokens {
        push(cursor..range.start, None);
        push(range.clone(), Some(style));
        cursor = range.end;
    }
    push(cursor..len, None);
    out
}

/// A cell's text: gpui's `StyledText` that reports its layout to the surface as it is painted,
/// and shows the text cursor over its glyphs.
pub(crate) struct DiffText {
    text: StyledText,
    display: SharedString,
    cell: DiffCell,
    source_map: Rc<Vec<usize>>,
    multi_row: bool,
    state: Rc<RefCell<DiffSelectionState>>,
}

impl DiffText {
    pub(crate) fn new(
        text: CellText,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        cell: DiffCell,
        state: Rc<RefCell<DiffSelectionState>>,
    ) -> Self {
        let multi_row = text.multi_row;
        let display: SharedString = text.display.clone().into();
        Self {
            text: StyledText::new(display.clone()).with_highlights(highlights),
            display,
            cell,
            source_map: Rc::new(text.into_source_map()),
            multi_row,
            state,
        }
    }
}

impl IntoElement for DiffText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DiffText {
    type RequestLayoutState = ();
    type PrepaintState = Vec<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, request_layout, window, cx);
        let layout = self.text.layout().clone();
        let line_height = layout.line_height();
        let mut glyph_rows = Vec::new();
        let mut top = bounds.top();
        for line in layout.line_layouts() {
            let width = line.width();
            if width > px(0.0) {
                glyph_rows.push(Bounds::new(
                    point(bounds.left(), top),
                    size(width, line_height),
                ));
            }
            top += line_height;
        }
        let hitboxes = glyph_rows
            .iter()
            .map(|row| window.insert_hitbox(*row, HitboxBehavior::Normal))
            .collect();
        self.state.borrow_mut().painted.push(PaintedCell {
            cell: self.cell,
            text: self.display.clone(),
            layout,
            bounds,
            glyph_rows,
            source_map: self.source_map.clone(),
            multi_row: self.multi_row,
        });
        hitboxes
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        hitboxes: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // `.git-file-diff-line-content` is text; the gutter numbers are not.
        for hitbox in hitboxes.iter() {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }
        self.text.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            &mut (),
            window,
            cx,
        );
    }
}

/// How the rows scroll: a uniform list while lines do not wrap, a measured list while they do.
#[derive(Clone)]
pub(crate) enum DiffScroll {
    Rows(UniformListScrollHandle),
    Wrapped(ListState),
}

impl DiffScroll {
    fn offset(&self) -> Point<Pixels> {
        match self {
            Self::Rows(handle) => handle.0.borrow().base_handle.offset(),
            Self::Wrapped(list) => list.scroll_px_offset_for_scrollbar(),
        }
    }

    /// The surface box as it scrolls: at least the viewport, as wide and tall as the rows.
    fn content_bounds(&self, viewport: Bounds<Pixels>) -> Bounds<Pixels> {
        let max = match self {
            Self::Rows(handle) => handle.0.borrow().base_handle.max_offset(),
            Self::Wrapped(list) => list.max_offset_for_scrollbar(),
        };
        Bounds::new(
            viewport.origin + self.offset(),
            size(
                viewport.size.width + max.x.max(px(0.0)),
                viewport.size.height + max.y.max(px(0.0)),
            ),
        )
    }
}

/// `.git-file-diff-surface` around its rows: registers the rows with the Kit selection, and paints
/// the surface's 1px border and `inset 0 1px 0` highlight on the scrolled box, so they scroll away
/// with the rows the way the React surface does inside its scroll container.
pub(crate) struct DiffSurfaceHost {
    pub(crate) rows: AnyElement,
    pub(crate) scroll: DiffScroll,
    pub(crate) selection: TextSelectionHandle,
    pub(crate) state: Rc<RefCell<DiffSelectionState>>,
    pub(crate) border: Hsla,
    pub(crate) highlight: Hsla,
    pub(crate) weight: FontWeight,
    /// The rows' padding, where the border is drawn: one device pixel.
    pub(crate) inset: Pixels,
}

impl IntoElement for DiffSurfaceHost {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The monospace advance at the diff's 11px, measured on a run of zeros as gpui shapes it.
fn measure_advance(weight: FontWeight, window: &Window) -> Pixels {
    const SAMPLE: usize = 32;
    let font = Font {
        weight,
        ..gpui::font(MODAL_MONO_FONT)
    };
    let run = TextRun {
        len: SAMPLE,
        font,
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line("0".repeat(SAMPLE).into(), px(11.0), &[run], None);
    line.width / SAMPLE as f32
}

impl Element for DiffSurfaceHost {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Bounds<Pixels>);

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.rows.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.state.borrow_mut().painted.clear();
        self.rows.prepaint(window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let scroll = self.scroll.offset();
        let row_width = bounds.size.width - self.inset * 2.0;
        let (runs, text_bounds, width_changed) = {
            let mut state = self.state.borrow_mut();
            state.origin = bounds.origin;
            state.scroll = scroll;
            if state.advance.is_none() {
                state.advance = Some(measure_advance(self.weight, window));
            }
            let width_changed = state.row_width != Some(row_width);
            state.row_width = Some(row_width);
            let runs: Vec<TextSelectionRun> = state
                .painted
                .iter()
                .map(|cell| {
                    TextSelectionRun::new(cell.text.clone(), cell.layout.clone(), cell.bounds)
                        .with_document_order(cell.cell.document_order())
                })
                .collect();
            let text_bounds: Vec<Bounds<Pixels>> = state
                .painted
                .iter()
                .flat_map(|cell| cell.glyph_rows.iter().copied())
                .collect();
            (runs, text_bounds, width_changed)
        };
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_scroll_offset(scroll)
            .with_text_bounds(text_bounds)
            .with_rendered_element(&self.selection, window, cx);
        self.selection.register(registration, window, cx);
        self.selection.update_runs(&runs, cx);
        // Wrapped rows are broken for last frame's width; a new width breaks them again.
        if width_changed && let DiffScroll::Wrapped(list) = &self.scroll {
            let list = list.clone();
            window.defer(cx, move |window, _| {
                list.remeasure();
                window.refresh();
            });
        }
        (hitbox, self.scroll.content_bounds(bounds))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        (hitbox, content): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let content = *content;
        let mask = Some(ContentMask { bounds });
        // `box-shadow: inset 0 1px 0 rgb(255 255 255 / 4%)` sits under the rows, inside the border.
        window.with_content_mask(mask, |window| {
            let inset = self.inset;
            window.paint_quad(gpui::fill(
                Bounds::new(
                    content.origin + point(inset, inset),
                    size(content.size.width - inset * 2.0, inset),
                ),
                self.highlight,
            ));
        });
        self.rows.paint(window, cx);
        window.with_content_mask(mask, |window| {
            window.paint_quad(gpui::quad(
                content,
                px(0.0),
                gpui::transparent_black(),
                self.inset,
                self.border,
                BorderStyle::Solid,
            ));
        });
        // Chrome selects a wrapped line's whole paragraph on a triple click; the Kit's line
        // selection would stop at the soft break this surface inserts.
        let state = self.state.clone();
        let selection = self.selection.clone();
        let hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble()
                || event.button != MouseButton::Left
                || event.click_count < 3
                || !hitbox.is_hovered(window)
            {
                return;
            }
            let cell = state.borrow().wrapped_cell_at(event.position);
            let Some(cell) = cell else {
                return;
            };
            {
                let mut state = state.borrow_mut();
                let generation = state.generation;
                state.whole_cell = Some((generation, cell));
            }
            selection.set_local_selection(true, cx);
            cx.stop_propagation();
            window.refresh();
        });
    }
}
