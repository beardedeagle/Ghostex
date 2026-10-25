//! The second half of Dictate: a saved recording (`recordings.rs`) sent to xAI's batch
//! speech-to-text (`POST /v1/stt`, which takes the `.m4a` or `.wav` as it is) for its text. A
//! failure is kept on the recording with a sentence that says why and what to do; the audio stays
//! whatever happens, so Transcribe again works once the cause is gone.
//!
//! CDXC:VoiceMode 2026-10-20 WHY:
//! The streaming dictation socket (`dictation.rs`) tied the recording to the provider connection: a refused key, an empty account or a dropped connection ended the dictation and lost what was said. Transcribing a finished file decouples the two (the user's decision in `recordings.rs`). Time-fixable failures (rate limits, xAI unavailable, no connection) are retried here a few times before the recording is marked failed.

use std::time::Duration;

use serde_json::{json, Value};

use super::keys;
use super::provider::{self, Provider};
use super::recordings::{self, Problem, Recording, Status};
use crate::server::AppState;

const XAI_BATCH_STT_URL: &str = "https://api.x.ai/v1/stt";
/// Words the recognizer should expect; it otherwise writes "Gostex".
const KEYTERMS: &[&str] = &["Ghostex"];
/// Waits before each retry of a failure time can fix.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

/// What a transcription found.
enum Outcome {
    Text(String),
    /// Worth trying again in a moment (rate limited, xAI unavailable, no connection).
    Transient(Problem),
    Final(Problem),
}

fn problem(code: &str, message: impl Into<String>) -> Problem {
    Problem {
        code: code.to_string(),
        message: message.into(),
    }
}

/// Transcribes `id`, saving its text or why it failed. A recording dismissed while this ran stays
/// dismissed and its text is not saved. Answers the recording as it now is.
pub(super) async fn transcribe(state: &AppState, id: &str) -> Result<Recording, String> {
    let paths = state.paths.clone();
    let recording = recordings::read(&paths, id)
        .ok_or_else(|| "That recording no longer exists.".to_string())?;
    if recording.status == Status::Transcribed {
        return Ok(recording);
    }
    if recording.text.is_some() {
        // Dismissed after xAI had already answered: Recover needs no second call.
        let paths = paths.clone();
        let id = id.to_string();
        return tokio::task::spawn_blocking(move || {
            recordings::update(&paths, &id, |recording| {
                recording.status = Status::Transcribed
            })
        })
        .await
        .map_err(|error| error.to_string())?;
    }
    let generation = recordings::generation(id);
    let recording = {
        let paths = paths.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            recordings::update(&paths, &id, |recording| {
                recording.status = Status::Transcribing;
                recording.error = None;
            })
        })
        .await
        .map_err(|error| error.to_string())??
    };
    let mut outcome = call_once(&paths, &recording).await;
    for delay in RETRY_DELAYS {
        if !matches!(outcome, Outcome::Transient(_)) || recordings::generation(id) != generation {
            break;
        }
        tokio::time::sleep(delay).await;
        outcome = call_once(&paths, &recording).await;
    }
    if let Outcome::Transient(problem) | Outcome::Final(problem) = &outcome {
        super::warn(
            state,
            "voiceAgent.dictationTranscribeFailed",
            json!({"code": problem.code, "durationMs": recording.duration_ms}),
        );
    }
    let id = id.to_string();
    tokio::task::spawn_blocking(move || {
        recordings::update(&paths, &id, |recording| {
            if recordings::generation(&recording.id) != generation {
                // Dismissed (or deleted and re-made) while xAI worked: the dismissal stands.
                return;
            }
            match outcome {
                Outcome::Text(text) => {
                    recording.status = Status::Transcribed;
                    recording.text = Some(text);
                    recording.error = None;
                }
                Outcome::Transient(problem) | Outcome::Final(problem) => {
                    recording.status = Status::Failed;
                    recording.error = Some(problem);
                }
            }
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn call_once(paths: &crate::paths::GxserverPaths, recording: &Recording) -> Outcome {
    let key = match keys::read_key(paths, Provider::Xai) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return Outcome::Final(problem(
                "noKey",
                "Dictation uses your xAI API key: add it in Settings > Extensions > Voice agent, then press Transcribe again.",
            ))
        }
        Err(message) => return Outcome::Final(problem("noKey", message)),
    };
    let audio_path = recordings::folder(paths).join(&recording.audio_file);
    let audio = match tokio::fs::read(&audio_path).await {
        Ok(audio) if !audio.is_empty() => audio,
        _ => {
            return Outcome::Final(problem(
                "audioMissing",
                "The recording's audio file is missing, so it can't be transcribed.",
            ))
        }
    };
    let file_name = recording.audio_file.clone();
    let language = recording.language.clone();
    // A minute of audio takes xAI a few seconds; long recordings get longer.
    let timeout = Duration::from_secs(60 + recording.duration_ms / 2_000);
    tokio::task::spawn_blocking(move || {
        post_batch(&key, &file_name, &audio, language.as_deref(), timeout)
    })
    .await
    .unwrap_or_else(|error| Outcome::Transient(problem("providerUnavailable", error.to_string())))
}

/// The multipart request xAI documents: options first, `file` last.
fn multipart_body(
    boundary: &str,
    file_name: &str,
    audio: &[u8],
    language: Option<&str>,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(audio.len() + 1024);
    let mut field = |name: &str, value: &str| {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    };
    if let Some(language) = language {
        field("language", language);
    }
    for keyterm in KEYTERMS {
        field("keyterm", keyterm);
    }
    let mime = if file_name.ends_with(".m4a") {
        "audio/mp4"
    } else {
        "audio/wav"
    };
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(audio);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn post_batch(
    key: &str,
    file_name: &str,
    audio: &[u8],
    language: Option<&str>,
    timeout: Duration,
) -> Outcome {
    let boundary = format!("ghostex-{}", uuid::Uuid::new_v4().simple());
    let body = multipart_body(&boundary, file_name, audio, language);
    let response = ureq::post(XAI_BATCH_STT_URL)
        .set("authorization", &format!("Bearer {key}"))
        .set(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .timeout(timeout)
        .send_bytes(&body);
    match response {
        Ok(response) => match response.into_json::<Value>() {
            Ok(answer) => Outcome::Text(
                answer
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            ),
            Err(error) => Outcome::Transient(problem(
                "providerError",
                format!("xAI's answer could not be read ({error}). Press Transcribe again."),
            )),
        },
        Err(ureq::Error::Status(status, response)) => {
            let detail = response.into_string().unwrap_or_default();
            status_problem(status, &detail)
        }
        Err(error) => Outcome::Transient(problem(
            "providerUnavailable",
            format!(
                "Couldn't reach xAI ({}). Check this computer's internet connection, then press Transcribe again.",
                error.kind()
            ),
        )),
    }
}

/// What a refusal means for the recording, in the same words the voice panel and the streaming
/// dictation use (`provider::refused_sentence`: xAI's own text when its answer has a body).
///
/// CDXC:VoiceMode 2026-10-20 WHY: xAI documents no HTTP 402 (its rate limits are 429), yet answered 402 for a few minutes on 2026-10-20 and then served again; an account on The X Subscription draws API usage from the plan's weekly included pool and needs prepaid credit beyond it. So a 402 is retried like a rate limit, says what xAI said, and never claims the account is out of credit.
fn status_problem(status: u16, body: &str) -> Outcome {
    let said = provider::refused_sentence(
        Provider::Xai.label(),
        status,
        (!body.trim().is_empty()).then_some(body.as_bytes()),
    );
    match status {
        401 | 403 => Outcome::Final(problem(
            "badKey",
            "xAI refused the API key. Check it in Settings > Extensions > Voice agent, then press Transcribe again.",
        )),
        400 | 413 | 415 | 422 => Outcome::Final(problem("badAudio", said)),
        402 => Outcome::Transient(problem("paymentRequired", said)),
        429 => Outcome::Transient(problem("rateLimited", said)),
        _ => Outcome::Transient(problem("providerUnavailable", said)),
    }
}
