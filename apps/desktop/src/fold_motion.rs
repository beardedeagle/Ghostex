//! The fold animation of every collapsible list: the sidebar's projects, sections and collections
//! (app/native_sidebar/disclosure.rs) and the Settings rail's General and Hotkeys sections and its
//! Extensions group (app/window/settings_modal/rail.rs). A fold's content stays drawn while it
//! closes, and while it moves its height is clipped to the measured height times the eased
//! fraction, fading with it, over Collapse animation speed (`sidebarCollapseAnimationDurationMs`)
//! on `cubic-bezier(0.22, 1, 0.36, 1)`. A press mid-flight reverses from the fraction on screen.
//!
//! CDXC:Settings 2026-10-19 SEE-ALSO: app/native_sidebar/disclosure.rs and
//! app/window/settings_modal/rail.rs fold through this one file, so both keep the same motion.
use gpui::{
    AnyElement, App, Div, Entity, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_component::v_flex;
use std::time::Duration;
use web_time::Instant;

/// One fold's motion: where it is heading, the content height last measured, and its clock.
pub(crate) struct FoldMotion {
    collapsed: bool,
    height: f32,
    started: Instant,
    duration: Duration,
    from: f32,
}

impl FoldMotion {
    /// A fold resting `collapsed` or open, with nothing to animate.
    pub(crate) fn settled(collapsed: bool) -> Self {
        Self {
            collapsed,
            height: 0.0,
            started: Instant::now(),
            duration: Duration::ZERO,
            from: if collapsed { 0.0 } else { 1.0 },
        }
    }

    /// Where the fold is heading (or resting).
    pub(crate) fn collapsed(&self) -> bool {
        self.collapsed
    }

    /// Starts folding toward `collapsed` from the fraction on screen; nothing when already heading there.
    pub(crate) fn fold_to(&mut self, collapsed: bool, duration: Duration) {
        if self.collapsed == collapsed {
            return;
        }
        self.from = self.fraction();
        self.started = Instant::now();
        self.duration = duration;
        self.collapsed = collapsed;
    }

    /// Jumps to `collapsed` without animating, keeping the measured height.
    pub(crate) fn settle(&mut self, collapsed: bool) {
        let height = self.height;
        *self = Self::settled(collapsed);
        self.height = height;
    }

    /// How open the fold is drawn, 0 (closed) to 1 (open), eased.
    pub(crate) fn fraction(&self) -> f32 {
        if self.duration.is_zero() {
            return if self.collapsed { 0.0 } else { 1.0 };
        }
        let t = (self.started.elapsed().as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        let eased = bezier(t, 0.22, 1.0, 0.36, 1.0);
        self.from + ((if self.collapsed { 0.0 } else { 1.0 }) - self.from) * eased
    }

    pub(crate) fn animating(&self) -> bool {
        self.started.elapsed() < self.duration
    }

    /// Whether the fold's content is drawn: open, or still closing.
    pub(crate) fn present(&self) -> bool {
        !self.collapsed || self.animating()
    }
}

/// The fold's duration: Collapse animation speed in milliseconds, or none under Reduce Motion.
pub(crate) fn fold_duration(duration_ms: u64, reduce_motion: bool) -> Duration {
    if reduce_motion {
        Duration::ZERO
    } else {
        Duration::from_millis(duration_ms)
    }
}

/// Draws `content` inside the fold `motion` moves (none: drawn as is). After each prepaint the
/// content's height is handed to the fold `locate` finds in `view`, and the next frame is asked
/// for while it moves.
pub(crate) fn render_fold<V: 'static>(
    view: Entity<V>,
    motion: Option<&FoldMotion>,
    locate: impl Fn(&mut V) -> Option<&mut FoldMotion> + 'static,
    content: impl IntoElement,
) -> AnyElement {
    let mut wrapper: Div = div().w_full().flex_shrink_0().on_children_prepainted(
        move |bounds, window: &mut Window, cx: &mut App| {
            if let Some(bounds) = bounds.first() {
                let height = f32::from(bounds.size.height);
                view.update(cx, |view, cx| {
                    if let Some(motion) = locate(view) {
                        if (motion.height - height).abs() > 0.01 {
                            motion.height = height;
                            cx.notify();
                        }
                        if motion.animating() {
                            window.request_animation_frame();
                            cx.notify();
                        }
                    }
                });
            }
        },
    );
    if let Some(motion) = motion.filter(|motion| motion.animating()) {
        let fraction = motion.fraction();
        wrapper = wrapper
            .max_h(px(motion.height * fraction))
            .overflow_hidden()
            .opacity(fraction);
    }
    wrapper
        .child(v_flex().w_full().flex_shrink_0().child(content))
        .into_any_element()
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `progress`: solve x(u) = progress, then read y(u).
pub(crate) fn bezier(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let sample = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let mid = (low + high) / 2.0;
        if sample(mid, x1, x2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample((low + high) / 2.0, y1, y2)
}
