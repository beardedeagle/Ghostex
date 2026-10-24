//! Every PR, GitHub issue and Linear ticket a session is linked to, as the work area header's
//! chips, the links list and the sidebar's hover line read them: which glyph and colour each entry
//! gets, its state word and its second line.
//!
//! The daemon publishes `PresentationSession.links` for every session (server/src/session_links/),
//! already sorted with open work first, so clients only draw it.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/sessionLinks.ts (the phone's copy of these rules,
//! which must give the same glyphs, words and lines).

use ghostex_gx_protocol::presentation::PresentationSessionLinks;

/// A session's links, in the client's own vocabulary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLinks {
    pub items: Vec<SessionLink>,
    pub suggested: Vec<SuggestedSessionLink>,
    /// Links a person removed, newest first: the lists' "Removed (N)" group, each with Restore.
    pub removed: Vec<SuggestedSessionLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkKind {
    PullRequest,
    Issue,
    Linear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLink {
    /// What removing it sends (`pr:owner/repo#318`).
    pub key: String,
    pub kind: SessionLinkKind,
    /// `#318`, `repo#12` or `GHX-142`.
    pub label: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub state_name: Option<String>,
    pub checks: Option<String>,
    pub review: Option<String>,
    pub url: Option<String>,
    /// "From branch", "Agent opened", "Closed by #318"…
    pub source_label: String,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestedSessionLink {
    pub key: String,
    pub kind: SessionLinkKind,
    pub label: String,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// The icon an entry draws; each renderer maps it to its own SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkGlyph {
    PullRequest,
    Merged,
    IssueOpen,
    IssueClosed,
    LinearBacklog,
    LinearTodo,
    LinearStarted,
    LinearReview,
    LinearDone,
    LinearCanceled,
    ChecksPassing,
    ChecksFailing,
    ChecksPending,
}

/// The colour an icon or a word takes; each renderer maps it to its palette (the sidebar's work
/// chip colours: GitHub's green, grey, purple, red and amber, Linear's yellow, green and indigo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkTone {
    Open,
    Draft,
    Merged,
    Closed,
    Pending,
    Started,
    Review,
    Done,
    Linear,
}

fn kind_of(text: &str) -> SessionLinkKind {
    match text {
        "pullRequest" => SessionLinkKind::PullRequest,
        "issue" => SessionLinkKind::Issue,
        _ => SessionLinkKind::Linear,
    }
}

fn other_link(
    item: &ghostex_gx_protocol::presentation::PresentationSuggestedLink,
) -> SuggestedSessionLink {
    SuggestedSessionLink {
        key: item.key.clone(),
        kind: kind_of(&item.kind),
        label: item.label.clone(),
        url: item.url.clone(),
        title: item.title.clone().filter(|title| !title.trim().is_empty()),
    }
}

impl SessionLinks {
    pub fn from_presentation(links: &PresentationSessionLinks) -> Self {
        Self {
            items: links
                .items
                .iter()
                .map(|item| SessionLink {
                    key: item.key.clone(),
                    kind: kind_of(&item.kind),
                    label: item.label.clone(),
                    title: item.title.clone().filter(|title| !title.trim().is_empty()),
                    state: item.state.clone(),
                    state_name: item.state_name.clone(),
                    checks: item.checks.clone(),
                    review: item.review.clone(),
                    url: item.url.clone(),
                    source_label: item.source_label.clone(),
                    finished: item.finished,
                })
                .collect(),
            suggested: links.suggested.iter().map(other_link).collect(),
            removed: links.removed.iter().map(other_link).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.suggested.is_empty() && self.removed.is_empty()
    }

    /// Whether a link left out of the first `shown` chips has failing checks: the "+N" chip then
    /// carries the red dot, so a failing PR is never hidden.
    pub fn overflow_has_failing(&self, shown: usize) -> bool {
        self.items
            .iter()
            .skip(shown)
            .any(|item| item.checks.as_deref() == Some("failing"))
    }
}

impl SessionLink {
    pub fn glyph(&self) -> (SessionLinkGlyph, SessionLinkTone) {
        let state = self.state.as_deref();
        match self.kind {
            SessionLinkKind::PullRequest => match state {
                Some("draft") => (SessionLinkGlyph::PullRequest, SessionLinkTone::Draft),
                Some("merged") => (SessionLinkGlyph::Merged, SessionLinkTone::Merged),
                Some("closed") => (SessionLinkGlyph::PullRequest, SessionLinkTone::Closed),
                Some(_) => (SessionLinkGlyph::PullRequest, SessionLinkTone::Open),
                None => (SessionLinkGlyph::PullRequest, SessionLinkTone::Draft),
            },
            SessionLinkKind::Issue => match state {
                Some("closed") => (SessionLinkGlyph::IssueClosed, SessionLinkTone::Merged),
                Some(_) => (SessionLinkGlyph::IssueOpen, SessionLinkTone::Open),
                None => (SessionLinkGlyph::IssueOpen, SessionLinkTone::Draft),
            },
            SessionLinkKind::Linear => {
                let in_review = self
                    .state_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("review"));
                match state {
                    Some("started") if in_review => {
                        (SessionLinkGlyph::LinearReview, SessionLinkTone::Review)
                    }
                    Some("started") => (SessionLinkGlyph::LinearStarted, SessionLinkTone::Started),
                    Some("completed") => (SessionLinkGlyph::LinearDone, SessionLinkTone::Done),
                    Some("canceled") => (SessionLinkGlyph::LinearCanceled, SessionLinkTone::Draft),
                    Some("backlog") | Some("triage") => {
                        (SessionLinkGlyph::LinearBacklog, SessionLinkTone::Draft)
                    }
                    Some(_) => (SessionLinkGlyph::LinearTodo, SessionLinkTone::Draft),
                    None => (SessionLinkGlyph::LinearTodo, SessionLinkTone::Linear),
                }
            }
        }
    }

    /// The mark after a PR's number: its checks.
    pub fn checks_mark(&self) -> Option<(SessionLinkGlyph, SessionLinkTone)> {
        match self.checks.as_deref()? {
            "passing" => Some((SessionLinkGlyph::ChecksPassing, SessionLinkTone::Open)),
            "failing" => Some((SessionLinkGlyph::ChecksFailing, SessionLinkTone::Closed)),
            "pending" => Some((SessionLinkGlyph::ChecksPending, SessionLinkTone::Pending)),
            _ => None,
        }
    }

    /// The state in words: "Open", "Draft", "Merged", "Closed", or the Linear team's own name.
    pub fn state_label(&self) -> Option<String> {
        if self.kind == SessionLinkKind::Linear {
            return self.state_name.clone();
        }
        let state = self.state.as_deref()?;
        let mut chars = state.chars();
        let first = chars.next()?.to_ascii_uppercase();
        Some(std::iter::once(first).chain(chars).collect())
    }

    /// The second line: checks, review and where the link came from.
    pub fn detail_line(&self) -> String {
        let checks = match self.checks.as_deref() {
            Some("passing") => Some("Checks passing"),
            Some("failing") => Some("Checks failing"),
            Some("pending") => Some("Checks running"),
            _ => None,
        };
        let review = match self.review.as_deref() {
            Some("reviewRequired") => Some("Review requested"),
            Some("approved") => Some("Approved"),
            Some("changesRequested") => Some("Changes requested"),
            _ => None,
        };
        [checks, review, Some(self.source_label.as_str())]
            .into_iter()
            .flatten()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The tooltip and the list's title fallback: `#318 · Port the Find screen · Open`.
    pub fn summary(&self) -> String {
        [
            Some(self.label.clone()),
            self.title.clone(),
            self.state_label(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

impl SuggestedSessionLink {
    pub fn glyph(&self) -> (SessionLinkGlyph, SessionLinkTone) {
        match self.kind {
            SessionLinkKind::Linear => (SessionLinkGlyph::LinearTodo, SessionLinkTone::Draft),
            SessionLinkKind::Issue => (SessionLinkGlyph::IssueOpen, SessionLinkTone::Draft),
            _ => (SessionLinkGlyph::PullRequest, SessionLinkTone::Draft),
        }
    }
}

/// What a sidebar row shows of its session's links (`sidebarSessionLinks`).
///
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User, of the four ways to see a session's links without hovering: "i want a, b, c,d to be options in settings so people pick but they want (fully disabled should be option too)". Always (A), Selected session (B), Badge (C), Open work only (D) and Off. Badge is the default: rows stay one line now that every project's sessions gather links, the badge still says how many and the most pressing state, and the floating card has the rest.
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User: "i think i we don't need a separate thing called "work mode" and instead we can just do this". Work mode cards follow this setting too; their permanent chip line (CDXC:WorkMode 2026-10-09) is gone, except while a merged PR's Clean up / Keep offer waits. This supersedes the 2026-10-24 rule that kept the sidebar line Work-mode-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionLinksDisplay {
    Always,
    Selected,
    #[default]
    Badge,
    OpenOnly,
    Off,
}

impl SessionLinksDisplay {
    pub const SETTING_KEY: &'static str = "sidebarSessionLinks";

    /// The setting's saved value; anything unknown is the default.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("always") => Self::Always,
            Some("selected") => Self::Selected,
            Some("open") => Self::OpenOnly,
            Some("off") => Self::Off,
            _ => Self::Badge,
        }
    }

    pub fn from_settings_json(settings: &serde_json::Value) -> Self {
        Self::from_setting(
            settings
                .get(Self::SETTING_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }
}

/// The badge a row shows instead of a line: how many links, coloured by the most pressing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionLinksBadge {
    pub count: usize,
    pub tone: SessionLinkTone,
}

/// What one sidebar row shows of its links.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLinksRow {
    /// Indices into `SessionLinks::items` the row's second line shows, in order; empty draws no
    /// line. The renderer fits what it can and shows "+N" for the rest.
    pub line: Vec<usize>,
    pub badge: Option<SessionLinksBadge>,
    /// Hovering the row (or its badge) opens the floating links card.
    pub hover_card: bool,
}

impl SessionLinks {
    /// The colour of the most pressing link: failing checks or changes asked for, then a review
    /// asked for or checks still running, then open work, then a draft, then finished work.
    pub fn most_pressing_tone(&self) -> SessionLinkTone {
        let rank = |link: &SessionLink| -> (u8, SessionLinkTone) {
            if link.checks.as_deref() == Some("failing")
                || link.review.as_deref() == Some("changesRequested")
            {
                (0, SessionLinkTone::Closed)
            } else if link.review.as_deref() == Some("reviewRequired")
                || link.checks.as_deref() == Some("pending")
            {
                (1, SessionLinkTone::Pending)
            } else if !link.finished && link.state.as_deref() != Some("draft") {
                (2, link.glyph().1)
            } else if !link.finished {
                (3, SessionLinkTone::Draft)
            } else {
                (4, SessionLinkTone::Merged)
            }
        };
        self.items
            .iter()
            .map(rank)
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, tone)| tone)
            .unwrap_or(SessionLinkTone::Draft)
    }

    /// What a row shows under `display`; `selected` is whether the row is the session the window
    /// shows.
    pub fn row(&self, display: SessionLinksDisplay, selected: bool) -> SessionLinksRow {
        if self.items.is_empty() || display == SessionLinksDisplay::Off {
            return SessionLinksRow::default();
        }
        let all = || (0..self.items.len()).collect::<Vec<_>>();
        let (line, badge) = match display {
            SessionLinksDisplay::Always => (all(), None),
            SessionLinksDisplay::Selected if selected => (all(), None),
            SessionLinksDisplay::Selected => (Vec::new(), None),
            SessionLinksDisplay::Badge => (
                Vec::new(),
                Some(SessionLinksBadge {
                    count: self.items.len(),
                    tone: self.most_pressing_tone(),
                }),
            ),
            SessionLinksDisplay::OpenOnly => (
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, link)| !link.finished)
                    .map(|(index, _)| index)
                    .collect(),
                None,
            ),
            SessionLinksDisplay::Off => unreachable!(),
        };
        SessionLinksRow {
            line,
            badge,
            hover_card: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(key: &str, state: &str, finished: bool) -> SessionLink {
        SessionLink {
            key: key.to_string(),
            kind: SessionLinkKind::PullRequest,
            label: key.to_string(),
            title: None,
            state: Some(state.to_string()),
            state_name: None,
            checks: None,
            review: None,
            url: None,
            source_label: String::new(),
            finished,
        }
    }

    fn links() -> SessionLinks {
        let mut failing = link("#1", "open", false);
        failing.checks = Some("failing".to_string());
        SessionLinks {
            items: vec![
                failing,
                link("#2", "open", false),
                link("#3", "merged", true),
            ],
            suggested: Vec::new(),
            removed: Vec::new(),
        }
    }

    #[test]
    fn the_setting_reads_every_option_and_defaults_to_badge() {
        use SessionLinksDisplay::*;
        for (value, display) in [
            ("always", Always),
            ("selected", Selected),
            ("badge", Badge),
            ("open", OpenOnly),
            ("off", Off),
            ("nonsense", Badge),
        ] {
            assert_eq!(SessionLinksDisplay::from_setting(Some(value)), display);
        }
        assert_eq!(SessionLinksDisplay::from_setting(None), Badge);
    }

    #[test]
    fn always_shows_every_link_on_every_row() {
        let row = links().row(SessionLinksDisplay::Always, false);
        assert_eq!(row.line, vec![0, 1, 2]);
        assert_eq!(row.badge, None);
        assert!(row.hover_card);
    }

    #[test]
    fn selected_shows_the_line_only_on_the_selected_row() {
        assert_eq!(
            links().row(SessionLinksDisplay::Selected, true).line,
            vec![0, 1, 2]
        );
        let other = links().row(SessionLinksDisplay::Selected, false);
        assert!(other.line.is_empty() && other.badge.is_none() && other.hover_card);
    }

    #[test]
    fn badge_counts_every_link_in_the_most_pressing_colour() {
        let row = links().row(SessionLinksDisplay::Badge, false);
        assert!(row.line.is_empty());
        assert_eq!(
            row.badge,
            Some(SessionLinksBadge {
                count: 3,
                tone: SessionLinkTone::Closed
            })
        );
        let finished = SessionLinks {
            items: vec![link("#3", "merged", true)],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(finished.most_pressing_tone(), SessionLinkTone::Merged);
    }

    #[test]
    fn open_only_hides_finished_links_and_the_line_once_all_are_done() {
        assert_eq!(
            links().row(SessionLinksDisplay::OpenOnly, false).line,
            vec![0, 1]
        );
        let done = SessionLinks {
            items: vec![link("#3", "merged", true)],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        let row = done.row(SessionLinksDisplay::OpenOnly, false);
        assert!(row.line.is_empty() && row.hover_card);
    }

    #[test]
    fn off_shows_nothing_and_opens_no_card() {
        assert_eq!(
            links().row(SessionLinksDisplay::Off, true),
            SessionLinksRow::default()
        );
    }
}
