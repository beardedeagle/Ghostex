//! The frosted child windows the app's menus and tooltips draw in while the main window is glass.
//!
//! CDXC:Theming 2026-09-25 DECISION:
//! User, of the sidebar menu and the header's ⋯ menu reading as near-opaque dark boxes: "is it possible to make context menu and these menus in the app match the look of the app when transparency is enabled better?", then "ok do ur best option / apply this to all the menus and tooltips like the ... and sidebar menu etc". Under window glass every menu and tooltip is a frosted surface: it draws in a blurred window of its own, filled with the frosted menu fill (`frosted_menu_fill`), with a faint ink border and the soft ink wash on its highlighted row. Menus that already had a window (the header's dropdowns, context menus, the chat's menus) keep it; the sidebar's menus and every tooltip, which were drawn inside the main window where nothing can blur, move into the windows kept here. Glass off, they all look as before.
//!
//! CDXC:Theming 2026-09-25 WHY:
//! Only one menu and one tooltip are ever up, so each kind keeps one window and hides it between uses rather than opening a new one each time (a GPUI window pays for a new Metal surface; tooltips come and go on every hover). A window's blur is limited to the rounded frames its content reports (`Window::report_frosted_region`), so a menu with a submenu, or a tooltip bubble with its margins, blurs only its panels. A window that is replaced is closed first, and nothing here holds its owner alive, so a leftover can never outlive what it showed (the lesson of the stacked scroll pills).

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, EntityId, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, Subscription, Window, WindowBounds,
    WindowHandle, WindowOptions, div,
};

use crate::app::helpers::window_glass_active;

/// What a frosted host window shows, rendered in that window.
pub(crate) type FrostedContent = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrostedHostKind {
    Tooltip,
    /// One panel of the sidebar's menu: the menu itself at 0, each submenu stacked above it.
    SidebarMenu(u8),
    /// The Docs toolbar over selected text (`native_docs/notes_windows.rs`).
    DocsSelectionToolbar,
    /// A Quick Access filter's picker (`window/quick_access/chrome.rs`), drawn over the Quick
    /// Access window rather than the main one.
    QuickAccessPicker,
    /// A Quick Access row's actions menu or its Actions panel (`window/quick_access/actions_menu.rs`).
    QuickAccessActions,
    /// The sidebar's account usage strip while it peeks over the list unpinned
    /// (`native_sidebar/usage.rs`).
    SidebarUsage,
    /// A native app modal's open dropdown (`window/modal_popover_host.rs`), drawn over the modal's
    /// window rather than the main one.
    ModalPopover,
}

/// How many stacked sidebar menu panels get a window of their own; deeper ones share none.
pub(crate) const SIDEBAR_MENU_HOST_LEVELS: u8 = 4;

/// The corner radius of a sidebar menu panel's window (the panel's own 8px at 100% zoom).
pub(crate) const SIDEBAR_MENU_HOST_RADIUS: f32 = 8.0;

/// The corner radius of the sidebar's peeking usage strip, which its window's blur takes too.
pub(crate) const SIDEBAR_USAGE_HOST_RADIUS: f32 = 8.0;

/// The corner radius of the Docs selection toolbar, which its window's blur takes too.
pub(crate) const DOCS_SELECTION_TOOLBAR_RADIUS: f32 = 8.0;

/// Whether menus and tooltips drawn inside the main window move into frosted host windows now.
/// macOS and Windows: a tooltip host's blur follows its bubble (a blur mask on macOS, the
/// window's own region on Windows); Linux keeps them in the window with their solid fill.
///
/// CDXC:Theming 2026-09-27 DECISION:
/// User: "same for ones that should be glass on windows but they're not". The frosted host windows (the sidebar's menus and usage strip, tooltips, modal dropdowns, Quick Access pickers) run on Windows too, where a pop-up window is already non-activating and blurred; supersedes the macOS-only rule.
pub(crate) fn frosted_hosting_active() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows")) && window_glass_active()
}

#[derive(Default)]
struct HostSlot {
    handle: Option<WindowHandle<FrostedHostView>>,
    /// The window the open host is attached to.
    parent: Option<AnyWindowHandle>,
    /// The frame on screen, in the parent's content coordinates, or `None` while hidden.
    shown: Option<Bounds<Pixels>>,
    /// Where the latest request wants it (`None` hides it).
    wanted: Option<(AnyWindowHandle, Bounds<Pixels>)>,
    content: Option<FrostedContent>,
    /// Identifies the content, so the same tooltip reported again changes nothing.
    content_key: Option<EntityId>,
    /// Content that must repaint when this entity notifies (a menu drawn from the app's state).
    observe: Option<gpui::Entity<crate::GhostexGpuiApp>>,
    busy: bool,
}

thread_local! {
    static TOOLTIP_HOST: RefCell<HostSlot> = RefCell::default();
    static DOCS_SELECTION_TOOLBAR_HOST: RefCell<HostSlot> = RefCell::default();
    static QUICK_ACCESS_PICKER_HOST: RefCell<HostSlot> = RefCell::default();
    static QUICK_ACCESS_ACTIONS_HOST: RefCell<HostSlot> = RefCell::default();
    static SIDEBAR_USAGE_HOST: RefCell<HostSlot> = RefCell::default();
    static MODAL_POPOVER_HOST: RefCell<HostSlot> = RefCell::default();
    static SIDEBAR_MENU_HOSTS: RefCell<Vec<HostSlot>> = RefCell::default();
}

fn with_slot<R>(kind: FrostedHostKind, f: impl FnOnce(&mut HostSlot) -> R) -> R {
    match kind {
        FrostedHostKind::Tooltip => TOOLTIP_HOST.with(|slot| f(&mut slot.borrow_mut())),
        FrostedHostKind::DocsSelectionToolbar => {
            DOCS_SELECTION_TOOLBAR_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::QuickAccessPicker => {
            QUICK_ACCESS_PICKER_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::QuickAccessActions => {
            QUICK_ACCESS_ACTIONS_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::SidebarUsage => SIDEBAR_USAGE_HOST.with(|slot| f(&mut slot.borrow_mut())),
        FrostedHostKind::ModalPopover => MODAL_POPOVER_HOST.with(|slot| f(&mut slot.borrow_mut())),
        FrostedHostKind::SidebarMenu(level) => SIDEBAR_MENU_HOSTS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let level = usize::from(level);
            if slots.len() <= level {
                slots.resize_with(level + 1, HostSlot::default);
            }
            f(&mut slots[level])
        }),
    }
}

/// Shows `content` in `kind`'s host at `frame` (in `parent`'s content coordinates). `key`
/// identifies the content: a request with the same key and frame as the last one does nothing.
/// `observe` repaints the host whenever that entity notifies. Safe to call while drawing.
pub(crate) fn show_frosted_host(
    kind: FrostedHostKind,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    key: Option<EntityId>,
    content: FrostedContent,
    observe: Option<gpui::Entity<crate::GhostexGpuiApp>>,
    cx: &mut App,
) {
    let schedule = with_slot(kind, |slot| {
        let same = key.is_some()
            && slot.content_key == key
            && slot.wanted == Some((parent, frame))
            && slot.content.is_some();
        if same {
            return false;
        }
        slot.content = Some(content);
        slot.content_key = key;
        slot.observe = observe;
        slot.wanted = Some((parent, frame));
        !std::mem::replace(&mut slot.busy, true)
    });
    if schedule {
        cx.defer(move |cx| apply(kind, cx));
    }
}

/// Hides `kind`'s host, keeping its window for the next use. Safe to call while drawing, and
/// cheap when it is already hidden.
pub(crate) fn hide_frosted_host(kind: FrostedHostKind, cx: &mut App) {
    let schedule = with_slot(kind, |slot| {
        if slot.wanted.is_none() && slot.content.is_none() {
            return false;
        }
        slot.wanted = None;
        slot.content = None;
        slot.content_key = None;
        slot.observe = None;
        !std::mem::replace(&mut slot.busy, true)
    });
    if schedule {
        cx.defer(move |cx| apply(kind, cx));
    }
}

/// Hides `kind`'s host only while it sits over `parent`, leaving a host over another window alone.
pub(crate) fn hide_frosted_host_over(kind: FrostedHostKind, parent: AnyWindowHandle, cx: &mut App) {
    if with_slot(kind, |slot| {
        slot.wanted.is_some_and(|(over, _)| over == parent)
    }) {
        hide_frosted_host(kind, cx);
    }
}

/// Hides every host sitting over `parent`, which is closing, so none keeps its app alive or stays
/// on screen without it (app/workspace_windows/).
#[allow(dead_code)] // the GPUI web build compiles this file and has a single window
pub(crate) fn hide_frosted_hosts_over(parent: AnyWindowHandle, cx: &mut App) {
    let menu_levels = SIDEBAR_MENU_HOSTS.with(|slots| slots.borrow().len());
    let kinds = [
        FrostedHostKind::Tooltip,
        FrostedHostKind::DocsSelectionToolbar,
        FrostedHostKind::QuickAccessPicker,
        FrostedHostKind::QuickAccessActions,
        FrostedHostKind::SidebarUsage,
        FrostedHostKind::ModalPopover,
    ]
    .into_iter()
    .chain(
        (0..menu_levels)
            .filter_map(|level| u8::try_from(level).ok().map(FrostedHostKind::SidebarMenu)),
    );
    for kind in kinds {
        hide_frosted_host_over(kind, parent, cx);
    }
}

fn apply(kind: FrostedHostKind, cx: &mut App) {
    loop {
        let (wanted, handle, parent, shown) = with_slot(kind, |slot| {
            (slot.wanted, slot.handle, slot.parent, slot.shown)
        });
        let handle = handle.filter(|handle| handle.update(cx, |_, _, _| ()).is_ok());
        match wanted {
            None => {
                if let Some(handle) = handle
                    && shown.is_some()
                {
                    set_visible(handle.into(), None, false, cx);
                }
                with_slot(kind, |slot| {
                    slot.handle = handle;
                    slot.shown = None;
                });
            }
            Some((target, frame)) => {
                if let Some(handle) = handle
                    && parent == Some(target)
                {
                    if shown != Some(frame) {
                        move_host(handle.into(), target, frame, cx);
                    }
                    if shown.is_none() {
                        // After the move above, which also runs from a task, so a reused window
                        // never shows at its last spot first.
                        cx.spawn(async move |cx| {
                            cx.update(|cx| set_visible(handle.into(), Some(target), true, cx));
                        })
                        .detach();
                    }
                    let observe = with_slot(kind, |slot| slot.observe.clone());
                    let _ = handle.update(cx, |view, _, cx| {
                        view.observe(observe, cx);
                        cx.notify();
                    });
                    with_slot(kind, |slot| slot.shown = Some(frame));
                } else {
                    if let Some(handle) = handle {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    let opened = open_host(kind, target, frame, cx);
                    with_slot(kind, |slot| {
                        slot.handle = opened;
                        slot.parent = opened.map(|_| target);
                        slot.shown = opened.map(|_| frame);
                    });
                }
            }
        }
        // A request that arrived while this one was applied is applied next.
        let done = with_slot(kind, |slot| {
            let done = slot.wanted == wanted;
            if done {
                slot.busy = false;
            }
            done
        });
        if done {
            return;
        }
    }
}

fn open_host(
    kind: FrostedHostKind,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    cx: &mut App,
) -> Option<WindowHandle<FrostedHostView>> {
    let parent_view = native_view_of(parent, cx)?;
    let (origin, owner) = parent
        .update(cx, |_, window, cx| {
            (
                crate::app::native_chat::child_window::content_bounds(window).origin,
                crate::app::window::popup_frame::PopupOwner::of(window, cx),
            )
        })
        .ok()?;
    let screen_frame = Bounds::new(origin + frame.origin, frame.size);
    let display_id = owner.display_for(screen_frame, cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(screen_frame)),
            display_id,
            titlebar: None,
            kind: gpui::WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            app_id: crate::gpui_platform_window_app_id(),
            icon: crate::gpui_platform_window_icon(),
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            ..Default::default()
        },
        move |window, cx| {
            crate::app::helpers::apply_frosted_menu_blur(window);
            match kind {
                // A tooltip bubble sits inside margins, so its window's blur follows the bubble.
                FrostedHostKind::Tooltip => window.set_frosted_surface(true),
                // A menu panel fills its window exactly, the way a header dropdown does, so the
                // window is simply rounded to the panel's corners.
                FrostedHostKind::SidebarMenu(_) => {
                    window.set_background_corner_radius(gpui::px(SIDEBAR_MENU_HOST_RADIUS))
                }
                FrostedHostKind::DocsSelectionToolbar => {
                    window.set_background_corner_radius(gpui::px(DOCS_SELECTION_TOOLBAR_RADIUS))
                }
                FrostedHostKind::QuickAccessPicker => {
                    window.set_background_corner_radius(gpui::px(
                        crate::app::window::quick_access::palette::QUICK_ACCESS_RADIUS_CONTROL,
                    ))
                }
                FrostedHostKind::QuickAccessActions => {
                    window.set_background_corner_radius(gpui::px(
                        crate::app::window::quick_access::palette::QUICK_ACCESS_RADIUS_ACTIONS_MENU,
                    ))
                }
                FrostedHostKind::SidebarUsage => {
                    window.set_background_corner_radius(gpui::px(SIDEBAR_USAGE_HOST_RADIUS))
                }
                FrostedHostKind::ModalPopover => window.set_background_corner_radius(gpui::px(
                    super::native_modal_kit::MODAL_RADIUS_CONTROL,
                )),
            }
            attach_host_window(window, parent_view, kind);
            let observe = with_slot(kind, |slot| slot.observe.clone());
            cx.new(|cx| {
                let mut view = FrostedHostView {
                    kind,
                    _observe: None,
                };
                view.observe(observe, cx);
                view
            })
        },
    )
    .ok()
}

pub(crate) struct FrostedHostView {
    kind: FrostedHostKind,
    _observe: Option<Subscription>,
}

impl FrostedHostView {
    fn observe(
        &mut self,
        entity: Option<gpui::Entity<crate::GhostexGpuiApp>>,
        cx: &mut Context<Self>,
    ) {
        self._observe = entity.map(|entity| cx.observe(&entity, |_, _, cx| cx.notify()));
    }
}

impl Render for FrostedHostView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The host window is reused, so it follows the Menu blur slider here rather than at open
        // (CDXC:Theming 2026-10-15 on `apply_frosted_menu_blur`).
        #[cfg(target_os = "macos")]
        crate::app::helpers::apply_frosted_menu_blur(window);
        // A sidebar menu's host hands its tooltips (the New Orchestrator… (i)) to the frosted
        // tooltip window, attached above this one, so the bubble is neither clipped to the menu's
        // window nor drawn under it.
        if matches!(self.kind, FrostedHostKind::SidebarMenu(_)) {
            sync_frosted_tooltip_presenter(window, cx);
        }
        let content = with_slot(self.kind, |slot| slot.content.clone());
        let mut host = div().size_full();
        if let Some(content) = content {
            host = host.child(content(window, cx));
        }
        host
    }
}

fn native_view_of(window: AnyWindowHandle, cx: &mut App) -> Option<*mut std::ffi::c_void> {
    window
        .update(cx, |_, window, _| native_view(window))
        .ok()
        .flatten()
}

#[cfg(target_os = "macos")]
fn native_view(window: &Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn native_view(window: &Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_view(_: &Window) -> Option<*mut std::ffi::c_void> {
    None
}

/// Moves an open host onto `frame` in `parent`'s content coordinates.
#[cfg(target_os = "macos")]
fn move_host(
    handle: AnyWindowHandle,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    cx: &mut App,
) {
    let parent_view = native_view_of(parent, cx);
    crate::app::native_chat::child_window::move_child_window(
        handle,
        parent_view.unwrap_or(std::ptr::null_mut()),
        frame,
        cx,
    );
}

/// Moves an open host onto `frame` in `parent`'s content coordinates, without activating it or
/// changing its stacking (a non-activating pop-up keeps the parent's keyboard).
///
/// CDXC:ContextMenus 2026-10-02 WHY:
/// `SetWindowPos` delivers `WM_SIZE` synchronously, and GPUI's resize callback updates the window's layout size through the app. Called straight from `apply` (a deferred callback that holds the app) that update failed, so a reused menu host grew to the new menu's frame but kept laying out at the previous menu's size: the agent launcher drew inside the compact session menu's 178px box and lost its last rows (New Coordinator…, Configure). The move runs from a task, as the macOS host's does, so the app is free when `WM_SIZE` arrives.
#[cfg(target_os = "windows")]
fn move_host(
    handle: AnyWindowHandle,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    cx: &mut App,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };
    cx.spawn(async move |cx| {
        let Some(origin) = parent
            .update(cx, |_, window, _| {
                crate::app::native_chat::child_window::content_bounds(window).origin
            })
            .ok()
        else {
            return;
        };
        let Some((hwnd, scale)) = handle
            .update(cx, |_, window, _| {
                native_view(window).map(|hwnd| (hwnd, window.scale_factor()))
            })
            .ok()
            .flatten()
        else {
            return;
        };
        let screen = Bounds::new(origin + frame.origin, frame.size);
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                device(screen.origin.x),
                device(screen.origin.y),
                device(screen.size.width),
                device(screen.size.height),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            );
        }
    })
    .detach();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn move_host(_: AnyWindowHandle, _: AnyWindowHandle, _: Bounds<Pixels>, _: &mut App) {}

/// Attaches the host above its parent without ever taking key status from it (the same attachment
/// the composer's suggestions use), and lets a tooltip pass the mouse through.
#[cfg(target_os = "macos")]
fn attach_host_window(window: &mut Window, parent: *mut std::ffi::c_void, kind: FrostedHostKind) {
    let ignores_mouse = kind == FrostedHostKind::Tooltip;
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
        fn GhostexGpuiSetWindowIgnoresMouse(view: *mut std::ffi::c_void, ignores: bool);
        fn GhostexGpuiSetWindowIdentifier(
            view: *mut std::ffi::c_void,
            identifier: *const std::ffi::c_char,
        );
    }
    if let Some(view) = native_view(window) {
        unsafe {
            GhostexGpuiAttachComposerSuggestionsWindow(view, parent);
            // Set either way: left at its default, AppKit passes a click through any part of a
            // non-opaque window it sees as transparent, which is how clicks on the frosted menu's
            // rows fell through to the sidebar underneath.
            GhostexGpuiSetWindowIgnoresMouse(view, ignores_mouse);
            if matches!(kind, FrostedHostKind::SidebarMenu(_)) {
                // The sidebar's outside-click monitor must not read a press here as leaving the menu.
                GhostexGpuiSetWindowIdentifier(view, c"ghostex.frostedSidebarMenu".as_ptr());
            }
        }
    }
}

/// On Windows a host is a pop-up owned by its parent window (`own_gpui_popup_window`)
/// (`WindowKind::PopUp` with `focus: false`). A tooltip's window is clipped to its bubble
/// (`set_frosted_surface`), and the pointer passes through the bubble itself
/// (`make_gpui_popup_window_click_through`), as on macOS.
///
/// CDXC:ContextMenus 2026-09-28 WHY:
/// GPUI activates even a `focus: false` pop-up when it is clicked, so a click on a frosted menu row deactivated the owner window first, and the owner's deactivation observer (which closes the sidebar menu when Ghostex loses focus) took the menu down before the click reached the row. The host keeps activation on its owner, as the macOS host and the titlebar dropdowns do, and as every frosted surface expects: the owner keeps the keyboard.
#[cfg(target_os = "windows")]
fn attach_host_window(window: &mut Window, parent: *mut std::ffi::c_void, kind: FrostedHostKind) {
    if kind == FrostedHostKind::Tooltip {
        super::make_gpui_popup_window_click_through(window);
    } else {
        super::make_gpui_popup_window_non_activating(window);
    }
    super::own_gpui_popup_window(window, parent);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn attach_host_window(_: &mut Window, _: *mut std::ffi::c_void, _: FrostedHostKind) {}

#[cfg(target_os = "macos")]
fn set_visible(
    handle: AnyWindowHandle,
    parent: Option<AnyWindowHandle>,
    visible: bool,
    cx: &mut App,
) {
    unsafe extern "C" {
        fn GhostexGpuiSetFrostedChildWindowVisible(
            child: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
            visible: bool,
        );
    }
    let parent_view = parent.and_then(|parent| native_view_of(parent, cx));
    if let Some(view) = native_view_of(handle, cx) {
        unsafe {
            GhostexGpuiSetFrostedChildWindowVisible(
                view,
                parent_view.unwrap_or(std::ptr::null_mut()),
                visible,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn set_visible(
    handle: AnyWindowHandle,
    _parent: Option<AnyWindowHandle>,
    visible: bool,
    cx: &mut App,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};
    if let Some(hwnd) = native_view_of(handle, cx) {
        unsafe {
            ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_visible(_: AnyWindowHandle, _: Option<AnyWindowHandle>, _: bool, _: &mut App) {}

/// Hands a window's tooltips to the frosted tooltip host while glass is on, and takes them back
/// when it is off. Called on every root render (the main window's glass sync, and every other
/// window through [`FrostedTooltipRootPlugin`]).
///
/// CDXC:Theming 2026-09-27 WHY:
/// The user saw sidebar tooltips "appearing and disappearing multiple times per second when transparency is enabled". Every window with a presenter reports each frame it draws, `None` when it shows no tooltip, and that `None` hid the one shared tooltip host even when the tooltip belonged to another window; any other window redrawing (a Docs window, a frosted chat control, a modal) took the main window's tooltip down and the next main-window frame put it back. A window now hides the host only while the host shows its own tooltip.
pub(crate) fn sync_frosted_tooltip_presenter(window: &mut Window, cx: &mut App) {
    let wanted = frosted_hosting_active();
    if window.tooltip_presenter_active() == wanted {
        return;
    }
    if !wanted {
        window.set_tooltip_presenter(None);
        hide_frosted_host(FrostedHostKind::Tooltip, cx);
        return;
    }
    let parent = window.window_handle();
    window.set_tooltip_presenter(Some(Rc::new(
        move |presentation, cx: &mut App| match presentation {
            Some((view, bounds)) => {
                let key = view.entity_id();
                show_frosted_host(
                    FrostedHostKind::Tooltip,
                    parent,
                    bounds,
                    Some(key),
                    Rc::new(move |_, _| div().size_full().child(view.clone()).into_any_element()),
                    None,
                    cx,
                );
            }
            // Only the window whose tooltip is up may take it down: every window with a presenter
            // reports `None` on each frame it draws without a tooltip.
            None => hide_frosted_host_over(FrostedHostKind::Tooltip, parent, cx),
        },
    )));
}

/// Gives every other GPUI window's tooltips to the frosted tooltip host too, the way the main
/// window's own root does (`render/root.rs`): the hover-out sidebar panel, the app modals, the
/// header's dropdowns and panels, and the chat's pop-up windows. Registered once as a
/// gpui-component root plugin, so it runs on each window root's render; the frosted host windows
/// themselves draw without a root, so a tooltip never hosts itself.
///
/// CDXC:Theming 2026-09-27 DECISION:
/// User: "yes pls make all those that could be glass glass". Under window glass a tooltip is frosted in every Ghostex window, not only the main one: each window hands its tooltips to the shared frosted tooltip host, which moves above whichever window asked.
pub(crate) struct FrostedTooltipRootPlugin {
    /// Hides the tooltip host when this window goes away while one of its tooltips is up.
    _release: Subscription,
}

impl gpui_component::RootPlugin for FrostedTooltipRootPlugin {
    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        sync_frosted_tooltip_presenter(window, cx);
    }
}

impl Render for FrostedTooltipRootPlugin {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Registers [`FrostedTooltipRootPlugin`] for every window opened from now on.
pub(crate) fn register_frosted_tooltip_root_plugin(cx: &mut App) {
    gpui_component::Root::register_plugin::<FrostedTooltipRootPlugin>(cx, |window, cx| {
        let parent = window.window_handle();
        FrostedTooltipRootPlugin {
            _release: cx.on_release(move |_, cx| {
                hide_frosted_host_over(FrostedHostKind::Tooltip, parent, cx);
                super::modal_popover_host::forget_modal_popover_over(parent, cx);
            }),
        }
    });
}
