//! The strips and side panels around the transcript: the working strip, the terminal tail, the
//! per-turn deferred work rows, and the composer menu's host actions.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The line above the composer while the agent works.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingStrip {
    /// The working word plus its clock, or `null` when the session is idle.
    pub label: Option<String>,
    /// What gxserver read off the agent's screen.
    ///
    /// Free-form for now: the activity record belongs to family f.
    pub activity: Option<Value>,
    /// The activity projected for drawing (the elapsed clock and its tone).
    ///
    /// Free-form for now: family f.
    pub presentation: Option<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// The working card at the transcript's end (`extras::working_card`): one line with the spark, a
/// label and the pills, which opens to the running tool's call and a section per pill.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingCard {
    /// The lane at the transcript's end shows the card.
    pub visible: bool,
    /// `working`, `worked` (the turn ended and its Worked line shows) or `idle`.
    pub phase: String,
    /// The spark turns: the agent is working.
    pub working: bool,
    /// The one-line label: the running tool's ("Bash(cargo test)"), a live activity's
    /// ("Compacting conversation"), the working word ("Considering…") or the Worked line
    /// ("Worked 2m 13s"); `null` when the card carries only pills.
    pub label: Option<String>,
    /// Which of those the label is: `tool`, `activity`, `word` or `worked`.
    pub label_kind: Option<String>,
    /// A live activity's clock and progress (compaction): `elapsedLabel`, `tokens`, `percent`,
    /// `indeterminate`, `hint`, as `workingStrip.presentation` carried them.
    pub activity: Option<Value>,
    /// The open card's first section, the call the running tool is making:
    /// `{ title: "Running now", label, detail }`.
    pub tool: Option<Value>,
    /// The pill ids (`chatPills`) whose sections the open card lists, in the pills' order.
    pub sections: Vec<String>,
    /// The card has a body to open.
    pub expandable: bool,
    /// The card is open.
    pub open: bool,
    /// What a screen reader says for the card.
    pub aria_label: String,
}

/// One pill on the working row (`extras::chat_pills`): a panel folded into a pill that opens a
/// floating menu with the panel's rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPill {
    /// `threads`, `tasks` or `subagents`: which panel value the menu draws.
    pub id: String,
    pub label: String,
    /// The number after the label ("3", "4/9"), or "" for none.
    pub count: String,
    /// Something in the menu needs the user (a thread waiting on an approval): an amber dot.
    pub attention: bool,
    /// The panel's muted detail ("2 working · 212 threads"), the menu's header line.
    pub meta: String,
    pub aria_label: String,
}

/// The agent terminal's last lines, offered when the chat cannot explain what is happening.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTail {
    /// Whether a tail can be read at all, once the host has looked.
    pub readiness: Option<String>,
    /// The single line the composer shows.
    pub preview: String,
    /// Why no tail is available.
    pub reason: Option<String>,
    pub notice: TerminalTailNotice,
}

/// The expanded tail sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTailNotice {
    pub open: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub excerpt: String,
    /// Why the sheet has nothing to show, or `null` when it has something.
    ///
    /// A sentence, not a flag: `apps/desktop/src/app/native_chat/composer_not_ready.rs` draws it
    /// as text, and `emptyCopy` in
    /// `packages/shared/session-chat-controller/native-terminal-tail.ts` told an unreadable
    /// screen apart from a blank one, as this does.
    pub empty: Option<String>,
}

/// One turn's off-screen work read: in flight, or failed and offering a retry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredWorkRow {
    pub loading: bool,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub error: Tri<String>,
}

/// One entry in the composer's host-action menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAction {
    pub id: String,
    pub label: String,
    /// The hotkey this action is bound to, absent when it has none.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub hotkey: Tri<String>,
    /// `agent` or `session`: which half of the menu the row sits in.
    pub group: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// The account switch's progress, as the controls read it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    /// The card to draw, or `null` when no switch is on screen.
    ///
    /// Free-form for now: the switch card belongs to family e.
    pub visible: Option<Value>,
    /// The clock the card's relative time is measured against, passed in by the host.
    pub now: i64,
    /// A switch is in flight, so sending is held.
    pub busy: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
