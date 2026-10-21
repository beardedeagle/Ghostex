//! Turn a report into an ordered list of filesystem operations, grouped so a
//! user can switch a whole group off before applying.
//!
//! CDXC:AgentSync 2026-09-16 WHY:
//! Five verbs only (link, write, backup, unlink, keep) plus mkdir and the opt-in lock drop.
//! There is no copy and no delete, so every step can be undone by hand. Dangling links are
//! removed without a backup.
//!
//! CDXC:AgentSync 2026-10-14 WHY:
//! Backups used to sit next to the original as `<name>.pre-sync-<stamp>.bak`. Inside a skills
//! folder every agent and the Agents Hub then read that backup as one more skill, so the same
//! skill showed up many times. Anything in the way is now moved to
//! `<Ghostex state>/agent-sync-backups/<stamp>/<path under HOME>` instead (`fsx::backup_path`).
//!
//! CDXC:AgentSync 2026-10-14 DECISION:
//! User: add a plan group, on by default, that removes only links pointing into `~/.agents/skills`
//! from the folders of agents that read that folder themselves (`RemoveRedundantLinks`), and move
//! leftover `<name>.pre-sync-*.bak` folders out of agents' skills folders into the backup folder
//! (`MoveBackups`): "Move, never delete, for other users."

use crate::catalog::source_root;
use crate::fsx;
use crate::pointer::{self, InstructionState};
use crate::scan::{
    AgentReport, HooksState, LockLinkState, SkillDirState, SkillEntryState, SyncReport,
};
use crate::SyncScope;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanVerb {
    Link,
    Write,
    Backup,
    Unlink,
    Keep,
    Mkdir,
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PlanGroupKind {
    RemoveDangling,
    RemoveRedundantLinks,
    MoveBackups,
    ConvertWholeFolder,
    PerSkillLinks,
    PointerFiles,
    Hooks,
    PruneLock,
}

impl PlanGroupKind {
    pub const ALL: [PlanGroupKind; 8] = [
        PlanGroupKind::RemoveDangling,
        PlanGroupKind::RemoveRedundantLinks,
        PlanGroupKind::MoveBackups,
        PlanGroupKind::ConvertWholeFolder,
        PlanGroupKind::PerSkillLinks,
        PlanGroupKind::PointerFiles,
        PlanGroupKind::Hooks,
        PlanGroupKind::PruneLock,
    ];

    pub fn parse(value: &str) -> Option<PlanGroupKind> {
        match value.trim() {
            "removeDangling" | "remove-dangling" | "dangling" => {
                Some(PlanGroupKind::RemoveDangling)
            }
            "removeRedundantLinks" | "remove-redundant-links" | "redundant-links" | "redundant" => {
                Some(PlanGroupKind::RemoveRedundantLinks)
            }
            "moveBackups" | "move-backups" | "backups" => Some(PlanGroupKind::MoveBackups),
            "convertWholeFolder" | "convert-whole-folder" | "whole-folder" => {
                Some(PlanGroupKind::ConvertWholeFolder)
            }
            "perSkillLinks" | "per-skill-links" | "skills" | "links" => {
                Some(PlanGroupKind::PerSkillLinks)
            }
            "pointerFiles" | "pointer-files" | "pointers" | "mds" => {
                Some(PlanGroupKind::PointerFiles)
            }
            "hooks" => Some(PlanGroupKind::Hooks),
            "pruneLock" | "prune-lock" | "lock" => Some(PlanGroupKind::PruneLock),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PlanGroupKind::RemoveDangling => "removeDangling",
            PlanGroupKind::RemoveRedundantLinks => "removeRedundantLinks",
            PlanGroupKind::MoveBackups => "moveBackups",
            PlanGroupKind::ConvertWholeFolder => "convertWholeFolder",
            PlanGroupKind::PerSkillLinks => "perSkillLinks",
            PlanGroupKind::PointerFiles => "pointerFiles",
            PlanGroupKind::Hooks => "hooks",
            PlanGroupKind::PruneLock => "pruneLock",
        }
    }

    fn title(self) -> &'static str {
        match self {
            PlanGroupKind::RemoveDangling => "Remove dangling links",
            PlanGroupKind::RemoveRedundantLinks => {
                "Remove links agents that read ~/.agents/skills do not need"
            }
            PlanGroupKind::MoveBackups => "Move backup folders out of skills folders",
            PlanGroupKind::ConvertWholeFolder => "Convert whole-folder links to per-skill links",
            PlanGroupKind::PerSkillLinks => "Per-skill links",
            PlanGroupKind::PointerFiles => "Instruction pointer files",
            PlanGroupKind::Hooks => "Hook scripts and lock file links",
            PlanGroupKind::PruneLock => "Prune stale lock entries",
        }
    }

    fn enabled_by_default(self) -> bool {
        !matches!(self, PlanGroupKind::PruneLock)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOp {
    pub verb: PlanVerb,
    /// Display path (`~/...`).
    pub path: String,
    /// Link target, backup destination, or lock entry name, depending on the verb.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Absolute path for the applier; never shown.
    #[serde(skip)]
    pub abs_path: PathBuf,
    #[serde(skip)]
    pub abs_target: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanGroup {
    pub kind: PlanGroupKind,
    pub title: String,
    pub enabled_by_default: bool,
    pub ops: Vec<PlanOp>,
    pub change_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub links: usize,
    pub writes: usize,
    pub backups: usize,
    pub unlinks: usize,
    pub keeps: usize,
    pub mkdirs: usize,
    pub drops: usize,
    /// Every non-keep op in groups that are enabled by default.
    pub default_changes: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub generated_at: String,
    pub scope: String,
    pub stamp: String,
    /// Display path of the folder this plan's backups go to (`~/.../<stamp>`).
    pub backup_dir: String,
    pub groups: Vec<PlanGroup>,
    pub summary: PlanSummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerPolicy {
    /// Back the file up and write only the pointer (the user's default, DECISION 2B).
    Replace,
    /// Keep the agent's own lines under the pointer.
    Prepend,
    /// Report only.
    LeaveAlone,
}

#[derive(Clone, Debug)]
pub struct PlanOptions {
    pub scope: SyncScope,
    pub stamp: String,
    /// Backups go to `<backup_root>/<stamp>/...`.
    pub backup_root: PathBuf,
    pub pointer_policy: PointerPolicy,
    pub link_hooks: bool,
    pub link_lock: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            scope: SyncScope::All,
            stamp: crate::now_backup_stamp(),
            backup_root: fsx::default_backup_root(),
            pointer_policy: PointerPolicy::Replace,
            link_hooks: true,
            link_lock: true,
        }
    }
}

struct Builder<'a> {
    home: &'a Path,
    stamp: &'a str,
    backup_root: &'a Path,
    groups: Vec<Vec<PlanOp>>,
}

impl<'a> Builder<'a> {
    fn push(&mut self, kind: PlanGroupKind, op: PlanOp) {
        self.groups[kind as usize].push(op);
    }

    fn op(&self, verb: PlanVerb, abs_path: PathBuf, agent_id: Option<&str>) -> PlanOp {
        PlanOp {
            verb,
            path: fsx::display_path(&abs_path, self.home),
            target: None,
            note: None,
            agent_id: agent_id.map(str::to_string),
            content: None,
            abs_path,
            abs_target: None,
        }
    }

    fn with_target(&self, mut op: PlanOp, abs_target: PathBuf) -> PlanOp {
        op.target = Some(fsx::display_path(&abs_target, self.home));
        op.abs_target = Some(abs_target);
        op
    }

    fn link_op(&self, link: PathBuf, target: &Path, agent_id: &str, relative: bool) -> PlanOp {
        let mut op = self.op(PlanVerb::Link, link.clone(), Some(agent_id));
        let written = if relative {
            fsx::link_target_for(&link, target)
        } else {
            target.to_path_buf()
        };
        op.target = Some(written.display().to_string());
        op.abs_target = Some(written);
        op
    }

    fn backup_op(&self, path: PathBuf, agent_id: Option<&str>, note: &str) -> PlanOp {
        let backup = fsx::backup_path(&path, self.home, self.backup_root, self.stamp);
        let mut op = self.with_target(self.op(PlanVerb::Backup, path, agent_id), backup);
        op.note = Some(note.to_string());
        op
    }

    fn keep_op(&self, path: PathBuf, agent_id: Option<&str>, note: &str) -> PlanOp {
        let mut op = self.op(PlanVerb::Keep, path, agent_id);
        op.note = Some(note.to_string());
        op
    }
}

fn abs_from_display(home: &Path, display: &str) -> PathBuf {
    match display.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if display == "~" => home.to_path_buf(),
        None => PathBuf::from(display),
    }
}

fn plan_agent(b: &mut Builder<'_>, agent: &AgentReport, options: &PlanOptions) {
    let Some(spec) = &agent.spec else {
        return;
    };
    let source = source_root(b.home);
    let source_skills = source.join("skills");
    let agent_id = agent.id.as_str();

    if let (Some(skills), Some(dir)) = (&agent.skills, spec.skills_dir.as_ref()) {
        for backup in &skills.backups {
            let path = abs_from_display(b.home, backup);
            let op = b.backup_op(path, Some(agent_id), "left over from an earlier sync");
            b.push(PlanGroupKind::MoveBackups, op);
        }
        match skills.dir_state {
            SkillDirState::WholeFolderLink if agent.universal => {
                let mut unlink = b.op(PlanVerb::Unlink, dir.clone(), Some(agent_id));
                unlink.note = Some(
                    "folder link to ~/.agents/skills; this agent reads that folder itself"
                        .to_string(),
                );
                b.push(PlanGroupKind::RemoveRedundantLinks, unlink);
            }
            SkillDirState::WholeFolderLink => {
                let mut unlink = b.op(PlanVerb::Unlink, dir.clone(), Some(agent_id));
                unlink.note = Some(
                    "folder symlink; replaced by a real folder of per-skill links".to_string(),
                );
                b.push(PlanGroupKind::ConvertWholeFolder, unlink);
                b.push(
                    PlanGroupKind::ConvertWholeFolder,
                    b.op(PlanVerb::Mkdir, dir.clone(), Some(agent_id)),
                );
                for entry in &skills.entries {
                    let op = b.link_op(
                        dir.join(&entry.name),
                        &source_skills.join(&entry.name),
                        agent_id,
                        true,
                    );
                    b.push(PlanGroupKind::ConvertWholeFolder, op);
                }
            }
            SkillDirState::OtherLink => {
                let op = b.keep_op(
                    dir.clone(),
                    Some(agent_id),
                    "skills folder is a link to somewhere else; review it by hand",
                );
                b.push(PlanGroupKind::PerSkillLinks, op);
            }
            SkillDirState::NotADir => {
                let op = b.keep_op(
                    dir.clone(),
                    Some(agent_id),
                    "not a folder; review it by hand",
                );
                b.push(PlanGroupKind::PerSkillLinks, op);
            }
            SkillDirState::Missing | SkillDirState::RealDir => {
                let mut need_mkdir = skills.dir_state == SkillDirState::Missing;
                for entry in &skills.entries {
                    let path = dir.join(&entry.name);
                    let source_path = source_skills.join(&entry.name);
                    match entry.state {
                        SkillEntryState::Dangling => {
                            let mut op = b.op(PlanVerb::Unlink, path, Some(agent_id));
                            op.note = entry
                                .link_target
                                .as_ref()
                                .map(|t| format!("target missing: {t}"));
                            b.push(PlanGroupKind::RemoveDangling, op);
                        }
                        SkillEntryState::Linked if agent.universal => {
                            let mut op = b.op(PlanVerb::Unlink, path, Some(agent_id));
                            op.note = Some("this agent reads ~/.agents/skills itself".to_string());
                            b.push(PlanGroupKind::RemoveRedundantLinks, op);
                        }
                        SkillEntryState::Linked => {
                            let op = b.keep_op(path, Some(agent_id), "already linked");
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::LinkedElsewhere => {
                            let note = format!(
                                "linked to {}, not the source; kept",
                                entry.link_target.clone().unwrap_or_default()
                            );
                            let op = b.keep_op(path, Some(agent_id), &note);
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::CopyIdentical => {
                            let backup = b.backup_op(
                                path.clone(),
                                Some(agent_id),
                                "identical to the source copy",
                            );
                            b.push(PlanGroupKind::PerSkillLinks, backup);
                            if !agent.universal {
                                let op = b.link_op(path, &source_path, agent_id, true);
                                b.push(PlanGroupKind::PerSkillLinks, op);
                            }
                        }
                        SkillEntryState::CopyDrifted => {
                            let op = b.keep_op(
                                path,
                                Some(agent_id),
                                "differs from the source; kept for review",
                            );
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::OnlyHere => {
                            let op =
                                b.keep_op(path, Some(agent_id), "only here; not in the source");
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::Missing => {
                            if need_mkdir {
                                need_mkdir = false;
                                b.push(
                                    PlanGroupKind::PerSkillLinks,
                                    b.op(PlanVerb::Mkdir, dir.clone(), Some(agent_id)),
                                );
                            }
                            let op = b.link_op(path, &source_path, agent_id, true);
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::ViaWholeFolder => {}
                    }
                }
                if agent.universal && skills.dir_state == SkillDirState::RealDir {
                    let op = b.keep_op(
                        dir.clone(),
                        Some(agent_id),
                        "reads ~/.agents/skills directly; no links added",
                    );
                    b.push(PlanGroupKind::PerSkillLinks, op);
                }
            }
        }
    }

    if let (Some(instructions), Some(path)) = (&agent.instructions, spec.instruction_file.as_ref())
    {
        let content = pointer::pointer_content(spec.instruction_kind);
        match instructions.state {
            InstructionState::Pointer => {
                let op = b.keep_op(path.clone(), Some(agent_id), "already the pointer");
                b.push(PlanGroupKind::PointerFiles, op);
            }
            InstructionState::Missing => {
                let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                op.content = Some(content);
                op.note = Some("pointer to ~/.agents/main.md".to_string());
                b.push(PlanGroupKind::PointerFiles, op);
            }
            InstructionState::LegacyPointer | InstructionState::OtherContent => {
                match options.pointer_policy {
                    PointerPolicy::Replace => {
                        let note = if instructions.state == InstructionState::LegacyPointer {
                            "older pointer wording"
                        } else {
                            "has content of its own"
                        };
                        b.push(
                            PlanGroupKind::PointerFiles,
                            b.backup_op(path.clone(), Some(agent_id), note),
                        );
                        let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                        op.content = Some(content);
                        b.push(PlanGroupKind::PointerFiles, op);
                    }
                    PointerPolicy::Prepend => {
                        let existing = std::fs::read_to_string(path).unwrap_or_default();
                        let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                        op.content = Some(format!("{content}\n{existing}"));
                        op.note = Some("pointer prepended above the existing content".to_string());
                        b.push(PlanGroupKind::PointerFiles, op);
                    }
                    PointerPolicy::LeaveAlone => {
                        let op = b.keep_op(
                            path.clone(),
                            Some(agent_id),
                            "has other content; left alone",
                        );
                        b.push(PlanGroupKind::PointerFiles, op);
                    }
                }
            }
            InstructionState::Symlink => {
                let op = b.keep_op(
                    path.clone(),
                    Some(agent_id),
                    "is a symlink; review it by hand",
                );
                b.push(PlanGroupKind::PointerFiles, op);
            }
            InstructionState::NotAFile => {
                let op = b.keep_op(
                    path.clone(),
                    Some(agent_id),
                    "not a file; review it by hand",
                );
                b.push(PlanGroupKind::PointerFiles, op);
            }
        }
    }

    if options.link_hooks {
        if let (Some(hooks), Some(dir)) = (&agent.hooks, spec.hooks_dir.as_ref()) {
            let source_hooks = source.join("hooks");
            match hooks.state {
                HooksState::Linked => {
                    let op = b.keep_op(dir.clone(), Some(agent_id), "already linked");
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::Missing => {
                    let op = b.link_op(dir.clone(), &source_hooks, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::RealDir => {
                    b.push(
                        PlanGroupKind::Hooks,
                        b.backup_op(dir.clone(), Some(agent_id), "agent-local hooks folder"),
                    );
                    let op = b.link_op(dir.clone(), &source_hooks, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::NothingShared => {}
                HooksState::OtherLink => {
                    let op = b.keep_op(
                        dir.clone(),
                        Some(agent_id),
                        "linked somewhere else; review it by hand",
                    );
                    b.push(PlanGroupKind::Hooks, op);
                }
            }
        }
    }

    if options.link_lock {
        if let (Some(lock), Some(path)) = (&agent.lock, spec.lock_link.as_ref()) {
            let source_lock = source.join(".skill-lock.json");
            match lock.state {
                LockLinkState::Linked => {
                    let op = b.keep_op(path.clone(), Some(agent_id), "already linked");
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::Missing => {
                    let op = b.link_op(path.clone(), &source_lock, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::RealFile => {
                    b.push(
                        PlanGroupKind::Hooks,
                        b.backup_op(path.clone(), Some(agent_id), "agent-local lock file"),
                    );
                    let op = b.link_op(path.clone(), &source_lock, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::OtherLink => {
                    let op = b.keep_op(
                        path.clone(),
                        Some(agent_id),
                        "linked somewhere else; review it by hand",
                    );
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::NothingShared => {}
            }
        }
    }
}

/// Build the plan for `report`. `scope` limits agent work to one agent; the
/// source-level groups (broken source links, lock pruning) are included only
/// for the `all` scope.
pub fn build_plan(home: &Path, report: &SyncReport, options: &PlanOptions) -> SyncPlan {
    let mut builder = Builder {
        home,
        stamp: &options.stamp,
        backup_root: &options.backup_root,
        groups: vec![Vec::new(); PlanGroupKind::ALL.len()],
    };

    if options.scope == SyncScope::All {
        for skill in report.source.skills.iter().filter(|s| s.broken) {
            let mut op = builder.op(PlanVerb::Unlink, abs_from_display(home, &skill.path), None);
            op.note =
                Some("broken link inside the source; reinstall the skill afterwards".to_string());
            builder.push(PlanGroupKind::RemoveDangling, op);
        }
    }

    for agent in &report.agents {
        match &options.scope {
            SyncScope::All => {}
            SyncScope::Agent(id) if id == &agent.id => {}
            SyncScope::Agent(_) => continue,
        }
        if agent.detected {
            plan_agent(&mut builder, agent, options);
        } else if let (Some(skills), Some(spec)) = (&agent.skills, &agent.spec) {
            // Leftover folders of agents that were never installed: only remove their dangling links.
            if let Some(dir) = spec.skills_dir.as_ref() {
                for entry in skills
                    .entries
                    .iter()
                    .filter(|e| e.state == SkillEntryState::Dangling)
                {
                    let mut op =
                        builder.op(PlanVerb::Unlink, dir.join(&entry.name), Some(&agent.id));
                    op.note = Some(
                        "leftover of an earlier install for an agent that is not set up here"
                            .to_string(),
                    );
                    builder.push(PlanGroupKind::RemoveDangling, op);
                }
            }
            for backup in &skills.backups {
                let path = abs_from_display(home, backup);
                let op = builder.backup_op(path, Some(&agent.id), "left over from an earlier sync");
                builder.push(PlanGroupKind::MoveBackups, op);
            }
        }
    }

    if options.scope == SyncScope::All && report.source.lock.exists {
        let lock_path = abs_from_display(home, &report.source.lock.path);
        for entry in &report.source.lock.stale_entries {
            let mut op = builder.op(PlanVerb::Drop, lock_path.clone(), None);
            op.target = Some(entry.clone());
            op.note = Some("no skill folder in ~/.agents/skills".to_string());
            builder.push(PlanGroupKind::PruneLock, op);
        }
    }

    let mut summary = PlanSummary::default();
    let groups: Vec<PlanGroup> = PlanGroupKind::ALL
        .iter()
        .zip(builder.groups)
        .map(|(kind, ops)| {
            let mut change_count = 0;
            for op in &ops {
                match op.verb {
                    PlanVerb::Link => summary.links += 1,
                    PlanVerb::Write => summary.writes += 1,
                    PlanVerb::Backup => summary.backups += 1,
                    PlanVerb::Unlink => summary.unlinks += 1,
                    PlanVerb::Keep => summary.keeps += 1,
                    PlanVerb::Mkdir => summary.mkdirs += 1,
                    PlanVerb::Drop => summary.drops += 1,
                }
                if op.verb != PlanVerb::Keep {
                    change_count += 1;
                }
            }
            if kind.enabled_by_default() {
                summary.default_changes += change_count;
            }
            PlanGroup {
                kind: *kind,
                title: kind.title().to_string(),
                enabled_by_default: kind.enabled_by_default(),
                ops,
                change_count,
            }
        })
        .collect();

    SyncPlan {
        generated_at: crate::now_iso8601(),
        scope: options.scope.as_str(),
        stamp: options.stamp.clone(),
        backup_dir: fsx::display_path(&options.backup_root.join(&options.stamp), home),
        groups,
        summary,
    }
}
