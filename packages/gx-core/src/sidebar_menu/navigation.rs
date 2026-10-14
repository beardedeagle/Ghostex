//! The sidebar's own "more" menu and the Sort & Filter page inside it.
//!
//! Ported from the TypeScript sidebar page (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/navigation.ts`; see git history).

use crate::app_lifecycle::APP_LIFECYCLE_ACTIONS;
use crate::sidebar_actions::GROUP_WORKING_SESSIONS_ACTION;
use crate::sidebar_view::tags::{
    normalize_tag_list_items, tag_list_item_filter, tag_list_item_label, tag_presentation,
    TagCatalog, TagListItemKind,
};
use crate::sidebar_view::{SidebarSettings, SidebarUiState, LOCAL_MACHINE_ID};

use super::commands::{message, MenuCommand};
use super::host::MenuHost;
use super::item::MenuItem;

const DISCORD_URL: &str = "https://discord.gg/df7b3G92CS";

/// `KEEP_AWAKE_DURATION_OPTIONS`.
const KEEP_AWAKE_OPTIONS: [(&str, i64); 3] =
    [("Until turned off", 0), ("2 hours", 120), ("5 hours", 300)];

/// Everything the more menu reads.
pub struct MoreMenuInput<'a> {
    pub ui: &'a SidebarUiState,
    pub settings: &'a SidebarSettings,
    /// What a tag id resolves to for its label and its icon: every machine's catalog.
    pub catalog: &'a TagCatalog,
    /// Which filter rows exist at all, and which of them are enabled: this computer's catalog.
    pub filter_catalog: &'a TagCatalog,
    pub host: &'a MenuHost,
    /// The project group ids the list draws, without the Chats group.
    pub drawn_project_group_ids: &'a [String],
}

/// `createNativeNavigation(...).moreMenu`.
pub fn more_menu(input: &MoreMenuInput<'_>) -> Vec<MenuItem> {
    let ui = input.ui;
    let is_local = ui.selected_machine_id == LOCAL_MACHINE_ID;
    let intent = |label: &str, icon: &str, action: &str| {
        MenuItem::row(label, icon, MenuCommand::sidebar_action(action))
    };

    let mut sort: Vec<MenuItem> = Vec::new();
    if is_local {
        sort.push(intent("Show Hidden", "eye", "showHidden").with_checked(ui.show_hidden));
    }
    // The Settings > Sidebar switch writes the same setting (CDXC:Sidebar 2026-10-05 in
    // sidebar_view/ordering.rs `is_grouped_working`).
    sort.push(
        intent(
            "Group Working Sessions",
            "stack",
            GROUP_WORKING_SESSIONS_ACTION,
        )
        .with_checked(input.settings.group_working_sessions),
    );
    sort.push(MenuItem::separator());
    for item in normalize_tag_list_items(&input.settings.tag_list_items, Some(input.filter_catalog))
    {
        if !item.visible {
            continue;
        }
        if item.kind == TagListItemKind::Separator {
            if item.enabled {
                sort.push(MenuItem::separator());
            }
            continue;
        }
        let Some(tag) = tag_list_item_filter(&item).map(str::to_string) else {
            continue;
        };
        sort.push(
            MenuItem {
                label: Some(tag_list_item_label(&item, input.catalog)),
                command: Some(MenuCommand::toggle_tag_filter(&tag)),
                ..MenuItem::default()
            }
            .with_tag_presentation(tag_presentation(Some(&tag), input.catalog).as_ref())
            .with_checked(ui.selected_tag_filters.iter().any(|filter| *filter == tag))
            .with_disabled(!item.enabled),
        );
    }

    /*
    CDXC:Sidebar 2026-09-21 DECISION:
    User: "Remove the manual sorting and last active sorting. I don't care about them right now."
    The two rows did nothing on desktop (the runtime had no handler for the sort mode). Without
    them the page can start or end on a separator, or be empty on a remote tab with no tag rows,
    so those separators are trimmed and an empty page is not drawn.
    The frozen TypeScript twin of this page (`tooling/gx-core/sidebar-page-frozen/navigation.ts`) is deleted; this is the only builder.
    */
    while sort.first().is_some_and(|item| item.separator) {
        sort.remove(0);
    }
    while sort.last().is_some_and(|item| item.separator) {
        sort.pop();
    }
    let mut more: Vec<MenuItem> = Vec::new();
    if is_local || input.host.machine_connected {
        more.push(intent("Add Project", "plus", "addProject"));
    }
    if !sort.is_empty() {
        more.push(
            MenuItem::submenu(
                "Sort & Filter",
                "filter",
                sort.into_iter()
                    .map(|item| {
                        let has_command = item.command.is_some();
                        MenuItem {
                            keep_open: has_command,
                            ..item
                        }
                    })
                    .collect(),
            )
            .with_page(),
        );
    }
    if !input.drawn_project_group_ids.is_empty() {
        let any_expanded = input
            .drawn_project_group_ids
            .iter()
            .any(|group_id| !ui.collapse.collapsed_groups.contains(group_id));
        more.push(intent(
            if any_expanded {
                "Collapse All"
            } else {
                "Expand Previous"
            },
            "arrows-diagonal",
            "toggleProjects",
        ));
    }
    if !is_local {
        more.push(intent("Edit Machine", "pencil", "editMachine"));
    }
    more.push(MenuItem::separator());
    more.push(intent("Sessions", "history", "sessions"));
    more.push(intent("Import Sessions", "download", "importSessions"));
    more.push(MenuItem::row(
        "Search by Prompt",
        "file-search",
        MenuCommand::command(message::search_previous_sessions_by_text()),
    ));
    more.push(MenuItem::separator());
    more.push(intent("Agents Hub", "users-group", "agentsHub"));
    more.push(MenuItem::row(
        "All Automations",
        "clock",
        MenuCommand::command(message::open_automations_page()),
    ));
    more.push(MenuItem::separator());
    more.push(intent("Mobile & Remote", "devices", "remoteSetup"));
    if input.settings.show_beta_features && !input.settings.hide_keep_awake_titlebar_control {
        let active = input.host.keep_awake_minutes;
        let mut children: Vec<MenuItem> = KEEP_AWAKE_OPTIONS
            .iter()
            .map(|(label, minutes)| {
                MenuItem::row(
                    label,
                    "coffee",
                    MenuCommand::command(message::start_keep_awake(*minutes)),
                )
                .with_checked(active == Some(*minutes))
            })
            .collect();
        if active.is_some() {
            children.push(MenuItem::row(
                "Don't Keep Awake",
                "square-minus",
                MenuCommand::command(message::stop_keep_awake()),
            ));
        }
        children.push(MenuItem::separator());
        children.push(intent("Power Settings", "settings", "powerSettings"));
        more.push(MenuItem::submenu(
            "Keep Awake",
            if active.is_some() { "coffee" } else { "moon" },
            children,
        ));
    }
    /*
    CDXC:Sidebar 2026-09-30 DECISION:
    User: "add the buttons that we have only in mac os menu bar under Ghostex to the dropdown in the sidebar (for example check for updates, quit ghostex, quit bg services, restart". The menu ends with those app rows, below Join Discord, wherever the host is the desktop app; Windows and Linux have no menu bar to find them in. This supersedes the 2026-09-21 rule that the menu ends at Join Discord. The rest of that rule stands: the menu has no Settings or Hotkeys entries, because Settings is the gear beside the Commands row and Hotkeys is a page inside Settings. About and the Hide rows stay in the macOS menu bar only.
    The frozen TypeScript twin of this menu (`tooling/gx-core/sidebar-page-frozen/navigation.ts`) is deleted; this is the only builder.
    SEE-ALSO: packages/gx-core/src/app_lifecycle.rs (the rows, shared with Quick Access's Commands tab).
    */
    more.push(MenuItem::row(
        "Join Discord",
        "users-group",
        MenuCommand::command(message::open_external_url(DISCORD_URL)),
    ));
    /*
    CDXC:Feedback 2026-10-14 DECISION:
    User: "please move the feedback button to be inside the more options dropdown in the sidebar under join discord." Send Feedback is the row directly under Join Discord, with the chat-bubble icon, and opens the Send Feedback pop-up (apps/desktop/src/app/window/feedback_modal/). This supersedes the 2026-10-11 rule that put a chat-bubble button in the sidebar's bottom bar right of the workspace switcher.
    */
    more.push(intent("Send Feedback", "message-circle", "feedback"));
    if input.host.app_lifecycle {
        more.push(MenuItem::separator());
        for action in APP_LIFECYCLE_ACTIONS {
            more.push(MenuItem::row(
                action.title,
                action.icon,
                MenuCommand::command(message::app_lifecycle(action.id)),
            ));
        }
    }
    more
}
