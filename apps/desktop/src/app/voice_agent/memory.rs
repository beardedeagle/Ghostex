//! The voice panel's memory of the last conversation: gxserver keeps one summary
//! (`server/src/voice_agent/memory.rs`) and names it in `ready`; the panel offers Continue / Start
//! fresh, shows "Remembering last conversation" once continued (by the button or by the agent's
//! `recall_last_conversation` tool), and forgets it on request.
//!
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: the user's answers are the DECISION in server/src/voice_agent/memory.rs; the phone's voice screen (apps/mobile/app/src/screens/VoiceScreen.tsx) shows the same card.

use serde_json::{Value, json};

use crate::*;

/// The remembered conversation, as gxserver sent it.
#[derive(Clone, Debug)]
pub(crate) struct VoiceMemory {
    pub(crate) gist: String,
    pub(crate) summary: String,
    pub(crate) ended_at: String,
}

impl VoiceMemory {
    fn from_value(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
        let summary = text("summary");
        (!summary.is_empty()).then(|| Self {
            gist: text("gist"),
            summary,
            ended_at: text("endedAt"),
        })
    }

    /// "25 min ago", "yesterday".
    pub(crate) fn ago(&self) -> String {
        let Ok(ended) = chrono::DateTime::parse_from_rfc3339(&self.ended_at) else {
            return String::new();
        };
        let minutes = (chrono::Utc::now() - ended.with_timezone(&chrono::Utc))
            .num_minutes()
            .max(0);
        match minutes {
            0 => "just now".to_string(),
            1..=59 => format!("{minutes} min ago"),
            60..=1439 => format!("{} h ago", minutes / 60),
            1440..=2879 => "yesterday".to_string(),
            _ => format!("{} days ago", minutes / 1440),
        }
    }
}

/// What the panel shows about the remembered conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VoiceMemoryState {
    /// Nothing remembered, or the card was put away (Start fresh, or the user just talked).
    #[default]
    Hidden,
    /// The Continue / Start fresh card.
    Offered,
    /// The agent has the summary: "Remembering last conversation".
    Continued,
    /// "Forgot the last conversation."
    Forgotten,
}

impl GhostexGpuiApp {
    /// `ready`: the remembered conversation, offered unless this call already continued it (a
    /// reattach).
    pub(super) fn apply_voice_agent_memory_ready(&mut self, event: &Value) {
        let agent = &mut self.voice_agent;
        agent.memory = VoiceMemory::from_value(&event["memory"]);
        agent.memory_details_open = false;
        agent.memory_state = match (&agent.memory, event["memoryContinued"].as_bool()) {
            (None, _) => VoiceMemoryState::Hidden,
            (Some(_), Some(true)) => VoiceMemoryState::Continued,
            (Some(_), _) => VoiceMemoryState::Offered,
        };
    }

    /// A `memory` frame: continued (Continue or the agent's recall), none, or forgotten anywhere.
    pub(super) fn apply_voice_agent_memory_event(&mut self, event: &Value) {
        let agent = &mut self.voice_agent;
        match event["state"].as_str().unwrap_or_default() {
            "continued" => {
                if let Some(memory) = VoiceMemory::from_value(&event["memory"]) {
                    agent.memory = Some(memory);
                }
                agent.memory_state = VoiceMemoryState::Continued;
            }
            "forgotten" => {
                agent.memory = None;
                agent.memory_details_open = false;
                agent.memory_state = VoiceMemoryState::Forgotten;
            }
            _ => {
                agent.memory = None;
                agent.memory_state = VoiceMemoryState::Hidden;
            }
        }
    }

    /// The user started talking with the card still up: that is Start fresh.
    pub(super) fn voice_agent_memory_user_spoke(&mut self) {
        if self.voice_agent.memory_state == VoiceMemoryState::Offered {
            self.voice_agent.memory_state = VoiceMemoryState::Hidden;
        }
    }

    /// Continue: gxserver hands the summary to the agent, which recaps it, and answers `memory`.
    pub(crate) fn continue_voice_agent_memory(&mut self, cx: &mut gpui::Context<Self>) {
        if self.send_voice_agent_frame(&json!({ "type": "continueMemory" })) {
            self.voice_agent.memory_state = VoiceMemoryState::Continued;
        }
        self.sync_voice_agent_panel(cx);
    }

    /// Start fresh: puts the card away; this call's own summary replaces the old one later.
    pub(crate) fn start_voice_agent_fresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.voice_agent.memory_state = VoiceMemoryState::Hidden;
        self.sync_voice_agent_panel(cx);
    }

    /// Forget: gxserver deletes the summary and answers `memory` `forgotten` to every open call.
    pub(crate) fn forget_voice_agent_memory(&mut self, cx: &mut gpui::Context<Self>) {
        if self.send_voice_agent_frame(&json!({ "type": "forgetMemory" })) {
            let agent = &mut self.voice_agent;
            agent.memory = None;
            agent.memory_details_open = false;
            agent.memory_state = VoiceMemoryState::Forgotten;
        }
        self.sync_voice_agent_panel(cx);
    }

    /// The "Remembering" line opens and closes the whole summary.
    pub(crate) fn toggle_voice_agent_memory_details(&mut self, cx: &mut gpui::Context<Self>) {
        let agent = &mut self.voice_agent;
        agent.memory_details_open = !agent.memory_details_open && agent.memory.is_some();
        self.sync_voice_agent_panel(cx);
    }
}
