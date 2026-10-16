//! The pages whose open dialog or form Settings brings back on its next open
//! (CDXC:Settings 2026-10-16 in settings_modal/restore.rs). A page reports its open form as JSON
//! (`RemembersOpenForm`), takes it back once from `OpenFormMemory::take` when its view is made,
//! and notifies whenever the form's draft changes, which is when the shell saves it.
use super::{open_targets, remote};
use gpui::{AnyView, App, Context, Subscription};
use serde_json::Value;

/// A page with a dialog or form worth reopening where the user left it.
pub(crate) trait RemembersOpenForm: 'static {
    /// The open dialog or form and its draft (never a password), or `None` when none is open.
    fn open_form(&self, cx: &App) -> Option<Value>;
}

/// The open form of page `view` (`RemembersOpenForm`); pages without one report `None`.
pub(crate) fn settings_tab_open_form(view: &AnyView, cx: &App) -> Option<Value> {
    fn form<T: RemembersOpenForm>(view: &AnyView, cx: &App) -> Option<Option<Value>> {
        let page = view.clone().downcast::<T>().ok()?;
        Some(page.read(cx).open_form(cx))
    }
    form::<remote::RemoteTab>(view, cx)
        .or_else(|| form::<open_targets::OpenTargetsTab>(view, cx))
        .flatten()
}

/// Runs `changed` whenever page `view`, if it remembers an open form, notifies.
pub(crate) fn observe_open_form<S: 'static>(
    view: &AnyView,
    cx: &mut Context<S>,
    changed: fn(&mut S, &mut Context<S>),
) -> Option<Subscription> {
    fn observe<T: RemembersOpenForm, S: 'static>(
        view: &AnyView,
        cx: &mut Context<S>,
        changed: fn(&mut S, &mut Context<S>),
    ) -> Option<Subscription> {
        let page = view.clone().downcast::<T>().ok()?;
        Some(cx.observe(&page, move |this, _, cx| changed(this, cx)))
    }
    observe::<remote::RemoteTab, S>(view, cx, changed)
        .or_else(|| observe::<open_targets::OpenTargetsTab, S>(view, cx, changed))
}
