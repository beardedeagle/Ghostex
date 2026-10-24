use gpui::{Bounds, Hsla, IntoElement, Pixels, Styled as _, Window, point, px, size, svg};
use gpui_component::tooltip::Tooltip;

use crate::*;

pub(crate) fn titlebar_svg_icon(
    path: &'static str,
    icon_size: f32,
    color: Hsla,
) -> impl IntoElement {
    svg().size(px(icon_size)).path(path).text_color(color)
}

/// CDXC:Titlebar 2026-09-19 DECISION:
/// User: a titlebar button tooltip names the button and then the hotkey assigned to it in Settings > Hotkeys, so rebinding the action there changes the tooltip too. Actions left unassigned show the label alone.
pub(crate) fn titlebar_tooltip_label(label: &str, action_id: &str) -> gpui::SharedString {
    match gpui_configured_hotkey_label(action_id) {
        Some(shortcut) if !shortcut.is_empty() => format!("{label} ({shortcut})").into(),
        _ => label.to_string().into(),
    }
}

/// Tooltip sized to sit fully inside the titlebar strip rather than
/// overflowing it, for the left/right placements that center on the trigger.
pub(crate) fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text)
        .my_0()
        .py_0()
        .h(px(TITLEBAR_TOOLTIP_HEIGHT))
        .text_size(px(TITLEBAR_TOOLTIP_TEXT_SIZE))
        .line_height(px(TITLEBAR_TOOLTIP_LINE_HEIGHT))
        .whitespace_nowrap()
        .build(window, cx)
}

/// How far below the pointer a list row's tooltip starts: past the pointer's arrow, so the
/// bubble never sits under the pointer's tip.
const LIST_ROW_TOOLTIP_DROP: f32 = 20.0;

/// The tooltip for a row in a list (a file's path, say): the titlebar tooltip's text, wrapped to
/// the room beside the pointer instead of cut at the list's edge, and dropped below the pointer.
///
/// CDXC:Tooltips 2026-10-09 DECISION:
/// User, of the files list's path tooltips: "the tooltip here should appear below the button of the current element so I can't hover on it (it blocks clicks)" and "this tooltip is showing cut off when the files list is not floating". A row's tooltip opens below the pointer, clear of its arrow, and wraps to the room it has (the floating list's own window, or the space beside a native pane) instead of running one line past the edge; its window never takes the pointer (CDXC:Tooltips 2026-10-09 in titlebar_popup_chrome.rs). `titlebar_tooltip` stays one fixed-height line for the titlebar strip.
pub(crate) fn list_row_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text)
        .mt(px(LIST_ROW_TOOLTIP_DROP))
        .mb_0()
        .text_size(px(TITLEBAR_TOOLTIP_TEXT_SIZE))
        .line_height(px(TITLEBAR_TOOLTIP_LINE_HEIGHT))
        .build(window, cx)
}

pub(crate) fn titlebar_popup_menu_width(kind: GpuiTitlebarPopupKind) -> f32 {
    match kind {
        GpuiTitlebarPopupKind::AccountUsage(_) => 380.0,
        GpuiTitlebarPopupKind::RemoteSites => TITLEBAR_POPUP_RESOURCES_WIDTH,
        GpuiTitlebarPopupKind::SessionLinks => {
            crate::app::native_sidebar::links_list::SESSION_LINKS_LIST_WIDTH
        }
        GpuiTitlebarPopupKind::Actions
        | GpuiTitlebarPopupKind::ContextMenu
        | GpuiTitlebarPopupKind::BrowserActions(_)
        | GpuiTitlebarPopupKind::OpenTargets => TITLEBAR_POPUP_COMPACT_WIDTH,
        GpuiTitlebarPopupKind::BrowserDeviceView(_) => TITLEBAR_POPUP_BROWSER_DEVICE_VIEW_WIDTH,
        GpuiTitlebarPopupKind::Extensions => TITLEBAR_POPUP_EXTENSIONS_WIDTH,
        GpuiTitlebarPopupKind::Git => TITLEBAR_POPUP_GIT_WIDTH,
        GpuiTitlebarPopupKind::Help => TITLEBAR_POPUP_HELP_WIDTH,
        GpuiTitlebarPopupKind::More => TITLEBAR_POPUP_COMPACT_WIDTH,
        GpuiTitlebarPopupKind::Notifications => TITLEBAR_POPUP_NOTIFICATIONS_WIDTH,
        GpuiTitlebarPopupKind::Resources => TITLEBAR_POPUP_RESOURCES_WIDTH,
        GpuiTitlebarPopupKind::Tips => TITLEBAR_POPUP_TIPS_WIDTH,
    }
}

pub(crate) fn titlebar_popup_menu_height_for_rows(row_heights: &[f32]) -> f32 {
    titlebar_popup_menu_height_for_rows_with_chrome(
        row_heights,
        TITLEBAR_POPUP_MENU_VERTICAL_CHROME,
    )
}

pub(crate) fn titlebar_popup_menu_height_for_rows_with_chrome(
    row_heights: &[f32],
    vertical_chrome: f32,
) -> f32 {
    let rows: f32 = row_heights.iter().sum();
    let gaps = TITLEBAR_POPUP_MENU_ITEM_GAP * row_heights.len().saturating_sub(1) as f32;
    rows + gaps + vertical_chrome
}

pub(crate) fn titlebar_popup_window_bounds_for_trigger_bounds(
    kind: GpuiTitlebarPopupKind,
    trigger_bounds: Bounds<Pixels>,
    content_height: f32,
    width: f32,
    window: &Window,
) -> Bounds<Pixels> {
    let main_window_bounds = window.bounds();
    /*
    CDXC:AgentProviders 2026-09-20 WHY:
    Account usage used to be pinned to the top of the window under the titlebar
    because its trigger was a titlebar button. Its meter now lives at the bottom
    of the sidebar, so it takes the ordinary trigger-relative path: it grows to
    the right of the meter and flips above it when there is no room below.
    */
    /*
    CDXC:ContextMenus 2026-09-26 DECISION:
    User: a context menu with many options is not scrollable. It grows to its full height and only
    the window itself limits it, so the dropdown cap below applies to header dropdowns alone.
    */
    let max_height = match kind {
        GpuiTitlebarPopupKind::ContextMenu => f32::INFINITY,
        GpuiTitlebarPopupKind::AccountUsage(_) => 640.0,
        GpuiTitlebarPopupKind::Notifications => TITLEBAR_POPUP_NOTIFICATIONS_MAX_HEIGHT,
        GpuiTitlebarPopupKind::Resources
        | GpuiTitlebarPopupKind::Tips
        | GpuiTitlebarPopupKind::RemoteSites => TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT,
        _ => TITLEBAR_POPUP_MENU_MAX_HEIGHT,
    };
    let available_height = (main_window_bounds.size.height.as_f32() - 28.0).max(180.0);
    let height = content_height.min(max_height).min(available_height);
    let horizontal_margin = 8.0;
    let min_left = main_window_bounds.origin.x.as_f32() + horizontal_margin;
    let max_left = main_window_bounds.origin.x.as_f32() + main_window_bounds.size.width.as_f32()
        - width
        - horizontal_margin;
    // The Notifications bell and the account usage meters sit at the left edge of
    // the window, in the sidebar, so their dropdowns grow to the right from the
    // trigger like a context menu instead of hanging off the trigger's right edge
    // like the titlebar's right-region buttons.
    let desired_left = main_window_bounds.origin.x.as_f32()
        + if matches!(
            kind,
            GpuiTitlebarPopupKind::AccountUsage(_)
                | GpuiTitlebarPopupKind::ContextMenu
                | GpuiTitlebarPopupKind::Notifications
                | GpuiTitlebarPopupKind::SessionLinks
        ) {
            trigger_bounds.left().as_f32()
        } else {
            trigger_bounds.top_right().x.as_f32() - width
        };
    let left = desired_left.clamp(min_left, max_left.max(min_left));
    // A menu dropping from a button keeps the header gap; a context menu anchored to the pointer
    // (a zero-height trigger) opens right at it.
    let trigger_gap = if trigger_bounds.size.height > px(1.0) {
        HEADER_MENU_TRIGGER_GAP
    } else {
        0.0
    };
    let below_top =
        main_window_bounds.origin.y.as_f32() + trigger_bounds.bottom().as_f32() + trigger_gap;
    let above_top =
        main_window_bounds.origin.y.as_f32() + trigger_bounds.top().as_f32() - trigger_gap - height;
    let bottom_limit = main_window_bounds.origin.y.as_f32()
        + main_window_bounds.size.height.as_f32()
        - horizontal_margin;
    let top =
        if below_top + height <= bottom_limit || above_top < main_window_bounds.origin.y.as_f32() {
            below_top
        } else {
            above_top
        };

    // A trigger near the bottom of the window (the sidebar usage strip) leaves no
    // room either below or fully above it, so the panel is held inside the window
    // the same way a context menu is.
    let top = if matches!(
        kind,
        GpuiTitlebarPopupKind::AccountUsage(_) | GpuiTitlebarPopupKind::ContextMenu
    ) {
        let min_top = main_window_bounds.origin.y.as_f32() + horizontal_margin;
        top.clamp(min_top, (bottom_limit - height).max(min_top))
    } else {
        top
    };

    Bounds {
        origin: point(px(left), px(top)),
        size: size(px(width), px(height)),
    }
}
