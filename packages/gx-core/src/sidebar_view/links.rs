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
            suggested: links
                .suggested
                .iter()
                .map(|item| SuggestedSessionLink {
                    key: item.key.clone(),
                    kind: kind_of(&item.kind),
                    label: item.label.clone(),
                    url: item.url.clone(),
                    title: item.title.clone().filter(|title| !title.trim().is_empty()),
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.suggested.is_empty()
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
            SessionLinkKind::Issue => (SessionLinkGlyph::IssueOpen, SessionLinkTone::Draft),
            _ => (SessionLinkGlyph::PullRequest, SessionLinkTone::Draft),
        }
    }
}
