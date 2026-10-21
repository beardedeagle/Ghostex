//! How the General page groups its sections, and the rail that lists the groups.

/// A General rail group and the search sections it collects, in rendered order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneralGroup {
    pub id: &'static str,
    pub title: &'static str,
    pub sections: &'static [&'static str],
}

/// One destination of the General page's rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavItem {
    pub id: &'static str,
    pub title: &'static str,
}

/// Which General-page search sections each navigation-rail group collects, in
/// rendered order. Single-section groups keep using that section's own search
/// result (no separate group-title match).
pub const GENERAL_GROUPS: &[GeneralGroup] = &[
    group("appearance", "Theme", &["theming"]),
    group("chat", "Chat", &["chat"]),
    group(
        "sidebar",
        "Sidebar",
        &["sidebar", "sessionCards", "sidebarTags"],
    ),
    group(
        "terminal",
        "Terminal",
        &["terminal", "terminalBehavior", "terminalScrolling"],
    ),
    group(
        "tools",
        "Tools",
        &["browser", "terminalDevServers", "editor"],
    ),
    group("notifications", "Notifications", &["sounds"]),
    group("system", "System", &["autoSleep", "power"]),
    group("advanced", "Advanced", &["sleepingSessions", "beta"]),
];

const fn group(
    id: &'static str,
    title: &'static str,
    sections: &'static [&'static str],
) -> GeneralGroup {
    GeneralGroup {
        id,
        title,
        sections,
    }
}

pub fn general_group(id: &str) -> Option<&'static GeneralGroup> {
    GENERAL_GROUPS.iter().find(|group| group.id == id)
}

/// The General rail, top to bottom.
///
/// Keep these destinations in the same order as their first rendered
/// section anchors. The grouped pages intentionally collect related
/// subsections, but clicking down this rail should always move down the
/// Settings page instead of jumping above an earlier-looking destination.
///
/// CDXC:Theming 2026-09-23 DECISION:
/// User: "make theme into it's own page in settings below General". The Theme and App Icon sections render on the Theme page (settings-modal/tabs/theme.tsx (deleted 2026-10-01)), so General's rail starts at Sidebar; their search rows and the `appearance` group stay in this catalog so one query still finds them.
///
/// CDXC:Settings 2026-06-12-04:13:
/// Ghostty terminal controls belong on the main Settings page so one search query can find app settings and terminal settings together.
pub fn general_navigation() -> Vec<NavItem> {
    vec![
        nav("sidebar", "Sidebar"),
        nav("chat", "Chat"),
        nav("tools", "Tools"),
        nav("terminal", "Terminal"),
        nav("system", "System"),
        nav("notifications", "Notifications"),
        nav("advanced", "Advanced"),
    ]
}

const fn nav(id: &'static str, title: &'static str) -> NavItem {
    NavItem { id, title }
}
