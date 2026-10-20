//! The effort levels an orchestrator is offered: in the New Orchestrator dialog (desktop and
//! phone) and in the effort pickers of an orchestrator's chat composer.
//!
//! CDXC:Coordinators 2026-10-17 DECISION:
//! User: "raise the effort level to xhigh for the orchestrator by default please, and allow just high or xhigh (no low/max/ultracode here in this pop up or in the effort level selection inside the orchestrator chat composer please)." An orchestrator is offered High and XHigh and starts on XHigh; an agent without XHigh is offered its two highest levels below Max (Antigravity: Medium and High) and starts on the higher one. Supersedes the 2026-09-30 decision that started a coordinator at medium effort.
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs (the dialog), server/src/ghostex_cli/coordinator/command.rs (`ghostex orchestrator options` and `create`, which the phone's sheet uses), server/src/coordinators/role.rs (`default_orchestrator_effort`).

/// Levels above XHigh that an orchestrator is never offered.
const BEYOND_ORCHESTRATOR: [&str; 3] = ["max", "ultra", "ultracode"];

/// The levels Ghostex recommends for an orchestrator out of `efforts` (an agent's levels, lowest
/// first): High and XHigh when the agent has both, else its two highest levels below Max.
fn recommended_orchestrator_efforts<T: AsRef<str>>(efforts: &[T]) -> Vec<String> {
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

/// The levels an orchestrator's effort pickers list out of `efforts` (an agent's levels, lowest
/// first). `show_all` is the Show all effort levels setting (`orchestratorAllEfforts`): every level
/// the agent has. Otherwise the recommended levels, plus `current` (the level the session or form is
/// on) when it is one of the agent's levels, so a level picked earlier still shows as chosen.
///
/// CDXC:Coordinators 2026-10-20 DECISION:
/// User: "Please add a setting under the orchestration settings where the user can toggle on showing more effort levels. By default, we just show high and X high, but if he enables this, he can see all of the effort levels. But we need to mention there in the tooltip that it's not recommended to use anything lower than high ... make the settings advanced." Off by default, so the 2026-10-17 High/XHigh rule above stands; on, the pickers list every level the agent offers. The level an orchestrator starts on stays the recommended one either way.
/// SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (`orchestratorAllEfforts`), server/src/session_chat_read.rs (the chat read carries it to every chat, the phone's too), apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (the dialog), server/src/ghostex_cli/coordinator/command.rs (`ghostex orchestrator options`).
pub fn orchestrator_efforts<T: AsRef<str>>(
    efforts: &[T],
    show_all: bool,
    current: Option<&str>,
) -> Vec<String> {
    if show_all {
        return efforts
            .iter()
            .map(|effort| effort.as_ref().to_string())
            .collect();
    }
    let recommended = recommended_orchestrator_efforts(efforts);
    efforts
        .iter()
        .map(AsRef::as_ref)
        .filter(|effort| recommended.iter().any(|kept| kept == effort) || Some(*effort) == current)
        .map(str::to_string)
        .collect()
}

/// The level an orchestrator starts on: the highest recommended one (XHigh when the agent has it).
pub fn default_orchestrator_effort<T: AsRef<str>>(efforts: &[T]) -> Option<String> {
    recommended_orchestrator_efforts(efforts).pop()
}
