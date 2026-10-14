//! Drag-to-reorder lists and grids: the native twin of the dnd-kit `useSortable` sortables of
//! Settings (Agents, Actions, Sidebar Tags, the session hover buttons, Your views cards, Arrange
//! views). As @dnd-kit/dom 0.3 draws them (default feedback, no overlay):
//! - the grip keeps its own cursor until a drag starts, then the cursor is `grabbing`;
//! - the dragged item itself lifts out and follows the pointer on both axes (at the list's
//!   `data-dragging` opacity: 1 for rows, 0.55 for the hover buttons, 0.6 for view cards, 0.5 in
//!   Arrange views), its slot stays as an invisible placeholder;
//! - the placeholder moves to the item under the pointer (`pointerIntersection`, else the one the
//!   lifted item overlaps most, `shapeIntersection`), in a column, a row or a wrapping grid, and the
//!   other items slide to their new places over 250ms `cubic-bezier(0.25, 1, 0.5, 1)`;
//! - releasing commits the live order (`moveId(ids, from, to)`) and the item glides into its slot
//!   over 250ms `ease`; Escape cancels and the item glides back;
//! - a list with a scroll container (`ReorderOptions::scroll`) auto-scrolls while the pointer is
//!   within 20% of its top or bottom edge, up to 25px a frame (`AutoScroller`).
//!
//! Usage: `let order = reorder_order(page, "agents", items.len(), cx)` gives the display order of
//! the item indexes; wrap each item in `reorder_row(page, list, index, slot, item, on_move, cx)`
//! and its grip in `reorder_handle(..)`; `reorder_options` sets the list's look and gesture.
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, Context, CursorStyle, DragMoveEvent,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window,
    anchored, deferred, div, point, px,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// `defaultSortableTransition.duration` and the Feedback plugin's drop `DEFAULT_DURATION`.
const ANIMATION: Duration = Duration::from_millis(250);
/// One animation frame of the ticker that drives slides, the drop and auto-scroll.
const FRAME: Duration = Duration::from_millis(16);
/// `detectScrollIntent`: the edge zone (share of the container height) and the top speed.
const SCROLL_THRESHOLD: f32 = 0.2;
const SCROLL_ACCELERATION: f32 = 25.0;
/// GPUI starts a drag after the pointer moves this far (`DRAG_THRESHOLD`).
const GPUI_DRAG_THRESHOLD: f32 = 2.0;
/// `SIDEBAR_REORDER_HOLD_DELAY_MS`: a held press activates without the distance.
const HOLD_ACTIVATION: Duration = Duration::from_millis(250);

/// How a list looks and starts while dragged.
#[derive(Clone)]
pub(crate) struct ReorderOptions {
    /// The lifted item's `[data-dragging='true']` opacity.
    pub(crate) lifted_opacity: f32,
    /// `PointerActivationConstraints.Distance` (0: GPUI's own threshold); a press held for 250ms
    /// activates without it, as `getSidebarReorderActivationConstraints` does.
    pub(crate) activation_distance: f32,
    /// The scroll container the list auto-scrolls.
    pub(crate) scroll: Option<ScrollHandle>,
    /// Each item fills its slot's width (rows and grid cells); off for a strip of buttons.
    pub(crate) fill: bool,
}

impl Default for ReorderOptions {
    fn default() -> Self {
        Self {
            lifted_opacity: 1.0,
            activation_distance: 0.0,
            scroll: None,
            fill: true,
        }
    }
}

/// The drag payload: which list and which item (its index in the saved order).
#[derive(Clone)]
pub(crate) struct ReorderDrag {
    pub(crate) list: SharedString,
    pub(crate) index: usize,
}

/// GPUI's drag preview, empty: the lifted item is drawn by the list itself.
struct ReorderDragView;

impl Render for ReorderDragView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// An item sliding from its old place to its new one.
#[derive(Clone, Copy, Debug)]
struct Slide {
    from: Point<f32>,
    start: Instant,
}

impl Slide {
    /// The item's remaining offset from its layout position (`cubic-bezier(0.25, 1, 0.5, 1)`).
    fn offset(&self, now: Instant) -> Point<f32> {
        let remaining = 1.0 - cubic_bezier(0.25, 1.0, 0.5, 1.0, progress(self.start, now));
        point(self.from.x * remaining, self.from.y * remaining)
    }
}

/// The dropped (or cancelled) item gliding from where it was released into its slot (`ease`).
#[derive(Clone, Copy, Debug)]
struct Dropping {
    index: usize,
    from: Point<Pixels>,
    start: Instant,
}

/// A list's drag state.
#[derive(Default)]
pub(crate) struct ReorderState {
    /// `(from, over)` while an item is lifted: its saved index and its live slot.
    pub(crate) dragging: Option<(usize, usize)>,
    /// A GPUI drag that has not met the activation constraint yet: the item and where and when
    /// it started.
    pending: Option<(usize, Point<Pixels>, Instant)>,
    /// The pointer, and where on the item it grabbed (pointer minus the item's origin).
    pointer: Point<Pixels>,
    grab: Point<Pixels>,
    /// The items this list rendered last frame.
    len: usize,
    /// Each item's slot (its unshifted layout box) last frame, by saved index.
    bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    slides: HashMap<usize, Slide>,
    dropping: Option<Dropping>,
    options: ReorderOptions,
    /// When a drag last ended on this list (the click a release fires is not a toggle).
    just_dragged: Option<Instant>,
    /// Escape cancels while an item is lifted.
    escape: Option<Subscription>,
    /// The frame ticker (its handle is kept only so it is not dropped while it runs).
    ticking: bool,
    ticker: Option<Task<()>>,
}

impl ReorderState {
    fn animating(&self, now: Instant) -> bool {
        self.dragging.is_some()
            || self.pending.is_some()
            || self.dropping.is_some()
            || self
                .slides
                .values()
                .any(|slide| now.duration_since(slide.start) < ANIMATION)
    }

    fn bounds_of(&self, index: usize) -> Option<Bounds<Pixels>> {
        self.bounds.borrow().get(&index).copied()
    }

    fn live_order(&self) -> Vec<usize> {
        let indexes: Vec<usize> = (0..self.len).collect();
        match self.dragging {
            Some((from, over)) if from < self.len && over < self.len => {
                move_index(&indexes, from, over)
            }
            _ => indexes,
        }
    }

    /// The live position the placeholder moves to: the item under the pointer, else the item the
    /// lifted one overlaps most (the placeholder itself included, which keeps it in place).
    fn target_position(&self) -> Option<usize> {
        let (from, _) = self.dragging?;
        let order = self.live_order();
        let dragged = self.bounds_of(from)?;
        let pointer = self.pointer;
        let distance = |bounds: &Bounds<Pixels>| {
            let center = bounds.center();
            (f32::from(center.x - pointer.x)).hypot(f32::from(center.y - pointer.y))
        };
        let mut best: Option<(usize, f32)> = None;
        for (position, index) in order.iter().enumerate() {
            let Some(bounds) = self.bounds_of(*index) else {
                continue;
            };
            if bounds.contains(&pointer) {
                let value = 1.0 / distance(&bounds).max(0.001);
                if best.is_none_or(|(_, current)| value > current) {
                    best = Some((position, value));
                }
            }
        }
        if best.is_some() {
            return best.map(|(position, _)| position);
        }
        let lifted = Bounds::new(pointer - self.grab, dragged.size);
        let area =
            |bounds: &Bounds<Pixels>| f32::from(bounds.size.width) * f32::from(bounds.size.height);
        for (position, index) in order.iter().enumerate() {
            let Some(bounds) = self.bounds_of(*index) else {
                continue;
            };
            let overlap = lifted.intersect(&bounds);
            if f32::from(overlap.size.width) <= 0.0 || f32::from(overlap.size.height) <= 0.0 {
                continue;
            }
            let shared = area(&overlap);
            let ratio = shared / (area(&lifted) + area(&bounds) - shared);
            let value = ratio / distance(&bounds).max(0.001);
            if best.is_none_or(|(_, current)| value > current) {
                best = Some((position, value));
            }
        }
        best.map(|(position, _)| position)
    }

    /// Moves the placeholder to the target and starts the other items' slides to their new
    /// places.
    fn update_over(&mut self, now: Instant) {
        let Some((from, over)) = self.dragging else {
            return;
        };
        let Some(target) = self.target_position() else {
            return;
        };
        if target == over {
            return;
        }
        let old_order = self.live_order();
        self.dragging = Some((from, target));
        let new_order = self.live_order();
        let Some(origins) = self.predict_origins(&old_order, &new_order) else {
            return;
        };
        for index in new_order.iter().copied().filter(|index| *index != from) {
            let (Some(bounds), Some(next)) = (self.bounds_of(index), origins.get(&index)) else {
                continue;
            };
            let current = self
                .slides
                .get(&index)
                .map(|slide| slide.offset(now))
                .unwrap_or_default();
            let from_offset = point(
                f32::from(bounds.origin.x) + current.x - next.x,
                f32::from(bounds.origin.y) + current.y - next.y,
            );
            if from_offset.x.abs() > 0.5 || from_offset.y.abs() > 0.5 {
                self.slides.insert(
                    index,
                    Slide {
                        from: from_offset,
                        start: now,
                    },
                );
            }
        }
    }

    /// Where every item lands in `new_order`: equal-sized items (a row of buttons, a grid of
    /// cards, a list of equal rows) take the slot boxes of the old layout in order; a column of
    /// rows of different heights restacks from the same top with the same gaps.
    fn predict_origins(
        &self,
        old_order: &[usize],
        new_order: &[usize],
    ) -> Option<HashMap<usize, Point<f32>>> {
        let slots: Vec<Bounds<Pixels>> = old_order
            .iter()
            .map(|index| self.bounds_of(*index))
            .collect::<Option<_>>()?;
        let first = slots.first()?.size;
        let equal = slots.iter().all(|slot| {
            (f32::from(slot.size.width) - f32::from(first.width)).abs() < 0.5
                && (f32::from(slot.size.height) - f32::from(first.height)).abs() < 0.5
        });
        let mut origins = HashMap::new();
        if equal {
            for (position, index) in new_order.iter().enumerate() {
                let slot = slots.get(position)?;
                origins.insert(
                    *index,
                    point(f32::from(slot.origin.x), f32::from(slot.origin.y)),
                );
            }
            return Some(origins);
        }
        let height: HashMap<usize, f32> = old_order
            .iter()
            .zip(slots.iter())
            .map(|(index, slot)| (*index, f32::from(slot.size.height)))
            .collect();
        let gaps: Vec<f32> = slots
            .windows(2)
            .map(|pair| {
                f32::from(pair[1].origin.y) - f32::from(pair[0].origin.y + pair[0].size.height)
            })
            .collect();
        let mut top = f32::from(slots[0].origin.y);
        for (position, index) in new_order.iter().enumerate() {
            let x = f32::from(self.bounds_of(*index)?.origin.x);
            origins.insert(*index, point(x, top));
            top += height[index] + gaps.get(position).copied().unwrap_or_default();
        }
        Some(origins)
    }

    /// Scrolls the list's container while the pointer is in its edge zone; true when it moved.
    fn auto_scroll(&mut self) -> bool {
        let (Some(handle), Some(_)) = (self.options.scroll.clone(), self.dragging) else {
            return false;
        };
        let viewport = handle.bounds();
        let height = f32::from(viewport.size.height);
        if height <= 0.0 {
            return false;
        }
        let zone = height * SCROLL_THRESHOLD;
        let y = f32::from(self.pointer.y);
        let top = f32::from(viewport.origin.y);
        let bottom = top + height;
        let offset = handle.offset();
        let max = f32::from(handle.max_offset().y);
        let scrolled = -f32::from(offset.y);
        let delta = if y <= top + zone && scrolled > 0.0 {
            -SCROLL_ACCELERATION * ((top + zone - y) / zone).min(1.0)
        } else if y >= bottom - zone && scrolled < max {
            SCROLL_ACCELERATION * ((y - (bottom - zone)) / zone).min(1.0)
        } else {
            0.0
        };
        if delta == 0.0 {
            return false;
        }
        let next = (scrolled + delta).clamp(0.0, max);
        handle.set_offset(point(offset.x, px(-next)));
        true
    }

    /// Ends a lift without committing: the item glides back to its own slot.
    fn cancel(&mut self, now: Instant) {
        let Some((from, _)) = self.dragging.take() else {
            self.pending = None;
            return;
        };
        self.pending = None;
        self.escape = None;
        self.just_dragged = Some(now);
        // The others slide back to the saved order from where they are now.
        let live: Vec<usize> = move_index(
            &(0..self.len).collect::<Vec<_>>(),
            from,
            self.live_position(from),
        );
        let saved: Vec<usize> = (0..self.len).collect();
        if let Some(origins) = self.predict_origins(&live, &saved) {
            for index in saved.iter().copied().filter(|index| *index != from) {
                let (Some(bounds), Some(next)) = (self.bounds_of(index), origins.get(&index))
                else {
                    continue;
                };
                let current = self
                    .slides
                    .get(&index)
                    .map(|slide| slide.offset(now))
                    .unwrap_or_default();
                let from_offset = point(
                    f32::from(bounds.origin.x) + current.x - next.x,
                    f32::from(bounds.origin.y) + current.y - next.y,
                );
                if from_offset.x.abs() > 0.5 || from_offset.y.abs() > 0.5 {
                    self.slides.insert(
                        index,
                        Slide {
                            from: from_offset,
                            start: now,
                        },
                    );
                }
            }
        }
        self.dropping = Some(Dropping {
            index: from,
            from: self.pointer - self.grab,
            start: now,
        });
    }

    /// Where `from` sits in the live order (its placeholder's position).
    fn live_position(&self, from: usize) -> usize {
        self.live_order()
            .iter()
            .position(|index| *index == from)
            .unwrap_or(from)
    }
}

fn progress(start: Instant, now: Instant) -> f32 {
    (now.duration_since(start).as_secs_f32() / ANIMATION.as_secs_f32()).clamp(0.0, 1.0)
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at time `t` (solved for x by Newton's method).
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let curve = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * a + 6.0 * inverse * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = t;
    for _ in 0..8 {
        let error = curve(x1, x2, s) - t;
        let derivative = slope(x1, x2, s);
        if error.abs() < 1e-5 || derivative.abs() < 1e-6 {
            break;
        }
        s = (s - error / derivative).clamp(0.0, 1.0);
    }
    curve(y1, y2, s)
}

/// `moveId`: `ids` with the item at `from` moved to `to`.
pub(crate) fn move_index<T: Clone>(items: &[T], from: usize, to: usize) -> Vec<T> {
    let mut next = items.to_vec();
    if from >= next.len() || to >= next.len() || from == to {
        return next;
    }
    let moved = next.remove(from);
    next.insert(to, moved);
    next
}

fn state_of<'a, V: SettingsPage>(page: &'a mut V, list: &str) -> &'a mut ReorderState {
    page.field_states()
        .reorders
        .entry(SharedString::from(list.to_string()))
        .or_default()
}

/// Keeps a frame ticker running while the list animates, drags or auto-scrolls.
fn ensure_ticker<V: SettingsPage>(page: &mut V, list: &str, cx: &mut Context<V>) {
    let state = state_of(page, list);
    if state.ticking {
        return;
    }
    state.ticking = true;
    let list = list.to_string();
    state.ticker = Some(cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(FRAME).await;
            let keep = this
                .update(cx, |page, cx| {
                    let now = Instant::now();
                    let state = state_of(page, &list);
                    if state.auto_scroll() {
                        state.update_over(now);
                    }
                    let keep = state.animating(now);
                    if state
                        .dropping
                        .is_some_and(|drop| progress(drop.start, now) >= 1.0)
                    {
                        state.dropping = None;
                    }
                    state
                        .slides
                        .retain(|_, slide| now.duration_since(slide.start) < ANIMATION);
                    if !keep {
                        state.ticking = false;
                    }
                    cx.notify();
                    keep
                })
                .unwrap_or(false);
            if !keep {
                break;
            }
        }
    }));
}

/// Sets how list `list` looks and starts while dragged (keep it the same on every render).
pub(crate) fn reorder_options<V: SettingsPage>(page: &mut V, list: &str, options: ReorderOptions) {
    state_of(page, list).options = options;
}

/// Registers the scroll container the list auto-scrolls while an item is dragged near its edges.
pub(crate) fn reorder_scroll_container<V: SettingsPage>(
    page: &mut V,
    list: &str,
    handle: ScrollHandle,
) {
    state_of(page, list).options.scroll = Some(handle);
}

/// The scroll handle of a list that scrolls itself (Arrange views): created once and kept with
/// the list's drag state, for the list's `track_scroll` and its auto-scroll.
pub(crate) fn reorder_scroll_handle<V: SettingsPage>(page: &mut V, list: &str) -> ScrollHandle {
    state_of(page, list)
        .options
        .scroll
        .get_or_insert_with(ScrollHandle::new)
        .clone()
}

/// The display order of `len` items: the saved order, or the live order while one is dragged.
/// A drag that ended where the list could not see it puts the item back.
pub(crate) fn reorder_order<V: SettingsPage>(
    page: &mut V,
    list: &str,
    len: usize,
    cx: &mut Context<V>,
) -> Vec<usize> {
    let has_drag = cx.has_active_drag();
    let state = state_of(page, list);
    state.len = len;
    if !has_drag && (state.dragging.is_some() || state.pending.is_some()) {
        state.cancel(Instant::now());
        ensure_ticker(page, list, cx);
    }
    state_of(page, list).live_order()
}

/// Whether item `index` of `list` is lifted (dragged, or gliding into its slot after a drop).
pub(crate) fn reorder_is_dragging<V: SettingsPage>(page: &mut V, list: &str, index: usize) -> bool {
    page.field_states().reorders.get(list).is_some_and(|state| {
        state.dragging.is_some_and(|(from, _)| from == index)
            || state.dropping.is_some_and(|drop| drop.index == index)
    })
}

/// True once after a drag on `list` ended: the click a release fires on the item it lifted is not
/// a toggle (the `didDragRef` of the React hover buttons).
pub(crate) fn reorder_take_just_dragged<V: SettingsPage>(page: &mut V, list: &str) -> bool {
    state_of(page, list)
        .just_dragged
        .take()
        .is_some_and(|ended| ended.elapsed() < Duration::from_millis(400))
}

/// Makes `grip` the drag handle of item `index` (the grip keeps its own cursor, as the React
/// `Button` does; the whole window shows `grabbing` once the drag starts).
pub(crate) fn reorder_handle(
    p: &SettingsPalette,
    list: &str,
    index: usize,
    label: impl Into<SharedString>,
    grip: AnyElement,
) -> AnyElement {
    let _ = (p, label.into());
    let payload = ReorderDrag {
        list: SharedString::from(list.to_string()),
        index,
    };
    div()
        .id(SharedString::from(format!("{list}-reorder-handle-{index}")))
        .flex_shrink_0()
        .on_drag(payload, |_, _, _, cx| cx.new(|_| ReorderDragView))
        .child(grip)
        .into_any_element()
}

/// Commits the drag of `list` at its live slot (a release anywhere over the lifted item).
fn drop_lifted<V: SettingsPage>(
    page: &mut V,
    list: &str,
    on_move: &dyn Fn(&mut V, usize, usize, &mut Window, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let state = state_of(page, list);
    let Some((from, over)) = state.dragging.take() else {
        return;
    };
    let now = Instant::now();
    state.escape = None;
    state.just_dragged = Some(now);
    // Every index moves the way the saved order is about to.
    let permutation = move_index(&(0..state.len).collect::<Vec<_>>(), from, over);
    let remap = |old: usize| permutation.iter().position(|index| *index == old);
    state.slides = state
        .slides
        .drain()
        .filter_map(|(old, slide)| remap(old).map(|new| (new, slide)))
        .collect();
    let moved: HashMap<usize, Bounds<Pixels>> = state
        .bounds
        .borrow()
        .iter()
        .filter_map(|(old, bounds)| remap(*old).map(|new| (new, *bounds)))
        .collect();
    *state.bounds.borrow_mut() = moved;
    state.dropping = Some(Dropping {
        index: over,
        from: state.pointer - state.grab,
        start: now,
    });
    ensure_ticker(page, list, cx);
    if from != over {
        on_move(page, from, over, window, cx);
    }
    cx.notify();
}

/// Wraps item `index` (shown at display position `slot`): it slides when the order changes,
/// lifts out and follows the pointer while dragged, and a release commits the move with
/// `on_move(from, to)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reorder_row<V: SettingsPage>(
    page: &mut V,
    list: &str,
    index: usize,
    slot: usize,
    row: AnyElement,
    on_move: impl Fn(&mut V, usize, usize, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let _ = slot;
    let now = Instant::now();
    let state = state_of(page, list);
    let bounds_cell = state.bounds.clone();
    let own_bounds = state.bounds_of(index);
    let lifted_opacity = state.options.lifted_opacity;
    let fill = state.options.fill;
    let slide = state
        .slides
        .get(&index)
        .map(|slide| slide.offset(now))
        .unwrap_or_default();
    let lifted: Option<Point<Pixels>> = match (state.dragging, state.dropping) {
        (Some((from, _)), _) if from == index => Some(state.pointer - state.grab),
        (_, Some(drop)) if drop.index == index => own_bounds.map(|bounds| {
            let t = cubic_bezier(0.25, 0.1, 0.25, 1.0, progress(drop.start, now));
            let x =
                f32::from(drop.from.x) + (f32::from(bounds.origin.x) - f32::from(drop.from.x)) * t;
            let y =
                f32::from(drop.from.y) + (f32::from(bounds.origin.y) - f32::from(drop.from.y)) * t;
            point(px(x), px(y))
        }),
        _ => None,
    };
    let dropping = state.dragging.is_none();
    let list_name = SharedString::from(list.to_string());
    let move_list = list_name.clone();
    let slot_box = match (lifted, own_bounds) {
        (Some(origin), Some(bounds)) => {
            let drop_list = list_name.clone();
            let on_move = Rc::new(on_move);
            let float = div()
                .id(SharedString::from(format!("{list}-reorder-lifted-{index}")))
                .w(bounds.size.width)
                // `[data-dragging='true']` holds only while the item is dragged.
                .when(!dropping, |this| this.opacity(lifted_opacity))
                .on_drop(
                    cx.listener(move |page: &mut V, drag: &ReorderDrag, window, cx| {
                        if drag.list != drop_list {
                            return;
                        }
                        let on_move = on_move.clone();
                        drop_lifted(page, &drop_list, &*on_move, window, cx);
                    }),
                )
                .child(row);
            div()
                .w(bounds.size.width)
                .h(bounds.size.height)
                .flex_shrink_0()
                .child(
                    deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Window)
                            .position(origin)
                            .child(float),
                    )
                    .with_priority(3),
                )
        }
        _ => div().when(fill, |this| this.w_full()).child(
            div()
                .when(fill, |this| this.w_full())
                .relative()
                .left(px(slide.x))
                .top(px(slide.y))
                .child(row),
        ),
    };
    let weak = cx.entity().downgrade();
    div()
        .when(fill, |this| this.w_full())
        .on_children_prepainted(move |children: Vec<Bounds<Pixels>>, _window, _cx| {
            if let Some(first) = children.first() {
                bounds_cell.borrow_mut().insert(index, *first);
            }
        })
        .on_drag_move(cx.listener(
            move |page: &mut V, event: &DragMoveEvent<ReorderDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.list != move_list || drag.index != index {
                    return;
                }
                let now = Instant::now();
                let list = move_list.to_string();
                let pointer = event.event.position;
                let state = state_of(page, &list);
                state.pointer = pointer;
                if state.dragging.map(|(from, _)| from) != Some(index) {
                    // `PointerActivationConstraints`: the distance, or a held press.
                    let (_, start, started) = *state.pending.get_or_insert((index, pointer, now));
                    let moved = f32::from(pointer.x - start.x)
                        .hypot(f32::from(pointer.y - start.y))
                        + GPUI_DRAG_THRESHOLD;
                    let distance = state.options.activation_distance;
                    if distance > 0.0
                        && moved < distance
                        && now.duration_since(started) < HOLD_ACTIVATION
                    {
                        ensure_ticker(page, &list, cx);
                        return;
                    }
                    // An item that is taller while open closes first; it lifts on a later move,
                    // once its closed layout was measured.
                    if page.reorder_prepare_lift(&list, index, cx) {
                        state_of(page, &list).bounds.borrow_mut().remove(&index);
                        ensure_ticker(page, &list, cx);
                        cx.notify();
                        return;
                    }
                    let state = state_of(page, &list);
                    let Some(origin) = state.bounds_of(index).map(|bounds| bounds.origin) else {
                        return;
                    };
                    state.pending = None;
                    state.grab = start - origin;
                    state.dragging = Some((index, index));
                    state.dropping = None;
                    // Escape cancels the drag and the item glides back (dnd-kit's KeyboardSensor
                    // and PointerSensor both cancel on Escape).
                    let weak = weak.clone();
                    let escape_list = list.clone();
                    state.escape = Some(cx.intercept_keystrokes(move |event, window, cx| {
                        if event.keystroke.key != "escape" {
                            return;
                        }
                        cx.stop_propagation();
                        cx.stop_active_drag(window);
                        let _ = weak.update(cx, |page, cx| {
                            let state = state_of(page, &escape_list);
                            state.cancel(Instant::now());
                            ensure_ticker(page, &escape_list, cx);
                            cx.notify();
                        });
                    }));
                }
                cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
                state_of(page, &list).update_over(now);
                ensure_ticker(page, &list, cx);
                cx.notify();
            },
        ))
        .child(slot_box)
        .into_any_element()
}
