//! What a `caption` or `tool` frame from gxserver makes of the voice panel's rows: the paragraphs a
//! long agent reply is drawn in, and the row a tool call lands on (its run's, when gxserver groups
//! calls of one tool). gxserver decides both (`server/src/voice_agent/paragraphs.rs`,
//! `tool_runs.rs`, where the user's decision is); the panel only follows.
//!
//! CDXC:VoiceMode 2026-10-24 SEE-ALSO: the phone's `src/voice/lines.ts` reads the same frames the same way, with the same tests.

use serde_json::Value;

/// gxserver's `paragraphs` of an agent caption; empty when the caption is one paragraph (a short
/// reply, a user caption, or a gxserver from before paragraphs).
pub(crate) fn caption_paragraphs(event: &Value) -> Vec<String> {
    event["paragraphs"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The row a `tool` frame updates, and what it shows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToolRowFrame {
    /// The run's id when the call is part of a run (the run's first call), else the call's id.
    pub(crate) key: String,
    pub(crate) label: String,
    /// `None` while it (or any call of its run) runs.
    pub(crate) ok: Option<bool>,
    pub(crate) started: bool,
}

pub(crate) fn tool_row_frame(event: &Value) -> ToolRowFrame {
    let started = event["phase"].as_str() == Some("started");
    let run = &event["run"];
    if let Some(id) = run["id"].as_str() {
        return ToolRowFrame {
            key: id.to_string(),
            label: run["label"].as_str().unwrap_or_default().to_string(),
            ok: (run["finished"].as_bool() == Some(true))
                .then(|| run["ok"].as_bool().unwrap_or(true)),
            started,
        };
    }
    ToolRowFrame {
        key: event["callId"].as_str().unwrap_or_default().to_string(),
        label: event["label"].as_str().unwrap_or_default().to_string(),
        ok: (!started).then(|| event["ok"].as_bool().unwrap_or(true)),
        started,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ToolRowFrame, caption_paragraphs, tool_row_frame};

    #[test]
    fn a_long_caption_brings_its_paragraphs_and_a_short_one_none() {
        let long = json!({"type": "caption", "role": "agent", "itemId": "i1", "text": "One. Two.", "final": true, "paragraphs": ["One.", "Two."]});
        assert_eq!(caption_paragraphs(&long), vec!["One.", "Two."]);
        let short = json!({"type": "caption", "role": "agent", "itemId": "i2", "text": "Done.", "final": true});
        assert!(caption_paragraphs(&short).is_empty());
    }

    #[test]
    fn a_single_call_is_its_own_row() {
        let started = json!({"type": "tool", "phase": "started", "callId": "c1", "name": "read_session", "label": "Reading the session"});
        assert_eq!(
            tool_row_frame(&started),
            ToolRowFrame {
                key: "c1".into(),
                label: "Reading the session".into(),
                ok: None,
                started: true
            }
        );
        let finished = json!({"type": "tool", "phase": "finished", "callId": "c1", "name": "read_session", "label": "Reading the session", "ok": false});
        assert_eq!(tool_row_frame(&finished).ok, Some(false));
    }

    #[test]
    fn a_call_of_a_run_lands_on_the_runs_row() {
        let second = json!({"type": "tool", "phase": "started", "callId": "c2", "name": "read_session", "label": "Reading the session",
            "run": {"id": "c1", "count": 2, "label": "Reading 2 sessions", "finished": false, "ok": true}});
        assert_eq!(
            tool_row_frame(&second),
            ToolRowFrame {
                key: "c1".into(),
                label: "Reading 2 sessions".into(),
                ok: None,
                started: true
            }
        );
        let done = json!({"type": "tool", "phase": "finished", "callId": "c21", "name": "read_session", "label": "Reading the session", "ok": true,
            "run": {"id": "c1", "count": 21, "label": "Read 21 sessions", "finished": true, "ok": true}});
        assert_eq!(
            tool_row_frame(&done),
            ToolRowFrame {
                key: "c1".into(),
                label: "Read 21 sessions".into(),
                ok: Some(true),
                started: false
            }
        );
    }
}
