//! The Files tab: the files of the user's projects that match the search, the current project
//! first. Each machine's gxserver lists and ranks its own projects' files
//! (`/api/searchProjectFiles`, packages/project-docs/src/file_search.rs); this tab only says which
//! projects to ask about and turns the answers into rows.
//!
//! CDXC:AppModal 2026-10-17 DECISION:
//! User: Quick Access (Cmd+P) gets a Files tab next to its other tabs that searches the files of the user's projects, the current project first; a picked file opens where Settings > File opening says, the Files view by default, never Code unless the user picked Code. Phone later, desktop only now.
//! SEE-ALSO: `openQuickAccessFiles` in packages/settings-catalog/src/hotkey_definitions.rs (Cmd+O, and Cmd+5 inside Quick Access), `file_open_target_for_path` in apps/desktop/src/app/native_docs/entry.rs.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::controller::{QuickAccessContext, QuickAccessController, QuickAccessEffect};
use super::data::QuickAccessData;
use super::wire::{QuickAccessGroup, QuickAccessIcon, QuickAccessRow};
use crate::keys::ProjectKey;
use crate::sidebar_view::text::js_trim;

/// How many files gxserver returns per project.
const FILES_PER_PROJECT: u64 = 50;
/// The current project shows every file gxserver returned; the others show their best few, so the
/// current project's matches are never pushed off the list.
const OTHER_PROJECT_ROWS: usize = 8;

/// One project the tab searches, in the order the tab lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchProject {
    /// The sidebar's project id (`remote:…` for a remote machine's project).
    pub(crate) workspace_id: String,
    /// `None` for this computer.
    pub(crate) machine_id: Option<String>,
    /// The id the project's own gxserver knows it by.
    pub(crate) project_id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectFiles {
    pub(crate) project_id: String,
    pub(crate) root_path: String,
    pub(crate) files: Vec<String>,
}

/// What the tab holds between keystrokes.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilesTabState {
    /// The newest answer from each machine ("" is this computer). An older answer stays on screen
    /// until the next one arrives, so typing never blanks the list.
    pub(crate) answers: BTreeMap<String, Vec<ProjectFiles>>,
    /// The request each machine is expected to answer next.
    pub(crate) pending: BTreeMap<String, String>,
}

fn machine_key(machine_id: Option<&str>) -> String {
    machine_id.unwrap_or_default().to_string()
}

/// Every project in the sidebar once, the active project first.
pub(crate) fn search_projects(data: &QuickAccessData) -> Vec<SearchProject> {
    let active = data.hud["activeProjectId"]
        .as_str()
        .filter(|id| !id.is_empty());
    let mut projects: Vec<SearchProject> = Vec::new();
    for group in &data.groups {
        let Some(workspace_id) = group
            .editor_project_id
            .as_deref()
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if projects
            .iter()
            .any(|project| project.workspace_id == workspace_id)
        {
            continue;
        }
        let Some(key) = ProjectKey::parse_workspace_project_id(workspace_id) else {
            continue;
        };
        projects.push(SearchProject {
            workspace_id: workspace_id.to_string(),
            machine_id: key.machine.remote_id().map(str::to_string),
            project_id: key.project_id,
            name: group.title.clone(),
        });
    }
    if let Some(index) = active.and_then(|active| {
        projects
            .iter()
            .position(|project| project.workspace_id == active)
    }) {
        let current = projects.remove(index);
        projects.insert(0, current);
    }
    projects
}

/// The glyph a file row shows, the way the Files view's list picks it.
fn file_icon(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tif" | "tiff"
        | "heic" | "heif" | "avif" => "photo",
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "ogv" => "movie",
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" | "opus" => "music",
        "md" | "markdown" | "mdx" => "markdown",
        "html" | "htm" => "file-type-html",
        "txt" | "log" | "" => "file-text",
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "go" | "rb" | "java" | "kt"
        | "swift" | "c" | "h" | "cpp" | "cc" | "hpp" | "cs" | "php" | "lua" | "zig" | "sh"
        | "bash" | "zsh" | "css" | "scss" | "json" | "jsonc" | "toml" | "yaml" | "yml" | "xml"
        | "sql" | "vue" | "svelte" | "astro" | "graphql" | "proto" | "ex" | "exs" | "scala"
        | "dart" => "file-code",
        _ => "file",
    }
}

fn row_key(project: &SearchProject, path: &str) -> String {
    format!("file:{}\u{1f}{path}", project.workspace_id)
}

/// The project and file behind a row key.
fn split_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix("file:")?.split_once('\u{1f}')
}

impl QuickAccessController {
    /// Asks every machine with a project in the sidebar for the files matching the query: one
    /// request per machine naming its projects. An empty query asks only for the current
    /// project's files nearest its root.
    pub(crate) fn request_project_files(
        &mut self,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let query = js_trim(&self.query).to_string();
        let mut projects = search_projects(context.data);
        if query.is_empty() {
            projects.truncate(1);
        }
        let mut by_machine: BTreeMap<String, (Option<String>, Vec<String>)> = BTreeMap::new();
        for project in &projects {
            by_machine
                .entry(machine_key(project.machine_id.as_deref()))
                .or_insert_with(|| (project.machine_id.clone(), Vec::new()))
                .1
                .push(project.project_id.clone());
        }
        // A machine no longer asked about keeps nothing on screen.
        self.files
            .answers
            .retain(|machine, _| by_machine.contains_key(machine));
        self.files.pending.clear();
        for (machine, (machine_id, project_ids)) in by_machine {
            let request_id = self.next_request_id("project-files", context.clock);
            self.files.pending.insert(machine, request_id.clone());
            let mut message = json!({
                "limit": FILES_PER_PROJECT,
                "projectIds": project_ids,
                "query": query,
                "requestId": request_id,
                "type": "requestProjectFiles",
            });
            if let Some(machine_id) = machine_id {
                message["machineId"] = json!(machine_id);
            }
            effects.push(QuickAccessEffect::Post(message));
        }
    }

    /// A machine's answer (`projectFilesResult`). Returns false for an answer nobody waits for.
    pub(crate) fn receive_project_files(&mut self, message: &Value) -> bool {
        let machine = machine_key(message["machineId"].as_str());
        let request_id = message["requestId"].as_str().unwrap_or_default();
        if self.files.pending.get(&machine).map(String::as_str) != Some(request_id) {
            return false;
        }
        self.files.pending.remove(&machine);
        let projects = message["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|project| ProjectFiles {
                project_id: project["projectId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                root_path: project["rootPath"].as_str().unwrap_or_default().to_string(),
                files: project["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            })
            .collect();
        self.files.answers.insert(machine, projects);
        true
    }

    pub(crate) fn files_loading(&self) -> bool {
        self.files.answers.is_empty() && !self.files.pending.is_empty()
    }

    fn project_answer(&self, project: &SearchProject) -> Option<&ProjectFiles> {
        self.files
            .answers
            .get(&machine_key(project.machine_id.as_deref()))?
            .iter()
            .find(|answer| answer.project_id == project.project_id)
    }

    /// One group per project with matches, headed by the project's name, the current project first.
    pub(crate) fn file_groups(&self, data: &QuickAccessData) -> Vec<QuickAccessGroup> {
        let mut groups = Vec::new();
        for (index, project) in search_projects(data).iter().enumerate() {
            let Some(answer) = self.project_answer(project) else {
                continue;
            };
            let limit = if index == 0 {
                usize::MAX
            } else {
                OTHER_PROJECT_ROWS
            };
            let rows: Vec<QuickAccessRow> = answer
                .files
                .iter()
                .take(limit)
                .map(|path| {
                    let (folder, name) = match path.rsplit_once('/') {
                        Some((folder, name)) => (folder, name),
                        None => ("", path.as_str()),
                    };
                    QuickAccessRow::File {
                        key: row_key(project, path),
                        title: name.to_string(),
                        folder: folder.to_string(),
                        icon: QuickAccessIcon::asset(file_icon(path), None),
                        tooltip: path.clone(),
                    }
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            groups.push(QuickAccessGroup {
                key: format!("files:{}", project.workspace_id),
                heading: project.name.clone(),
                separated: !groups.is_empty(),
                rows,
            });
        }
        groups
    }

    pub(crate) fn files_empty_copy(&self, data: &QuickAccessData) -> &'static str {
        if search_projects(data).is_empty() {
            "Add a project to search its files."
        } else if js_trim(&self.query).is_empty() {
            "No files in this project."
        } else {
            "No files match that search."
        }
    }

    /// The project, its folder on its own machine and the file a row stands for.
    fn find_file(
        &self,
        key: &str,
        data: &QuickAccessData,
    ) -> Option<(SearchProject, String, String)> {
        let (workspace_id, path) = split_key(key)?;
        let project = search_projects(data)
            .into_iter()
            .find(|project| project.workspace_id == workspace_id)?;
        let answer = self.project_answer(&project)?;
        answer.files.iter().find(|file| file.as_str() == path)?;
        let root = answer.root_path.clone();
        Some((project, root, path.to_string()))
    }

    /// Enter on a file: the host opens it where Settings > File opening says.
    pub(crate) fn activate_file(
        &mut self,
        key: &str,
        context: &QuickAccessContext<'_>,
        effects: &mut Vec<QuickAccessEffect>,
    ) {
        let Some((project, root, path)) = self.find_file(key, context.data) else {
            return;
        };
        let mut message = json!({
            "path": path,
            "projectId": project.workspace_id,
            "rootPath": root,
            "type": "openProjectFile",
        });
        if let Some(machine_id) = &project.machine_id {
            message["machineId"] = json!(machine_id);
            message["remoteProjectId"] = json!(project.project_id);
        }
        effects.push(QuickAccessEffect::Post(message));
        self.close(effects);
    }

    /// The file's full path on its own machine.
    pub(crate) fn file_absolute_path(&self, key: &str, data: &QuickAccessData) -> Option<String> {
        let (_, root, path) = self.find_file(key, data)?;
        let separator = if root.contains('\\') && !root.contains('/') {
            "\\"
        } else {
            "/"
        };
        Some(format!(
            "{}{separator}{}",
            root.trim_end_matches(['/', '\\']),
            if separator == "\\" {
                path.replace('/', "\\")
            } else {
                path
            }
        ))
    }

    pub(crate) fn has_file(&self, key: &str, data: &QuickAccessData) -> bool {
        self.find_file(key, data).is_some()
    }
}
