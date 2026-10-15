//! `Related settings`: the last section of the Extensions page, a few link rows that open the
//! Settings sections someone managing views and extensions tends to need next.
//!
//! CDXC:Extensions 2026-10-15 DECISION:
//! User: remove "Account usage in the sidebar" from the Extensions page and "instead let's make it
//! have links to other useful sections". This replaces the 2026-09-10 decision that put the
//! account usage stars here. The stars live only on Settings > Accounts, and its
//! `accountTitlebarUsage` search row moved there.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{icon, settings_list_item, settings_section};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::ExtensionsTab;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div,
};

/// One link: its row id, title, one-line description, the page it opens and the section it
/// scrolls to (`None` opens the page where it was).
struct RelatedLink {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    tab: SettingsTabId,
    section: Option<&'static str>,
}

/// Why these five: Accounts is where the usage stars that used to sit here live; Projects is where
/// a view or its home URL is set for one project or worktree; Actions is the built-in view whose
/// saved commands and pages are managed on its own page; Sidebar holds the settings for the
/// surface most views and built-ins appear in; Hotkeys holds the view-switching chords.
const LINKS: [RelatedLink; 5] = [
    RelatedLink {
        id: "related-accounts",
        title: "Accounts",
        detail: "Star saved Claude and Codex accounts to show their usage at the bottom of the sidebar.",
        tab: SettingsTabId::Accounts,
        section: Some("accounts"),
    },
    RelatedLink {
        id: "related-projects",
        title: "Project settings",
        detail: "Turn work mode on, or choose a project's own views and home URLs.",
        tab: SettingsTabId::Projects,
        section: Some("projectSettings"),
    },
    RelatedLink {
        id: "related-actions",
        title: "Actions",
        detail: "Saved terminal commands and web pages you start in one click.",
        tab: SettingsTabId::Actions,
        section: Some("actions"),
    },
    RelatedLink {
        id: "related-sidebar",
        title: "Sidebar",
        detail: "Session cards, tags, status indicators and what the sidebar shows.",
        tab: SettingsTabId::General,
        section: Some("sidebar"),
    },
    RelatedLink {
        id: "related-hotkeys",
        title: "Hotkeys",
        detail: "Rebind the hotkeys that open and switch between views.",
        tab: SettingsTabId::Hotkeys,
        section: None,
    },
];

impl ExtensionsTab {
    pub(crate) fn render_related_settings_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = hsla(p.raised_hover);
        let rows: Vec<AnyElement> = LINKS
            .iter()
            .map(|link| {
                let chevron = modal_icon(icon::CHEVRON_RIGHT, 16.0, p.muted).into_any_element();
                let tab = link.tab;
                let section = link.section;
                div()
                    .id(SharedString::from(link.id))
                    .w_full()
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover))
                    .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| match section {
                            Some(section) => store.scroll_to_section(tab, section, cx),
                            None => store.set_active_tab(tab, cx),
                        });
                    }))
                    .child(settings_list_item(
                        p,
                        None,
                        None,
                        link.title,
                        Some(div().child(link.detail).into_any_element()),
                        Some(chevron),
                    ))
                    .into_any_element()
            })
            .collect();
        settings_section(p, "Related settings", None, None, rows)
            .map(IntoElement::into_any_element)
            .unwrap_or_else(|| div().into_any_element())
    }
}
