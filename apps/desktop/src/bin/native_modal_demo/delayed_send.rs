//! Demo host for the native Session Automations dialog.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `active` (an armed 1h 35m timer
//! with Close after Done on), `agentstops` (armed "when this agent finishes"),
//! `specific` (armed on a specific awake agent), `manyagents` (40 awake agents),
//! `noagents` (the awake list comes back empty), `agenterror` (the awake read fails), `nopicker` (the
//! all-agents trigger is unsupported, so no awake picker), `noicon` (no agent
//! logo and no session title).
use super::delayed_send_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

fn fake_sessions() -> Vec<DelayedSendAgentOption> {
    [
        ("Restyle Session Automations", "G12"),
        ("Fix hookless agent modal", "G7"),
        ("Write release notes", "G31"),
    ]
    .into_iter()
    .map(|(title, session_id)| DelayedSendAgentOption {
        reference: DelayedSendAgentReference {
            project_id: "ghostex".to_string(),
            session_id: session_id.to_string(),
        },
        label: format!("{title} ({session_id})"),
    })
    .collect()
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let specific =
        matches!(state.as_str(), "specific" | "manyagents").then(|| DelayedSendAgentReference {
            project_id: "ghostex".to_string(),
            session_id: "G7".to_string(),
        });
    let config = DelayedSendModalConfig {
        agent_icon_path: (state != "noicon").then(|| "agent-icons/codex.svg".to_string()),
        close_after_done_active: state == "active",
        supports_close_after_done: true,
        delayed_send_deadline_remaining_ms: (state == "active").then_some(95 * 60 * 1000),
        delayed_send_remaining_label: (state == "active").then(|| "1h 35m".to_string()),
        send_when_all_project_sessions_stop_active: false,
        send_when_agent_stops_active: state == "agentstops",
        send_when_specific_agent_finishes: specific.clone(),
        supports_send_when_agent_stops: true,
        supports_send_when_all_project_sessions_stop: state != "nopicker",
        title: (state != "noicon").then(|| "Restyle Session Automations".to_string()),
        status_accent: None,
        palette: demo.palette,
    };
    let slot: Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiDelayedSendModalWindow>)>>> =
        Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let host: DelayedSendModalHost = Rc::new(move |command, cx: &mut App| match command {
        DelayedSendModalCommand::RequestAgents { request_id } => {
            eprintln!("request agents {request_id}");
            let slot = host_slot.clone();
            let state = state.clone();
            let specific = specific.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let target = slot.borrow().clone();
                let Some((window, view)) = target else {
                    return;
                };
                cx.update(|cx| {
                    let _ = window.update(cx, |_root, window, cx| {
                        view.update(cx, |modal, cx| {
                            let (sessions, error) = match state.as_str() {
                                "manyagents" => (
                                    (1..=40)
                                        .map(|index| DelayedSendAgentOption {
                                            reference: DelayedSendAgentReference {
                                                project_id: "ghostex".to_string(),
                                                session_id: format!("G{index}"),
                                            },
                                            label: format!("Agent session {index:02} (G{index})"),
                                        })
                                        .collect(),
                                    None,
                                ),
                                "noagents" => (Vec::new(), None),
                                "agenterror" => (
                                    Vec::new(),
                                    Some("Could not load agent sessions.".to_string()),
                                ),
                                _ => (fake_sessions(), None),
                            };
                            modal.receive_agents(
                                &request_id,
                                sessions,
                                specific.clone(),
                                error,
                                window,
                                cx,
                            );
                        });
                    });
                });
            })
            .detach();
        }
        DelayedSendModalCommand::Save {
            toggle_close_after_done,
            send,
        } => {
            match send {
                DelayedSendModalSend::Schedule {
                    delay_ms,
                    send_when_agent_stops,
                    send_when_all_project_sessions_stop,
                    send_when_specific_agent_finishes,
                } => eprintln!(
                    "save: toggleCloseAfterDone={toggle_close_after_done} schedule delayMs={delay_ms:?} agentStops={send_when_agent_stops} allStop={send_when_all_project_sessions_stop} specific={send_when_specific_agent_finishes:?}"
                ),
                DelayedSendModalSend::CancelTimer => {
                    eprintln!("save: toggleCloseAfterDone={toggle_close_after_done} cancel timer")
                }
                DelayedSendModalSend::Keep => {
                    eprintln!("save: toggleCloseAfterDone={toggle_close_after_done} keep send")
                }
            }
            cx.quit();
        }
        DelayedSendModalCommand::Cancel => {
            eprintln!("cancel");
            cx.quit();
        }
    });
    let (window, view) = super::open_modal_window(
        DELAYED_SEND_MODAL_WIDTH,
        DELAYED_SEND_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiDelayedSendModalWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view));
}
