//! Hover transitions and keyboard focus for the onboarding's controls: the page's CSS
//! `transition`s, `:hover`, `:focus-visible` and the browser's Tab / Shift+Tab / Enter / Space.
//!
//! CDXC:Onboarding 2026-09-28 WHY:
//! GPUI's `.hover()` swaps a style at once and GPUI tab stops follow paint order, while the page
//! eased every colour change over its CSS `transition` (0.2-0.3s on the onboarding's own classes,
//! 120ms from the modal host's base-layer `button` rule) and tabbed in DOM order. Colours are
//! therefore eased here per element and property (whatever changed them: hover or state), hover is
//! tracked per element id, and focus moves through an order the panels register in DOM order.
use super::GpuiOnboardingWindow;
use super::stage::*;
use gpui::{
    AnyElement, Context, Div, FocusHandle, Hsla, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Rgba, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    Window, div,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The modal host's `button:where(:not([data-slot]))` transition (core-ui styles/theme.css).
pub(crate) const BUTTON_MS: u64 = 120;

#[derive(Default)]
struct HoverState {
    frame: u64,
    hovered: HashSet<SharedString>,
    tweens: HashMap<(SharedString, &'static str), Tween>,
}

struct Tween {
    from: [f32; 4],
    to: [f32; 4],
    started: Instant,
    duration: Duration,
    seen: u64,
}

impl Tween {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        progress(self.started, now, self.duration, Ease::Ease)
    }

    fn value(&self, now: Instant) -> [f32; 4] {
        let t = self.progress(now);
        std::array::from_fn(|index| self.from[index] + (self.to[index] - self.from[index]) * t)
    }
}

thread_local! {
    static HOVER: RefCell<HoverState> = RefCell::new(HoverState::default());
}

/// Starts a frame: transitions of elements that were not drawn last frame end, as an unmounted
/// element's would.
pub(crate) fn begin_frame() {
    HOVER.with(|state| {
        let mut state = state.borrow_mut();
        state.frame += 1;
        let keep_from = state.frame.saturating_sub(1);
        state.tweens.retain(|_, tween| tween.seen >= keep_from);
    });
}

/// Forgets every hover and transition (a new onboarding window).
pub(crate) fn reset() {
    HOVER.with(|state| *state.borrow_mut() = HoverState::default());
}

/// Whether the pointer is over the element registered with [`watch_hover`] under `id`.
pub(crate) fn hovered(id: &str) -> bool {
    HOVER.with(|state| state.borrow().hovered.contains(id))
}

/// Tracks the pointer over `element` for [`hovered`].
pub(crate) fn watch_hover(element: Stateful<Div>, id: impl Into<SharedString>) -> Stateful<Div> {
    let id: SharedString = id.into();
    element.on_hover(move |hovered, window, _| {
        HOVER.with(|state| {
            let mut state = state.borrow_mut();
            if *hovered {
                state.hovered.insert(id.clone());
            } else {
                state.hovered.remove(&id);
            }
        });
        window.refresh();
    })
}

fn tween4(id: &str, prop: &'static str, target: [f32; 4], ms: u64) -> [f32; 4] {
    let now = Instant::now();
    HOVER.with(|state| {
        let mut state = state.borrow_mut();
        let frame = state.frame;
        let key = (SharedString::from(id.to_string()), prop);
        match state.tweens.get_mut(&key) {
            Some(tween) => {
                tween.seen = frame;
                if tween.to != target {
                    let current = tween.value(now);
                    let full = Duration::from_millis(ms);
                    // A transition sent back to where it came from runs only as long as it ran
                    // (CSS "reversing shortening factor").
                    let duration = if tween.from == target && tween.progress(now) < 1.0 {
                        full.mul_f32(tween.progress(now))
                    } else {
                        full
                    };
                    *tween = Tween {
                        from: current,
                        to: target,
                        started: now,
                        duration,
                        seen: frame,
                    };
                }
                tween.value(now)
            }
            None => {
                state.tweens.insert(
                    key,
                    Tween {
                        from: target,
                        to: target,
                        started: now,
                        duration: Duration::ZERO,
                        seen: frame,
                    },
                );
                target
            }
        }
    })
}

/// `transition: <prop> <ms>ms` for a colour: eases from wherever it is toward `target`.
/// Interpolated premultiplied, as CSS does.
pub(crate) fn tween_color(id: &str, prop: &'static str, target: Hsla, ms: u64) -> Hsla {
    if ms == 0 {
        return target;
    }
    let rgba: Rgba = target.into();
    let value = tween4(
        id,
        prop,
        [rgba.r * rgba.a, rgba.g * rgba.a, rgba.b * rgba.a, rgba.a],
        ms,
    );
    let alpha = value[3];
    if alpha <= f32::EPSILON {
        return Rgba {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
            a: 0.0,
        }
        .into();
    }
    Rgba {
        r: value[0] / alpha,
        g: value[1] / alpha,
        b: value[2] / alpha,
        a: alpha,
    }
    .into()
}

/// `transition: <prop> <ms>ms` for a number (opacity, offsets).
pub(crate) fn tween_value(id: &str, prop: &'static str, target: f32, ms: u64) -> f32 {
    if ms == 0 {
        return target;
    }
    tween4(id, prop, [target, 0.0, 0.0, 0.0], ms)[0]
}

/// The colour an element's `:hover` rule sets, eased over its transition.
pub(crate) fn hover_color(id: &str, prop: &'static str, base: Hsla, hover: Hsla, ms: u64) -> Hsla {
    tween_color(id, prop, if hovered(id) { hover } else { base }, ms)
}

/// Where the next Tab lands once the focused element is gone.
#[derive(Clone, Copy, Default)]
pub(crate) enum TabAnchor {
    /// Nothing was focused yet: the first stop.
    #[default]
    Start,
    /// A panel was just mounted: the panel's first stop (the browser's focus-navigation starting
    /// point stays where the old panel's content was).
    PanelStart,
}

/// The tab-order region an element belongs to, in the page's DOM order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TabGroup {
    Dots = 0,
    Panel = 1,
    Back = 2,
}

/// Which keys activate a focused control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keys {
    /// A `<button>` or a `buttonProps` element: Enter and Space.
    EnterSpace,
    /// The agent radio rows only answer Space.
    Space,
    /// A range input: arrows only.
    None,
}

type Run = Rc<dyn Fn(&mut GpuiOnboardingWindow, &mut Window, &mut Context<GpuiOnboardingWindow>)>;

/// The focus ring an element draws when `:focus-visible` (outline 2px, offset 2px).
#[derive(Clone, Copy)]
pub(crate) struct Ring {
    pub(crate) radius: f32,
    pub(crate) border: f32,
}

impl Ring {
    pub(crate) const fn new(radius: f32, border: f32) -> Self {
        Self { radius, border }
    }
}

#[derive(Default)]
pub(crate) struct FocusState {
    handles: RefCell<HashMap<SharedString, FocusHandle>>,
    order: RefCell<Vec<(TabGroup, SharedString)>>,
    runs: RefCell<HashMap<SharedString, (Keys, Run)>>,
    group: Cell<Option<TabGroup>>,
    focused: RefCell<Option<SharedString>>,
    visible: Cell<bool>,
    pub(crate) anchor: Cell<TabAnchor>,
}

impl FocusState {
    pub(crate) fn set_group(&self, group: TabGroup) {
        self.group.set(Some(group));
    }

    pub(crate) fn is_visible(&self, key: &str) -> bool {
        self.visible.get() && self.focused.borrow().as_deref() == Some(key)
    }

    pub(crate) fn is_focused(&self, key: &str) -> bool {
        self.focused.borrow().as_deref() == Some(key)
    }
}

/// `:focus-visible { outline: 2px solid rgba(110, 150, 255, 0.8); outline-offset: 2px }`.
pub(crate) fn focus_ring(s: S, ring: Ring) -> AnyElement {
    let out = 4.0 + ring.border;
    div()
        .absolute()
        .top(s.px(-out))
        .left(s.px(-out))
        .right(s.px(-out))
        .bottom(s.px(-out))
        .rounded(s.px(if ring.radius > 0.0 {
            ring.radius + 4.0
        } else {
            0.0
        }))
        .border(s.px(2.0))
        .border_color(rgba(110, 150, 255, 0.8))
        .into_any_element()
}

impl GpuiOnboardingWindow {
    fn focus_handle_for(&self, key: &SharedString, cx: &mut Context<Self>) -> FocusHandle {
        self.focus
            .handles
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Makes `element` a keyboard- and pointer-operable control: focusable in the registered tab
    /// order, activated by a click or its keys, hover-tracked for [`hovered`], and ringed while
    /// its focus is visible.
    pub(crate) fn control(
        &self,
        s: S,
        element: Stateful<Div>,
        key: impl Into<SharedString>,
        ring: Ring,
        keys: Keys,
        cx: &mut Context<Self>,
        run: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let key: SharedString = key.into();
        let run: Run = Rc::new(run);
        let handle = self.focus_handle_for(&key, cx);
        let group = self.focus.group.get().unwrap_or(TabGroup::Panel);
        self.focus.order.borrow_mut().push((group, key.clone()));
        self.focus
            .runs
            .borrow_mut()
            .insert(key.clone(), (keys, run.clone()));
        let visible = self.focus.is_visible(&key);
        watch_hover(element, key)
            .track_focus(&handle)
            .on_click(cx.listener(move |this, _, window, cx| run(this, window, cx)))
            .when(visible, |this| this.child(focus_ring(s, ring)))
    }

    /// A focusable element that is not clicked as a whole (a range input): tab order and focus only.
    pub(crate) fn focusable(
        &self,
        element: Stateful<Div>,
        key: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key: SharedString = key.into();
        let handle = self.focus_handle_for(&key, cx);
        let group = self.focus.group.get().unwrap_or(TabGroup::Panel);
        self.focus.order.borrow_mut().push((group, key.clone()));
        element.track_focus(&handle)
    }

    /// Starts a frame's registration: which control holds focus and whether it shows.
    pub(crate) fn begin_focus_frame(&self, window: &Window) {
        let focused = self
            .focus
            .handles
            .borrow()
            .iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(key, _)| key.clone());
        *self.focus.focused.borrow_mut() = focused;
        self.focus.visible.set(window.last_input_was_keyboard());
        self.focus.order.borrow_mut().clear();
        self.focus.runs.borrow_mut().clear();
        self.focus.group.set(None);
    }

    /// Ends a frame's registration: a focused control that is no longer drawn loses focus to the
    /// window, as a removed DOM element gives it to the body.
    pub(crate) fn end_focus_frame(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focus.focused.borrow().clone();
        if let Some(key) = focused
            && !self
                .focus
                .order
                .borrow()
                .iter()
                .any(|(_, entry)| *entry == key)
        {
            *self.focus.focused.borrow_mut() = None;
            self.focus.anchor.set(TabAnchor::PanelStart);
            window.focus(&self.focus_handle, cx);
        }
        let order = self.focus.order.borrow().clone();
        let live: HashSet<SharedString> = order.into_iter().map(|(_, key)| key).collect();
        self.focus
            .handles
            .borrow_mut()
            .retain(|key, _| live.contains(key));
    }

    /// Tab / Shift+Tab.
    pub(crate) fn move_focus(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut order = self.focus.order.borrow().clone();
        order.sort_by_key(|(group, _)| *group);
        if order.is_empty() {
            return;
        }
        let focused = self.focus.focused.borrow().clone();
        let current = focused.and_then(|key| order.iter().position(|(_, entry)| *entry == key));
        let next = match current {
            Some(index) if back => (index + order.len() - 1) % order.len(),
            Some(index) => (index + 1) % order.len(),
            None => {
                let panel_start = order
                    .iter()
                    .position(|(group, _)| *group >= TabGroup::Panel)
                    .unwrap_or(0);
                let start = match self.focus.anchor.get() {
                    TabAnchor::Start => 0,
                    TabAnchor::PanelStart => panel_start,
                };
                if back {
                    (start + order.len() - 1) % order.len()
                } else {
                    start
                }
            }
        };
        let key = order[next].1.clone();
        let handle = self.focus.handles.borrow().get(&key).cloned();
        if let Some(handle) = handle {
            window.focus(&handle, cx);
            *self.focus.focused.borrow_mut() = Some(key);
            cx.notify();
        }
    }

    /// Enter / Space on the focused control. Returns whether it acted.
    pub(crate) fn activate_focused(
        &mut self,
        space: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(key) = self.focus.focused.borrow().clone() else {
            return false;
        };
        let Some((keys, run)) = self.focus.runs.borrow().get(&key).cloned() else {
            return false;
        };
        let accepts = match keys {
            Keys::EnterSpace => true,
            Keys::Space => space,
            Keys::None => false,
        };
        if accepts {
            run(self, window, cx);
        }
        accepts
    }
}

use gpui::prelude::FluentBuilder as _;
