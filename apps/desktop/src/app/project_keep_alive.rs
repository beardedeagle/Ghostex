// Keep-alive for projects the user just left: the terminals, chat pages, and
// view page that were visible when a project was parked stay running for the
// Settings-owned window instead of being released on the switch.

use crate::app::model::*;
use crate::*;
use std::time::{Duration, Instant};

/// A Code, Kanban, Automate, or Docs page that was visible when its project was left, kept loaded
/// until the same runtime URL asks for it again or the keep-alive window ends.
pub(crate) struct ParkedProjectWorkareaSurface {
    pub(crate) owned: ProjectWorkareaRuntimeCefSurface,
    pub(crate) slot_key: ProjectWorkareaCefSurfaceSlotKey,
    pub(crate) parked_at: Instant,
    /// The project that was left, when the page was parked on a project switch.
    pub(crate) project_id: Option<String>,
    /// The page a background Space opens on, which outlives the keep-alive window
    /// (`refresh_space_kept_parked_workarea_surfaces`).
    pub(crate) space_kept: bool,
}

impl ParkedProjectWorkareaSurface {
    fn kept(&self, keep: Option<Duration>) -> bool {
        self.space_kept || project_keep_alive_active(Some(self.parked_at), keep)
    }
}

/// True while `parked_at` is still inside the keep-alive window; `None` for either means no keep-alive.
pub(crate) fn project_keep_alive_active(
    parked_at: Option<Instant>,
    keep: Option<Duration>,
) -> bool {
    match (parked_at, keep) {
        (Some(parked_at), Some(keep)) => parked_at.elapsed() < keep,
        _ => false,
    }
}

/// Whether a parked page may go back to `wanted`: any page of the same address, except that an
/// extra Code window's page belongs to that window alone.
///
/// CDXC:CodeEditor 2026-10-25 WHY:
/// Every empty Code window has the same address (`/?ew=true`) and two windows can have the same folder open, in one project or in two. Matched by address alone, a project's new window took another project's live page, and the page's own messages (title, address, File > Close Window) still named the window it was made for. A Code window's id is never reused, so the slot is the page's owner; the project's own Code page and its windows never take each other's page either.
fn parked_page_is_for_slot(
    parked: ProjectWorkareaCefSurfaceSlotKey,
    wanted: ProjectWorkareaCefSurfaceSlotKey,
) -> bool {
    let code_window = |slot: ProjectWorkareaCefSurfaceSlotKey| {
        matches!(slot, ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_))
    };
    if code_window(parked) || code_window(wanted) {
        parked == wanted
    } else {
        true
    }
}

fn keep_alive_remaining(parked_at: Option<Instant>, keep: Option<Duration>) -> Option<Duration> {
    let keep = keep?;
    keep.checked_sub(parked_at?.elapsed())
        .filter(|remaining| !remaining.is_zero())
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-19 DECISION:
    /// User: switching Spaces (which switches projects) must keep the previous project live in the background instead of closing and relaunching it on every swap, for a slider-chosen number of minutes.
    /// Only what was visible when the project was left is kept: its visible terminal viewers, visible chat pages, and the open Code/Kanban/Automate/Docs page. Hidden tabs keep the 2026-09-13 release rules, and 0 minutes restores those rules for everything.
    pub(crate) fn project_switch_keep_alive(&self) -> Option<Duration> {
        shared_settings::shared_sidebar_settings_snapshot().project_switch_keep_alive()
    }

    /// The Agents terminal viewers currently on screen, captured before a project switch parks them.
    pub(crate) fn agents_terminal_keep_alive_viewer_sessions(&self) -> HashSet<TerminalSessionId> {
        if self.project_switch_keep_alive().is_none() {
            return HashSet::new();
        }
        self.agents_gpui_engine_terminals
            .keys()
            .copied()
            .filter(|id| self.agents_terminal_viewer_is_visible(*id))
            .collect()
    }

    pub(crate) fn park_project_workarea_runtime_cef_surface(
        &mut self,
        owned: ProjectWorkareaRuntimeCefSurface,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        was_visible: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if owned.runtime_url.project_view_key.is_some() {
            self.park_custom_project_view(owned);
            return;
        }
        if !was_visible || self.project_switch_keep_alive().is_none() {
            return;
        }
        self.parked_project_workarea_surfaces.retain(|parked| {
            !(parked.owned.matches_runtime_url(&owned.runtime_url)
                && parked_page_is_for_slot(parked.slot_key, slot_key))
        });
        self.parked_project_workarea_surfaces
            .push(ParkedProjectWorkareaSurface {
                owned,
                slot_key,
                parked_at: Instant::now(),
                project_id: self.agents_workspace_project_id.clone(),
                space_kept: false,
            });
        self.ensure_project_keep_alive_expiry_scheduled(cx);
    }

    /// CDXC:Workarea 2026-09-27 WHY:
    /// The page on screen is parked here, before the incoming project's view state is applied. The ordinary prune runs after that swap, when `active_mode` already belongs to the incoming project, so it asked the incoming project whether the page was visible: the page the user left was closed whenever the other project showed a different view, and coming back rebuilt it behind its skeleton. With keep-alive off the prune keeps its old rules.
    /// CDXC:Workarea 2026-09-30 DECISION:
    /// User: the page a background Space opens on is not slept, so swiping between Spaces shows it instantly (CDXC:Spaces 2026-09-30). It stays past the keep-alive slider for as long as that Space opens on it; every other parked page, the side panel's active view included, still closes when the slider's minutes run out, and 0 minutes parks nothing. Supersedes the 2026-09-27 rule that the slider was the limit for every parked page.
    pub(crate) fn park_visible_project_workarea_surfaces_before_switch(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_switch_keep_alive().is_none() {
            return;
        }
        let visible_slots = self
            .project_workarea_runtime_cef_surfaces
            .keys()
            .copied()
            .filter(|slot| self.project_workarea_runtime_cef_surface_may_be_visible(*slot))
            .collect::<Vec<_>>();
        for slot_key in visible_slots {
            if let Some(owned) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key) {
                owned
                    .surface
                    .update(cx, |surface, _| surface.set_visible(false));
                self.park_project_workarea_runtime_cef_surface(owned, slot_key, true, cx);
            }
        }
    }

    pub(crate) fn take_parked_project_workarea_runtime_cef_surface(
        &mut self,
        runtime_url: &ProjectWorkareaRealRuntimeUrl,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<ProjectWorkareaRuntimeCefSurface> {
        if let Some(owned) = self.take_custom_project_view(runtime_url) {
            return Some(owned);
        }
        let keep = self.project_switch_keep_alive();
        let index = self
            .parked_project_workarea_surfaces
            .iter()
            .position(|parked| {
                parked.owned.matches_runtime_url(runtime_url)
                    && parked_page_is_for_slot(parked.slot_key, slot_key)
                    && parked.kept(keep)
            })?;
        Some(self.parked_project_workarea_surfaces.remove(index).owned)
    }

    /// Closes the parked pages of the Code view (its own window's and every extra window's), as
    /// stopping or losing the editor does for the pages on screen.
    pub(crate) fn release_parked_code_editor_pages(&mut self) -> bool {
        let before = self.parked_project_workarea_surfaces.len();
        self.parked_project_workarea_surfaces
            .retain(|parked| !parked.slot_key.is_code_editor());
        self.parked_project_workarea_surfaces.len() != before
    }

    /// The parked page of one extra Code window, whichever project is on screen.
    pub(crate) fn parked_code_window_page_mut(
        &mut self,
        id: CodeWindowId,
    ) -> Option<&mut ParkedProjectWorkareaSurface> {
        self.parked_project_workarea_surfaces
            .iter_mut()
            .find(|parked| parked.slot_key == ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id))
    }

    /// Closes the parked page of one extra Code window.
    pub(crate) fn release_parked_code_window_page(&mut self, id: CodeWindowId) {
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.slot_key != ProjectWorkareaCefSurfaceSlotKey::CodeWindow(id));
    }

    fn expire_parked_project_workarea_surfaces(&mut self) {
        let keep = self.project_switch_keep_alive();
        // Dropping the entity closes the page, exactly as the prune did before keep-alive.
        self.parked_project_workarea_surfaces
            .retain(|parked| parked.kept(keep));
    }

    /// Marks the parked pages the background Spaces open on, and closes the ones that were held only
    /// for a Space that no longer opens on them and are past the keep-alive window.
    pub(crate) fn refresh_space_kept_parked_workarea_surfaces(
        &mut self,
        space_kept: &crate::app::web_page_sleep::SpaceKeptWebPages,
    ) {
        let mut released = false;
        for parked in &mut self.parked_project_workarea_surfaces {
            let kept = parked.project_id.as_deref().is_some_and(|project_id| {
                space_kept.keeps_view(project_id, parked.slot_key.titlebar_mode())
            });
            released |= parked.space_kept && !kept;
            parked.space_kept = kept;
        }
        if released {
            self.expire_parked_project_workarea_surfaces();
        }
    }

    fn next_project_keep_alive_expiry(&self) -> Option<Duration> {
        let keep = self.project_switch_keep_alive();
        let workarea = self
            .parked_project_workarea_surfaces
            .iter()
            .filter(|parked| !parked.space_kept)
            .filter_map(|parked| keep_alive_remaining(Some(parked.parked_at), keep));
        let terminals = self
            .parked_agents_terminal_runtimes_by_project
            .values()
            .filter(|parked| !parked.kept_alive_viewer_sessions.is_empty())
            .filter_map(|parked| keep_alive_remaining(parked.parked_at, keep));
        workarea.chain(terminals).min()
    }

    /// One timer covers every kept project: it fires at the earliest expiry, runs the ordinary
    /// release passes (which now see the window as closed), and re-arms for the next project.
    pub(crate) fn ensure_project_keep_alive_expiry_scheduled(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_keep_alive_expiry_scheduled {
            return;
        }
        let Some(delay) = self.next_project_keep_alive_expiry() else {
            return;
        };
        self.project_keep_alive_expiry_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(delay + Duration::from_millis(100))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.project_keep_alive_expiry_scheduled = false;
                this.expire_parked_project_workarea_surfaces();
                this.release_unused_agents_gpui_terminal_viewers(false, &HashSet::new(), cx);
                this.ensure_project_keep_alive_expiry_scheduled(cx);
                cx.notify();
            });
        })
        .detach();
    }
}
