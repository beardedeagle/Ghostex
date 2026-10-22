use super::*;

/// Grok Build's key into its queue panel: Up in an empty prompt selects the newest queued row,
/// and each further Up selects the one above it.
const GROK_QUEUE_SELECT_KEY: &str = "\u{1b}[A";
/// Grok's key for deleting the selected queue row.
const GROK_QUEUE_DELETE_KEY: &str = "x";
/// Grok's key for leaving the queue panel back to its prompt ("Space:prompt").
const GROK_QUEUE_LEAVE_KEY: &str = " ";
/// Backspace, for an `x` the prompt took because the queue was gone before it arrived.
const GROK_PROMPT_BACKSPACE: &str = "\u{7f}";
/// How long each key waits for Grok to repaint the queue panel.
const GROK_QUEUE_KEY_WAIT_MS: u64 = 1_500;
const GROK_QUEUE_POLL_MS: u64 = 50;
/// The footer lines read for the queue panel's own shortcuts.
const GROK_FOOTER_SCAN_LINES: usize = 3;

/// One row of Grok's queue panel as the screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GrokQueueRow {
    number: usize,
    /// The row's first line, whitespace folded, without its markers.
    text: String,
    /// The row ends in `…`, so `text` is only the start of the message.
    truncated: bool,
    /// The row carries Grok's `[Send now][edit][cancel]` actions.
    selected: bool,
}

/// Grok's queue panel: the count in its `▾ Queued N` head and the rows it shows (three at most;
/// the panel scrolls to keep the selected row in view).
#[derive(Clone, Debug, PartialEq, Eq)]
struct GrokQueue {
    count: usize,
    rows: Vec<GrokQueueRow>,
}

impl GrokQueue {
    fn selected(&self) -> Option<&GrokQueueRow> {
        self.rows.iter().find(|row| row.selected)
    }
}

/// A screen line without the panel's scrollbar glyphs at its right edge.
fn strip_scrollbar(line: &str) -> &str {
    line.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '█' | '▲' | '▼' | '░' | '▒')
    })
}

fn read_grok_queue(screen: &str) -> Option<GrokQueue> {
    let lines: Vec<&str> = screen.lines().collect();
    let (head, count) = lines.iter().enumerate().rev().find_map(|(index, line)| {
        let rest = strip_scrollbar(line)
            .trim_start()
            .strip_prefix(['▾', '▸'])?
            .trim_start()
            .strip_prefix("Queued ")?;
        Some((index, rest.trim().parse::<usize>().ok()?))
    })?;
    let mut rows = Vec::new();
    for line in &lines[head + 1..] {
        let Some(row) = strip_scrollbar(line).trim_start().strip_prefix('#') else {
            break;
        };
        let digits = row.bytes().take_while(u8::is_ascii_digit).count();
        let Ok(number) = row[..digits].parse::<usize>() else {
            break;
        };
        let mut text = row[digits..].trim();
        let selected = text.ends_with("[cancel]");
        if selected {
            text = text
                .rfind("[Send now]")
                .map_or(text, |start| text[..start].trim_end());
        }
        // A message of several lines shows its first one and `(+N line)` / `(+N lines)`.
        if let Some(start) = text.rfind(" (+") {
            let tail = &text[start + 3..];
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && matches!(&tail[digits..], " line)" | " lines)") {
                text = text[..start].trim_end();
            }
        }
        let truncated = text.ends_with('…');
        let text = text.trim_end_matches('…');
        rows.push(GrokQueueRow {
            number,
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            truncated,
            selected,
        });
    }
    Some(GrokQueue { count, rows })
}

/// Whether a queue row is the message the chat asked for, compared on its first line.
fn grok_row_matches(row: &GrokQueueRow, wanted_first_line: &str) -> bool {
    let row_text = take_back_key(&row.text);
    if row_text.is_empty() {
        return false;
    }
    if row.truncated {
        wanted_first_line.starts_with(&row_text)
    } else {
        wanted_first_line == row_text
    }
}

/// Whether Grok's focus is in its queue panel or transcript rather than its prompt: the footer
/// then lists the panel's keys or "Space:prompt".
fn grok_prompt_unfocused(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(GROK_FOOTER_SCAN_LINES)
        .any(|line| line.contains("x:delete row") || line.contains("Space:prompt"))
}

/// Captures until `done` holds for the screen or the wait runs out; the last screen either way.
async fn wait_for_grok_screen(zmx_name: &str, done: impl Fn(&str) -> bool) -> Option<String> {
    let deadline = Instant::now() + Duration::from_millis(GROK_QUEUE_KEY_WAIT_MS);
    loop {
        let screen = capture_session_terminal_text(zmx_name).await;
        if screen.as_deref().is_some_and(&done) || Instant::now() >= deadline {
            return screen;
        }
        tokio::time::sleep(Duration::from_millis(GROK_QUEUE_POLL_MS)).await;
    }
}

fn grok_take_back_failed(message: &str) -> SessionChatSendError {
    SessionChatSendError::new(SessionChatSendFailure::Write, message.to_string())
}

/// Takes one message out of Grok Build's queue: answers whether it was there to take. Runs inside
/// the session's send queue (the `PullGrokQueue` step), so no other send types between the keys.
///
/// CDXC:SessionChat 2026-10-22 WHY:
/// Grok Build (1.0.62) keeps its queue only on screen (`▾ Queued N`, at most three rows shown, the panel scrolling to the selected one) and writes a queued prompt to its transcript only once it runs it. Up in an empty prompt selects the newest row and each further Up the one above; `x` deletes the selected row and `e` moves it into the prompt, which would then need emptying, and Ctrl+U, the usual clear, restarts Grok to update it ("press ctrl+u to restart"). So the pull steps up row by row until the selected row reads as the message, deletes it with `x`, leaves the panel with Space (which types nothing), and the chat keeps the text it already has. Checked live: the deleted message never runs.
pub(crate) async fn pull_grok_queue(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    wanted: &str,
    cancelled: &(impl Fn() -> bool + Sync + ?Sized),
) -> Result<bool, SessionChatSendError> {
    let screen = capture_session_terminal_text_vt(zmx_name)
        .await
        .ok_or_else(|| {
            SessionChatSendError::new(
                SessionChatSendFailure::ComposerNotReady,
                "Grok's screen could not be read, so nothing was taken back.".to_string(),
            )
        })?;
    let notice =
        crate::session_chat_notice::classify_session_chat_terminal_notice(Some("grok"), &screen);
    let ready = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("grok"),
        &screen,
        notice.as_ref(),
    );
    if ready.state != crate::session_chat_composer::SessionChatComposerState::Ready
        || crate::session_chat_grok_blocking::detect_grok_blocking_screen(&screen).is_some()
        || grok_prompt_unfocused(&screen)
    {
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::ComposerNotReady,
            "Grok is not showing its prompt, so nothing was taken back.".to_string(),
        ));
    }
    // Up moves the cursor in a prompt that holds text instead of reaching the queue.
    let input = crate::session_chat_composer::session_chat_composer_input("grok", &screen);
    if !input.is_some_and(|input| input.is_empty()) {
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::ComposerNotCleared,
            "Grok's prompt has text in it. Send or clear it in the terminal, then try again."
                .to_string(),
        ));
    }
    let Some(queue) = capture_session_terminal_text(zmx_name)
        .await
        .as_deref()
        .and_then(read_grok_queue)
    else {
        // Nothing queued: Grok already took the message, and Up would recall an old prompt.
        return Ok(false);
    };
    if cancelled() {
        return Err(SessionChatSendError::not_attempted(
            SESSION_CHAT_SEND_CANCELLED.to_string(),
        ));
    }
    let wanted_first_line = wanted
        .lines()
        .map(take_back_key)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let write = |payload: &'static str| {
        write_session_chat_payload(project_id, session_id, zmx_name, source, payload)
    };
    let leave_panel = || async {
        write(GROK_QUEUE_LEAVE_KEY)
            .await
            .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
        let screen = wait_for_grok_screen(zmx_name, |screen| !grok_prompt_unfocused(screen)).await;
        if screen.as_deref().is_none_or(grok_prompt_unfocused) {
            return Err(grok_take_back_failed(
                "Grok stayed in its queue panel; press Space in the terminal to get back to its prompt.",
            ));
        }
        Ok(())
    };
    let mut found = None;
    for presses in 1..=queue.count {
        write(GROK_QUEUE_SELECT_KEY)
            .await
            .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
        let expected = queue.count + 1 - presses;
        let screen = wait_for_grok_screen(zmx_name, |screen| {
            read_grok_queue(screen).and_then(|queue| queue.selected().map(|row| row.number))
                == Some(expected)
        })
        .await;
        let selected = screen
            .as_deref()
            .and_then(read_grok_queue)
            .and_then(|queue| queue.selected().cloned());
        match selected {
            Some(row) if row.number == expected => {
                if grok_row_matches(&row, &wanted_first_line) {
                    found = Some(row);
                    break;
                }
            }
            // Grok ran its queue meanwhile, or the panel did not move as expected. With the queue
            // gone the Up recalled an old prompt into the box, which is emptied again.
            _ => {
                if screen.as_deref().is_some_and(grok_prompt_unfocused) {
                    leave_panel().await?;
                } else {
                    crate::session_chat_send::clear_session_chat_composer(
                        project_id, session_id, zmx_name, source, "grok", cancelled,
                    )
                    .await?;
                }
                return Ok(false);
            }
        }
    }
    let Some(row) = found else {
        leave_panel().await?;
        return Ok(false);
    };
    write(GROK_QUEUE_DELETE_KEY)
        .await
        .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
    let remaining = queue.count - 1;
    let screen = wait_for_grok_screen(zmx_name, |screen| {
        read_grok_queue(screen).map_or(0, |queue| queue.count) == remaining
    })
    .await;
    let deleted = screen
        .as_deref()
        .is_some_and(|screen| read_grok_queue(screen).map_or(0, |queue| queue.count) == remaining);
    if !deleted {
        // The queue ran before the `x` arrived, so the prompt took it as text.
        let typed = capture_session_terminal_text_vt(zmx_name)
            .await
            .and_then(|screen| {
                crate::session_chat_composer::session_chat_composer_input("grok", &screen)
            })
            .is_some_and(|input| input.text.trim() == GROK_QUEUE_DELETE_KEY);
        if typed {
            write(GROK_PROMPT_BACKSPACE).await.map_err(|message| {
                SessionChatSendError::new(SessionChatSendFailure::Write, message)
            })?;
            return Ok(false);
        }
        return Err(grok_take_back_failed(&format!(
            "Grok did not delete the queued message \"#{} {}\"; check its queue in the terminal.",
            row.number, row.text
        )));
    }
    leave_panel().await?;
    Ok(true)
}

pub(super) async fn take_back_grok_queued_message(
    target: &SessionChatSendTarget,
    text: String,
) -> Result<bool, SessionChatSendError> {
    let (completion_tx, completion_rx) = oneshot::channel();
    let (draft_tx, draft_rx) = oneshot::channel();
    queue_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        TAKE_BACK_SOURCE,
        vec![SessionChatSendStep::PullGrokQueue { text }],
        Some(completion_tx),
        Some(draft_tx),
        None,
    )
    .map_err(SessionChatSendError::not_attempted)?;
    completion_rx.await.map_err(|_| {
        SessionChatSendError::not_attempted(
            "The session chat send worker stopped before taking the message back.".to_string(),
        )
    })??;
    Ok(draft_rx
        .await
        .is_ok_and(|draft| !draft.queued_messages.is_empty()))
}
