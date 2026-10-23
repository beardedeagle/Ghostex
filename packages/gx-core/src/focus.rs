//! Focus state and its reducer: active project, active group, focused session, visible sessions.
//!
//! Nothing here performs I/O. A focus change is a plain state change the host reads back and
//! draws; attaching a terminal or waking a session is the host's follow-up, never a precondition.

use serde::{Deserialize, Serialize};

use crate::keys::{
    encode_workspace_subgroup_id, parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey,
};
use crate::presentation_store::PresentationStore;

/// The sidebar group whose sessions are the workspace tabs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActiveGroup {
    /// A project's own group.
    Project(ProjectKey),
    /// A user-made session group inside a project.
    Subgroup {
        project: ProjectKey,
        group_id: String,
    },
}

impl ActiveGroup {
    /// The sidebar group id string the old runtime and client storage use.
    pub fn to_sidebar_group_id(&self) -> String {
        match self {
            Self::Project(project) => project.to_sidebar_group_id(),
            Self::Subgroup { project, group_id } => encode_workspace_subgroup_id(project, group_id),
        }
    }

    /// Inverse of [`Self::to_sidebar_group_id`].
    pub fn parse_sidebar_group_id(value: &str) -> Option<Self> {
        if let Some((project, group_id)) = parse_workspace_subgroup_id(value) {
            return Some(Self::Subgroup { project, group_id });
        }
        ProjectKey::parse_sidebar_group_id(value).map(Self::Project)
    }
}

/// One field of an external focus update: leave it, empty it, or set it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FocusField<T> {
    /// The sender knows nothing about this field; it stays as it is.
    #[default]
    Keep,
    /// The sender says this field is empty.
    Clear,
    Set(T),
}

impl<T> FocusField<T> {
    fn apply_to(self, slot: &mut Option<T>) {
        match self {
            Self::Keep => {}
            Self::Clear => *slot = None,
            Self::Set(value) => *slot = Some(value),
        }
    }
}

/// A focus update that did not originate from a local intent: a daemon `focusSession` renderer
/// command, the old runtime's echo while it still runs beside the store, or a bootstrap hint.
///
/// Every field defaults to "keep", so a sender that knows only a session id says only that:
/// `ExternalFocusUpdate::new(stamp).with_focused_session(key)`. The active project and group then
/// follow from the session, because the focused session owns the active project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ExternalFocusUpdate {
    /// The value of [`FocusState::local_stamp`] the sender had seen when it produced this update.
    /// An update that observed an older stamp than the current one lost the race to a newer local
    /// intent and is dropped.
    pub observed_stamp: u64,
    pub active_project: FocusField<ProjectKey>,
    pub active_group: FocusField<ActiveGroup>,
    pub focused_session: FocusField<SessionKey>,
    /// `Clear` empties the visible set.
    pub visible_sessions: FocusField<Vec<SessionKey>>,
}

impl ExternalFocusUpdate {
    /// An update that changes nothing yet; add fields with the `with_` and `clear_` methods.
    pub fn new(observed_stamp: u64) -> Self {
        Self {
            observed_stamp,
            active_project: FocusField::Keep,
            active_group: FocusField::Keep,
            focused_session: FocusField::Keep,
            visible_sessions: FocusField::Keep,
        }
    }

    pub fn with_active_project(mut self, project: ProjectKey) -> Self {
        self.active_project = FocusField::Set(project);
        self
    }

    pub fn with_active_group(mut self, group: ActiveGroup) -> Self {
        self.active_group = FocusField::Set(group);
        self
    }

    pub fn with_focused_session(mut self, session: SessionKey) -> Self {
        self.focused_session = FocusField::Set(session);
        self
    }

    pub fn clear_focused_session(mut self) -> Self {
        self.focused_session = FocusField::Clear;
        self
    }

    pub fn with_visible_sessions(mut self, sessions: Vec<SessionKey>) -> Self {
        self.visible_sessions = FocusField::Set(sessions);
        self
    }
}

/// What a focus reducer call did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusOutcome {
    /// Active project, active group, focused session, or the visible set changed.
    pub changed: bool,
    pub displayed_changed: bool,
    /// Set when the remembered last session of a project changed; the host persists it.
    pub remembered: Option<(ProjectKey, SessionKey)>,
    /// Set when an external update lost to a newer local intent: `(local, observed)`.
    pub stale_external: Option<(u64, u64)>,
}

/// Who is focused and what is on screen.
///
/// CDXC:FocusRouting 2026-09-19 WHY:
/// One owner for a fact the old system kept in seven places. A local intent applies at once and bumps `local_stamp`; anything that arrives later but was produced against an older stamp is dropped, which replaces the three time-based echo suppressors of the old bridge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusState {
    pub active_project: Option<ProjectKey>,
    pub active_group: Option<ActiveGroup>,
    pub focused_session: Option<SessionKey>,
    /// Sessions that own a pane in the focus projection. Replaced by the host's exact report
    /// whenever one arrives; between reports a focus change updates it by the local rule.
    pub visible_sessions: Vec<SessionKey>,
    /// Sessions the host says are on screen right now. Always a host-reported fact, never derived
    /// from focus history: Auto Sleep safety depends on it.
    pub displayed_sessions: Vec<SessionKey>,
    /// The session the user last selected in each project, including sleeping sessions. At most
    /// one entry per project (a list, not a map, so the state serializes to plain JSON).
    pub last_session_by_project: Vec<SessionKey>,
    /// Count of local focus intents. Monotonic; never reset while the core lives.
    pub local_stamp: u64,
    /// Host time of the newest local intent (diagnostics only; ordering uses the stamp).
    pub last_local_intent_at_ms: Option<u64>,
}

impl FocusState {
    /// The session the user last selected in a project.
    pub fn last_session_of_project(&self, project: &ProjectKey) -> Option<&SessionKey> {
        self.last_session_by_project
            .iter()
            .find(|session| session.project_key() == *project)
    }

    fn stamp(&mut self, now_ms: u64) {
        self.local_stamp += 1;
        self.last_local_intent_at_ms = Some(now_ms);
    }

    /// Local intent: focus a session.
    ///
    /// The active project and group follow from the session (see [`Self::reconcile`]); a caller
    /// cannot name a group that disagrees with where the session lives, because the tab list of
    /// such a group would not contain the focused session. `exact_visible` is the host's rendered
    /// set when the selection came from the workspace (tab click, next or previous tab); without
    /// it the visible set is updated by [`next_visible_sessions_for_local_focus`].
    pub fn focus_session(
        &mut self,
        store: &PresentationStore,
        session: SessionKey,
        exact_visible: Option<Vec<SessionKey>>,
        now_ms: u64,
    ) -> FocusOutcome {
        let before = self.projection();
        self.stamp(now_ms);

        let project = session.project_key();
        let visible = match exact_visible {
            Some(exact) => dedupe(exact),
            None if session.machine.is_local() => next_visible_sessions_for_local_focus(
                store,
                &self.visible_sessions,
                self.focused_session.as_ref(),
                &session,
            ),
            // A remote session is shown alone until the host reports the rendered set.
            None => vec![session.clone()],
        };

        let remembered = (self.last_session_of_project(&project) != Some(&session)).then(|| {
            self.last_session_by_project
                .retain(|remembered| remembered.project_key() != project);
            self.last_session_by_project.push(session.clone());
            (project.clone(), session.clone())
        });
        self.active_group = Some(store.group_of_session(&session));
        self.active_project = Some(project);
        self.focused_session = Some(session);
        self.visible_sessions = visible;

        FocusOutcome {
            changed: before != self.projection(),
            remembered,
            ..FocusOutcome::default()
        }
    }

    /// Local intent: make a project active without choosing a session.
    ///
    /// A focused session of another project is cleared: the focused session owns the active
    /// project, so leaving it would snap the active project straight back. The visible set is left
    /// alone; the host reports the new one.
    pub fn focus_project(&mut self, project: ProjectKey, now_ms: u64) -> FocusOutcome {
        let before = self.projection();
        self.stamp(now_ms);
        if self
            .focused_session
            .as_ref()
            .is_some_and(|focused| focused.project_key() != project)
        {
            self.focused_session = None;
        }
        self.active_group = Some(ActiveGroup::Project(project.clone()));
        self.active_project = Some(project);
        FocusOutcome {
            changed: before != self.projection(),
            ..FocusOutcome::default()
        }
    }

    /// Local intent: make a USER-MADE session group active, which is what
    /// `createWorkspaceGroupFromSession` does after it mints one.
    ///
    /// It is `focus_project` plus the group, in that order and with the same rule about the
    /// focused session, rather than a second body: the only thing this adds is that the active
    /// group is the subgroup the caller names instead of the project's default one. A group id the
    /// document does not hold is re-homed by `reconcile` on the next store change, which is what
    /// keeps a stale id from naming a group the list draws no row for.
    pub fn focus_subgroup(
        &mut self,
        project: ProjectKey,
        group_id: String,
        now_ms: u64,
    ) -> FocusOutcome {
        let before = self.projection();
        let mut outcome = self.focus_project(project.clone(), now_ms);
        self.active_group = Some(ActiveGroup::Subgroup { project, group_id });
        outcome.changed = before != self.projection();
        outcome
    }

    /// Local intent: the host reports the exact set of sessions that own a pane.
    pub fn set_visible_sessions(&mut self, sessions: Vec<SessionKey>, now_ms: u64) -> FocusOutcome {
        self.stamp(now_ms);
        let sessions = dedupe(sessions);
        let changed = self.visible_sessions != sessions;
        self.visible_sessions = sessions;
        FocusOutcome {
            changed,
            ..FocusOutcome::default()
        }
    }

    /// The host reports which sessions are on screen. Not a focus intent: it does not bump the
    /// stamp and never touches focus.
    pub fn set_displayed_sessions(&mut self, sessions: Vec<SessionKey>) -> FocusOutcome {
        let sessions = dedupe(sessions);
        let displayed_changed = self.displayed_sessions != sessions;
        self.displayed_sessions = sessions;
        FocusOutcome {
            displayed_changed,
            ..FocusOutcome::default()
        }
    }

    /// A focus update from outside. Applied only when it is not older than the newest local
    /// intent; it never bumps the stamp, so it can never beat a local intent itself. The caller
    /// runs [`Self::reconcile`] afterwards, which drops anything the update named that the store
    /// does not hold.
    pub fn apply_external(&mut self, update: ExternalFocusUpdate) -> FocusOutcome {
        if update.observed_stamp < self.local_stamp {
            return FocusOutcome {
                stale_external: Some((self.local_stamp, update.observed_stamp)),
                ..FocusOutcome::default()
            };
        }
        let before = self.projection();
        update.active_project.apply_to(&mut self.active_project);
        update.active_group.apply_to(&mut self.active_group);
        update.focused_session.apply_to(&mut self.focused_session);
        match update.visible_sessions {
            FocusField::Keep => {}
            FocusField::Clear => self.visible_sessions.clear(),
            FocusField::Set(visible) => self.visible_sessions = dedupe(visible),
        }
        FocusOutcome {
            changed: before != self.projection(),
            ..FocusOutcome::default()
        }
    }

    /// A machine went away: drop everything that points at it.
    pub fn machine_gone(&mut self, machine: &MachineId) -> FocusOutcome {
        let before = self.projection();
        if self
            .focused_session
            .as_ref()
            .is_some_and(|focused| focused.machine == *machine)
        {
            self.focused_session = None;
        }
        // An active project on a machine that is gone can never load again, so it must not stay
        // active: clearing it lets the reconcile re-home focus.
        if self
            .active_project
            .as_ref()
            .is_some_and(|project| project.machine == *machine)
        {
            self.active_project = None;
            self.active_group = None;
        }
        self.visible_sessions.retain(|key| key.machine != *machine);
        let displayed_before = self.displayed_sessions.len();
        self.displayed_sessions
            .retain(|key| key.machine != *machine);
        FocusOutcome {
            changed: before != self.projection(),
            displayed_changed: displayed_before != self.displayed_sessions.len(),
            ..FocusOutcome::default()
        }
    }

    /// Makes focus agree with the store. Runs after every event.
    ///
    /// CDXC:FocusRouting 2026-09-19 WHY:
    /// The TypeScript runtime re-derived the active project and group from the focused session on every publish (`ensureActiveProject`), and the desktop workspace depends on that: the active group's tab list is its authority for which tabs exist, so a group that does not contain the focused session, or that names a project that is gone, reads as "no tabs" and clears the workspace. Doing it here, after every event, keeps the invariant in one place instead of at each call site. The rules, in order: (1) focus never points at a session the store does not hold, judged only for machines that are loaded; (2) a focused session owns the active project, and the group is the user-made group that contains the session, else the project's own group; (3) otherwise the active project stays while it exists and is not hidden, with a group that belongs to it; (4) otherwise the first local project in sidebar order (the manual project order when there is one, else `sortKey`; TypeScript took the first by `sortKey` only) becomes active, else nothing is (2026-10-23: the Chats collection that used to take rules 2 and 4 was removed with the projectless chat workspaces). The remembered last session of a project is never forgotten here: it must survive the project being closed and reopened, so memory and the host's persisted copy always agree.
    pub fn reconcile(&mut self, store: &PresentationStore) -> FocusOutcome {
        let before = self.projection();
        let exists =
            |key: &SessionKey| store.loaded(&key.machine).is_none() || store.session(key).is_some();
        if self
            .focused_session
            .as_ref()
            .is_some_and(|key| !exists(key))
        {
            self.focused_session = None;
        }
        self.visible_sessions.retain(exists);
        let displayed_before = self.displayed_sessions.len();
        self.displayed_sessions.retain(exists);
        self.ensure_active(store);
        FocusOutcome {
            changed: before != self.projection(),
            displayed_changed: displayed_before != self.displayed_sessions.len(),
            ..FocusOutcome::default()
        }
    }

    fn ensure_active(&mut self, store: &PresentationStore) {
        if let Some(focused) = &self.focused_session {
            if store.loaded(&focused.machine).is_none() {
                // Not judged yet (startup restore, or a remote machine that is still connecting).
                return;
            }
            let project = focused.project_key();
            if store.project(&project).is_some() {
                self.active_group = Some(store.group_of_session(focused));
                self.active_project = Some(project);
                return;
            }
        }
        if let Some(project) = &self.active_project {
            if store.loaded(&project.machine).is_none() {
                return;
            }
            if store.project(project).is_some() {
                let group_fits = match &self.active_group {
                    Some(ActiveGroup::Project(owner)) => owner == project,
                    Some(ActiveGroup::Subgroup {
                        project: owner,
                        group_id,
                    }) => {
                        owner == project
                            && store
                                .user_groups_of_project(project)
                                .iter()
                                .any(|group| group.group_id == *group_id)
                    }
                    None => false,
                };
                if !group_fits {
                    self.active_group = Some(ActiveGroup::Project(project.clone()));
                }
                return;
            }
        }
        // Nothing valid is active: re-home, but only once the local machine can be judged.
        if store.loaded(&MachineId::Local).is_none() {
            return;
        }
        let first_project = store.first_project(&MachineId::Local);
        match first_project {
            Some(project) => {
                self.active_group = Some(ActiveGroup::Project(project.clone()));
                self.active_project = Some(project);
            }
            None => {
                self.active_project = None;
                self.active_group = None;
            }
        }
    }

    fn projection(
        &self,
    ) -> (
        Option<ProjectKey>,
        Option<ActiveGroup>,
        Option<SessionKey>,
        Vec<SessionKey>,
    ) {
        (
            self.active_project.clone(),
            self.active_group.clone(),
            self.focused_session.clone(),
            self.visible_sessions.clone(),
        )
    }
}

/// Whether the store confirms an empty workspace tab list, judged by the group it holds for the
/// project the list names. `false` for every list that is not `Loaded` and empty, so a machine
/// without its first snapshot and a group that does not exist never read as "no tabs".
///
/// CDXC:Workarea 2026-09-21 WHY:
/// An empty tab list clears every restored tab, split and mapping of a project (the guard and the failure it exists for are in selectors.rs `Loadable` and in the host's `confirms_empty_tab_list`), so it may only be believed when the store agrees, and a disagreement must always refuse rather than allow. A project's own tab list leaves out the sessions of its user-made groups, so an EMPTY user-made group is not evidence that the project has no tabs: it is one group of it that is empty, which is what the last member of a group being closed leaves behind. Judging by such a group alone confirmed a clear the store's own rows contradicted, so a user-made group confirms only together with the project's own list.
pub fn empty_tab_list_confirmed(store: &PresentationStore, group: &ActiveGroup) -> bool {
    let empty = |group: &ActiveGroup| matches!(store.tab_sessions(group), crate::selectors::Loadable::Loaded(tabs) if tabs.is_empty());
    match group {
        ActiveGroup::Subgroup { project, .. } => {
            empty(group) && empty(&ActiveGroup::Project(project.clone()))
        }
        ActiveGroup::Project(_) => empty(group),
    }
}

/// The visible set after a local focus change that came without the host's rendered set.
///
/// The clicked session replaces the focused pane's session: the previously focused session leaves
/// the set, the other panes keep theirs, and the target is added. Remote ids are kept as they
/// are; local ids are kept only while the session still exists. While the local machine is not
/// loaded yet, liveness cannot be judged, so local ids are kept (the TypeScript rule dropped them,
/// but there a click before the first snapshot was impossible).
pub fn next_visible_sessions_for_local_focus(
    store: &PresentationStore,
    visible: &[SessionKey],
    previous_focused: Option<&SessionKey>,
    target: &SessionKey,
) -> Vec<SessionKey> {
    let mut next: Vec<SessionKey> = visible
        .iter()
        .filter(|key| Some(*key) != previous_focused && *key != target)
        .filter(|key| match (&key.machine, store.loaded(&key.machine)) {
            (MachineId::Remote(_), _) | (MachineId::Local, None) => true,
            (MachineId::Local, Some(loaded)) => loaded
                .server_session(&key.project_id, &key.session_id)
                .is_some(),
        })
        .cloned()
        .collect();
    next.push(target.clone());
    next
}

fn dedupe(sessions: Vec<SessionKey>) -> Vec<SessionKey> {
    let mut unique: Vec<SessionKey> = Vec::with_capacity(sessions.len());
    for session in sessions {
        if !unique.contains(&session) {
            unique.push(session);
        }
    }
    unique
}
