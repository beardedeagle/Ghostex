//! The queued prompt's play button: an Escape that must not outlive the message it was for.

use super::*;

/// The most transcript a stale-click check reads past the click.
const TRANSCRIPT_SCAN_LIMIT_BYTES: u64 = 512 * 1024;
/// The faint text Claude Code puts in its empty input box while messages wait in its queue.
const CLAUDE_QUEUE_HINT: &str = "press up to edit queued messages";

/// Where the session's transcript ended when the play button was clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPromptTranscriptMark {
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: PathBuf,
    offset: u64,
}

impl QueuedPromptTranscriptMark {
    /// BLOCKING (transcript path resolution). `None` when the session's agent keeps no transcript
    /// Ghostex can read or has not written one yet.
    pub(super) fn sample(session: &Value) -> Option<Self> {
        let agent = crate::session_chat::resolve_session_chat_transcript_agent(
            session_chat_agent_for_session(session).as_deref(),
        )?;
        let path = crate::session_chat::resolve_session_chat_transcript_path(
            agent,
            read_runtime_text(session, "agentSessionId").as_deref(),
            read_runtime_text(session, "agentSessionPath").as_deref(),
        )?;
        let offset = fs::metadata(&path).ok()?.len();
        Some(Self {
            agent,
            path,
            offset,
        })
    }

    /// BLOCKING (file read). Whether the agent recorded a prompt it took after the click.
    fn prompt_recorded_since(&self) -> bool {
        let Ok(mut file) = fs::File::open(&self.path) else {
            return false;
        };
        let mut appended = Vec::new();
        if std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(self.offset)).is_err()
            || std::io::Read::read_to_end(
                &mut std::io::Read::take(file, TRANSCRIPT_SCAN_LIMIT_BYTES),
                &mut appended,
            )
            .is_err()
        {
            return false;
        }
        String::from_utf8_lossy(&appended).lines().any(|line| {
            crate::session_chat_watchdog::transcript_line_records_prompt(self.agent, line)
        })
    }
}

/// Whether the agent's own screen shows that nothing waits in its queue. `false` whenever the
/// screen cannot settle it (unreadable, another agent, Claude's input box holding text), so an
/// unclear screen never drops a click.
async fn agent_queue_shows_empty(zmx_name: &str, agent: Option<&str>) -> bool {
    match crate::agents::identity::normalize_agent_id(agent).as_deref() {
        Some(agent @ ("claude" | "openclaude")) => capture_session_terminal_text_vt(zmx_name)
            .await
            .and_then(|screen| {
                crate::session_chat_composer::session_chat_composer_input(agent, &screen)
            })
            .is_some_and(|input| {
                input.text_is_empty()
                    && !input.shell_mode
                    && !input
                        .text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                        .contains(CLAUDE_QUEUE_HINT)
            }),
        Some("codex") => capture_session_terminal_text(zmx_name)
            .await
            .is_some_and(|screen| {
                crate::session_chat_notice::codex_queued_input_previews(&screen).is_empty()
            }),
        _ => false,
    }
}

/// When each session last had a submit key (Return, or Empryo's Alt+Q) written to it.
fn last_submits() -> &'static Mutex<HashMap<String, Instant>> {
    static SUBMITS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    SUBMITS.get_or_init(Mutex::default)
}

/// Notes a submit key written to the session's terminal; `write_session_chat_payload` calls it.
pub(super) fn note_session_chat_submit(project_id: &str, session_id: &str, payload: &str) {
    if payload != SESSION_CHAT_SUBMIT && payload != SESSION_CHAT_EMPRYO_SUBMIT {
        return;
    }
    if let Ok(mut submits) = last_submits().lock() {
        submits.insert(
            crate::server::session_observer_key(project_id, session_id),
            Instant::now(),
        );
    }
}

/// CDXC:SessionChat 2026-10-22 WHY:
/// The play button's Escape ("Queued, click to interrupt and send") waits in the session's send queue behind any send still in flight, so a click made while the message was still being typed for a busy agent ran after that message's own Return. Claude Code hands a prompt back to its input box, without recording it, when an Escape lands between taking its Return and writing the prompt to the transcript; on 2026-10-22 (Main Orchestrator, mastro_ed) five clicks made while the send waited out the agent's turn were typed 2 to 8 seconds after its Return, the first one cancelled the message Claude had just taken, and the user saw "Your message might not have reached the agent" until they sent it again. A later click has the same hazard once the agent starts the queued prompt itself (its turn ended, or a first click already interrupted it): the next Escape cancels or interrupts that prompt. A click is not always made before that start, so "a prompt recorded after the click" alone missed it: live on 2026-10-22 a second click five seconds after the first interrupted the queued prompt Claude had already started. The Escape therefore also needs the agent's own screen to show a message waiting: Claude Code's "Press up to edit queued messages" in its empty input box, Codex's queued-message preview. The screen, not the transcript: a prompt can wait in Claude's queue behind thousands of transcript rows (1% of the queued prompts in this machine's transcripts waited behind more than 6,700), some enqueue rows are never released by a queue row, and Codex writes nothing while a message waits. Live on 2026-10-22, Codex 0.163 had already taken a message sent mid-turn into the running turn, a click then interrupted that turn and the message went unanswered, and a second click armed "esc again to edit previous message" on an idle Codex.
/// CDXC:SessionChat 2026-10-22 DECISION:
/// User, asked what a play-button click made while the message is still being typed should do: "Nothing, the message waits" (the agent queues it behind its turn and the button stays to be clicked again). Asked whether to also drop the click once the agent starts the prompt itself: "Yes, close it". So the Escape is written only while none of these has happened since the click: no submit key reached the session, the transcript records no prompt the agent took, and the agent's screen does not show its queue empty.
pub(super) async fn queued_prompt_escape_is_stale(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: Option<&str>,
    requested_at: Instant,
    transcript: Option<&QueuedPromptTranscriptMark>,
) -> bool {
    let submitted_since_click = last_submits()
        .lock()
        .ok()
        .and_then(|submits| {
            submits
                .get(&crate::server::session_observer_key(project_id, session_id))
                .copied()
        })
        .is_some_and(|submitted_at| submitted_at > requested_at);
    if submitted_since_click || agent_queue_shows_empty(zmx_name, agent).await {
        return true;
    }
    let Some(transcript) = transcript.cloned() else {
        return false;
    };
    tokio::task::spawn_blocking(move || transcript.prompt_recorded_since())
        .await
        .unwrap_or(false)
}
