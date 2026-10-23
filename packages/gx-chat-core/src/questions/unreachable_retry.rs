//! Answers that wait for gxserver to come back: a question form's send and a question or approval
//! card's answer that got no answer at all from Ghostex's service (it is restarting, or the
//! connection dropped) stay on their way and go again every [`UNREACHABLE_RETRY_DELAY_MS`] under
//! the same `sendRequestId` until it answers.
//!
//! CDXC:SessionChat 2026-10-23 DECISION: User (through the orchestrator): picking a choice is local form state and must work while gxserver restarts; only sending needs gxserver, and that "should wait and retry on its own until gxserver is back, with a small 'reconnecting' note on the card, never a dead click". Before, a send that met a restarting gxserver gave up after the desktop's 5 s restart wait and left the card with a connection error. Now it keeps its answers and the card says it is reconnecting (`reconnecting` on the form and on `questionCard`), retried without limit while the chat is open as the session's own reads are (the SessionChat 2026-10-08 DECISION on `UNREACHABLE_RETRY_DELAY_MS`). A retry repeats the first call's `sendRequestId`, so gxserver's send ledger (server/src/session_chat_send_requests.rs) answers a call that did land with its first result instead of delivering the answers twice. A refusal gxserver sent itself still ends the send with its error.
//! SEE-ALSO: apps/desktop/src/app/native_chat/question_form.rs, question.rs, approval.rs; apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx, cards/QuestionCard.tsx, cards/ApprovalCard.tsx (the note).

use serde_json::json;

use crate::effect::Effect;
use crate::session::constants::UNREACHABLE_RETRY_DELAY_MS;
use crate::session::reads::is_unreachable;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

const RETRY_TIMER: &str = "c:unreachable-send-retry";

/// Where a send stands with a gxserver that did not answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Retry {
    /// The last attempt got no answer from gxserver; the card says it is reconnecting.
    pub reconnecting: bool,
    /// A retry is out now, so the timer sends nothing more until it settles.
    pub in_flight: bool,
}

impl Retry {
    /// Due for another attempt.
    pub fn due(&self) -> bool {
        self.reconnecting && !self.in_flight
    }

    /// Takes the outcome of an attempt, and says whether it means "gxserver never answered, wait
    /// and send again" rather than a result or a refusal.
    pub fn wait_out(retry: &mut Retry, outcome: &RpcOutcome) -> bool {
        retry.in_flight = false;
        let unreachable =
            matches!(outcome, RpcOutcome::Err { code, .. } if is_unreachable(code.as_deref()));
        retry.reconnecting = unreachable;
        unreachable
    }
}

/// Names the card's answer call with a `sendRequestId` and keeps its parameters, so a retry is the
/// same send to gxserver. Runs over the effects of every turn that can start an answer.
pub fn stamp_answer_send(state: &mut ChatState, context: &ChatContext, effects: &mut [Effect]) {
    let Some(request) = state.questions.answer_request.as_mut() else {
        return;
    };
    for effect in effects.iter_mut() {
        if let Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::AnswerSessionChatPrompt,
            params,
        } = effect
        {
            if *request_id != request.request_id {
                continue;
            }
            if params.get("sendRequestId").is_none() {
                params["sendRequestId"] = json!(format!(
                    "question-answer-{request_id}-{}",
                    context.now_millis()
                ));
            }
            request.params = (**params).clone();
        }
    }
}

/// The retry clock: sends again what is due when it fires, and keeps it armed while anything waits.
pub fn settle(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    if state.core.timer_fired(RETRY_TIMER) {
        effects.extend(crate::questions::forms::resend_unreachable(state));
        let request_id = state
            .questions
            .answer_request
            .as_ref()
            .filter(|request| request.retry.due())
            .map(|_| state.core.allocate_request_id());
        if let (Some(request_id), Some(request)) =
            (request_id, state.questions.answer_request.as_mut())
        {
            request.request_id = request_id;
            request.retry.in_flight = true;
            effects.push(Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::AnswerSessionChatPrompt,
                params: Box::new(request.params.clone()),
            });
        }
    }
    let waiting = crate::questions::forms::any_waiting_for_server(state)
        || state
            .questions
            .answer_request
            .as_ref()
            .is_some_and(|request| request.retry.due());
    if waiting {
        state.core.timers.arm_once(
            RETRY_TIMER,
            context.now_ms,
            UNREACHABLE_RETRY_DELAY_MS as f64,
        );
    }
    effects
}
