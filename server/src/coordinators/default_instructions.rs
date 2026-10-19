//! Orchestrator instructions: the standing instructions every new orchestrator starts with. One
//! text for all projects (the `metadata` row below) and an optional override per project (its
//! `launchSettings.coordinatorInstructions`); a worktree project takes its parent project's.
//!
//! CDXC:Coordinators 2026-10-19 DECISION:
//! User: "ok i want to have a button to click on to set the orchestrator prompt and i want to be able to set a different orchestrator prompt for a specific project too (the orchestrator prompt that gets overriden for a project should apply for all it's worktrees too please)". Q301 "Default standing instructions": the text becomes a new orchestrator's standing instructions, so it also reaches every thread it briefs. Q302 "New Orchestrator dialog" and "Settings > Chat", "for project should be in the project settings as a button that shows a pop up for that". Q303 "New orchestrators only": saving never changes an orchestrator that already exists. Q304 "Desktop only": no phone editor, but the phone's New Orchestrator gets the text because gxserver applies it at creation. Make Orchestrator (promote) applies it too, since it makes a new orchestrator; a create that names its own instructions keeps them.
//! SEE-ALSO: server/src/coordinators/create.rs and promote.rs (where it is applied), server/src/ghostex_cli/coordinator/instructions.rs (the CLI verbs), apps/desktop/src/app/window/orchestrator_instructions/ (the desktop editors), packages/settings-catalog/src/general/chat.rs (the Settings > Chat row).

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error, COORDINATOR_INSTRUCTIONS_MAX_CHARS};
use crate::domain::{DomainRepository, DomainStateError};

const DEFAULT_INSTRUCTIONS_METADATA_KEY: &str = "coordinatorDefaultInstructions";
const PROJECT_INSTRUCTIONS_KEY: &str = "coordinatorInstructions";

pub const COORDINATOR_DEFAULT_INSTRUCTIONS_ENDPOINTS: &[&str] = &[
    "/api/readCoordinatorDefaultInstructions",
    "/api/setCoordinatorDefaultInstructions",
];

/// Where a new orchestrator's instructions come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrchestratorInstructionsSource {
    Project,
    Default,
    None,
}

impl OrchestratorInstructionsSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Default => "default",
            Self::None => "none",
        }
    }
}

pub fn read_default_orchestrator_instructions(db: &Connection) -> Result<String, DomainStateError> {
    Ok(db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [DEFAULT_INSTRUCTIONS_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .unwrap_or_default())
}

fn write_default_orchestrator_instructions(
    db: &Connection,
    instructions: &str,
) -> Result<(), DomainStateError> {
    if instructions.is_empty() {
        db.execute(
            "DELETE FROM metadata WHERE key = ?1",
            [DEFAULT_INSTRUCTIONS_METADATA_KEY],
        )
        .map_err(sql_error)?;
        return Ok(());
    }
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![DEFAULT_INSTRUCTIONS_METADATA_KEY, instructions, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn project_text<'a>(project: &'a Value, key: &str) -> &'a str {
    project.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The project whose override covers `project_id`: the project itself, or the parent checkout of a
/// worktree project (when that parent still exists).
pub fn orchestrator_instructions_owner(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<Value, DomainStateError> {
    let project = repository.get_project(project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("No project matched \"{project_id}\"."))
    })?;
    let parent_id = project
        .pointer("/worktree/parentProjectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    if let Some(parent_id) = parent_id {
        if let Some(parent) = repository.get_project(parent_id)? {
            return Ok(parent);
        }
    }
    Ok(project)
}

fn stored_project_instructions(project: &Value) -> String {
    project
        .pointer(&format!("/launchSettings/{PROJECT_INSTRUCTIONS_KEY}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// The instructions a new orchestrator in `project_id` starts with: its project's override (the
/// parent's for a worktree), else the default for all projects, else none.
pub fn resolve_orchestrator_instructions(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<(String, OrchestratorInstructionsSource), DomainStateError> {
    if !project_id.trim().is_empty() {
        if let Ok(owner) = orchestrator_instructions_owner(repository, project_id) {
            let own = stored_project_instructions(&owner);
            if !own.is_empty() {
                return Ok((own, OrchestratorInstructionsSource::Project));
            }
        }
    }
    let default = read_default_orchestrator_instructions(db)?;
    Ok(if default.trim().is_empty() {
        (String::new(), OrchestratorInstructionsSource::None)
    } else {
        (default, OrchestratorInstructionsSource::Default)
    })
}

/// `{ defaultInstructions, project?, instructions, source }` for an optional `projectId`.
fn instructions_view(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let default = read_default_orchestrator_instructions(db)?;
    let mut view = json!({ "defaultInstructions": default });
    if let Some(project_id) = project_id {
        let owner = orchestrator_instructions_owner(repository, project_id)?;
        let owner_id = project_text(&owner, "projectId");
        view["project"] = json!({
            "projectId": owner_id,
            "name": project_text(&owner, "name"),
            "requestedProjectId": project_id,
            "isWorktree": owner_id != project_id,
            "instructions": stored_project_instructions(&owner),
        });
    }
    let (instructions, source) =
        resolve_orchestrator_instructions(db, repository, project_id.unwrap_or_default())?;
    view["instructions"] = json!(instructions);
    view["source"] = json!(source.as_str());
    Ok(view)
}

fn param_text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// `/api/readCoordinatorDefaultInstructions` `{ projectId? }` and
/// `/api/setCoordinatorDefaultInstructions` `{ instructions, projectId? }`: without `projectId`
/// the set changes the text for all projects, with one that project's override (a worktree's goes
/// to its parent project). An empty text clears it. Both answer the read's view.
pub fn handle_coordinator_default_instructions_endpoint(
    endpoint_path: &str,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = param_text(params, "projectId");
    if endpoint_path == "/api/setCoordinatorDefaultInstructions" {
        let instructions = params
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| {
                DomainStateError::bad_request("Pass instructions (an empty text clears them).")
            })?;
        if instructions.chars().count() > COORDINATOR_INSTRUCTIONS_MAX_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "Orchestrator instructions are limited to {COORDINATOR_INSTRUCTIONS_MAX_CHARS} characters."
            )));
        }
        match project_id {
            None => write_default_orchestrator_instructions(db, instructions)?,
            Some(project_id) => {
                let owner = orchestrator_instructions_owner(repository, project_id)?;
                // `launchSettings` is replaced whole on update, so the rest of it is written back.
                let mut launch_settings = owner
                    .get("launchSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if instructions.is_empty() {
                    launch_settings.remove(PROJECT_INSTRUCTIONS_KEY);
                } else {
                    launch_settings
                        .insert(PROJECT_INSTRUCTIONS_KEY.to_string(), json!(instructions));
                }
                let mut update = Map::new();
                update.insert(
                    "projectId".to_string(),
                    json!(project_text(&owner, "projectId")),
                );
                update.insert("launchSettings".to_string(), Value::Object(launch_settings));
                repository.update_project(&update)?;
            }
        }
    }
    instructions_view(db, repository, project_id)
}
