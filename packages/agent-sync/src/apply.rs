//! Execute a plan. The plan is recomputed from a fresh scan right before it
//! runs so what executes matches the disk, and only the caller's enabled groups
//! run. Failures are recorded and never roll back earlier steps: the plan is
//! idempotent, so rerunning finishes the job.

use crate::fsx;
use crate::plan::{build_plan, PlanGroupKind, PlanOp, PlanOptions, PlanVerb, SyncPlan};
use crate::scan::scan;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyFailure {
    pub op: PlanOp,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub generated_at: String,
    pub scope: String,
    pub enabled_groups: Vec<PlanGroupKind>,
    pub done: Vec<PlanOp>,
    pub failed: Vec<ApplyFailure>,
    pub skipped_keeps: usize,
    /// The plan that was executed, for display.
    pub plan: SyncPlan,
}

fn run_op(op: &PlanOp) -> Result<(), String> {
    let path = &op.abs_path;
    match op.verb {
        PlanVerb::Keep => Ok(()),
        PlanVerb::Unlink => fsx::remove_link(path).map_err(|e| e.to_string()),
        PlanVerb::Mkdir => fs::create_dir_all(path).map_err(|e| e.to_string()),
        PlanVerb::Backup => {
            let target = op
                .abs_target
                .as_ref()
                .ok_or("backup without a destination")?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::rename(path, target).map_err(|e| e.to_string())
        }
        PlanVerb::Link => {
            let target = op.abs_target.as_ref().ok_or("link without a target")?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if fs::symlink_metadata(path).is_ok() {
                return Err("something is already at this path".to_string());
            }
            let resolved = if target.is_absolute() {
                target.clone()
            } else {
                path.parent()
                    .map(|p| p.join(target))
                    .unwrap_or_else(|| target.clone())
            };
            let is_dir = fs::metadata(&resolved).map(|m| m.is_dir()).unwrap_or(true);
            if is_dir {
                fsx::create_dir_link(target, path).map_err(|e| e.to_string())
            } else {
                fsx::create_file_link(target, path).map_err(|e| e.to_string())
            }
        }
        PlanVerb::Write => {
            let content = op.content.as_deref().ok_or("write without content")?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, content).map_err(|e| e.to_string())
        }
        PlanVerb::Drop => Ok(()),
    }
}

fn prune_lock(lock_path: &Path, names: &[String]) -> Result<(), String> {
    let text = fs::read_to_string(lock_path).map_err(|e| e.to_string())?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let Some(skills) = value.get_mut("skills").and_then(|s| s.as_object_mut()) else {
        return Err("lock file has no skills object".to_string());
    };
    for name in names {
        skills.remove(name);
    }
    let rendered = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(lock_path, format!("{rendered}\n")).map_err(|e| e.to_string())
}

/// Scan, plan, and run the enabled groups of the plan.
pub fn apply(home: &Path, options: &PlanOptions, enabled: &[PlanGroupKind]) -> ApplyResult {
    let report = scan(home);
    let plan = build_plan(home, &report, options);
    let mut done = Vec::new();
    let mut failed = Vec::new();
    let mut skipped_keeps = 0;
    let mut drops: Vec<(std::path::PathBuf, String)> = Vec::new();
    for group in &plan.groups {
        if !enabled.contains(&group.kind) {
            continue;
        }
        for op in &group.ops {
            match op.verb {
                PlanVerb::Keep => skipped_keeps += 1,
                PlanVerb::Drop => {
                    if let Some(name) = &op.target {
                        drops.push((op.abs_path.clone(), name.clone()));
                    }
                }
                _ => match run_op(op) {
                    Ok(()) => done.push(op.clone()),
                    Err(error) => failed.push(ApplyFailure {
                        op: op.clone(),
                        error,
                    }),
                },
            }
        }
    }
    if let Some((lock_path, _)) = drops.first().cloned() {
        let names: Vec<String> = drops.iter().map(|(_, name)| name.clone()).collect();
        match prune_lock(&lock_path, &names) {
            Ok(()) => {
                for group in &plan.groups {
                    for op in group.ops.iter().filter(|op| op.verb == PlanVerb::Drop) {
                        done.push(op.clone());
                    }
                }
            }
            Err(error) => {
                for group in &plan.groups {
                    for op in group.ops.iter().filter(|op| op.verb == PlanVerb::Drop) {
                        failed.push(ApplyFailure {
                            op: op.clone(),
                            error: error.clone(),
                        });
                    }
                }
            }
        }
    }
    ApplyResult {
        generated_at: crate::now_iso8601(),
        scope: options.scope.as_str(),
        enabled_groups: enabled.to_vec(),
        done,
        failed,
        skipped_keeps,
        plan,
    }
}
