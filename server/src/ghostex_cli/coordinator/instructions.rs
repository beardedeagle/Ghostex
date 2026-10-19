//! `ghostex orchestrator default-instructions` and `set-default-instructions`: the Orchestrator
//! instructions every new orchestrator starts with, for all projects or one project (and its
//! worktrees). See CDXC:Coordinators 2026-10-19 in server/src/coordinators/default_instructions.rs.

use serde_json::{json, Value};

use super::command::{flag_text, server_flags, text_or_file};
use crate::ghostex_cli::{
    agents,
    args::{Flags, ParsedArgs},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The project a command names (`--project-id`, or the calling session's with `--this-project`),
/// with the flags that reach the machine it lives on.
fn target_project(parsed: &ParsedArgs, required: bool) -> CliResult<(Option<String>, Flags)> {
    let base = server_flags(&parsed.flags);
    if let Some(project_id) = flag_text(&parsed.flags, "projectId") {
        return Ok((Some(project_id), base));
    }
    let wants_own = parsed.flags.truthy("thisProject");
    if !wants_own && !required {
        // Show what a new orchestrator here would get when run inside a session.
        if let Ok(caller) = agents::caller() {
            let project_id = agents::text(&caller, "projectId").to_string();
            if !project_id.is_empty() {
                let flags = agents::inventory_flags(&base, agents::text(&caller, "globalRef"))?;
                return Ok((Some(project_id), flags));
            }
        }
        return Ok((None, base));
    }
    if !wants_own {
        return Ok((None, base));
    }
    let caller = agents::caller().map_err(|error| {
        CliError::Other(format!(
            "{error} Outside an agent session, pass --project-id <id> (ghostex sessions --json lists project ids)."
        ))
    })?;
    let flags = agents::inventory_flags(&base, agents::text(&caller, "globalRef"))?;
    Ok((Some(agents::text(&caller, "projectId").to_string()), flags))
}

fn print_block(label: &str, text: &str) {
    if text.trim().is_empty() {
        println!("{label}: (none)");
        return;
    }
    println!("{label}:");
    for line in text.lines() {
        println!("  {line}");
    }
}

fn print_view(view: &Value) {
    print_block(
        "For all projects",
        agents::text(view, "defaultInstructions"),
    );
    let project = &view["project"];
    if project.is_object() {
        let name = agents::text(project, "name");
        let label = if project["isWorktree"] == true {
            format!("For {name} (also covers this worktree)")
        } else {
            format!("For {name} and its worktrees")
        };
        print_block(&label, agents::text(project, "instructions"));
        let source = match agents::text(view, "source") {
            "project" => format!("{name}'s instructions"),
            "default" => "the instructions for all projects".to_string(),
            _ => "no instructions".to_string(),
        };
        println!("A new orchestrator here starts with {source}.");
    }
}

/// `default-instructions [--project-id <id> | --this-project] [--json]`.
pub(super) fn show(parsed: &ParsedArgs) -> CliResult<()> {
    let (project_id, flags) = target_project(parsed, false)?;
    let mut params = json!({});
    if let Some(project_id) = project_id {
        params["projectId"] = json!(project_id);
    }
    let view = call_gxserver_rpc("/api/readCoordinatorDefaultInstructions", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
    } else {
        print_view(&view);
    }
    Ok(())
}

/// `set-default-instructions (<text> | --body-file <path> | --clear) [--project-id <id> |
/// --this-project] [--json]`.
pub(super) fn set(parsed: &ParsedArgs) -> CliResult<()> {
    let text = if parsed.flags.truthy("clear") {
        String::new()
    } else {
        match text_or_file(parsed, "text")? {
            Some(text) => text,
            None => parsed.rest.join(" "),
        }
    };
    if text.trim().is_empty() && !parsed.flags.truthy("clear") {
        return Err(CliError::Other(
            "Usage: ghostex orchestrator set-default-instructions (\"<text>\" | --body-file <path> | --clear) [--project-id <id> | --this-project]. Put flags after the text.".into(),
        ));
    }
    let (project_id, flags) = target_project(parsed, true)?;
    let mut params = json!({ "instructions": text });
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    let view = call_gxserver_rpc("/api/setCoordinatorDefaultInstructions", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    let cleared = text.trim().is_empty();
    match project_id {
        None if cleared => println!("Orchestrator instructions for all projects cleared."),
        None => println!("Orchestrator instructions for all projects saved."),
        Some(_) => {
            let name = agents::text(&view["project"], "name");
            if cleared {
                println!("{name} no longer has its own Orchestrator instructions; its new orchestrators get the ones for all projects.");
            } else {
                println!("Orchestrator instructions for {name} and its worktrees saved.");
            }
        }
    }
    println!("New orchestrators start with them; existing orchestrators keep their own standing instructions.");
    Ok(())
}
