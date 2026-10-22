//! CDXC:SessionStatus 2026-09-25 WHY:
//! A Claude dynamic workflow starts its agents without Agent tool calls. The parent records one `local_workflow` launch (task id plus run id) and each agent writes `subagents/workflows/<runId>/agent-<id>.jsonl` beside a `meta.json` holding its label. Claude's footer folds the run into one "4/6 agents done" row, so the Subagents card read nothing but the lead's own Agent children. Read the run folder instead; one run id can be launched again (a resumed workflow), and each launch gets a new task id.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{read_transcript_file_version, MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES};
use crate::session_chat_fleet_transcript::timestamp;

#[derive(Clone)]
pub(crate) struct WorkflowRun {
    pub name: String,
    /// The latest launch of this run id.
    pub launched_at: i64,
    /// Set by the terminal notification for the latest launch; a relaunch clears it.
    pub finished_at: Option<i64>,
}

/// One Workflow call: what the chat's workflow card is drawn from.
#[derive(Clone)]
pub(crate) struct WorkflowLaunch {
    /// The `tool_use` id, which the chat's tool row names.
    pub call_id: String,
    pub task_id: String,
    pub run_id: String,
    pub name: String,
    /// The script's one-line `meta.description`, which Claude repeats as `summary`.
    pub summary: Option<String>,
    pub script_path: Option<String>,
    pub launched_at: i64,
    pub finished_at: Option<i64>,
    /// The terminal notification's `<status>` (`completed`, `failed`, `killed`…).
    pub status: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct WorkflowRuns {
    runs: HashMap<String, WorkflowRun>,
    run_by_task: HashMap<String, String>,
    /// Every launch in the order the parent recorded them.
    launches: Vec<WorkflowLaunch>,
    /// The inline script each Workflow call passed, by call id.
    scripts: HashMap<String, String>,
}

/// The longest script a workflow card carries; a longer one is cut at a line end.
const MAX_WORKFLOW_SCRIPT_BYTES: usize = 64 * 1024;

pub(crate) struct WorkflowAgent {
    pub id: String,
    pub path: PathBuf,
    pub run: WorkflowRun,
    pub label: Option<String>,
    pub launched_at: Option<i64>,
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

impl WorkflowRuns {
    /// Records a Workflow call's inline script, which only the call carries.
    pub(crate) fn call(&mut self, call_id: &str, input: &Value) {
        let Some(script) = text(input.get("script")) else {
            return;
        };
        let mut script = script.to_string();
        if script.len() > MAX_WORKFLOW_SCRIPT_BYTES {
            let cut = script[..MAX_WORKFLOW_SCRIPT_BYTES]
                .rfind('\n')
                .unwrap_or_else(|| {
                    (0..=MAX_WORKFLOW_SCRIPT_BYTES)
                        .rev()
                        .find(|at| script.is_char_boundary(*at))
                        .unwrap_or(0)
                });
            script.truncate(cut);
        }
        self.scripts.insert(call_id.to_string(), script);
    }

    /// Records the parent's `local_workflow` launch acknowledgement; `call_id` is the Workflow
    /// call the acknowledgement answers.
    pub(crate) fn launch(&mut self, result: &Value, at: i64, call_id: Option<&str>) {
        if text(result.get("taskType")) != Some("local_workflow")
            || text(result.get("status")) != Some("async_launched")
        {
            return;
        }
        let (Some(task), Some(run)) = (text(result.get("taskId")), text(result.get("runId")))
        else {
            return;
        };
        if !safe_id(run) {
            return;
        }
        self.run_by_task.insert(task.to_string(), run.to_string());
        if let Some(call_id) = call_id.filter(|id| !id.is_empty()) {
            self.launches.retain(|launch| launch.call_id != call_id);
            self.launches.push(WorkflowLaunch {
                call_id: call_id.to_string(),
                task_id: task.to_string(),
                run_id: run.to_string(),
                name: text(result.get("workflowName")).unwrap_or(run).to_string(),
                summary: text(result.get("summary")).map(str::to_string),
                script_path: text(result.get("scriptPath")).map(str::to_string),
                launched_at: at,
                finished_at: None,
                status: None,
            });
        }
        self.runs.insert(
            run.to_string(),
            WorkflowRun {
                name: text(result.get("workflowName")).unwrap_or(run).to_string(),
                launched_at: at,
                finished_at: None,
            },
        );
    }

    /// True when `task` is a workflow launch, whose terminal notification ends the run.
    pub(crate) fn finish(&mut self, task: &str, at: i64, status: &str) -> bool {
        if let Some(launch) = self
            .launches
            .iter_mut()
            .rev()
            .find(|launch| launch.task_id == task && launch.finished_at.is_none())
        {
            launch.finished_at = Some(at);
            launch.status = Some(status.to_string());
        }
        let Some(run) = self
            .run_by_task
            .get(task)
            .and_then(|run| self.runs.get_mut(run))
        else {
            return false;
        };
        // An older launch's late notification must not end a relaunch of the same run.
        if at >= run.launched_at {
            run.finished_at = Some(at);
        }
        true
    }

    /// Every Workflow launch, oldest first, with the script its call passed inline.
    pub(crate) fn launches(&self) -> impl Iterator<Item = (&WorkflowLaunch, Option<&str>)> {
        self.launches.iter().map(|launch| {
            (
                launch,
                self.scripts.get(&launch.call_id).map(String::as_str),
            )
        })
    }

    /// The agents of every run that is still going. A finished run cannot list anything:
    /// its agents are over and the footer row that kept them visible is gone.
    pub(crate) fn running_agents(&self, subagents: &Path) -> io::Result<Vec<WorkflowAgent>> {
        let mut agents = Vec::new();
        for (run_id, run) in &self.runs {
            if run.finished_at.is_some() {
                continue;
            }
            let files = match fs::read_dir(subagents.join("workflows").join(run_id)) {
                Ok(files) => files,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            for entry in files {
                let path = entry?.path();
                let Some(id) = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("agent-"))
                    .and_then(|name| name.strip_suffix(".jsonl"))
                    .filter(|id| safe_id(id))
                else {
                    continue;
                };
                let label = match File::open(path.with_extension("meta.json")) {
                    Ok(file) => {
                        let mut bytes = Vec::new();
                        file.take(64 * 1024).read_to_end(&mut bytes)?;
                        serde_json::from_slice::<Value>(&bytes)
                            .ok()
                            .and_then(|meta| text(meta.get("description")).map(str::to_string))
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e),
                };
                agents.push(WorkflowAgent {
                    id: id.to_string(),
                    launched_at: first_record_time(&path)?,
                    path,
                    run: run.clone(),
                    label,
                });
            }
        }
        Ok(agents)
    }
}

/// A workflow agent has no launch record in the parent; its transcript's first record is its start.
fn first_record_time(path: &Path) -> io::Result<Option<i64>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (String, i64)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let identity = read_transcript_file_version(path)?.identity;
    if let Some(at) = cache.lock().ok().and_then(|cache| {
        cache
            .get(path)
            .filter(|(known, _)| *known == identity)
            .map(|(_, at)| *at)
    }) {
        return Ok(Some(at));
    }
    let mut line = Vec::new();
    BufReader::new(File::open(path)?)
        .take(MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES as u64)
        .read_until(b'\n', &mut line)?;
    // A first record still being written has no time yet.
    let Some(at) = (line.last() == Some(&b'\n'))
        .then(|| serde_json::from_slice::<Value>(&line).ok())
        .flatten()
        .and_then(|record| timestamp(&record))
    else {
        return Ok(None);
    };
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (identity, at));
    }
    Ok(Some(at))
}

/// A workflow agent's transcript, which lives in its run's folder rather than beside the Agent children.
pub(crate) fn find_workflow_transcript(subagents: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    if !safe_id(id) {
        return Ok(None);
    }
    let runs = match fs::read_dir(subagents.join("workflows")) {
        Ok(runs) => runs,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    for run in runs {
        let path = run?.path().join(format!("agent-{id}.jsonl"));
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
