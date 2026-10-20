//! A sign-in started on the Accounts page keeps running in gxserver after the page closes (Settings
//! closed, or another page opened). This watch takes it over from there and, once it completes,
//! brings Settings back at Accounts, where the page's own watch highlights and opens the account.
//!
//! CDXC:AgentProviders 2026-09-08 DECISION:
//! Finishing login reopens Settings at Accounts, even if the user left Settings while the browser was open. The account is already registered before this completion is announced.
use super::super::super::model::{SettingsModalCommand, SettingsModalHost};
use super::client::{ACCOUNT_SETUP_OWNER, ACCOUNTS_TIMEOUT};
use gpui::App;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// How often the watch asks for the sign-in's state (the page's own watch runs every 2s).
const WATCH_POLL: Duration = Duration::from_secs(2);
/// A sign-in still running after this long was abandoned in the browser; stop watching it.
const WATCH_LIMIT: Duration = Duration::from_secs(30 * 60);

thread_local! {
    /// The sign-ins being watched. An Accounts page that opens takes them back (`stop`).
    static WATCHED_JOBS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The Accounts page is open again and watches its sign-ins itself.
pub(crate) fn stop() {
    WATCHED_JOBS.with(|jobs| jobs.borrow_mut().clear());
}

fn watching(job_ids: &[String]) -> bool {
    WATCHED_JOBS.with(|jobs| jobs.borrow().as_slice() == job_ids)
}

/// Watches `job_ids` (the sign-ins still running when the page closed) until one completes.
pub(crate) fn watch(host: SettingsModalHost, job_ids: Vec<String>, cx: &mut App) {
    // Leaving Accounts and then closing Settings hand the same sign-ins over twice.
    if job_ids.is_empty() || watching(&job_ids) {
        return;
    }
    WATCHED_JOBS.with(|jobs| *jobs.borrow_mut() = job_ids.clone());
    poll(host, job_ids, Instant::now(), cx);
}

fn poll(host: SettingsModalHost, job_ids: Vec<String>, started: Instant, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(WATCH_POLL).await;
        cx.update(|cx| {
            if !watching(&job_ids) {
                return;
            }
            if started.elapsed() > WATCH_LIMIT {
                stop();
                return;
            }
            let next = host.clone();
            host(
                SettingsModalCommand::GxserverRpc {
                    path: "/api/agentAccounts".to_string(),
                    params: json!({ "operation": "setupStatus", "owner": ACCOUNT_SETUP_OWNER }),
                    timeout: ACCOUNTS_TIMEOUT,
                    reply: Box::new(move |result, cx| {
                        if !watching(&job_ids) {
                            return;
                        }
                        // An unreachable gxserver is retried; an answer decides.
                        let Ok(state) = result else {
                            poll(next, job_ids, started, cx);
                            return;
                        };
                        let jobs: Vec<&Value> = state["setupJobs"]
                            .as_array()
                            .map(|jobs| {
                                jobs.iter()
                                    .filter(|job| {
                                        job["id"].as_str().is_some_and(|id| {
                                            job_ids.iter().any(|watched| watched == id)
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if jobs.iter().any(|job| job["status"] == "complete") {
                            stop();
                            next(SettingsModalCommand::OpenAccounts, cx);
                        } else if jobs.iter().any(|job| {
                            !matches!(job["status"].as_str(), Some("complete" | "failed"))
                        }) {
                            poll(next, job_ids, started, cx);
                        } else {
                            // Failed, cancelled or gone: nothing to come back for.
                            stop();
                        }
                    }),
                },
                cx,
            );
        });
    })
    .detach();
}
