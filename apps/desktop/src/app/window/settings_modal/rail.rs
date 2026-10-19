//! `SettingsSidebarNavigation`: the left rail with every Settings page, the expandable General
//! and Hotkeys sections (titles navigate, only the chevrons expand: CDXC:Settings 2026-09-12
//! DECISION), About pinned to the bottom, and Show Advanced in the footer.
use super::super::native_modal_kit::*;
use super::catalog::{module, settings_catalog};
use super::fields::{settings_icon, settings_switch};
use super::model::SettingsTabId;
use super::palette::SettingsPalette;
use super::search::{
    SectionSearch, general_group_of, general_navigation, should_show_section, title_matches,
};
use super::shell::GpuiSettingsModalWindow;
use super::store::SettingsStore;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::collections::HashMap;

/// `--settings-section-sidebar-width`.
pub(crate) const RAIL_WIDTH: f32 = 192.0;

/// A rail section (a General group or a Hotkeys section) with its third-level rows.
pub(crate) struct RailSection {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) active: bool,
    pub(crate) subsections: Vec<(String, String, bool)>,
}

pub(crate) struct RailPage {
    pub(crate) tab: SettingsTabId,
    pub(crate) sections: Vec<RailSection>,
}

/// `HOTKEY_SETTINGS_SECTIONS` searched the way `useHotkeySettings` does, as `(id, title, result)`.
/// `hotkey_shown` drops the hotkeys of a built-in extension that is off (`built_in_extensions`),
/// and a section left with none of its hotkeys goes with them.
pub(crate) fn hotkey_section_searches(
    query: &str,
    hotkey_shown: &dyn Fn(&str) -> bool,
) -> Vec<(String, String, SectionSearch)> {
    let catalog = settings_catalog();
    let sections = catalog
        .module_value(module::SETTINGS_TYPES, "HOTKEY_SETTINGS_SECTIONS")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let page_matches = title_matches(query, "Hotkeys");
    sections
        .iter()
        .filter_map(|section| {
            let id = section.get("id")?.as_str()?.to_string();
            let title = section.get("title")?.as_str()?.to_string();
            let mut rows: Vec<super::catalog::SettingRowDef> = Vec::new();
            let row = |key: &str, title: &str, subtitle: &str| super::catalog::SettingRowDef {
                key: key.to_string(),
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                options: Vec::new(),
                advanced: false,
            };
            if id == "projects" {
                rows.push(row(
                    "expandCollapsedProjectsOnJump",
                    "Expand collapsed projects on jump",
                    "Reveal a collapsed Projects row before focusing it from Jump to Project hotkeys.",
                ));
            }
            if id == "navigation" {
                rows.push(row(
                    "sidebarSessionCycleSkipsSleeping",
                    "Skip sleeping sessions",
                    "Next Session and Previous Session jump over sleeping sessions in the sidebar.",
                ));
            }
            let section_ids = section.get("ids")?.as_array()?;
            for hotkey_id in section_ids {
                let hotkey_id = hotkey_id.as_str()?;
                if !hotkey_shown(hotkey_id) {
                    continue;
                }
                if let Some(definition) = catalog.hotkeys.iter().find(|definition| definition.id == hotkey_id) {
                    rows.push(super::catalog::SettingRowDef {
                        key: definition.id.clone(),
                        title: definition.title.clone(),
                        subtitle: definition.description.clone(),
                        options: vec![super::catalog::SettingOption {
                            label: definition.default_key.clone(),
                            value: definition.default_key.clone(),
                        }],
                        advanced: false,
                    });
                }
            }
            if !section_ids.is_empty() && rows.is_empty() {
                return None;
            }
            let mut result = super::search::section_search(query, &title, &rows);
            if page_matches {
                result.section_matches = true;
            }
            Some((id, title, result))
        })
        .collect()
}

/// `createSettingsSidebarPages`: the pages the rail lists (filtered to those with matches while
/// searching) and the expandable sections of General and Hotkeys.
pub(crate) fn rail_pages(store: &SettingsStore) -> Vec<RailPage> {
    let searching = store.is_searching();
    let query = store.search_query().to_string();
    let active_tab = store.active_tab();
    let general = store.general_search();
    let power_visible = store.bool("showBetaFeatures");
    let active_general = store
        .active_section(SettingsTabId::General)
        .unwrap_or_else(|| "sidebar".to_string());
    let active_general_group = general_group_of(&active_general);
    let general_sections: Vec<RailSection> = general_navigation(general, power_visible)
        .into_iter()
        .map(|group| RailSection {
            active: active_tab == SettingsTabId::General && active_general_group == group.id,
            subsections: group
                .subsections
                .iter()
                .filter(|(_, title)| *title != group.title)
                .map(|(id, title)| {
                    (
                        id.clone(),
                        title.clone(),
                        active_tab == SettingsTabId::General && active_general == *id,
                    )
                })
                .collect(),
            id: group.id,
            title: group.title,
        })
        .collect();
    let hotkey_sections = hotkey_section_searches(&query, &|id| {
        ghostex_settings_catalog::built_in_extensions::hotkey_shown_with(id, |key| {
            Some(store.bool(key))
        })
    });
    let active_hotkey = store
        .active_section(SettingsTabId::Hotkeys)
        .unwrap_or_else(|| "general".to_string());
    let visible_hotkey_sections: Vec<RailSection> = hotkey_sections
        .iter()
        .filter(|(_, _, result)| should_show_section(result, true))
        .map(|(id, title, _)| RailSection {
            id: id.clone(),
            title: title.clone(),
            active: active_tab == SettingsTabId::Hotkeys && active_hotkey == *id,
            subsections: Vec::new(),
        })
        .collect();
    let has_matches = |tab: SettingsTabId, sections: &[RailSection]| -> bool {
        if !searching {
            return true;
        }
        match tab {
            SettingsTabId::General => !sections.is_empty() || title_matches(&query, "General"),
            SettingsTabId::Theme => {
                general.subsection_visible("theming", power_visible)
                    || general.subsection_visible("appIcon", power_visible)
            }
            SettingsTabId::Hotkeys => !sections.is_empty(),
            other => store.tab_search(other).has_matches(),
        }
    };
    let show_advanced = store.show_advanced();
    SettingsTabId::RAIL_ORDER
        .into_iter()
        .filter(|tab| match tab {
            SettingsTabId::OsIntegration => store.os_integration_visible(),
            SettingsTabId::Connectors => store.connectors_visible(),
            // CDXC:Settings 2026-09-26 DECISION: Debugging leaves the rail while Show Advanced is off; a search still finds it.
            SettingsTabId::Debugging => show_advanced || searching,
            _ => store.built_in_extension_allows_page(*tab),
        })
        .filter_map(|tab| {
            let sections = match tab {
                SettingsTabId::General => general_sections.iter().map(clone_section).collect(),
                SettingsTabId::Hotkeys => {
                    visible_hotkey_sections.iter().map(clone_section).collect()
                }
                _ => Vec::new(),
            };
            has_matches(tab, &sections).then_some(RailPage { tab, sections })
        })
        .collect()
}

fn clone_section(section: &RailSection) -> RailSection {
    RailSection {
        id: section.id.clone(),
        title: section.title.clone(),
        active: section.active,
        subsections: section.subsections.clone(),
    }
}

/// The rail's expanded pages and sections.
#[derive(Default)]
pub(crate) struct RailState {
    pub(crate) expanded_pages: HashMap<SettingsTabId, bool>,
    pub(crate) expanded_sections: HashMap<String, bool>,
}

impl RailState {
    pub(crate) fn new() -> Self {
        let mut state = Self::default();
        state.expanded_pages.insert(SettingsTabId::General, true);
        state
    }
}

/// CDXC:Settings 2026-09-12 DECISION:
/// User: Settings table-of-contents titles only navigate; only the small chevron on the right expands or collapses their entries. This replaces the full-header toggle behavior.
fn chevron(expanded: bool) -> &'static str {
    if expanded {
        super::fields::icon::CHEVRON_DOWN
    } else {
        super::fields::icon::CHEVRON_RIGHT
    }
}

/// One rail page: its row and, when it has them and is expanded, its sections.
fn render_page(
    shell: &GpuiSettingsModalWindow,
    p: &SettingsPalette,
    page: RailPage,
    active_tab: SettingsTabId,
    cx: &mut Context<GpuiSettingsModalWindow>,
) -> AnyElement {
    let accent = p.raised_hover;
    let resting = p.foreground_alpha(0.82);
    let dim = p.rail_dim();
    let tab = page.tab;
    let expanded = shell
        .rail
        .expanded_pages
        .get(&tab)
        .copied()
        .unwrap_or(false);
    let has_sections = !page.sections.is_empty();
    let active = active_tab == tab;
    let fill_when_active = active && !(has_sections && expanded);
    let row = h_flex()
        .w_full()
        .min_w_0()
        .gap(px(2.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .when(fill_when_active, |this| this.bg(hsla(accent)))
        .when(!(has_sections && expanded), |this| {
            this.hover(move |this| this.bg(hsla(accent)))
        })
        .child(
            h_flex()
                .id(SharedString::from(format!("rail-page-{}", tab.id())))
                .role(gpui::Role::Button)
                .aria_label(tab.title())
                .aria_selected(active)
                .accessibility_id(format!("rail-page-{}", tab.id()))
                .flex_1()
                .min_w_0()
                .min_h(px(32.0))
                .px(px(10.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(if active { p.foreground } else { resting }))
                .when(active, |this| this.font_weight(FontWeight::MEDIUM))
                .hover(|this| this.text_color(hsla(p.foreground)))
                .on_press(cx, move |shell, window, cx| {
                    shell.select_page(tab, window, cx);
                })
                .child(
                    settings_icon(
                        tab.icon(),
                        16.0,
                        if active { p.foreground } else { resting },
                    )
                    .flex_shrink_0(),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(tab.title()),
                ),
        )
        .when(has_sections, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!(
                        "rail-page-disclosure-{}",
                        tab.id()
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{} sections", tab.title()))
                    .aria_expanded(expanded)
                    .flex_shrink_0()
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .cursor_pointer()
                    .on_press(cx, move |shell, _window, cx| {
                        let entry = shell.rail.expanded_pages.entry(tab).or_insert(false);
                        *entry = !*entry;
                        cx.notify();
                    })
                    .child(settings_icon(
                        chevron(expanded),
                        12.0,
                        if active { p.foreground } else { p.muted },
                    )),
            )
        });
    let mut group = v_flex().w_full().min_w_0().gap(px(2.0)).child(row);
    if has_sections && expanded {
        let mut list = v_flex().ml(px(8.0)).min_w_0().gap(px(2.0));
        for section in page.sections {
            let section_key = format!("{}:{}", tab.id(), section.id);
            let has_sub = !section.subsections.is_empty();
            let sub_expanded = has_sub
                && shell
                    .rail
                    .expanded_sections
                    .get(&section_key)
                    .copied()
                    .unwrap_or(false);
            let any_sub_active = section.subsections.iter().any(|(_, _, active)| *active);
            let section_id = section.id.clone();
            let button = div()
                .id(SharedString::from(format!("rail-section-{section_key}")))
                .role(gpui::Role::Button)
                .aria_label(section.title.clone())
                .aria_selected(section.active)
                .accessibility_id(format!("rail-section-{section_key}"))
                .flex_1()
                .min_w_0()
                .h(px(32.0))
                .pl(px(24.0))
                .pr(px(10.0))
                .flex()
                .items_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .cursor_pointer()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(if section.active { p.foreground } else { dim }))
                .when(section.active, |this| this.font_weight(FontWeight::MEDIUM))
                .when(!section.active, |this| {
                    this.hover(|this| this.text_color(hsla(p.muted)))
                })
                .on_press(cx, move |shell, window, cx| {
                    shell.select_section(tab, &section_id, window, cx);
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(section.title.clone()),
                );
            let key_for_toggle = section_key.clone();
            list = list.child(h_flex().w_full().min_w_0().gap(px(2.0)).child(button).when(
                has_sub,
                |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!(
                                "rail-section-disclosure-{section_key}"
                            )))
                            .role(gpui::Role::Button)
                            .aria_label(format!("{} subsections", section.title))
                            .aria_expanded(sub_expanded)
                            .flex_shrink_0()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .cursor_pointer()
                            .on_press(cx, move |shell, _window, cx| {
                                let entry = shell
                                    .rail
                                    .expanded_sections
                                    .entry(key_for_toggle.clone())
                                    .or_insert(false);
                                *entry = !*entry;
                                cx.notify();
                            })
                            .child(settings_icon(
                                chevron(sub_expanded),
                                12.0,
                                if any_sub_active || section.active {
                                    p.foreground
                                } else {
                                    dim
                                },
                            )),
                    )
                },
            ));
            if sub_expanded {
                for (sub_id, sub_title, sub_active) in section.subsections {
                    let sub_target = sub_id.clone();
                    list = list.child(
                        div()
                            .id(SharedString::from(format!(
                                "rail-subsection-{}:{sub_id}",
                                tab.id()
                            )))
                            .role(gpui::Role::Button)
                            .aria_label(sub_title.clone())
                            .aria_selected(sub_active)
                            .w_full()
                            .min_w_0()
                            .h(px(32.0))
                            .pl(px(38.0))
                            .pr(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .cursor_pointer()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(if sub_active { p.foreground } else { dim }))
                            .when(sub_active, |this| this.font_weight(FontWeight::MEDIUM))
                            .when(!sub_active, |this| {
                                this.hover(|this| this.text_color(hsla(p.muted)))
                            })
                            .on_press(cx, move |shell, window, cx| {
                                shell.select_section(tab, &sub_target, window, cx);
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(sub_title),
                            ),
                    );
                }
            }
        }
        group = group.child(list);
    }
    group.into_any_element()
}

/// The Extensions group (`SettingsTabId::EXTENSIONS_GROUP`, CDXC:Settings 2026-10-19 in model.rs):
/// under a hairline, a header that only opens and closes it, and its pages indented below. A
/// search keeps it open. Collapsed over the open page, the header takes the selected fill.
fn render_extensions_group(
    p: &SettingsPalette,
    pages: Vec<AnyElement>,
    open: bool,
    searching: bool,
    holds_active: bool,
    cx: &mut Context<GpuiSettingsModalWindow>,
) -> AnyElement {
    let accent = p.raised_hover;
    let resting = p.foreground_alpha(0.82);
    let foreground = p.foreground;
    let selected = holds_active && !open;
    let header = h_flex()
        .id("rail-extensions-group")
        .role(gpui::Role::Button)
        .aria_label("Extensions")
        .aria_expanded(open)
        .aria_selected(selected)
        .accessibility_id("rail-extensions-group")
        .w_full()
        .min_w_0()
        .min_h(px(32.0))
        .pl(px(10.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(hsla(if selected { p.foreground } else { resting }))
        .when(selected, |this| {
            this.bg(hsla(accent)).font_weight(FontWeight::MEDIUM)
        })
        .hover(move |this| this.bg(hsla(accent)).text_color(hsla(foreground)))
        .on_press(cx, move |shell, _window, cx| {
            if searching {
                return;
            }
            let store = shell.store.clone();
            store.update(cx, |store, cx| store.toggle_extensions_group(cx));
        })
        .child(
            settings_icon(
                "modals/settings/puzzle.svg",
                16.0,
                if selected { p.foreground } else { resting },
            )
            .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child("Extensions"),
        )
        .child(
            div()
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .child(settings_icon(
                    chevron(open),
                    12.0,
                    if selected { p.foreground } else { p.muted },
                )),
        );
    v_flex()
        .w_full()
        .min_w_0()
        .pt(px(8.0))
        .gap(px(2.0))
        .border_t_1()
        .border_color(hsla(p.hairline))
        .child(header)
        .when(open, |this| {
            this.child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .pl(px(8.0))
                    .gap(px(4.0))
                    .children(pages),
            )
        })
        .into_any_element()
}

/// The rail.
pub(crate) fn render_rail(
    shell: &GpuiSettingsModalWindow,
    p: &SettingsPalette,
    pages: Vec<RailPage>,
    active_tab: SettingsTabId,
    show_advanced: bool,
    cx: &mut Context<GpuiSettingsModalWindow>,
) -> AnyElement {
    let accent = p.raised_hover;
    let (group_open, searching) = {
        let store = shell.store.read(cx);
        (
            store.is_searching() || store.extensions_group_open(),
            store.is_searching(),
        )
    };
    let mut groups: Vec<AnyElement> = Vec::new();
    let mut extension_pages: Vec<AnyElement> = Vec::new();
    let mut about: Option<AnyElement> = None;
    for page in pages {
        let tab = page.tab;
        let element = render_page(shell, p, page, active_tab, cx);
        if tab == SettingsTabId::About {
            about = Some(element);
        } else if tab.in_extensions_group() {
            extension_pages.push(element);
        } else {
            groups.push(element);
        }
    }
    let mut bottom = v_flex()
        .mt_auto()
        .pt(px(8.0))
        .w_full()
        .min_w_0()
        .gap(px(4.0));
    if !extension_pages.is_empty() {
        bottom = bottom.child(render_extensions_group(
            p,
            extension_pages,
            group_open,
            searching,
            active_tab.in_extensions_group(),
            cx,
        ));
    }
    groups.push(bottom.children(about).into_any_element());
    let footer = div()
        .w_full()
        .flex_shrink_0()
        .pt(px(10.0))
        .pr(px(RAIL_PADDING))
        .border_t_1()
        .border_color(hsla(p.hairline))
        .child(
            h_flex()
                .id("settings-show-advanced")
                .role(gpui::Role::Switch)
                .aria_label("Show Advanced")
                .aria_toggled(a11y_toggled(show_advanced))
                .accessibility_id("settings-show-advanced")
                .w_full()
                .min_h(px(36.0))
                .px(px(10.0))
                .py(px(6.0))
                .gap(px(10.0))
                .items_center()
                .justify_between()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(transparent())
                .cursor_pointer()
                .hover(move |this| this.bg(hsla(accent)))
                .on_press(cx, move |shell, _window, cx| {
                    let store = shell.store.clone();
                    store.update(cx, |store, cx| {
                        store.update_setting(
                            "showAdvancedSettings",
                            serde_json::json!(!show_advanced),
                            cx,
                        );
                    });
                })
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(hsla(p.foreground))
                        .child("Show Advanced"),
                )
                .child(settings_switch(p, show_advanced, false)),
        );
    let list_scroll = shell.rail_scroll.clone();
    v_flex()
        .w(px(RAIL_WIDTH))
        .flex_shrink_0()
        .h_full()
        .mt(px(-1.0))
        .pl(px(RAIL_PADDING))
        .py(px(10.0))
        .gap(px(12.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.raised))
        .overflow_hidden()
        // The bar sits beside the scroll area, as on the page (CDXC:Settings 2026-10-08 in page.rs),
        // and the rows keep a right gutter inside the scroll area (the panel's old right padding),
        // so the bar never covers a row, its chevron or its hover background.
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(
                    div()
                        .id("settings-rail-list")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&list_scroll)
                        .child(
                            v_flex()
                                .w_full()
                                .min_h_full()
                                .pr(px(RAIL_SCROLLBAR_GUTTER))
                                .gap(px(4.0))
                                .children(groups),
                        ),
                )
                .child(app_vertical_scrollbar(&list_scroll)),
        )
        .child(footer)
        .into_any_element()
}

/// The rail panel's side padding.
const RAIL_PADDING: f32 = 8.0;

/// The rows' right gutter inside the rail's scroll area: room for the scrollbar's thumb, 4px in
/// from the panel's edge and 8px wide while hovered, plus a 2px gap before the rows.
const RAIL_SCROLLBAR_GUTTER: f32 = 14.0;

/// `SettingsSearchNoMatchesNotice`: what a page shows when the search leaves nothing on it, with
/// buttons to the pages that do match.
pub(crate) fn render_no_matches(
    p: &SettingsPalette,
    active_tab: SettingsTabId,
    matching: &[SettingsTabId],
    on_select: impl Fn(SettingsTabId, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
) -> AnyElement {
    let others: Vec<SettingsTabId> = matching
        .iter()
        .copied()
        .filter(|tab| *tab != active_tab)
        .collect();
    let muted_fill = if p.light {
        gpui::rgb(0xf1f1f1)
    } else {
        gpui::rgb(0x262626)
    };
    v_flex()
        .w_full()
        .items_center()
        .px(px(16.0))
        .py(px(24.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(css_fade(muted_fill, 0.3)))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.muted))
        .child(if others.is_empty() {
            "No settings match your search."
        } else {
            "No settings on this page match your search."
        })
        .when(!others.is_empty(), |this| {
            this.child(
                h_flex()
                    .mt(px(12.0))
                    .flex_wrap()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child("Matches on:")
                    .children(others.into_iter().map(|tab| {
                        let on_select = on_select.clone();
                        let fills = neutral_button_fills(p.light);
                        h_flex()
                            .id(SharedString::from(format!("no-matches-{}", tab.id())))
                            .role(gpui::Role::Button)
                            .aria_label(tab.title())
                            .h(px(32.0))
                            .pl(px(10.0))
                            .pr(px(12.0))
                            .gap(px(6.0))
                            .items_center()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .border_1()
                            .border_color(transparent())
                            .bg(hsla(fills.rest))
                            .text_color(hsla(p.foreground))
                            .cursor_pointer()
                            .hover(move |this| this.bg(hsla(fills.hover)))
                            .active(move |this| this.bg(hsla(fills.pressed)))
                            .on_click(move |_: &ClickEvent, window, cx| on_select(tab, window, cx))
                            .child(settings_icon(tab.icon(), 16.0, p.foreground))
                            .child(tab.title())
                    })),
            )
        })
        .into_any_element()
}
