//! What a Space switch restores the focus to.
//!
//! CDXC:Spaces 2026-09-21 DECISION:
//! User: switching to a Space reopens the session that Space was last left on, in the view its
//! project was in (`sidebarSpaceSwitchBehavior: restore`, the default). The MEMORY of that session
//! is written on every focus change and every reveal (`sidebar_ui/store.rs`,
//! `RememberSpaceSession`); this is the other half, the read, and it runs only when the switch
//! actually changes the selected Space.
//!
//! **Declared difference from `switchNativeSidebarSpace`.** The rows are taken from the list the
//! sidebar DRAWS for the newly selected Space, where the TypeScript takes `state.groupOrder`
//! filtered by the Space alone. So a project the user HID and a row a tag filter removed are not
//! restore targets here, and the first of those two is the only one that can change what a user
//! sees: the TypeScript could focus a session inside a project
//! the sidebar is not showing. The same "the rows the list draws" rule already decides
//! `collectionAction:select` and `sidebarAction:toggleProjects`.
//!
//! Ported from the sidebar page's Space navigation (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/space-navigation.ts`; see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/space_switch.rs.

use super::view::SidebarView;

/// The one focus a Space switch asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceSwitchFocus {
    /// `post({ type: 'focusSession', sessionId, keepView: true })`.
    Session { sidebar_session_id: String },
    /// `post({ type: 'focusGroup', groupId })`, for a Space whose projects hold no session at all.
    Group { group_id: String },
}

/// The row to restore, or `None` when the Space shows nothing.
///
/// `recent_sidebar_session_ids` is the Space's own memory, newest first, exactly as
/// `recentSessionIdsBySpace[sectionKey][spaceId]` holds it. A remembered row the new Space no
/// longer draws is skipped rather than focused, which is what keeps a session that has since moved
/// to another project from pulling the user out of the Space they just chose.
pub fn plan_space_switch_restore(
    view: &SidebarView,
    recent_sidebar_session_ids: &[String],
) -> Option<SpaceSwitchFocus> {
    let drawn = drawn_session_ids(view);
    // `?? visible.flatMap(...)[0]`: with nothing remembered the Space opens on its first row, so a
    // Space switch always lands somewhere rather than leaving the previous Space's session on
    // screen behind a list that no longer holds it.
    if let Some(sidebar_session_id) =
        remembered_drawn_row(&drawn, recent_sidebar_session_ids).or_else(|| drawn.first().copied())
    {
        return Some(SpaceSwitchFocus::Session {
            sidebar_session_id: sidebar_session_id.clone(),
        });
    }
    view.groups.first().map(|group| SpaceSwitchFocus::Group {
        group_id: group.core.group_id.clone(),
    })
}

/// What a window's workspace switch restores: the session the window last had open in that
/// workspace, when the list (now built for the new workspace) still draws it, and otherwise
/// nothing, so the window shows no session rather than one it did not have open there.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: switching a window's workspace selects the session that window last had open in that
/// workspace (or none) and closes the views of projects outside it, the way switching Spaces
/// restores the last session. It is the Space switch's own step (the newest remembered row the
/// list draws) without the Space's first-row fallback, which "or none" rules out.
pub fn plan_workspace_switch_restore(
    view: &SidebarView,
    recent_sidebar_session_ids: &[String],
) -> Option<SpaceSwitchFocus> {
    let drawn = drawn_session_ids(view);
    remembered_drawn_row(&drawn, recent_sidebar_session_ids).map(|sidebar_session_id| {
        SpaceSwitchFocus::Session {
            sidebar_session_id: sidebar_session_id.clone(),
        }
    })
}

fn drawn_session_ids(view: &SidebarView) -> Vec<&String> {
    view.groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .map(|session| &session.row.sidebar_session_id)
        .collect()
}

/// The newest remembered row the list draws. A remembered row the list no longer draws is skipped
/// rather than focused.
fn remembered_drawn_row<'a>(
    drawn: &[&'a String],
    recent_sidebar_session_ids: &[String],
) -> Option<&'a String> {
    recent_sidebar_session_ids
        .iter()
        .find_map(|session_id| drawn.iter().copied().find(|drawn| *drawn == session_id))
}
