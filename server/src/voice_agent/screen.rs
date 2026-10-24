//! `get_active_session`: which session the user is looking at, so "this session" and "the one I'm
//! looking at" name one.
//!
//! The desktop's focused session is asked of the app over the renderer command channel `show.rs`
//! already uses (`readFocusedSession`): nothing reports it to gxserver, and the shown-sessions
//! report (`/api/holdSessionsAwake`) lists every pane on screen without saying which is focused.
//! A phone's open chat is the chat lease it already reports while that chat is in its foreground
//! (`/api/holdSessionChatGrid`), so a phone that shows a chat is the device in the user's hand.
//!
//! CDXC:VoiceMode 2026-10-21 DECISION:
//! User: "Give the voice agent a way to see which session is currently the active one on screen in the Ghostex app (the session the user is looking at in the desktop window) ... so that when the user says 'this session' / 'the one I'm looking at', the agent knows which it is." The tool answers the desktop's focused session (and what else is on screen there) and any chat a phone shows right now; the instructions say a phone chat wins when there is one, since the phone reports a chat only while it is in front of the user.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::loopback;
use super::sessions::{self, SessionInfo};
use crate::server::AppState;

/// `client_remote`: the call's client is the app on another computer (`clientRemote=1`), whose
/// screen this computer's app cannot see.
pub(super) async fn get_active_session(
    state: &Arc<AppState>,
    client_remote: bool,
) -> Result<Value, String> {
    let all = sessions::list_sessions_async(state).await?;
    let describe = |project_id: &str, session_id: &str| -> Value {
        let reference = format!("{project_id}:{session_id}");
        all.iter()
            .find(|session| session.reference() == reference)
            .map(SessionInfo::to_tool_value)
            .unwrap_or_else(|| json!({"ref": reference}))
    };
    let mut output = json!({"ok": true});
    let screen = if client_remote {
        Err(String::new())
    } else {
        loopback::call(
            state,
            "/api/dispatchRendererCommand",
            json!({"action": "readFocusedSession", "payload": {}}),
            Duration::from_secs(4),
        )
        .await
    };
    match screen {
        Ok(screen) => {
            let key = |value: &Value| {
                Some((
                    value.get("projectId")?.as_str()?.to_string(),
                    value.get("sessionId")?.as_str()?.to_string(),
                ))
            };
            let focused = screen.get("focused").and_then(key);
            output["desktop"] = match &focused {
                Some((project_id, session_id)) => describe(project_id, session_id),
                None => json!("The app shows no session of this computer right now."),
            };
            let others: Vec<Value> = screen
                .get("onScreen")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(key)
                .filter(|other| Some(other) != focused.as_ref())
                .map(|(project_id, session_id)| {
                    let session = describe(&project_id, &session_id);
                    json!({"ref": session["ref"], "title": session["title"]})
                })
                .collect();
            if !others.is_empty() {
                output["desktopAlsoOnScreen"] = json!(others);
            }
        }
        Err(_) if client_remote => {
            output["desktop"] = json!(
                "The user's Ghostex app is on another computer, so which session it shows is unknown here. Ask which session they mean."
            );
        }
        Err(error) => {
            output["desktop"] = json!(format!(
                "The Ghostex app on the computer did not say: {error}"
            ));
        }
    }
    let phone: Vec<Value> = crate::session_chat_grid_claim::chats_shown_by(
        crate::session_chat_grid_claim::PHONE_CHAT_HOLDER_PREFIX,
    )
    .iter()
    .map(|(project_id, session_id)| describe(project_id, session_id))
    .collect();
    if !phone.is_empty() {
        output["phoneChat"] = json!(phone);
    }
    Ok(output)
}
