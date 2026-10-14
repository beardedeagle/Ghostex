//! Agents Hub Skills tab: one row per skill name, merged across every agent
//! skill folder the catalog scans.
//!
//! CDXC:AgentSkills 2026-10-14 DECISION:
//! User: "We really need to deduplicate the skills and not show all these .bak ones. Users basically want the same skill on all their coding agents." The Skills tab lists each skill name once, with every agent that has it as a chip; it flags the row only when the copies' files actually differ, and never lists backup folders (`*.pre-sync-*`, `*.bak` and the like).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::app::helpers::*;

/// One skill folder (or link) found in one agent skill root.
pub(crate) struct GpuiAgentsHubSkillCopy {
    pub(crate) description: &'static str,
    pub(crate) dir: PathBuf,
    pub(crate) id_prefix: String,
    pub(crate) name: String,
    pub(crate) profiles: Vec<GpuiAgentsHubProfileItem>,
    pub(crate) root: PathBuf,
}

/// Folders an agent, the `skills` CLI, Agent Sync or a user left behind as a
/// backup of a skill. They are not skills, so the Hub never lists them.
pub(crate) fn gpui_agents_hub_is_backup_skill_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains(".pre-sync-")
        || lower.contains(".bak-")
        || lower.contains(".backup-")
        || [".bak", ".backup", ".orig", ".old", "~"]
            .iter()
            .any(|suffix| lower.ends_with(suffix))
}

/// Collect every skill folder under one agent skill root (and its `.system`
/// folder) without adding rows yet; `gpui_agents_hub_add_merged_skill_groups`
/// merges them by name once every root has been read.
pub(crate) fn gpui_agents_hub_collect_skill_root(
    copies: &mut Vec<GpuiAgentsHubSkillCopy>,
    id_prefix: &str,
    root: &Path,
    description: &'static str,
    system_description: &'static str,
    profiles: Vec<GpuiAgentsHubProfileItem>,
) {
    for skill_dir in gpui_agents_hub_list_directories(root) {
        let skill_name = gpui_file_name_string(&skill_dir);
        if skill_name.starts_with('.') && skill_name != ".system" {
            continue;
        }
        if skill_name == ".system" {
            for system_skill in gpui_agents_hub_list_directories(&skill_dir) {
                let name = gpui_file_name_string(&system_skill);
                if gpui_agents_hub_is_backup_skill_name(&name) {
                    continue;
                }
                copies.push(GpuiAgentsHubSkillCopy {
                    description: system_description,
                    dir: system_skill,
                    id_prefix: format!("{id_prefix}-system"),
                    name,
                    profiles: profiles.clone(),
                    root: skill_dir.clone(),
                });
            }
            continue;
        }
        if gpui_agents_hub_is_backup_skill_name(&skill_name) {
            continue;
        }
        copies.push(GpuiAgentsHubSkillCopy {
            description,
            dir: skill_dir,
            id_prefix: id_prefix.to_string(),
            name: skill_name,
            profiles: profiles.clone(),
            root: root.to_path_buf(),
        });
    }
}

/// Add one Skills row per skill name. Copies that resolve to the same folder
/// (the per-skill links Agent Sync and Ghostex make) are one version; real
/// copies are compared by content, and each version that differs from the
/// first adds its own files, labelled with where that copy lives.
pub(crate) fn gpui_agents_hub_add_merged_skill_groups(
    builder: &mut GpuiAgentsHubCatalogBuilder,
    copies: Vec<GpuiAgentsHubSkillCopy>,
) {
    let mut order: Vec<String> = Vec::new();
    let mut by_name: HashMap<String, Vec<GpuiAgentsHubSkillCopy>> = HashMap::new();
    for copy in copies {
        if !by_name.contains_key(&copy.name) {
            order.push(copy.name.clone());
        }
        by_name.entry(copy.name.clone()).or_default().push(copy);
    }
    let home = builder.home.clone();
    for name in order {
        let Some(copies) = by_name.remove(&name) else {
            continue;
        };
        // Versions in scan order, so the `~/.agents/skills` copy (scanned first) leads.
        let mut versions: Vec<(Option<String>, PathBuf, PathBuf)> = Vec::new();
        let mut seen_real: HashSet<PathBuf> = HashSet::new();
        let distinct_real = copies
            .iter()
            .filter_map(|copy| fs::canonicalize(&copy.dir).ok())
            .collect::<HashSet<_>>()
            .len();
        for copy in &copies {
            let Ok(real) = fs::canonicalize(&copy.dir) else {
                continue;
            };
            if !seen_real.insert(real.clone()) {
                continue;
            }
            // Hash only when there is more than one real folder to compare.
            let hash = if distinct_real > 1 {
                ghostex_agent_sync::fsx::dir_content_hash(&real)
            } else {
                None
            };
            let same_as_known = hash.is_some()
                && versions
                    .iter()
                    .any(|(known, _, _)| known.as_ref() == hash.as_ref());
            if !same_as_known {
                versions.push((hash, copy.dir.clone(), copy.root.clone()));
            }
        }
        let Some((_, primary_dir, _)) = versions.first().cloned() else {
            continue;
        };
        let primary = copies
            .iter()
            .find(|copy| copy.dir == primary_dir)
            .unwrap_or(&copies[0]);
        let mut profiles: Vec<GpuiAgentsHubProfileItem> = Vec::new();
        for copy in &copies {
            for profile in &copy.profiles {
                if !profiles.iter().any(|known| {
                    known.agent_icon == profile.agent_icon
                        && known.profile_path == profile.profile_path
                }) {
                    profiles.push(profile.clone());
                }
            }
        }
        let folders = copies
            .iter()
            .map(|copy| gpui_agents_hub_display_path(&copy.root, &home))
            .collect::<Vec<_>>();
        let description = if versions.len() > 1 {
            format!(
                "{} versions of this skill differ: {}. Each version's files are listed below.",
                versions.len(),
                folders.join(", ")
            )
        } else if folders.len() > 1 {
            format!(
                "{} The same skill is in {} skill folders: {}.",
                primary.description,
                folders.len(),
                folders.join(", ")
            )
        } else {
            primary.description.to_string()
        };
        let mut files = gpui_agents_hub_walk_files(&primary_dir, 3, gpui_agents_hub_is_skill_file);
        let mut labelled: Vec<(PathBuf, String)> = Vec::new();
        for (_, dir, root) in versions.iter().skip(1) {
            let label = gpui_agents_hub_display_path(root, &home);
            for file in gpui_agents_hub_walk_files(dir, 3, gpui_agents_hub_is_skill_file) {
                let relative = gpui_relative_path_name(&file, dir)
                    .unwrap_or_else(|| gpui_file_name_string(&file));
                labelled.push((file.clone(), format!("{label}/{name}/{relative}")));
                files.push(file);
            }
        }
        builder.add_skill_group(
            format!(
                "{}-{}",
                primary.id_prefix,
                gpui_agents_hub_file_id(&primary_dir)
            ),
            name.clone(),
            primary_dir.clone(),
            description,
            files,
            profiles,
            versions.len(),
            labelled,
        );
    }
}

fn gpui_agents_hub_display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", gpui_path_string(rest)),
        Err(_) => gpui_path_string(path),
    }
}

impl GpuiAgentsHubCatalogBuilder {
    /// `add_group` for a merged skill row: files of the versions after the
    /// first carry `labelled` names, and the row records how many versions differ.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_skill_group(
        &mut self,
        group_id: String,
        name: String,
        root_path: PathBuf,
        description: String,
        files: Vec<PathBuf>,
        profiles: Vec<GpuiAgentsHubProfileItem>,
        variant_count: usize,
        labelled: Vec<(PathBuf, String)>,
    ) {
        let labels = labelled
            .into_iter()
            .filter_map(|(path, label)| fs::canonicalize(path).ok().map(|real| (real, label)))
            .collect::<HashMap<_, _>>();
        let resolved_files = files
            .into_iter()
            .filter_map(|candidate| {
                let mut item = self.file_item(&candidate, &root_path)?;
                let real = fs::canonicalize(&candidate).ok();
                if let Some(label) = real.and_then(|real| labels.get(&real)) {
                    item["name"] = serde_json::Value::String(label.clone());
                }
                Some(item)
            })
            .collect::<Vec<_>>();
        if resolved_files.is_empty() {
            return;
        }
        self.insert_open_path_if_allowed(&root_path);
        for profile in &profiles {
            self.insert_open_path_if_allowed(&profile.profile_path);
        }
        self.groups_by_tab
            .entry("skills")
            .or_default()
            .push(serde_json::json!({
                "description": description,
                "files": resolved_files,
                "id": group_id,
                "name": name,
                "path": gpui_path_string(&root_path),
                "profiles": profiles.into_iter().map(gpui_agents_hub_profile_json).collect::<Vec<_>>(),
                "variantCount": variant_count,
            }));
    }
}
