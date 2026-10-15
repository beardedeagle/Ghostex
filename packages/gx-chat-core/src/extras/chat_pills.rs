//! The pills on the working row: one small pill per panel that used to stand above the composer.
//! A pill opens a floating menu with that panel's rows; the renderers only lay the row out and
//! draw the menu from the panel values beside it (`coordinatorThreadsPanel`, `agentTasksPanel`,
//! `agentFleetStrip`, `backgroundTasksPanel`). The renderers put two icon pills of their own before
//! these: "Scroll to bottom" (it depends on their scroll) and "Previous question".
//!
//! CDXC:SessionChat 2026-10-15 DECISION:
//! User: "i want threads to be a small pill above the chat and i want other things to also be pills above the chat that are aligned to the right on the same row as "working" and scrollable horizontally when there's no space", "i want to reduce the amout of vertical space taken up by these", "on mobile or when the chat is really narrow then this pills row can be below the "working" status line instead of in the same row as it", and "when i click on the Threads pill we should show a menu that shows what we have now but floating basically instead of pushing up the chat transcript like we do now". So Threads, Tasks, Subagents and (2026-10-15, user's answer) Background are pills in this order, right-aligned on the working row (the word keeps the left side), scrolling sideways and never wrapping; a narrow chat (and the phone, always) puts the pills on their own line under the word; a pill's menu floats over the transcript and closes on a click outside or Escape. Background tasks were first left a card; the user then asked for them as a pill too (extras/background_tasks.rs). Later the same day: "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons", then "make the scroll to bottom appear left most so it doesn't push stuff when it appears": the renderers draw scroll-to-bottom (a down arrow) first, then Previous question (a ?), then these.
//!
//! CDXC:SessionChat 2026-10-15 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_strip.rs and chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/WorkingStrip.tsx and ChatPills.tsx (phone) draw this list and must match.

use serde_json::Value;

use crate::document::ChatPill;

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The pills, in the order the row shows them, from the four projected panel values (`null` when
/// the panel is absent).
pub fn chat_pills(
    threads: &Value,
    tasks: &Value,
    fleet: &Value,
    background: &Value,
) -> Vec<ChatPill> {
    let mut pills = Vec::new();
    if threads.is_object() {
        // The count is what the menu lists first: the threads working or waiting on the user.
        let in_flight = count(threads, "inFlight");
        let meta = text(threads, "meta");
        pills.push(ChatPill {
            id: "threads".to_string(),
            label: "Threads".to_string(),
            count: if in_flight > 0 {
                in_flight.to_string()
            } else {
                String::new()
            },
            attention: threads.get("attention") == Some(&Value::Bool(true)),
            aria_label: format!("Threads, {meta}"),
            meta,
        });
    }
    if tasks.is_object() {
        let meta = text(tasks, "meta");
        pills.push(ChatPill {
            id: "tasks".to_string(),
            label: "Tasks".to_string(),
            count: format!("{}/{}", count(tasks, "done"), count(tasks, "total")),
            attention: false,
            aria_label: format!("Tasks, {meta}"),
            meta,
        });
    }
    if fleet.is_object() {
        let rows = fleet
            .get("rows")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let meta = text(fleet, "countLabel");
        pills.push(ChatPill {
            id: "subagents".to_string(),
            label: "Subagents".to_string(),
            count: rows.to_string(),
            attention: false,
            aria_label: format!("Subagents, {meta}"),
            meta,
        });
    }
    if background.is_object() {
        let meta = text(background, "meta");
        pills.push(ChatPill {
            id: "background".to_string(),
            label: "Background".to_string(),
            count: count(background, "count").to_string(),
            attention: false,
            aria_label: format!("Background tasks, {meta}"),
            meta,
        });
    }
    pills
}
