//! How each agent spells a skill invocation: in the `$` picker's rows, in the composer's skill pill,
//! and in the text the agent receives.
//!
//! CDXC:AgentSkills 2026-09-27 DECISION:
//! User: "let's invoke skills in claude code like /skill-name" and "for codex the actual way to invoke skills is just $", and the skill pill in the GPUI and React Native composers shows that spelling. A Claude agent handed `[$cua-driver](…/SKILL.md)` answered that the skill only loads from the slash command itself, "without the link or `$`". So a Claude draft keeps a `[/name](…/SKILL.md)` link (that is what makes it a pill) and Claude receives the bare `/name`; Codex keeps the `[$name](…/SKILL.md)` link its own composer writes. The `$` key still opens the picker for every agent.
//! SEE-ALSO: `server/src/session_chat_skill_invocation.rs` types the same bare `/name` into Claude's terminal; both sides must convert exactly the same links, or the optimistic echo stops matching the turn Claude records.

use std::borrow::Cow;

use crate::composer::reference_pills::{composer_references, ReferenceKind};
use crate::composer::text::Utf16Text;

/// Whether the agent invokes a skill as a slash command. Copilot CLI runs a skill as `/name` too
/// (1.0.98 completes `/figm` to the `/figma` skill).
fn invokes_skills_with_slash(agent: Option<&str>) -> bool {
    matches!(
        agent,
        Some("claude" | "openclaude" | "claude-code" | "copilot")
    )
}

/// The character the agent invokes a skill with: `/` for Claude Code, `$` for Codex and every
/// other agent.
pub fn skill_sigil(agent: Option<&str>) -> char {
    if invokes_skills_with_slash(agent) {
        '/'
    } else {
        '$'
    }
}

/// `/name` or `$name`, the way the agent would type it.
pub fn skill_invocation(name: &str, agent: Option<&str>) -> String {
    format!("{}{name}", skill_sigil(agent))
}

/// The line every new orchestrator's first message starts with, in the agent's own skill
/// spelling: "Read and adhere to this skill: /ghostex-orchestrator" for Claude Code and Copilot,
/// `$ghostex-orchestrator` for every other agent. gxserver sends it, and the New Orchestrator
/// dialog shows it as the locked first line.
/// SEE-ALSO: server/src/coordinators/role.rs (`orchestrator_first_message`, the CDXC:Coordinators 2026-10-17 decision).
pub fn orchestrator_skill_line(agent: Option<&str>) -> String {
    format!(
        "Read and adhere to this skill: {}",
        skill_invocation("ghostex-orchestrator", agent)
    )
}

/// The name in a `$name` or `/name` pill label, when it is one gxserver's scanner also accepts,
/// so the two sides convert the same links.
fn skill_name(label: &str) -> Option<&str> {
    label.strip_prefix(['$', '/']).filter(|name| {
        !name.is_empty()
            && name.chars().all(|character| {
                character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
            })
    })
}

/// The text the agent receives for a composer draft: for Claude, every skill pill becomes its bare
/// `/name`; every other agent gets the draft unchanged.
///
/// gxserver does the conversion on its way to the terminal; the core runs it only to know what the
/// agent will record, for the send's classification and its optimistic echo.
pub fn agent_skill_text<'a>(text: &'a str, agent: Option<&str>) -> Cow<'a, str> {
    if !invokes_skills_with_slash(agent) {
        return Cow::Borrowed(text);
    }
    let references = composer_references(text);
    let skills: Vec<_> = references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::Skill)
        .filter_map(|reference| Some((reference, skill_name(&reference.label)?)))
        .collect();
    if skills.is_empty() {
        return Cow::Borrowed(text);
    }
    let indexed = Utf16Text::new(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (reference, name) in skills {
        let start = indexed.index_of_offset(reference.start);
        out.push_str(&indexed.slice(cursor, start));
        out.push('/');
        out.push_str(name);
        cursor = indexed.index_of_offset(reference.end);
    }
    out.push_str(&indexed.slice(cursor, indexed.len()));
    Cow::Owned(out)
}
