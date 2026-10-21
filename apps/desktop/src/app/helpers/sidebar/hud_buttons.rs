// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the sidebar HUD button state derivation for agents and commands, plus stored-agent/command normalization.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;

#[derive(Clone, Debug)]
pub(crate) struct GpuiSidebarHudButtons {
    pub(crate) agents: serde_json::Value,
    pub(crate) commands: serde_json::Value,
    /*
    CDXC:AgentLauncher 2026-08-01-19:00:
    The Settings app modal reads its Actions lists from this Rust-side HUD fetch,
    not from the sidebar TypeScript runtime, so Global Actions have to be carried
    here too or the new section renders empty no matter what is stored.
    */
    pub(crate) global_commands: serde_json::Value,
}

pub(crate) fn gpui_sidebar_hud_array_field(
    result: &serde_json::Value,
    key: &str,
) -> Result<serde_json::Value, String> {
    result
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .map(serde_json::Value::Array)
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
}

/// CDXC:AgentHooks 2026-09-04 DECISION:
/// User: the Tips hook warning must not name agents the user "isn't actually using currently in the sidebar".
/// Maps every visible sidebar launcher (gxserver's `hud.agents`) to the built-in agent whose hooks it needs: default launchers by id, custom ones by their icon.
pub(crate) fn gpui_sidebar_default_agent_ids_from_hud_agents(
    agents: &serde_json::Value,
) -> HashSet<String> {
    agents
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            let agent_id = agent.get("agentId").and_then(serde_json::Value::as_str)?;
            if let Some(default_agent) = gpui_default_sidebar_agent_by_id(agent_id) {
                return Some(default_agent.agent_id.to_string());
            }
            let icon = agent.get("icon").and_then(serde_json::Value::as_str)?;
            gpui_default_sidebar_agent_by_icon(icon).map(|agent| agent.agent_id.to_string())
        })
        .collect()
}

pub(crate) fn gpui_normalized_string_order_from_values(items: &[serde_json::Value]) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen_ids = HashSet::new();
    for item in items {
        let Some(item) = item
            .as_str()
            .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
        else {
            continue;
        };
        if seen_ids.insert(item.to_string()) {
            order.push(item.to_string());
        }
    }
    order
}
