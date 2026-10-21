use super::*;
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabState {
    Loaded,
    AddressOnly,
}

impl BrowserTabState {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "loaded" => Some(Self::Loaded),
            "address-only" => Some(Self::AddressOnly),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::AddressOnly => "address-only",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTabChromeStatus {
    LoadedSurface,
    RestoredPlaceholder,
    AddressOnly,
}

impl BrowserTabChromeStatus {
    /*
    CDXC:Browser 2026-06-22-16:48:
    Browser tab chrome must distinguish a loaded tab backed by a live CEF surface from a restored loaded placeholder that has no materialized surface. Derive this render-only status from BrowserTabState plus runtime browser_surfaces membership so shell-state persistence and favicon privacy stay unchanged.
    */
    pub(crate) fn from_state(state: BrowserTabState, has_cef_surface: bool) -> Self {
        match (state, has_cef_surface) {
            (BrowserTabState::Loaded, true) => Self::LoadedSurface,
            (BrowserTabState::Loaded, false) => Self::RestoredPlaceholder,
            (BrowserTabState::AddressOnly, _) => Self::AddressOnly,
        }
    }

    /// A restored tab shows its cached favicon before its surface exists; an address-only tab has no page to take one from.
    pub(crate) fn allows_runtime_favicon(self) -> bool {
        self != Self::AddressOnly
    }
}

/*
CDXC:Browser 2026-06-22-17:13:
Browser tab chrome is focus-invariant: pane focus may drive toolbar ownership and CEF surface sync, but tab-bar brightness derives only from a tab's shell state, runtime surface presence, and active membership inside its own Browser tab group. BrowserTabModel.focused_pane is intentionally excluded.
*/

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserRuntimeLifecycleInput {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) browser_awake: bool,
    pub(crate) browser_tab_drag_active: bool,
    pub(crate) command_tab_drag_active: bool,
    pub(crate) workspace_tab_drag_active: bool,
}

impl BrowserRuntimeLifecycleInput {
    /*
    CDXC:Workarea 2026-07-03:
    Workspace/Agents tab drags join the same hide-during-drag gate as Browser and command tab drags so the GPUI drag ghost and drop feedback can never sit under a native CEF child view. Hide-and-hold only; no CEF teardown, recreation, or overlay layering.
    */
    pub(crate) fn allows_cef_child_views(self) -> bool {
        !self.browser_tab_drag_active
            && !self.command_tab_drag_active
            && !self.workspace_tab_drag_active
            && self.active_mode == TitlebarMode::Browser
            && self.browser_awake
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserRuntimeSurfacePolicy {
    Visible,
    HiddenHold,
    RestoredPlaceholder,
}

impl BrowserRuntimeSurfacePolicy {
    /*
    CDXC:Browser 2026-06-23-11:32:
    Phase 8 Browser runtime lifecycle is hide-and-hold at the native CEF child-view boundary. Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hide existing CEF views while retaining tab-owned CEF entities and shell tab metadata; visibility must not teardown, recreate, or materialize restored loaded tabs.

    CDXC:Browser 2026-06-23-14:30:
    Hide-and-hold is the only current Browser runtime lifecycle decision. HiddenHold means an existing tab-owned CEF child view is not shown; it must not be expanded into CEF teardown, CEF suspend, CEF recreation, restored-tab materialization, shell-state writes, or popup content transfer.
    */
    pub(crate) fn for_tab(
        lifecycle: BrowserRuntimeLifecycleInput,
        tab_state: Option<BrowserTabState>,
        rendered_active_loaded: bool,
        has_cef_surface: bool,
    ) -> Self {
        if !lifecycle.allows_cef_child_views() {
            return Self::HiddenHold;
        }

        match (tab_state, rendered_active_loaded, has_cef_surface) {
            (Some(BrowserTabState::Loaded), true, true) => Self::Visible,
            (Some(BrowserTabState::Loaded), true, false) => Self::RestoredPlaceholder,
            _ => Self::HiddenHold,
        }
    }

    pub(crate) fn shows_cef_child_view(self) -> bool {
        matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserPopupTargetPolicy {
    OpenLoadedTab,
    IgnoreWithoutTransfer,
}

impl BrowserPopupTargetPolicy {
    /*
    CDXC:Browser 2026-06-23-12:48:
    Phase 8 popup handling is intentionally source-only until a compatible blank-popup content-transfer contract exists. Only non-empty CEF target URLs may create loaded Browser tabs; empty script-created popups are handled as no-ops with no address-only tab, CEF surface, persistence, notification, page-content copy, or fallback transfer path.

    CDXC:Browser 2026-06-23-14:30:
    Blank or whitespace script-created popup targets remain blocked at the shell boundary. Empty targets must no-op without address-only fallback, Browser notification, CEF surface creation, shell-state write, page-content copy, or any synthetic content-transfer path; non-empty targets are trimmed only as target identifiers for loaded-tab creation.
    */
    pub(crate) fn for_target_url(target_url: &str) -> Self {
        if target_url.trim().is_empty() {
            Self::IgnoreWithoutTransfer
        } else {
            Self::OpenLoadedTab
        }
    }

    pub(crate) fn opens_loaded_tab(self) -> bool {
        matches!(self, Self::OpenLoadedTab)
    }
}

pub(crate) fn browser_loaded_popup_target_url(requested_url: &str) -> Option<String> {
    BrowserPopupTargetPolicy::for_target_url(requested_url)
        .opens_loaded_tab()
        .then(|| requested_url.trim().to_string())
}

pub(crate) fn browser_runtime_visible_surface_tab_ids(
    lifecycle: BrowserRuntimeLifecycleInput,
    browser_tabs: &BrowserTabModel,
    surface_tab_ids: impl IntoIterator<Item = BrowserTabId>,
) -> HashSet<BrowserTabId> {
    /*
    CDXC:Browser 2026-06-23-14:30:
    The visibility pass is a pure hide-and-hold filter over already-owned CEF surfaces. It must not create missing restored-tab surfaces, suspend or tear down hidden CEF entities, infer popup content transfer, or mutate Browser shell state.
    */
    let rendered_active_loaded_tab_ids = browser_tabs.rendered_active_loaded_tab_ids();
    surface_tab_ids
        .into_iter()
        .filter(|tab_id| {
            BrowserRuntimeSurfacePolicy::for_tab(
                lifecycle,
                browser_tabs.tab(*tab_id).map(|tab| tab.state),
                rendered_active_loaded_tab_ids.contains(tab_id),
                true,
            )
            .shows_cef_child_view()
        })
        .collect()
}
