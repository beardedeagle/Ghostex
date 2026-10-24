pub(crate) mod window;
#[allow(dead_code)]
pub(crate) mod model {
    include!(concat!(env!("OUT_DIR"), "/floating_reveal_model.rs"));
}

impl crate::GhostexGpuiApp {
    /// The page never floats the sidebar in a window of its own (window.rs), so the links card's
    /// hover gate (native_sidebar/session_hover.rs) always sees a docked sidebar.
    pub(crate) fn floating_sidebar_shown(&self) -> bool {
        false
    }
}
