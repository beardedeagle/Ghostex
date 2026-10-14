//! The Agents Hub catalog as the modal holds it: the four file tabs of `agentsHubCatalog`
//! (packages/shared/session-grid-contract.ts (deleted 2026-10-01), built by apps/desktop/src/app/helpers/agents_hub/catalog_builder.rs),
//! the per-file content answers, and the pure rules the React surface applied to them
//! (search filter, first file, profile badge).
use serde::Deserialize;

/// The Hub's five tabs in rail order.
///
/// CDXC:AgentLauncher 2026-08-24 (round 3):
/// Tab order is Skills first, then MDs, Hooks, Configs & MCPs — most-edited first, and the
/// Cmd+N hints follow it. Agent Sync is the fifth tab (Cmd+5) and lists agents, not files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentsHubTab {
    Skills,
    Mds,
    Hooks,
    Configs,
    Sync,
}

impl AgentsHubTab {
    pub(crate) const ALL: [AgentsHubTab; 5] = [
        AgentsHubTab::Skills,
        AgentsHubTab::Mds,
        AgentsHubTab::Hooks,
        AgentsHubTab::Configs,
        AgentsHubTab::Sync,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            AgentsHubTab::Skills => "Skills",
            AgentsHubTab::Mds => "MDs",
            AgentsHubTab::Hooks => "Hooks",
            AgentsHubTab::Configs => "Configs & MCPs",
            AgentsHubTab::Sync => "Agent Sync",
        }
    }

    /// The shortcut that selects the tab from anywhere in the Hub window.
    pub(crate) fn hotkey(self) -> &'static str {
        match self {
            AgentsHubTab::Skills => "cmd+1",
            AgentsHubTab::Mds => "cmd+2",
            AgentsHubTab::Hooks => "cmd+3",
            AgentsHubTab::Configs => "cmd+4",
            AgentsHubTab::Sync => "cmd+5",
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    /// The file tab's slot in the catalog, `None` for Agent Sync.
    pub(crate) fn file_slot(self) -> Option<usize> {
        match self {
            AgentsHubTab::Skills => Some(0),
            AgentsHubTab::Mds => Some(1),
            AgentsHubTab::Hooks => Some(2),
            AgentsHubTab::Configs => Some(3),
            AgentsHubTab::Sync => None,
        }
    }

    /// The `initialTab` ids the React modal accepted.
    pub(crate) fn from_id(value: &str) -> Option<Self> {
        match value {
            "skills" => Some(AgentsHubTab::Skills),
            "mds" => Some(AgentsHubTab::Mds),
            "hooks" => Some(AgentsHubTab::Hooks),
            "configs" => Some(AgentsHubTab::Configs),
            "sync" => Some(AgentsHubTab::Sync),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsHubProfile {
    #[serde(default)]
    pub(crate) agent_icon: String,
    #[serde(default)]
    pub(crate) file_path: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) profile_path: String,
    #[serde(default)]
    pub(crate) target_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsHubFile {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) language: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) path: String,
    /// Only the stories inline file bodies; the desktop catalog is metadata-only.
    #[serde(default)]
    pub(crate) content: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsHubGroup {
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) files: Vec<AgentsHubFile>,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) profiles: Vec<AgentsHubProfile>,
    /// Skills tab only: how many differing versions of this skill the agent folders hold.
    #[serde(default)]
    pub(crate) variant_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AgentsHubGroupsByTab {
    #[serde(default)]
    pub(crate) skills: Vec<AgentsHubGroup>,
    #[serde(default)]
    pub(crate) mds: Vec<AgentsHubGroup>,
    #[serde(default)]
    pub(crate) hooks: Vec<AgentsHubGroup>,
    #[serde(default)]
    pub(crate) configs: Vec<AgentsHubGroup>,
}

impl AgentsHubGroupsByTab {
    pub(crate) fn slot(&self, slot: usize) -> &[AgentsHubGroup] {
        match slot {
            0 => &self.skills,
            1 => &self.mds,
            2 => &self.hooks,
            _ => &self.configs,
        }
    }
}

/// One `agentsHubCatalog` generation.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsHubCatalog {
    #[serde(default)]
    pub(crate) generated_at: String,
    #[serde(default)]
    pub(crate) groups_by_tab: AgentsHubGroupsByTab,
}

impl AgentsHubCatalog {
    pub(crate) fn from_json(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

/// One `agentsHubFileContent` answer: the body, or why it could not be read.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsHubFileContent {
    #[serde(default)]
    pub(crate) file_path: String,
    #[serde(default)]
    pub(crate) request_id: String,
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
}

impl AgentsHubFileContent {
    pub(crate) fn from_json(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

/// `useFilteredGroups`: a group matches when its name, path or description, or any of its
/// files' names and paths, contain the trimmed, lower-cased query.
pub(crate) fn filtered_groups<'a>(
    groups: &'a [AgentsHubGroup],
    query: &str,
) -> Vec<&'a AgentsHubGroup> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return groups.iter().collect();
    }
    groups
        .iter()
        .filter(|group| {
            let group_text =
                format!("{} {} {}", group.name, group.path, group.description).to_lowercase();
            let file_text = group
                .files
                .iter()
                .map(|file| format!("{} {}", file.name, file.path))
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            group_text.contains(&needle) || file_text.contains(&needle)
        })
        .collect()
}

/// CDXC:AgentLauncher 2026-08-24 (round 2):
/// A plain lookup. It used to fall back to the tab's first file when the id was unknown, which
/// left the selected id empty while the editor showed the fallback file and the list highlighted
/// nothing. Callers resolve the default themselves through [`first_file_id`].
pub(crate) fn find_file<'a>(
    groups: &'a [AgentsHubGroup],
    file_id: &str,
) -> Option<&'a AgentsHubFile> {
    groups
        .iter()
        .flat_map(|group| group.files.iter())
        .find(|file| file.id == file_id)
}

pub(crate) fn first_file_id(groups: &[AgentsHubGroup]) -> String {
    groups
        .first()
        .and_then(|group| group.files.first())
        .map(|file| file.id.clone())
        .unwrap_or_default()
}

/// CDXC:AgentLauncher 2026-05-13-08:16
/// Main agent profiles show only the agent logo. Non-main profile chips take their corner badge
/// from the first alphanumeric character of the profile folder name, so `personal` maps to P and
/// `work` to W without per-agent badge labels. Windows paths use either separator.
pub(crate) fn agent_profile_badge(profile_path: &str) -> Option<String> {
    let normalized = profile_path.replace('\\', "/");
    if !normalized.contains("-profiles/") {
        return None;
    }
    let folder = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .last()?
        .to_lowercase();
    folder
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase().to_string())
}

/// The Kit highlighter's language for a catalog `language` (`gpui_agents_hub_language_for`,
/// the Monaco ids).
pub(crate) fn editor_language(language: &str) -> &'static str {
    match language {
        "markdown" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yaml" => "yaml",
        "shell" => "bash",
        "javascript" => "javascript",
        "typescript" => "typescript",
        "python" => "python",
        _ => "text",
    }
}
