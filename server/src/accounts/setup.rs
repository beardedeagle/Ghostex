use super::{
    codex_blockers, endpoint, helpers,
    model::Provider,
    setup_terminal::{LoginTerminal, COLS, ROWS},
    store,
};
use crate::{domain::DomainStateError, server::AppState};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

pub(crate) struct SetupJob {
    view: Value,
    process_id: Option<u32>,
    writer: Option<Box<dyn Write + Send>>,
    killer: Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}
pub(crate) type SetupJobs = Mutex<HashMap<String, Arc<Mutex<SetupJob>>>>;

impl SetupJob {
    fn cancel(&mut self) -> Result<(), DomainStateError> {
        if self.killer.is_none() {
            return Ok(());
        }
        // portable-pty starts a new session, so this group contains only this login and its CLI children.
        #[cfg(unix)]
        if let Some(pid) = self.process_id {
            let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(store::error(std::io::Error::last_os_error()));
            }
        }
        #[cfg(not(unix))]
        if let Some(killer) = &mut self.killer {
            killer.kill().map_err(store::error)?;
        }
        self.writer = None;
        Ok(())
    }
}
pub(crate) fn cancel_all(state: &AppState) {
    if let Ok(jobs) = state.accounts.setup_jobs.lock() {
        for job in jobs.values() {
            if let Ok(mut job) = job.lock() {
                let _ = job.cancel();
            }
        }
    }
}

fn response(jobs: Vec<Value>) -> Value {
    json!({"accounts":[],"helpers":[],"defaults":{"claude":{},"codex":{}},"defaultAccounts":{},"newSessionAccounts":{},"setupJobs":jobs})
}
fn field<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() < 512)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required")))
}
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// Settings owns account login from start to finish, including browser/code prompts, registration and returning to the new account. Completion follows helper exit and its saved identity, never a matching success phrase in terminal output.
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let operation = field(params, "operation")?;
    let owner = field(params, "owner")?;
    if !matches!(
        operation,
        "setupStart" | "setupStatus" | "setupInput" | "setupCancel" | "setupAcknowledge"
    ) {
        return Err(DomainStateError::bad_request(
            "Unknown account login operation.",
        ));
    }
    if operation == "setupStart" {
        let provider: Provider =
            serde_json::from_value(params.get("provider").cloned().unwrap_or_default())
                .map_err(store::error)?;
        let email = params
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|email| !email.is_empty());
        if email.is_some_and(|email| {
            email.len() >= 512 || !email.contains('@') || email.chars().any(char::is_whitespace)
        }) {
            return Err(DomainStateError::bad_request("Enter the account email."));
        }
        if params.get("shareHistory").and_then(Value::as_bool) != Some(true) {
            return Err(DomainStateError::bad_request(
                "Allow shared conversations to add an account.",
            ));
        }
        let reconnect = params.get("accountId").and_then(Value::as_str);
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
        let registry = store::read(&db)?;
        let selected = reconnect
            .map(|id| {
                registry
                    .accounts
                    .iter()
                    .find(|a| a.id == id && a.provider == provider)
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "The selected account is no longer registered.",
                        )
                    })
            })
            .transpose()?;
        let repair_selector = params
            .get("selector")
            .and_then(Value::as_str)
            .filter(|slot| slot.parse::<u32>().is_ok_and(|number| number > 0));
        let selector = selected
            .map(|account| account.selector.as_str())
            .or(repair_selector);
        let binary = helpers::executable(
            &state.paths.home_dir,
            if provider == Provider::Claude {
                "ghostex"
            } else {
                "xswap"
            },
        )
        .ok_or_else(|| DomainStateError::bad_request("Install the account login helper first."))?;
        let mut command = CommandBuilder::new(binary);
        if provider == Provider::Claude {
            command.args(["account-login", "claude", "--json"]);
            if let Some(selector) = selector {
                // The CLI pre-fills the browser with the saved account's own email.
                command.args(["--account", selector]);
            } else if let Some(email) = email {
                command.args(["--email", email]);
            }
        } else if let Some(selector) = selector {
            command.args(["login", selector]);
        } else {
            // xswap checks the browser login against the email it was given and prompts for one
            // when it has none, which a Settings login cannot answer.
            let email = email
                .ok_or_else(|| DomainStateError::bad_request("Enter the Codex account's email."))?;
            command.args([
                "add",
                "--login",
                "--share-history",
                "--email",
                email,
                "--json",
            ]);
        }
        command.cwd(&state.paths.home_dir);
        command.env("TERM", "dumb");
        if provider == Provider::Codex {
            let stop_codex = params.get("stopCodex").and_then(Value::as_bool) == Some(true);
            if stop_codex {
                let sessions = params
                    .get("sleepSessions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                codex_blockers::sleep_sessions(state, &sessions)?;
            }
            // The login runs in a terminal, so xswap would otherwise ask there; Settings offers its own button instead.
            command.env(
                "XSWAP_STOP_CODEX",
                if stop_codex { "always" } else { "never" },
            );
        }
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(store::error)?;
        let reader = pair.master.try_clone_reader().map_err(store::error)?;
        let writer = pair.master.take_writer().map_err(store::error)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut child = pair.slave.spawn_command(command).map_err(store::error)?;
        drop(pair.slave);
        let job = Arc::new(Mutex::new(SetupJob {
            view: json!({"id":id,"owner":owner,"provider":provider,"email":email,"status":"signingIn","output":"","accountId":reconnect,"acknowledged":false,"createdAt":chrono::Utc::now().timestamp_millis()}),
            process_id: child.process_id(),
            writer: Some(writer),
            killer: Some(child.clone_killer()),
        }));
        {
            let mut jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
            jobs.retain(|_, job| {
                job.lock().is_ok_and(|job| {
                    job.view["acknowledged"] != true && job.view["status"] != "failed"
                })
            });
            if jobs.len() >= 20 {
                if let Ok(mut job) = job.lock() {
                    let _ = job.cancel();
                }
                let _ = child.wait();
                return Err(DomainStateError::bad_request(
                    "Finish an existing account login first.",
                ));
            }
            jobs.insert(id.clone(), job.clone());
        }
        let state = state.clone();
        let selector = selector.map(str::to_string);
        std::thread::spawn(move || {
            let output_job = job.clone();
            let output = std::thread::spawn(move || read_output(reader, output_job));
            let result = child.wait().map_err(store::error).and_then(|exit| {
                if exit.success() {
                    Ok(())
                } else {
                    Err(DomainStateError::bad_request(
                        "Login did not complete. Check the terminal details and try again.",
                    ))
                }
            });
            if let Ok(mut job) = job.lock() {
                job.writer = None;
                job.killer = None;
                job.process_id = None;
            }
            drop(pair.master);
            let transcript = output.join().unwrap_or_default();
            let wrong_account = result
                .as_ref()
                .err()
                .and_then(|_| wrong_account(&transcript));
            let blockers = (provider == Provider::Codex && result.is_err())
                .then(|| codex_blockers::pids_in_refusal(&transcript))
                .filter(|pids| !pids.is_empty())
                .map(|pids| codex_blockers::describe(&state, &pids));
            let result = result.and_then(|_| {
                let selector = selector.or_else(|| transcript.char_indices().filter(|(_, c)| *c == '{').filter_map(|(index, _)| serde_json::from_str::<Value>(transcript[index..].trim()).ok()).find_map(|value| value.pointer("/account/number").and_then(Value::as_u64).map(|n| n.to_string()))).ok_or_else(|| DomainStateError::bad_request("The helper did not report the saved account. Update the helper and try again."))?;
                if let Ok(mut job) = job.lock() { job.view["status"] = json!("saving"); }
                state.accounts.invalidate();
                let registered = endpoint::dispatch(&state, json!({"operation":"register","provider":provider,"selector":selector,"shareHistory":true}).as_object().unwrap())?;
                registered["accounts"].as_array().and_then(|rows| rows.iter().find(|a| a["selector"] == selector && a["provider"] == json!(provider) && a["registered"] == true)).and_then(|a| a["id"].as_str()).map(str::to_string).ok_or_else(|| DomainStateError::bad_request("The connected account could not be registered."))
            });
            if let Ok(mut job) = job.lock() {
                job.writer = None;
                job.killer = None;
                match result {
                    Ok(id) => {
                        job.view["status"] = json!("complete");
                        job.view["accountId"] = json!(id);
                        job.view["output"] = json!("");
                        job.view["url"] = Value::Null;
                    }
                    Err(error) => {
                        job.view["status"] = json!("failed");
                        if let Some(blockers) = blockers {
                            job.view["error"] = json!("Codex is still running and has to stop before this login can finish.");
                            job.view["blockers"] = blockers;
                        } else if let Some(signed_in) = wrong_account {
                            job.view["error"] =
                                json!("The browser signed in to a different account.");
                            job.view["wrongAccount"] = json!({ "signedIn": signed_in });
                        } else {
                            job.view["error"] = json!(error.message);
                        }
                    }
                }
            }
        });
    } else if matches!(operation, "setupInput" | "setupCancel" | "setupAcknowledge") {
        let id = field(params, "jobId")?;
        let jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
        let mut job = jobs
            .get(id)
            .ok_or_else(|| DomainStateError::not_found("Login job not found."))?
            .lock()
            .map_err(store::error)?;
        if job.view["owner"] != owner {
            return Err(DomainStateError::bad_request(
                "This login belongs to another window.",
            ));
        }
        if operation == "setupInput" {
            let input = field(params, "input")?;
            job.writer
                .as_mut()
                .ok_or_else(|| DomainStateError::bad_request("This login has finished."))?
                .write_all(format!("{}\r", input.trim()).as_bytes())
                .map_err(store::error)?;
        } else if operation == "setupCancel" {
            job.cancel()?;
        } else {
            job.view["acknowledged"] = json!(true);
            job.view["output"] = json!("");
            job.view["url"] = Value::Null;
        }
    }
    let jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
    let mut views: Vec<_> = jobs
        .values()
        .filter_map(|job| {
            job.lock()
                .ok()
                .filter(|job| job.view["owner"] == owner)
                .map(|job| job.view.clone())
        })
        .collect();
    views.sort_by_key(|view| view["createdAt"].as_i64().unwrap_or_default());
    Ok(response(views))
}
/// The login ended on another account than the one asked for: the email the browser signed in to
/// (empty when the helper did not say), from `ghostex account-login`'s "signed in to <email>
/// instead of" or xswap's "signed in to a different email".
fn wrong_account(transcript: &str) -> Option<String> {
    const CLAUDE: &str = "The browser signed in to ";
    if let Some(start) = transcript.find(CLAUDE) {
        let rest = &transcript[start + CLAUDE.len()..];
        let email = rest
            .split_whitespace()
            .next()
            .filter(|word| word.contains('@'))
            .unwrap_or_default();
        return Some(email.to_string());
    }
    let lower = transcript.to_lowercase();
    (lower.contains("signed in to a different email")
        || lower.contains("now signed into another account"))
    .then(String::new)
}
fn read_output(mut reader: Box<dyn Read + Send>, job: Arc<Mutex<SetupJob>>) -> String {
    let mut terminal = LoginTerminal::new();
    let mut bytes = [0; 4096];
    loop {
        let Ok(n) = reader.read(&mut bytes) else {
            break;
        };
        if n == 0 {
            break;
        }
        let replies = terminal.process(&bytes[..n]);
        let output = terminal.text();
        if let Ok(mut job) = job.lock() {
            // A login that already exited or was cancelled has no one left to answer.
            if let Some(writer) = job.writer.as_mut().filter(|_| !replies.is_empty()) {
                let _ = writer.write_all(&replies);
            }
            job.view["output"] = json!(output);
            for word in output.split_whitespace() {
                if let Some(start) = word.find("https://") {
                    let candidate = word[start..].trim_end_matches(['\'', '"', '>']);
                    if let Ok(url) = url::Url::parse(candidate) {
                        if matches!(
                            url.host_str(),
                            Some(
                                "claude.com"
                                    | "claude.ai"
                                    | "platform.claude.com"
                                    | "auth.openai.com"
                                    | "chatgpt.com"
                                    | "auth0.openai.com"
                            )
                        ) {
                            job.view["url"] = json!(url.as_str());
                        }
                    }
                }
            }
        }
    }
    terminal.text()
}
