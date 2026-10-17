//! The effort levels an orchestrator is offered: in the New Orchestrator dialog (desktop and
//! phone) and in the effort pickers of an orchestrator's chat composer.
//!
//! CDXC:Coordinators 2026-10-17 DECISION:
//! User: "raise the effort level to xhigh for the orchestrator by default please, and allow just high or xhigh (no low/max/ultracode here in this pop up or in the effort level selection inside the orchestrator chat composer please)." An orchestrator is offered High and XHigh and starts on XHigh; an agent without XHigh is offered its two highest levels below Max (Antigravity: Medium and High) and starts on the higher one. Supersedes the 2026-09-30 decision that started a coordinator at medium effort.
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs (the dialog), server/src/ghostex_cli/coordinator/command.rs (`ghostex orchestrator options` and `create`, which the phone's sheet uses), server/src/coordinators/role.rs (`default_orchestrator_effort`).

/// Levels above XHigh that an orchestrator is never offered.
const BEYOND_ORCHESTRATOR: [&str; 3] = ["max", "ultra", "ultracode"];

/// The levels an orchestrator is offered out of `efforts` (an agent's levels, lowest first): High
/// and XHigh when the agent has both, else its two highest levels below Max.
pub fn orchestrator_efforts<T: AsRef<str>>(efforts: &[T]) -> Vec<String> {
    let offered: Vec<&str> = efforts
        .iter()
        .map(AsRef::as_ref)
        .filter(|effort| !BEYOND_ORCHESTRATOR.contains(effort))
        .collect();
    if offered.contains(&"high") && offered.contains(&"xhigh") {
        return vec!["high".to_string(), "xhigh".to_string()];
    }
    offered[offered.len().saturating_sub(2)..]
        .iter()
        .map(|effort| effort.to_string())
        .collect()
}

/// The level an orchestrator starts on: the highest it is offered (XHigh when the agent has it).
pub fn default_orchestrator_effort<T: AsRef<str>>(efforts: &[T]) -> Option<String> {
    orchestrator_efforts(efforts).pop()
}
