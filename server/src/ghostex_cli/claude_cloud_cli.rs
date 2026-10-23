//! `ghostex claude-cloud`: Claude cloud sessions (Claude Code on the web) as sidebar sessions. The
//! work happens in gxserver (`/api/claudeCloud`, server/src/claude_cloud/); `attach` is the client
//! a cloud session's own terminal runs.

use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

const HELP: &str =
    "Claude sessions that run in Claude's cloud (Claude Code on the web) and show in the sidebar.

USAGE
  ghostex create-agent claude-cloud --project-id <id> [--prompt <task>]
  ghostex claude-cloud status <session> [--json]
  ghostex claude-cloud send <session> <message>
  ghostex claude-cloud bring-back <session> [--json]
  ghostex claude-cloud copy <session> --account <account id> [--json]

  create-agent claude-cloud starts a cloud session in the project's GitHub repo
  (with --prompt it starts right away; without, its terminal asks for the task).
  The row sits under the repo's main project folder. status prints its link,
  branch and latest status. send posts a follow-up message. bring-back continues
  the session on this computer in a new worktree (claude --teleport) and parks
  the cloud row. copy starts a new cloud session on another Claude account that
  carries on from the session's branch (for a session whose account is out of
  usage; `ghostex accounts` lists the ids). <session> is a title, session id or
  global ref, as in `ghostex sessions`.

  Needs Claude Code signed in with claude.ai (Pro, Max, Team or Enterprise) and
  the repo on GitHub with Claude's GitHub app. Cloud sessions use your Claude
  plan's limits.

EXAMPLES
  ghostex create-agent claude-cloud --project-id P1 --prompt \"Fix the flaky login test\"
  ghostex claude-cloud status \"Fix the flaky login test\"
  ghostex claude-cloud bring-back \"Fix the flaky login test\"
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn resolve(reference: Option<&String>, flags: &Flags, verb: &str) -> CliResult<(Value, Value)> {
    let reference = reference.ok_or_else(|| {
        CliError::Other(format!("claude-cloud {verb} needs a session.\n\n{HELP}"))
    })?;
    let rows = sessions::fetch_session_list(flags, false)?;
    let session = selector::resolve_one_listed_session(reference, &rows, flags)?;
    Ok((session["projectId"].clone(), session["sessionId"].clone()))
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    // A start and a teleport's worktree take a while; a follow-up up to 90 seconds.
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "240000");
    }
    let json_output = flags.contains("json");
    let request = |params: Value| call_gxserver_rpc("/api/claudeCloud", &params, &flags);
    match parsed.rest[0].as_str() {
        "attach" => attach(&flags),
        "status" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "status")?;
            let record = request(json!({
                "action": "get", "projectId": project_id, "sessionId": session_id,
            }))?;
            if json_output {
                print_json(&record);
            } else {
                for line in status_lines(&record) {
                    println!("{line}");
                }
            }
            Ok(())
        }
        "send" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "send")?;
            let message = parsed.rest[2..].join(" ");
            if message.trim().is_empty() {
                return Err(CliError::Other(
                    "claude-cloud send needs a message.".to_string(),
                ));
            }
            request(json!({
                "action": "send", "projectId": project_id, "sessionId": session_id, "text": message,
            }))?;
            println!("Sent.");
            Ok(())
        }
        "bring-back" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "bring-back")?;
            let result = request(json!({
                "action": "bringBack", "projectId": project_id, "sessionId": session_id,
            }))?;
            if json_output {
                print_json(&result);
            } else {
                println!(
                    "Continues on this computer in {} (session {}).",
                    text(&result, "worktreePath"),
                    text(&result, "sessionId")
                );
            }
            Ok(())
        }
        "copy" => {
            let (project_id, session_id) = resolve(parsed.rest.get(1), &flags, "copy")?;
            let account = flags.string_value("account").ok_or_else(|| {
                CliError::Other("claude-cloud copy needs --account <account id>.".to_string())
            })?;
            let result = request(json!({
                "action": "copy", "projectId": project_id, "sessionId": session_id,
                "accountId": account,
            }))?;
            if json_output {
                print_json(&result);
            } else {
                println!(
                    "Started a copy (session {}); it starts in Claude's cloud in the background.",
                    text(&result, "sessionId")
                );
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown claude-cloud command \"{other}\".\n\n{HELP}"
        ))),
    }
}

/// What `status` and the client's screen say about a cloud session.
fn status_lines(record: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    match text(record, "state") {
        "draft" => lines.push("Claude cloud · waiting for its task".to_string()),
        "starting" => lines.push("Starting in Claude's cloud…".to_string()),
        "failed" => lines.push(format!(
            "Could not start in Claude's cloud: {}",
            text(record, "error")
        )),
        "moved" => lines.push("Moved to this computer.".to_string()),
        _ => lines.push("Running in Claude's cloud".to_string()),
    }
    for (label, key) in [
        ("Link", "url"),
        ("Branch", "branch"),
        ("Status", "statusText"),
        ("Pull request", "prUrl"),
        ("Account", "accountName"),
    ] {
        let value = text(record, key);
        if !value.is_empty() {
            lines.push(format!("{label:<13}{value}"));
        }
    }
    lines
}

/// The session this terminal belongs to, from the variables every Ghostex session exports.
fn own_session() -> CliResult<(String, String)> {
    let read = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    if let Some((project_id, session_id)) = read("GHOSTEX_GLOBAL_SESSION_REF")
        .map(|global| crate::agent_hooks::probing::parse_global_session_ref(&global))
        .and_then(|(project, session)| Some((project?, session?)))
    {
        return Ok((project_id, session_id));
    }
    read("GHOSTEX_NATIVE_SESSION_ID")
        .and_then(|value| {
            let (project, session) = value.split_once(':')?;
            Some((project.trim().to_string(), session.trim().to_string()))
        })
        .filter(|(project, session)| !project.is_empty() && !session.is_empty())
        .ok_or_else(|| {
            CliError::Other(
                "claude-cloud attach runs inside a Claude cloud session's own terminal."
                    .to_string(),
            )
        })
}

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
    let _ = std::io::stdout().flush();
}

/// One line typed by the user; `None` when the terminal has no input left.
fn read_line(prompt: &str) -> Option<String> {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

/// Runs `claude --cloud <link>` in this terminal on the session's account. Returns whether it
/// stayed attached (Claude allows attaching for the account) rather than refusing at once.
///
/// CDXC:ClaudeCloud 2026-10-23 WHY: Claude Code 2.1.301 answers "Attaching to an existing cloud session is not enabled for your account." and exits at once (verified 2026-10-23); the help still lists the flag, so it is a per-account switch. The client tries once per start of the terminal: a refusal shows the status screen and the row keeps opening the claude.ai page, and an attach that stays up turns the row into an ordinary terminal session from then on.
fn try_attach(record: &Value, report: &dyn Fn(bool)) -> bool {
    let Some(argv) = record["claudeArgv"].as_array() else {
        return false;
    };
    let argv: Vec<String> = argv
        .iter()
        .filter_map(|part| part.as_str().map(str::to_string))
        .collect();
    let Some((program, wrapper)) = argv.split_first() else {
        return false;
    };
    let url = text(record, "url");
    if url.is_empty() {
        return false;
    }
    let started = Instant::now();
    let Ok(mut child) = std::process::Command::new(program)
        .args(wrapper)
        .args(["--cloud", url])
        .spawn()
    else {
        return false;
    };
    let mut reported = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {}
        }
        if !reported && started.elapsed() > Duration::from_secs(8) {
            report(true);
            reported = true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if !reported {
        report(false);
    }
    reported
}

/// The client a Claude cloud session's terminal runs: asks for the task, starts the session,
/// attaches when Claude allows it, and otherwise shows its status and sends follow-ups.
fn attach(flags: &Flags) -> CliResult<()> {
    let (project_id, session_id) = own_session()?;
    let call = |params: Value| -> CliResult<Value> {
        let mut params = params;
        params["projectId"] = json!(project_id);
        params["sessionId"] = json!(session_id);
        call_gxserver_rpc("/api/claudeCloud", &params, flags)
    };
    let mut attach_tried = false;
    loop {
        let record = match call(json!({ "action": "get" })) {
            Ok(record) => record,
            Err(error) => {
                println!("Ghostex is not answering ({error}). Trying again…");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        clear_screen();
        for line in status_lines(&record) {
            println!("{line}");
        }
        println!();
        match text(&record, "state") {
            "draft" | "failed" => {
                let previous = record["task"].as_str().map(str::to_string);
                let task = if text(&record, "state") == "draft" && previous.is_some() {
                    previous
                } else {
                    let hint = if previous.is_some() {
                        "Write the task again, or press Enter to retry the last one."
                    } else {
                        "What should Claude do in the cloud? Write the task and press Enter."
                    };
                    println!("{hint}");
                    match read_line("> ") {
                        None => {
                            std::thread::sleep(Duration::from_secs(5));
                            continue;
                        }
                        Some(line) if line.is_empty() => previous,
                        Some(line) => Some(line),
                    }
                };
                let Some(task) = task else {
                    continue;
                };
                println!("Starting in Claude's cloud… this takes up to a few minutes.");
                if let Err(error) = call(json!({ "action": "start", "task": task })) {
                    println!("{error}");
                    std::thread::sleep(Duration::from_secs(3));
                }
            }
            "starting" => std::thread::sleep(Duration::from_secs(3)),
            "running" => {
                if !attach_tried {
                    attach_tried = true;
                    let report = |live: bool| {
                        let _ = call(json!({ "action": "attach", "live": live }));
                    };
                    if try_attach(&record, &report) {
                        continue;
                    }
                    clear_screen();
                    for line in status_lines(&record) {
                        println!("{line}");
                    }
                    println!();
                }
                println!(
                    "Claude's replies show on the session's claude.ai page (click this session in the sidebar)."
                );
                println!("Type a follow-up and press Enter to send it; an empty line refreshes.");
                match read_line("> ") {
                    None => std::thread::sleep(Duration::from_secs(30)),
                    Some(line) if line.is_empty() => {}
                    Some(line) => match call(json!({ "action": "send", "text": line })) {
                        Ok(_) => {
                            println!("Sent.");
                            std::thread::sleep(Duration::from_secs(1));
                        }
                        Err(error) => {
                            println!("{error}");
                            std::thread::sleep(Duration::from_secs(3));
                        }
                    },
                }
            }
            _ => {
                println!(
                    "This session continues on this computer; see its new row in the sidebar."
                );
                if read_line("").is_none() {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
        }
    }
}
