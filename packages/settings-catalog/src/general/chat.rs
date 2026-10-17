use crate::data::*;
use crate::json::opt;
use crate::rows::{row, section, Section};

pub(crate) fn chat() -> Section {
    section(
        "chat",
        "Chat",
        vec![
            row("preferredAgentInterface", "Default view for compatible agents", "Automatically switch to chat as soon as Ghostex detects that an agent session supports it.").options(PREFERRED_AGENT_INTERFACE_OPTIONS),
            row("closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs."),
            row("sessionChatFontFamily", "Chat font family", "Use any installed font in chat messages and the prompt composer."),
            row("sessionChatZoomPercent", "Default chat zoom (%)", "Scale every desktop chat, including messages and the prompt composer, from 70% to 200% in 5% steps. Cmd+= and Cmd+- in a chat change it too, and Cmd+0 sets 100%. Default: 100%."),
            row("sessionChatCustomTranscriptWidthEnabled", "Custom transcript width", "Let the transcript use a different width from the prompt composer."),
            row("sessionChatTranscriptWidthPercent", "Transcript width", "Set the centered transcript width without changing the prompt composer."),
            row("sessionChatFileEditPreviews", "Show file edit previews", "Show the first seven code lines in each file edit. Turn off to show only the path and change counts."),
            row("sessionChatMutedThinking", "Grey thinking", "Show the agent's thinking in grey and its replies in the full text colour, so the two are easy to tell apart."),
            row("sessionChatKeepComposerExpanded", "Keep chat box expanded while scrolling", "Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end."),
            row("sessionChatConfirmEscapeInterrupt", "Press Escape twice to interrupt", "While the agent is working, the first Escape asks you to press Escape again within 2 seconds before it interrupts. Turn off to interrupt on the first Escape."),
            row("sessionChatSimpleMode", "Simple mode", "Simplify all chats: hide tool command previews and group file edits behind an expandable file count."),
            row("chatCatchMeUp", "Catch me up button", "Show a Catch me up button at the top right of the chat box while it is empty. It has your Title Generation Agent (a small, cheap model) read the session's last 30 agent messages and shows where the session stands: what needs you, what is done, in progress or untested."),
            row("chatPreviousQuestion", "Previous question button", "Show a Previous question button next to Scroll to bottom while the chat has questions you haven't answered yet. Each click goes to the next older one and briefly outlines it."),
            row("chatSendOnPick", "Send when you pick an answer", "When a question card asks one question, picking an answer sends it right away, without Send answers. Cards with several questions, multiple-choice questions, Other and text answers still wait for Send."),
            row("markedHandovers", "Marked handovers in every session", "Orchestrators and their threads start each part of the message they hand back with a marker such as ❓ Needs you, ✅ Done, ⚠️ Read this, 🧪 Untested or 🛑 Blocked, so a quick skim shows what needs you. Turn this on to have every other agent session do the same. Applies to sessions started or resumed after you change it."),
            row("markedHandoversEmoji", "Show marker emojis", "Not recommended. Show the marker emoji (❓ ✅ ⚠️ and the rest) at the start of each coloured section, in every style. Off: the colour and the label show the kind. Shown while Marked handovers is on."),
            row("markedHandoversStyle", "Marked sections style", "How the chat colours the sections an agent marks: coloured labels, a thin left rule, or a coloured badge in place of the emoji. Shown while Marked handovers is on.").options(MARKED_HANDOVERS_STYLE_OPTIONS),
            row("orchestratorPlainReplies", "Orchestrators answer without markers", "Not recommended. Orchestrators answer normally, without marking the sections of their handovers. Their threads still mark the reports they send to the orchestrator. Applies to orchestrators started or resumed after you change it."),
            row("sessionChatVerboseMode", "Verbose mode", "Expand thinking blocks to show their tool calls by default. Each chat can override it from its composer."),
        ],
    )
}

pub(crate) fn sounds() -> Section {
    section(
        "sounds",
        "Sounds",
        vec![
            row("completionSound", "Completion Sound", "Sound for terminal completions, or Off.").options(&[opt("Off", "off")]).options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("showMacOSAttentionNotifications", "Attention Notifications", "Show a system notification when a session needs attention."),
            row("resetExpirySystemNotifications", "Reset Expiry Notifications", "Show a system notification when a banked Claude or Codex usage reset expires within 3 days and again within 24 hours. The red notification in the bell stays either way."),
            row("attentionNotificationActions", "Agent Completion Alert Test", "Test the current completion alert settings or open Notification Settings."),
            row("actionCompletionSound", "Action Completion Sound", "Sound for action completions.").options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("copySound", "Copy Sound", "Play a short sound when copying to the clipboard, including text from the chat composer."),
        ],
    )
}
