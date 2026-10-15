//! `ghostex connectors`: a workspace's connectors (crate::connectors): its Composio sign-in, the
//! apps connected there and the tools its agents get. A workspace is named by id or name; without
//! `--workspace` it is the default one (Personal).

use std::process::Stdio;

use serde_json::{json, Map, Value};

use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};
use super::web::browser_open_command;

/// Whether Connectors is on for this machine (crate::connectors::connectors_enabled_in), read from
/// its settings file like the built-in extension switches.
pub(super) fn connectors_enabled() -> bool {
    let settings = super::settings::read_settings_file().unwrap_or_default();
    crate::connectors::connectors_enabled_in(Some(&Value::Object(settings)))
}

/// `Ok` while Connectors is on; otherwise how to turn it on.
pub(super) fn require_connectors() -> CliResult<()> {
    if connectors_enabled() {
        return Ok(());
    }
    Err(CliError::Other(
        crate::connectors::CONNECTORS_OFF_MESSAGE.to_string(),
    ))
}

/// `ghostex connectors [status] | sign-in [--no-open] | sign-out | refresh | connect <app>
/// [--no-open] | tools | call <tool> [--args <json>] | executor …`, each with
/// `[--workspace name|id]`.
pub(super) fn connectors_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("status");
    let rest = parsed.rest.get(1..).unwrap_or_default();
    let mut params = Map::new();
    if let Some(workspace) = flags.string_value("workspace") {
        params.insert("workspaceId".to_string(), json!(workspace));
    }
    let open = !flags.contains("no-open");
    match subcommand {
        "status" | "list" | "ls" => call_and_print("/api/readConnectors", params, flags),
        "sign-in" | "signin" | "login" => {
            let result = rpc::call_gxserver_rpc("/api/signInComposio", &Value::Object(params), flags)?;
            match result.get("signInUrl").and_then(Value::as_str) {
                Some(url) => {
                    println!("Sign in to Composio in your browser:\n{url}");
                    if open {
                        open_in_browser(url)?;
                    }
                    println!("Then check with: ghostex connectors status");
                }
                None => println!("Already signed in to Composio."),
            }
            Ok(())
        }
        "sign-out" | "signout" | "logout" => call_and_print("/api/signOutComposio", params, flags),
        "refresh" => call_and_print("/api/refreshComposio", params, &slow(flags)),
        "connect" => {
            let app = rest.join(" ");
            let app = app.trim();
            if app.is_empty() {
                return Err(CliError::Other(
                    "Name the app: ghostex connectors connect <app>, like gmail or slack.".to_string(),
                ));
            }
            params.insert("app".to_string(), json!(app));
            let result =
                rpc::call_gxserver_rpc("/api/connectComposioApp", &Value::Object(params), &slow(flags))?;
            let name = result.get("name").and_then(Value::as_str).unwrap_or(app);
            if result.get("alreadyConnected").and_then(Value::as_bool) == Some(true) {
                println!("{name} is already connected.");
            } else if let Some(url) = result.get("url").and_then(Value::as_str) {
                println!("Connect {name} in your browser:\n{url}");
                if open {
                    open_in_browser(url)?;
                }
            }
            Ok(())
        }
        "tools" => {
            if flags.contains("full") {
                params.insert("full".to_string(), json!(true));
            }
            call_and_print("/api/listComposioTools", params, &slow(flags))
        }
        "call" => {
            let Some(tool) = rest.first().map(|tool| tool.trim().to_uppercase()) else {
                return Err(CliError::Other(
                    "Name the tool: ghostex connectors call <tool> [--args '<json>'], like GMAIL_FETCH_EMAILS.".to_string(),
                ));
            };
            let arguments = match flags.string_value("args") {
                Some(text) => serde_json::from_str::<Value>(text).map_err(|error| {
                    CliError::Other(format!("--args is not valid JSON: {error}"))
                })?,
                None => json!({}),
            };
            // An app tool (GMAIL_FETCH_EMAILS) runs through Composio's own runner tool.
            let (tool, arguments) = if tool.starts_with("COMPOSIO_") {
                (tool, arguments)
            } else {
                (
                    "COMPOSIO_MULTI_EXECUTE_TOOL".to_string(),
                    json!({
                        "tools": [{ "tool_slug": tool, "arguments": arguments }],
                        "sync_response_to_workbench": false,
                    }),
                )
            };
            params.insert("tool".to_string(), json!(tool));
            params.insert("arguments".to_string(), arguments);
            let result =
                rpc::call_gxserver_rpc("/api/callComposioTool", &Value::Object(params), &slow(flags))?;
            println!(
                "{}",
                result.get("output").and_then(Value::as_str).unwrap_or_default()
            );
            Ok(())
        }
        "executor" => super::connectors_executor::executor_command(rest, params, flags, open_in_browser),
        other => Err(CliError::Other(format!(
            "Unknown connectors command \"{other}\". Use status, sign-in, sign-out, refresh, connect, tools, call or executor."
        ))),
    }
}

fn open_in_browser(url: &str) -> CliResult<()> {
    browser_open_command()?
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| CliError::Other(format!("Couldn't open the browser: {error}")))
}

/// A longer RPC timeout than the CLI's 15 seconds for commands that wait on Composio or Executor:
/// a tool call may run for minutes, Executor may have to start first, and adding an app builds it.
pub(super) fn slow(flags: &Flags) -> Flags {
    let mut flags = flags.clone();
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "300000");
    }
    flags
}

fn call_and_print(path: &str, params: Map<String, Value>, flags: &Flags) -> CliResult<()> {
    let result = rpc::call_gxserver_rpc(path, &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}
