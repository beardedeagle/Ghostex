//! `/api/voice`: the WebSocket every client opens for a voice conversation or a dictation. Binary
//! frames are 24 kHz mono PCM16 audio both ways; text frames are JSON control and events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ws::{
    rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade,
};
use axum::extract::State;
use axum::http::{HeaderMap, Response, StatusCode, Uri};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::conversation::{ClientFrame, Conversation, Input};
use super::provider::{self, ConnectError, Provider};
use super::{announcements, dictation, keys, prompt, registry, tools};
use crate::auth::{is_authorized_headers, is_expected_gxserver_auth_token};
use crate::protocol::{protocol_mismatch_error, rpc_error};
use crate::server::{
    is_expected_protocol_version, json_response, query_value, read_protocol_version, request_id,
    AppState,
};

static NEXT_ATTACH: AtomicU64 = AtomicU64::new(1);

pub(crate) async fn handle_voice(
    State(state): State<Arc<AppState>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let Ok(ws) = ws else {
        return json_response(
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                "A voice WebSocket is required.",
                Some(request_id),
            ),
        );
    };
    let query_token = query_value(&uri, "authToken");
    let authorized = is_authorized_headers(&headers, &state.auth_token)
        || query_token
            .as_deref()
            .is_some_and(|token| is_expected_gxserver_auth_token(token, &state.auth_token));
    if !authorized {
        return json_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }
    let version = read_protocol_version(&headers, &uri, None);
    if !is_expected_protocol_version(version.as_ref()) {
        return json_response(
            StatusCode::UPGRADE_REQUIRED,
            protocol_mismatch_error(version, Some(request_id)),
        );
    }
    let mode = query_value(&uri, "mode").unwrap_or_else(|| "conversation".to_string());
    let conversation_id = query_value(&uri, "conversationId").filter(|id| !id.trim().is_empty());
    let provider = query_value(&uri, "provider").and_then(|id| Provider::from_id(&id));
    let language = query_value(&uri, "language")
        .map(|language| language.trim().to_string())
        .filter(|language| {
            !language.is_empty()
                && language.len() <= 16
                && language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    ws.on_upgrade(move |socket| async move {
        match mode.as_str() {
            "dictation" => dictation::run(&state, socket, language).await,
            "conversation" => {
                run_conversation_socket(state, socket, conversation_id, provider).await
            }
            _ => {
                fail(
                    &mut { socket },
                    "badRequest",
                    "mode must be conversation or dictation.",
                )
                .await
            }
        }
    })
}

/// Sends a fatal error and closes the socket.
pub(super) async fn fail(socket: &mut WebSocket, code: &str, message: &str) {
    let _ = socket
        .send(Message::Text(
            json!({"type": "error", "code": code, "message": message, "fatal": true})
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket
        .send(Message::Text(
            json!({"type": "ended", "reason": "error", "message": message})
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

async fn run_conversation_socket(
    state: Arc<AppState>,
    mut socket: WebSocket,
    conversation_id: Option<String>,
    provider: Option<Provider>,
) {
    let (tx, resumed) = match conversation_id {
        Some(id) => match registry::sender(&id) {
            Some(tx) => (tx, true),
            None => {
                fail(
                    &mut socket,
                    "notFound",
                    "That voice conversation has ended. Start a new one.",
                )
                .await;
                return;
            }
        },
        None => match start_conversation(&state, &mut socket, provider).await {
            Some(tx) => (tx, false),
            None => return,
        },
    };
    let attach_id = NEXT_ATTACH.fetch_add(1, Ordering::Relaxed);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientFrame>();
    if tx
        .send(Input::Attach {
            attach_id,
            out: out_tx,
            resumed,
        })
        .is_err()
    {
        fail(
            &mut socket,
            "notFound",
            "That voice conversation has ended. Start a new one.",
        )
        .await;
        return;
    }
    let (mut client_sink, mut client_stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let message = match frame {
                ClientFrame::Json(value) => Message::Text(value.to_string().into()),
                ClientFrame::Audio(bytes) => Message::Binary(bytes),
                ClientFrame::Close => {
                    let _ = client_sink.send(Message::Close(None)).await;
                    break;
                }
            };
            if client_sink.send(message).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = client_stream.next().await {
        let input = match message {
            Message::Binary(bytes) => Input::Audio(bytes),
            Message::Text(text) => {
                let value: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                let field = |key: &str| {
                    value
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                match value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "text" => Input::Text(field("text")),
                    "interrupt" => Input::Interrupt,
                    "playback" => Input::Playback {
                        item_id: field("itemId"),
                        played_ms: value.get("playedMs").and_then(Value::as_u64).unwrap_or(0),
                    },
                    "end" => Input::End,
                    "continueMemory" => Input::ContinueMemory,
                    "forgetMemory" => {
                        super::memory::forget(&state);
                        continue;
                    }
                    "voice" => Input::Voice(field("voice")),
                    "mute" => {
                        Input::Mute(value.get("muted").and_then(Value::as_bool) == Some(true))
                    }
                    "settingsChanged" => Input::SettingsChanged,
                    "playbackRate" => match value.get("rate").and_then(Value::as_f64) {
                        Some(rate) if rate.is_finite() => Input::PlaybackRate(rate),
                        _ => continue,
                    },
                    _ => continue,
                }
            }
            Message::Close(_) => break,
            _ => continue,
        };
        if tx.send(input).is_err() {
            break;
        }
    }
    let _ = tx.send(Input::Detach { attach_id });
    // The conversation closes the writer when it ends; a detached one just stops writing here.
    writer.abort();
}

/// Opens the provider session and starts the conversation task, or tells the client why not.
async fn start_conversation(
    state: &Arc<AppState>,
    socket: &mut WebSocket,
    provider: Option<Provider>,
) -> Option<mpsc::UnboundedSender<Input>> {
    let provider = provider.unwrap_or_else(|| keys::selected_provider(&state.paths));
    let key = match keys::read_key(&state.paths, provider) {
        Ok(Some(key)) => key,
        Ok(None) => {
            fail(
                socket,
                "noKey",
                &format!(
                    "Add your {} API key in Settings > Voice agent.",
                    provider.label()
                ),
            )
            .await;
            return None;
        }
        Err(message) => {
            fail(socket, "noKey", &message).await;
            return None;
        }
    };
    let _ = socket
        .send(Message::Text(
            json!({"type": "state", "state": "connecting"})
                .to_string()
                .into(),
        ))
        .await;
    let mut upstream = match provider::connect_realtime(provider, &key).await {
        Ok(upstream) => upstream,
        Err(ConnectError::BadKey) => {
            fail(
                socket,
                "badKey",
                &format!(
                    "{} refused the API key. Check it in Settings > Voice agent.",
                    provider.label()
                ),
            )
            .await;
            return None;
        }
        Err(ConnectError::Refused(message)) => {
            super::warn_error(
                state,
                "voiceAgent.providerRefused",
                &message,
                json!({"provider": provider.id()}),
            );
            fail(socket, "providerRefused", &message).await;
            return None;
        }
        Err(ConnectError::Unavailable(message)) => {
            super::warn(
                state,
                "voiceAgent.providerUnavailable",
                json!({"provider": provider.id(), "message": message}),
            );
            fail(
                socket,
                "providerUnavailable",
                &format!("Couldn't reach {}: {message}", provider.label()),
            )
            .await;
            return None;
        }
    };
    let voice = keys::selected_voice(&state.paths, provider);
    let keyterms = keyterms(state).await;
    let update = provider.session_update(
        &voice,
        &prompt::instructions(),
        &tools::definitions(),
        &keyterms,
    );
    if upstream
        .send(WsMessage::Text(update.to_string().into()))
        .await
        .is_err()
    {
        fail(
            socket,
            "providerUnavailable",
            &format!("{} closed the connection.", provider.label()),
        )
        .await;
        return None;
    }
    // Wait for the provider to accept the configuration, so a bad voice or model fails here.
    let configured = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(frame) = upstream.next().await {
            let Ok(WsMessage::Text(text)) = frame else {
                if frame.is_err() {
                    return Err(format!("{} closed the connection.", provider.label()));
                }
                continue;
            };
            let event: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
            match event.get("type").and_then(Value::as_str) {
                Some("session.updated") => return Ok(()),
                Some("error") => {
                    return Err(event
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("The provider refused the voice session settings.")
                        .to_string())
                }
                _ => {}
            }
        }
        Err(format!("{} closed the connection.", provider.label()))
    })
    .await;
    match configured {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            super::warn(
                state,
                "voiceAgent.sessionRefused",
                json!({"provider": provider.id(), "message": message}),
            );
            fail(socket, "providerError", &message).await;
            return None;
        }
        Err(_) => {
            fail(
                socket,
                "providerUnavailable",
                &format!("{} did not answer within 10 seconds.", provider.label()),
            )
            .await;
            return None;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::unbounded_channel();
    registry::insert(&id, tx.clone(), provider);
    announcements::ensure_running(state);
    let conversation = Conversation::new(
        id,
        state.clone(),
        provider,
        voice,
        update,
        upstream,
        tx.clone(),
        rx,
    );
    tokio::spawn(conversation.run());
    Some(tx)
}

/// Names the recognizer should expect: Ghostex, the agents and the user's project names (xAI takes
/// at most 100 terms of 50 characters).
async fn keyterms(state: &Arc<AppState>) -> Vec<String> {
    let mut terms: Vec<String> = ["Ghostex", "Claude", "Codex", "Cursor", "OpenCode", "Gemini"]
        .iter()
        .map(|term| term.to_string())
        .collect();
    if let Ok(projects) = super::sessions::list_projects_async(state).await {
        for project in projects {
            let name = project.name.trim().to_string();
            if !name.is_empty()
                && name.chars().count() <= 50
                && !name.starts_with("Chat 20")
                && !terms.contains(&name)
            {
                terms.push(name);
            }
        }
    }
    terms.truncate(100);
    terms
}
