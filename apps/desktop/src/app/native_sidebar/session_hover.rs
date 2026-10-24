use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use gpui::{
    App, Bounds, DispatchPhase, Entity, Hitbox, HitboxBehavior, MouseMoveEvent, Pixels, Window,
};

use super::model::NativeSidebarSession;
use crate::GhostexGpuiApp;

/// How long after the last wheel tick the list counts as still scrolling.
const SCROLL_SETTLE: Duration = Duration::from_millis(120);

thread_local! {
    static LAST_WHEEL: Cell<Option<Instant>> = const { Cell::new(None) };
    static SETTLE_PENDING: Cell<bool> = const { Cell::new(false) };
}

fn scroll_in_flight() -> bool {
    LAST_WHEEL
        .get()
        .is_some_and(|at| at.elapsed() < SCROLL_SETTLE)
}

/// The hovered row one session list asks for: take the row under the pointer, or let go of one of its own rows.
enum SessionHoverChange {
    Claim(String),
    Release(String),
}

/// CDXC:Sidebar 2026-09-19 DECISION:
/// The user asked to click a session card's X and, when the next card slides up under the cursor, click again to close it without moving the mouse to bring up that card's hover buttons.
/// Row `on_hover` only fires on mouse moves, so a card that moved under a still pointer (close, reorder, scroll) never learned it was hovered.
/// Each visible row gets a non-interactive hit-test probe, and the list resolves the hovered card from the frame's hit test after every paint and on every mouse move.
pub(super) struct SessionHoverProbes {
    probes: Vec<(String, Hitbox)>,
    sessions: Vec<Arc<NativeSidebarSession>>,
}

impl SessionHoverProbes {
    /// Inserted during prepaint, before the rows' own hitboxes, so the probes sit beneath the rows and block nothing.
    pub(super) fn insert(
        sessions: Vec<Arc<NativeSidebarSession>>,
        rows: impl Iterator<Item = (usize, Bounds<Pixels>)>,
        window: &mut Window,
    ) -> Self {
        let probes = rows
            .map(|(index, bounds)| {
                (
                    sessions[index].session_id.clone(),
                    window.insert_hitbox(bounds, HitboxBehavior::Normal),
                )
            })
            .collect();
        Self { probes, sessions }
    }

    /// Called during paint, once this frame's hit test is known.
    pub(super) fn track(self, view: Entity<GhostexGpuiApp>, window: &mut Window, cx: &mut App) {
        if !scroll_in_flight()
            && let Some(change) = self.change(view.read(cx), window, cx)
        {
            let view = view.clone();
            window.defer(cx, move |_, cx| {
                view.update(cx, |app, cx| app.apply_native_session_hover(change, cx))
            });
        }
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if let Some(change) = self.change(view.read(cx), window, cx) {
                view.update(cx, |app, cx| app.apply_native_session_hover(change, cx));
            }
        });
    }

    fn change(
        &self,
        app: &GhostexGpuiApp,
        window: &Window,
        cx: &App,
    ) -> Option<SessionHoverChange> {
        // Keyboard input suppresses hit-test hover; keep whatever the pointer last showed.
        if window.last_input_was_keyboard() {
            return None;
        }
        let current = app.native_sidebar.hovered_session.as_deref();
        let under_pointer = if app.native_sidebar.pointer_inside && !cx.has_active_drag() {
            self.probes
                .iter()
                .find(|(_, hitbox)| hitbox.is_hovered(window))
                .map(|(id, _)| id.as_str())
        } else {
            None
        };
        match under_pointer {
            Some(id) => (current != Some(id)).then(|| SessionHoverChange::Claim(id.to_owned())),
            None => current
                .filter(|id| {
                    self.sessions
                        .iter()
                        .any(|session| session.session_id == *id)
                })
                .map(|id| SessionHoverChange::Release(id.to_owned())),
        }
    }
}

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// A mouse wheel tick moves the list by a row or more, so the post-paint hover resolve found a new card under the still pointer after every tick and notified the root again: two full-window frames per tick, which made wheel scrolling stall while a trackpad, which crosses a row every ten frames or so, stayed smooth.
    /// While ticks keep arriving the post-paint resolve is skipped, and one redraw after the last tick resolves the card under the pointer, which keeps the 2026-09-19 decision for a pointer that has stopped. Real pointer movement still resolves at once.
    pub(super) fn native_sidebar_scroll_wheel_moved(&mut self, cx: &mut gpui::Context<Self>) {
        LAST_WHEEL.set(Some(Instant::now()));
        if SETTLE_PENDING.replace(true) {
            return;
        }
        cx.spawn(async move |app, cx| {
            while scroll_in_flight() {
                cx.background_executor().timer(SCROLL_SETTLE).await;
            }
            SETTLE_PENDING.set(false);
            let _ = app.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn apply_native_session_hover(
        &mut self,
        change: SessionHoverChange,
        cx: &mut gpui::Context<Self>,
    ) {
        let hovered = &mut self.native_sidebar.hovered_session;
        match change {
            SessionHoverChange::Claim(id) if hovered.as_deref() != Some(&id) => {
                *hovered = Some(id.clone());
                self.schedule_native_session_link_line(id, cx);
            }
            SessionHoverChange::Release(id) if hovered.as_deref() == Some(&id) => *hovered = None,
            _ => return,
        }
        if self.native_sidebar.link_line_session.is_some()
            && self.native_sidebar.link_line_session != self.native_sidebar.hovered_session
        {
            self.native_sidebar.link_line_session = None;
        }
        cx.notify();
    }

    /// Opens the hovered card's links line once the pointer has rested on it a little (link_chips.rs),
    /// so sweeping over the list does not make every card jump.
    /// It opens a little before the title's tooltip would, so the card has grown by the time the
    /// tooltip shows beside it (link_chips.rs `session_title_tooltip_placement`).
    fn schedule_native_session_link_line(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        let has_links = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.groups.iter().any(|group| {
                    group.sessions.iter().any(|session| {
                        session.session_id == id
                            && session.links.is_some()
                            && session.work.is_none()
                    })
                })
            });
        if !has_links {
            return;
        }
        let delay = self.configured_tooltip_delay() * 3 / 5;
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(delay).await;
            let _ = app.update(cx, |app, cx| {
                if app.native_sidebar.hovered_session.as_deref() == Some(&id)
                    && app.native_sidebar.link_line_session.as_deref() != Some(&id)
                {
                    app.native_sidebar.link_line_session = Some(id);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
