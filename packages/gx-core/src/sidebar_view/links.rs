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
    /// Where it came from (`branch`, `closes`, `user`…); only `closes` shows, as `source_label`.
    pub source: String,
    /// "From branch", "Agent opened", "Closed by #318"…
    pub source_label: String,
    pub finished: bool,
    /// A person pinned it: gxserver lists it first, and every sidebar option but Off shows it.
    pub pinned: bool,
    /// Never read yet, with a read on its way: the lists say "Loading…" and keep it out of Open.
    pub loading: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestedSessionLink {
    pub key: String,
    pub kind: SessionLinkKind,
    pub label: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub state_name: Option<String>,
    /// The second line: "Mentioned in the chat", or where a removed link came from.
    pub source_label: String,
    pub finished: bool,
    pub loading: bool,
}

/// The icon an entry draws; each renderer maps it to its own SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkGlyph {
    /// Every pull request: a branch.
    Branch,
    /// Every GitHub issue: the GitHub mark.
    Github,
    /// Every Linear ticket: the Linear logo.
    Linear,
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
        state: item.state.clone(),
        state_name: item.state_name.clone(),
        source_label: item.source_label.clone(),
        finished: item.finished,
        loading: item.loading,
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
                    source: item.source.clone(),
                    source_label: item.source_label.clone(),
                    finished: item.finished,
                    pinned: item.pinned,
                    loading: item.loading,
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
    /// The icon says the kind (a branch for a PR, the GitHub mark for an issue, the Linear logo
    /// for a ticket) and its colour the state.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "Let's please not show text for issue and for Pull Request I think it's better to just show branch icon for PRs and show github icon for issues and a linear icon for linear issues." Each icon keeps its state colour (open green, merged purple, closed red, a closed issue purple, draft grey, Linear's state colours). This supersedes the same day's kind word at the start of the second line and the per-state PR and issue icons.
    pub fn glyph(&self) -> (SessionLinkGlyph, SessionLinkTone) {
        let state = self.state.as_deref();
        match self.kind {
            SessionLinkKind::PullRequest => (
                SessionLinkGlyph::Branch,
                match state {
                    Some("draft") => SessionLinkTone::Draft,
                    Some("merged") => SessionLinkTone::Merged,
                    Some("closed") => SessionLinkTone::Closed,
                    Some(_) => SessionLinkTone::Open,
                    None => SessionLinkTone::Draft,
                },
            ),
            SessionLinkKind::Issue => (
                SessionLinkGlyph::Github,
                match state {
                    Some("closed") => SessionLinkTone::Merged,
                    Some(_) => SessionLinkTone::Open,
                    None => SessionLinkTone::Draft,
                },
            ),
            SessionLinkKind::Linear => {
                let in_review = self
                    .state_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("review"));
                (
                    SessionLinkGlyph::Linear,
                    match state {
                        Some("started") if in_review => SessionLinkTone::Review,
                        Some("started") => SessionLinkTone::Started,
                        Some("completed") => SessionLinkTone::Done,
                        Some(_) => SessionLinkTone::Draft,
                        None => SessionLinkTone::Linear,
                    },
                )
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

    /// The second line: only what is worth acting on, an open PR's checks and review and the PR
    /// that closes an issue ("Closed by #284"); empty draws a one-line row.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "i dont like the "You added"/"mentioned in chat" text there i think no need for that". Where a link came from ("You added", "Agent added", "From branch", "Mentioned in the chat") is not shown; only "Closed by #284" stays, with an open PR's checks and review.
    pub fn detail_line(&self) -> String {
        let (checks, review) = (self.checks_words(), self.review_words());
        let closed_by = (self.source == "closes").then_some(self.source_label.as_str());
        [checks, review, closed_by]
            .into_iter()
            .flatten()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn checks_words(&self) -> Option<&'static str> {
        match self.checks.as_deref() {
            Some("passing") => Some("Checks passing"),
            Some("failing") => Some("Checks failing"),
            Some("pending") => Some("Checks running"),
            _ => None,
        }
    }

    fn review_words(&self) -> Option<&'static str> {
        match self.review.as_deref() {
            Some("reviewRequired") => Some("Review requested"),
            Some("approved") => Some("Approved"),
            Some("changesRequested") => Some("Changes requested"),
            _ => None,
        }
    }

    /// What hovering a row's icon says: the state in words, then a PR's checks and review
    /// (`Open · Checks failing · Review requested`); "Loading…" before it was read.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "no need for the word merged and closed i should be able to hover their icon to get that word instead in tooltip". The lists' rows show no state word; the icon's colour carries the state and its tooltip says it, with a PR's checks and review.
    pub fn state_tooltip(&self) -> String {
        let state = self
            .state_label()
            .or_else(|| self.loading.then(|| "Loading…".to_string()));
        [state.as_deref(), self.checks_words(), self.review_words()]
            .into_iter()
            .flatten()
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
    /// The entry as a linked one, which the lists draw with the same row: its glyph and state come
    /// from what was read about it, and it has no second line.
    pub fn as_link(&self) -> SessionLink {
        SessionLink {
            key: self.key.clone(),
            kind: self.kind,
            label: self.label.clone(),
            title: self.title.clone(),
            state: self.state.clone(),
            state_name: self.state_name.clone(),
            checks: None,
            review: None,
            url: self.url.clone(),
            source: String::new(),
            source_label: self.source_label.clone(),
            finished: self.finished,
            pinned: false,
            loading: self.loading,
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
    /// The session has no links yet, only suggestions: the badge counts those and is drawn
    /// quietly (muted, dashed).
    pub suggested: bool,
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
        Self::most_pressing_tone_of(self.items.iter())
    }

    fn most_pressing_tone_of<'a>(links: impl Iterator<Item = &'a SessionLink>) -> SessionLinkTone {
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
        links
            .map(rank)
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, tone)| tone)
            .unwrap_or(SessionLinkTone::Draft)
    }

    /// What a row shows under `display`; `selected` is whether the row is the session the window
    /// shows.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "why didn't we suggest #263 here for this session?" A session whose only links are suggestions shows a quiet badge (muted and dashed, counting the suggestions) that opens the floating card on hover, under every option but Off, instead of nothing.
    pub fn row(&self, display: SessionLinksDisplay, selected: bool) -> SessionLinksRow {
        if display == SessionLinksDisplay::Off {
            return SessionLinksRow::default();
        }
        if self.items.is_empty() {
            if self.suggested.is_empty() {
                return SessionLinksRow::default();
            }
            return SessionLinksRow {
                line: Vec::new(),
                badge: Some(SessionLinksBadge {
                    count: self.suggested.len(),
                    tone: SessionLinkTone::Draft,
                    suggested: true,
                }),
                hover_card: true,
            };
        }
        // Pinned links lead `items` (gxserver sorts them first) and show under every option but
        // Off (CDXC:SessionLinks 2026-10-24 in server/src/session_links/store.rs).
        let all = || (0..self.items.len()).collect::<Vec<_>>();
        let pinned = || {
            self.items
                .iter()
                .enumerate()
                .filter(|(_, link)| link.pinned)
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        };
        let (line, badge) = match display {
            SessionLinksDisplay::Always => (all(), None),
            SessionLinksDisplay::Selected if selected => (all(), None),
            SessionLinksDisplay::Selected => (pinned(), None),
            // The pinned chips on the line under the name; the title row's badge counts the rest.
            SessionLinksDisplay::Badge => {
                let rest = self.items.iter().filter(|link| !link.pinned);
                let count = rest.clone().count();
                (
                    pinned(),
                    (count > 0).then(|| SessionLinksBadge {
                        count,
                        tone: Self::most_pressing_tone_of(rest),
                        suggested: false,
                    }),
                )
            }
            SessionLinksDisplay::OpenOnly => (
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, link)| link.pinned || !link.finished)
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

/// How many of a links line's chips fit in `available`, in order: the most that fit together with
/// the "+N" pill (sized by `pill_width` for its own count) whenever any link is left out. Links the
/// line never shows (`never_shown`, the finished ones under Open work only) count into "+N" too.
/// Widths are the renderer's own measurements, in one unit.
///
/// CDXC:SessionLinks 2026-10-24 WHY:
/// The line once guessed the room from the sidebar's width and a fixed 34px pill, and its "+11" ran past the card's edge ("stuff is cut off here fam"). Every renderer now measures its chips, its pill and the line's real width, and drops chips until both fit.
pub fn link_chips_that_fit(
    chip_widths: &[f32],
    never_shown: usize,
    available: f32,
    gap: f32,
    pill_width: impl Fn(usize) -> f32,
) -> usize {
    let total = chip_widths.len();
    (0..=total)
        .rev()
        .find(|&shown| {
            let chips =
                chip_widths[..shown].iter().sum::<f32>() + gap * shown.saturating_sub(1) as f32;
            let hidden = total - shown + never_shown;
            let pill = if hidden == 0 {
                0.0
            } else {
                pill_width(hidden) + if shown > 0 { gap } else { 0.0 }
            };
            chips + pill <= available
        })
        .unwrap_or(0)
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
            source: String::new(),
            source_label: String::new(),
            finished,
            pinned: false,
            loading: false,
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
                tone: SessionLinkTone::Closed,
                suggested: false,
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

    #[test]
    fn chips_fit_with_room_for_their_pill() {
        // "+N" grows by a digit's width per digit.
        let pill = |hidden: usize| 20.0 + hidden.to_string().len() as f32 * 6.0;
        let chips = [40.0; 5];
        // Everything fits: no pill.
        assert_eq!(link_chips_that_fit(&chips, 0, 240.0, 10.0, pill), 5);
        // Five chips need 240; at 239 the fifth gives way to "+1" (four chips 190 + 10 + 26).
        assert_eq!(link_chips_that_fit(&chips, 0, 239.0, 10.0, pill), 4);
        // The pill's room is reserved: four chips alone fit 190, but not with "+1".
        assert_eq!(link_chips_that_fit(&chips, 0, 225.0, 10.0, pill), 3);
        // Links the line never shows go into "+N" even when every chip fits.
        assert_eq!(link_chips_that_fit(&chips, 2, 240.0, 10.0, pill), 4);
        // A pill sized for "+11" needs more room than one sized for "+1".
        assert_eq!(link_chips_that_fit(&chips, 10, 230.0, 10.0, pill), 3);
        // Nothing fits but the pill.
        assert_eq!(link_chips_that_fit(&chips, 0, 30.0, 10.0, pill), 0);
    }
}
