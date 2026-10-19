//! The Orchestrator instructions editor nested in Settings: the Edit button of Settings > Chat
//! (the text for all projects) and of a project on the Projects page (that project's own, which
//! covers its worktrees). Both read and save through gxserver
//! (`/api/readCoordinatorDefaultInstructions`, `/api/setCoordinatorDefaultInstructions`).
//!
//! CDXC:Coordinators 2026-10-19 WHY: the editor is a dialog nested in the Settings window, not an app modal of its own: opening another app modal replaces the open one (`open_native_app_modal`), which would close Settings under the reader. The user's decision is on server/src/coordinators/default_instructions.rs.
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, SettingsPage, settings_button, settings_dialog_sheet_with_corner_close,
    settings_textarea,
};
use super::super::palette::SettingsPalette;
use super::super::store::store_gxserver_rpc;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement as _,
    Styled as _, Subscription, Window, div, px,
};
use gpui_component::h_flex;
use gpui_component::input::{InputEvent, TextareaState};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const PLACEHOLDER: &str = "e.g. Branch from main and never push without asking.";

/// What one scope's text is: the one for all projects (`None`) or a project's own.
#[derive(Clone, Debug, Default)]
pub(crate) struct InstructionsSummary {
    pub(crate) text: String,
    /// The project whose own text this is (a worktree's parent), and its name.
    pub(crate) owner_id: String,
    pub(crate) owner_name: String,
    pub(crate) is_worktree: bool,
}

/// The open editor.
pub(crate) struct InstructionsSheet {
    focus: FocusHandle,
    /// The project as the page asked for it (`None`: all projects).
    project_id: Option<String>,
    /// Made by the first render after the text arrived (setting a field's text needs a window).
    input: Option<(Entity<TextareaState>, Subscription)>,
    loaded_text: Option<String>,
    saving: bool,
    error: Option<String>,
}

/// Per page: the summaries read so far (keyed by project id, `None` for all projects) and the
/// open editor.
#[derive(Default)]
pub(crate) struct OrchestratorInstructionsState {
    summaries: HashMap<Option<String>, InstructionsSummary>,
    requested: std::collections::HashSet<Option<String>>,
    sheet: Option<InstructionsSheet>,
    draft: String,
}

/// A Settings page with the editor.
pub(crate) trait HasOrchestratorInstructions: SettingsPage + Sized {
    fn orchestrator_instructions(&mut self) -> &mut OrchestratorInstructionsState;
}

fn summary_of(view: &Value, project_id: Option<&str>) -> InstructionsSummary {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match project_id {
        None => InstructionsSummary {
            text: text(view, "defaultInstructions"),
            ..Default::default()
        },
        Some(_) => {
            let project = &view["project"];
            InstructionsSummary {
                text: text(project, "instructions"),
                owner_id: text(project, "projectId"),
                owner_name: text(project, "name"),
                is_worktree: project["isWorktree"] == true,
            }
        }
    }
}

/// The summary of a scope, reading it from gxserver the first time it is asked for.
pub(crate) fn instructions_summary<V: HasOrchestratorInstructions>(
    page: &mut V,
    project_id: Option<&str>,
    cx: &mut Context<V>,
) -> Option<InstructionsSummary> {
    let key = project_id.map(str::to_string);
    let state = page.orchestrator_instructions();
    if let Some(summary) = state.summaries.get(&key) {
        return Some(summary.clone());
    }
    if state.requested.insert(key.clone()) {
        read_scope(page, key, cx);
    }
    None
}

fn read_scope<V: HasOrchestratorInstructions>(
    page: &mut V,
    project_id: Option<String>,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    if !store.read(cx).request().gxserver_rpc_available {
        return;
    }
    let params = match &project_id {
        Some(id) => json!({ "projectId": id }),
        None => json!({}),
    };
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorDefaultInstructions",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                if let Ok(view) = result {
                    remember_view(page, &view, project_id.as_deref());
                    cx.notify();
                }
            });
        },
        cx,
    );
}

/// Stores what a read or a save answered: the scope asked for, and the one for all projects.
fn remember_view<V: HasOrchestratorInstructions>(
    page: &mut V,
    view: &Value,
    project_id: Option<&str>,
) {
    let state = page.orchestrator_instructions();
    state.summaries.insert(None, summary_of(view, None));
    if let Some(project_id) = project_id {
        state.summaries.insert(
            Some(project_id.to_string()),
            summary_of(view, Some(project_id)),
        );
    }
}

/// Opens the editor on the scope's current text (read fresh, so another client's save shows).
pub(crate) fn open_instructions_sheet<V: HasOrchestratorInstructions>(
    page: &mut V,
    project_id: Option<String>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let focus = cx.focus_handle();
    focus.focus(window, cx);
    {
        let state = page.orchestrator_instructions();
        state.draft.clear();
        state.sheet = Some(InstructionsSheet {
            focus,
            project_id: project_id.clone(),
            input: None,
            loaded_text: None,
            saving: false,
            error: None,
        });
    }
    let store = page.settings_store().clone();
    let params = match &project_id {
        Some(id) => json!({ "projectId": id }),
        None => json!({}),
    };
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/readCoordinatorDefaultInstructions",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                match result {
                    Ok(view) => {
                        remember_view(page, &view, project_id.as_deref());
                        let text = summary_of(&view, project_id.as_deref()).text;
                        let state = page.orchestrator_instructions();
                        state.draft = text.clone();
                        if let Some(sheet) = state.sheet.as_mut() {
                            sheet.loaded_text = Some(text);
                        }
                    }
                    Err(message) => {
                        if let Some(sheet) = page.orchestrator_instructions().sheet.as_mut() {
                            sheet.error = Some(message);
                        }
                    }
                }
                cx.notify();
            });
        },
        cx,
    );
    cx.notify();
}

fn close_sheet<V: HasOrchestratorInstructions>(page: &mut V, cx: &mut Context<V>) {
    page.orchestrator_instructions().sheet = None;
    cx.notify();
}

fn save_sheet<V: HasOrchestratorInstructions>(page: &mut V, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    let state = page.orchestrator_instructions();
    let instructions = state.draft.trim().to_string();
    let Some(sheet) = state.sheet.as_mut() else {
        return;
    };
    if sheet.saving || sheet.input.is_none() {
        return;
    }
    sheet.saving = true;
    sheet.error = None;
    let project_id = sheet.project_id.clone();
    let mut params = json!({ "instructions": instructions });
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    let this = cx.weak_entity();
    store_gxserver_rpc(
        &store,
        "/api/setCoordinatorDefaultInstructions",
        params,
        RPC_TIMEOUT,
        move |result, cx| {
            let _ = this.update(cx, |page, cx| {
                match result {
                    Ok(view) => {
                        remember_view(page, &view, project_id.as_deref());
                        page.orchestrator_instructions().sheet = None;
                    }
                    Err(message) => {
                        if let Some(sheet) = page.orchestrator_instructions().sheet.as_mut() {
                            sheet.saving = false;
                            sheet.error = Some(message);
                        }
                    }
                }
                cx.notify();
            });
        },
        cx,
    );
    cx.notify();
}

/// The editor, while it is open.
pub(crate) fn render_instructions_sheet<V: HasOrchestratorInstructions>(
    page: &mut V,
    p: &SettingsPalette,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let pending = page
        .orchestrator_instructions()
        .sheet
        .as_mut()
        .filter(|sheet| sheet.input.is_none())
        .and_then(|sheet| sheet.loaded_text.take());
    if let Some(text) = pending {
        let input = cx.new(|cx| {
            // 13 rows fill the field's 280px; longer text grows it to 24 rows, then scrolls.
            TextareaState::new(window, cx)
                .auto_grow(13, 24)
                .placeholder(PLACEHOLDER)
                .default_value(text)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |page: &mut V, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    page.orchestrator_instructions().draft = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        input.update(cx, |input, cx| input.focus(window, cx));
        if let Some(sheet) = page.orchestrator_instructions().sheet.as_mut() {
            sheet.input = Some((input, subscription));
        }
    }
    let state = page.orchestrator_instructions();
    let sheet = state.sheet.as_ref()?;
    let focus = sheet.focus.clone();
    let input = sheet.input.as_ref().map(|(input, _)| input.clone());
    let (loaded, saving) = (input.is_some(), sheet.saving);
    let error = sheet.error.clone();
    let project_id = sheet.project_id.clone();
    let project = project_id
        .as_ref()
        .and_then(|id| state.summaries.get(&Some(id.clone())))
        .cloned();
    let all_projects = state
        .summaries
        .get(&None)
        .map(|summary| summary.text.clone())
        .unwrap_or_default();
    let (title, description) = match (&project_id, &project) {
        (None, _) => (
            "Orchestrator instructions".to_string(),
            "Every new orchestrator starts with these as its standing instructions, so every thread it starts gets them too. A project with its own (Settings > Projects) uses those instead. Orchestrators you already have keep theirs.".to_string(),
        ),
        (Some(_), summary) => {
            let summary = summary.as_ref();
            let name = summary
                .map(|summary| summary.owner_name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "this project".to_string());
            let worktree = if summary.is_some_and(|summary| summary.is_worktree) {
                " This is a worktree, so this edits its project's text."
            } else {
                ""
            };
            (
                format!("Orchestrator instructions for {name}"),
                format!(
                    "New orchestrators in {name} and all of its worktrees start with these instead of the ones for all projects. Leave empty to use the ones for all projects.{worktree} Orchestrators you already have keep theirs."
                ),
            )
        }
    };
    let field = match &input {
        Some(input) => settings_textarea(p, input, 280.0, false, saving, window, cx),
        None => div()
            .w_full()
            .h(px(280.0))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .text_color(hsla(p.muted))
            .child(if error.is_some() { "" } else { "Loading…" })
            .into_any_element(),
    };
    let mut body: Vec<AnyElement> = vec![field];
    if project_id.is_some() && !all_projects.trim().is_empty() {
        body.push(
            div()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(hsla(p.muted))
                .line_clamp(2)
                .child(format!(
                    "For all projects: {}",
                    all_projects.lines().next().unwrap_or_default()
                ))
                .into_any_element(),
        );
    }
    if let Some(error) = error {
        body.push(
            div()
                .text_size(px(12.0))
                .text_color(hsla(p.destructive))
                .child(error)
                .into_any_element(),
        );
    }
    let cancel = settings_button(
        p,
        "orchestrator-instructions-cancel",
        "Cancel",
        None,
        ButtonVariant::Outline,
        false,
        None,
        |page: &mut V, _window, cx| close_sheet(page, cx),
        cx,
    );
    let save = settings_button(
        p,
        "orchestrator-instructions-save",
        if saving { "Saving…" } else { "Save" },
        None,
        ButtonVariant::Primary,
        !loaded || saving,
        None,
        |page: &mut V, _window, cx| save_sheet(page, cx),
        cx,
    );
    let footer = h_flex()
        .w_full()
        .justify_end()
        .gap(px(8.0))
        .child(cancel)
        .child(save)
        .into_any_element();
    let max_height = f32::from(window.viewport_size().height) * 0.9;
    // The preview binary's editor states cannot hover, so their X shows at rest.
    let close_shown = page
        .settings_store()
        .read(cx)
        .request()
        .preview_state
        .as_deref()
        .is_some_and(|state| state.ends_with("orchestrator-instructions-edit"));
    Some(settings_dialog_sheet_with_corner_close(
        p,
        "orchestrator-instructions",
        &focus,
        title,
        Some(div().child(description).into_any_element()),
        560.0,
        Some(max_height),
        24.0,
        16.0,
        body,
        Some(footer),
        |page: &mut V, _window, cx| close_sheet(page, cx),
        close_shown,
        window,
        cx,
    ))
}

/// The row's control: the scope's one-line state, then its Edit button.
pub(crate) fn instructions_control(
    p: &SettingsPalette,
    state: &str,
    edit: AnyElement,
) -> AnyElement {
    h_flex()
        .gap(px(10.0))
        .child(
            div()
                .max_w(px(220.0))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(state.to_string()),
        )
        .child(edit)
        .into_any_element()
}

/// The one-line state of a scope shown next to its Edit button.
pub(crate) fn summary_line(summary: Option<&InstructionsSummary>, project: bool) -> String {
    match summary {
        None => "Loading…".to_string(),
        Some(summary) if summary.text.trim().is_empty() && project => {
            "Uses the instructions for all projects".to_string()
        }
        Some(summary) if summary.text.trim().is_empty() => "None set".to_string(),
        Some(summary) => summary
            .text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}
