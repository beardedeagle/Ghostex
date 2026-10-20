//! Reading a work branch: its title, and the Linear and GitHub issue IDs it names.

/// Branches that hold no particular piece of work. A session on one of them works on the
/// project itself, so it keeps its normal title and links nothing automatically.
const DEFAULT_BRANCHES: [&str; 3] = ["main", "master", "trunk"];

pub(crate) fn is_default_branch(branch: &str) -> bool {
    let branch = branch.trim();
    DEFAULT_BRANCHES
        .iter()
        .any(|default| branch.eq_ignore_ascii_case(default))
}

/// The work branch a card should talk about: `None` for a default branch or an empty name.
pub(crate) fn work_branch(branch: Option<&str>) -> Option<&str> {
    let branch = branch?.trim();
    (!branch.is_empty()
        && !is_default_branch(branch)
        // A new worktree's placeholder branch until Ghostex renames it a minute later.
        && !crate::worktree_sessions::is_worktree_temp_branch(branch))
    .then_some(branch)
}

/// The session title a work branch gives: the last path segment without the ticket ID in front,
/// so `yahia/spx-1245-copy-link` reads `copy-link` and `yahia/218-arabic-plan-cards` reads
/// `arabic-plan-cards`.
///
/// CDXC:WorkMode 2026-10-09 DECISION:
/// User: in work mode a session's title is its branch name by default, without the user prefix
/// and without the ticket ID, because the ID already shows on the card's second line; renaming
/// the session still wins.
pub(crate) fn branch_title(branch: &str) -> Option<String> {
    let segment = branch.trim().rsplit('/').next()?.trim();
    if segment.is_empty() {
        return None;
    }
    let separators = |c: char| c == '-' || c == '_' || c == '.';
    // A branch can name several tickets in front (`spx-1241_spx-1242-two`); strip them all.
    let mut rest = segment;
    while let Some(stripped) = strip_ticket_prefix(rest) {
        let stripped = stripped.trim_start_matches(separators);
        if stripped.len() == rest.len() {
            break;
        }
        rest = stripped;
    }
    let rest = rest.trim_matches(separators);
    if rest.is_empty() {
        Some(segment.to_string())
    } else {
        Some(rest.to_string())
    }
}

/// `spx-1245-copy-link` → `copy-link`; `218-arabic` → `arabic`; anything else → `None`.
fn strip_ticket_prefix(segment: &str) -> Option<&str> {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_alphanumeric() && index < 12 {
        if bytes[index] == b'-' {
            break;
        }
        index += 1;
    }
    let head = &segment[..index];
    // `<key>-<number>`: a short alphabetic key, a dash, then digits.
    if !head.is_empty()
        && head.chars().all(|c| c.is_ascii_alphabetic())
        && bytes.get(index) == Some(&b'-')
    {
        let digits_end = segment[index + 1..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|end| index + 1 + end)
            .unwrap_or(segment.len());
        if digits_end > index + 1 {
            return Some(&segment[digits_end..]);
        }
    }
    // `<number>-…`: a GitHub issue number in front.
    let digits_end = segment
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(segment.len());
    if digits_end > 0 && (digits_end == segment.len() || bytes[digits_end] == b'-') {
        return Some(&segment[digits_end..]);
    }
    None
}

/// Every `<key>-<number>` in the branch whose key is one of the Linear team keys, uppercased
/// (`SPX-1245`). Only real team keys count, so `fix-utf-8-paste` names nothing.
pub(crate) fn linear_identifiers_in_branch(branch: &str, team_keys: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for token in branch.split(['/', '_', '.']) {
        let parts: Vec<&str> = token.split('-').collect();
        for window in parts.windows(2) {
            let (key, number) = (window[0], window[1]);
            if key.is_empty()
                || number.is_empty()
                || !number.chars().all(|c| c.is_ascii_digit())
                || !key.chars().all(|c| c.is_ascii_alphanumeric())
            {
                continue;
            }
            if team_keys
                .iter()
                .any(|team_key| team_key.eq_ignore_ascii_case(key))
            {
                let identifier = format!("{}-{}", key.to_ascii_uppercase(), number);
                if !found.contains(&identifier) {
                    found.push(identifier);
                }
            }
        }
    }
    found
}

/// The GitHub issue number a branch starts its last segment with (`yahia/218-arabic-plan-cards`).
pub(crate) fn github_issue_in_branch(branch: &str) -> Option<u64> {
    let segment = branch.trim().rsplit('/').next()?;
    let digits_end = segment.find(|c: char| !c.is_ascii_digit())?;
    if digits_end == 0 || segment.as_bytes()[digits_end] != b'-' {
        return None;
    }
    segment[..digits_end]
        .parse()
        .ok()
        .filter(|number| *number > 0)
}
