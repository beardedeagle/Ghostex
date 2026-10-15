use base64::Engine as _;
use serde_json::{json, Value};

use crate::ghostex_cli::args::{multi_value_flag, parse_args};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/// How long a send may take: five 5 MB screenshots on a slow line, as the pop-up allows
/// (gxserver gives the relay 170 seconds).
const SEND_TIMEOUT_MS: &str = "180000";

/// `ghostex feedback send` files a Ghostex bug or feedback issue through gxserver's feedback relay,
/// the same post the Send Bug / Feedback pop-up makes. The feedback agent that pop-up starts posts
/// with it (server/src/feedback/agent.rs writes its instructions), so the issue says
/// `collectedByAgent: true`; `--dry-run` prints the exact request instead of posting.
pub fn feedback_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("send") => send(&args[1..]),
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", crate::ghostex_cli::usage::feedback_usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown feedback command: {other}\n\n{}",
            crate::ghostex_cli::usage::feedback_usage()
        ))),
    }
}

fn send(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", crate::ghostex_cli::usage::feedback_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let usage_error = |message: &str| {
        CliError::Other(format!(
            "{message}\n\n{}",
            crate::ghostex_cli::usage::feedback_usage()
        ))
    };
    let title = flags.text("title").unwrap_or_default();
    if title.trim().is_empty() {
        return Err(usage_error("feedback send requires --title."));
    }
    let body = match (flags.text("bodyFile"), flags.text("body")) {
        (Some(path), _) => std::fs::read_to_string(&path)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?,
        (None, Some(body)) => body,
        (None, None) => return Err(usage_error("feedback send requires --body-file or --body.")),
    };
    let github_user = flags
        .text("githubUser")
        .or_else(|| flags.text("githubUsername"))
        .unwrap_or_default();
    if github_user.trim().is_empty() {
        return Err(usage_error("feedback send requires --github-user."));
    }
    let images = multi_value_flag(args, &["image"])
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path)
                .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
            let name = std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(json!({
                "name": name,
                "dataBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }))
        })
        .collect::<CliResult<Vec<Value>>>()?;
    let payload = json!({
        "app": "desktop",
        "title": title,
        "body": body,
        "githubUsername": github_user,
        "images": images,
        "collectedByAgent": true,
        "dryRun": flags.truthy("dryRun"),
    });
    let mut rpc_flags = flags.clone();
    if rpc_flags.text("timeout").is_none() {
        rpc_flags.insert_text("timeout", SEND_TIMEOUT_MS);
    }
    let result = call_gxserver_rpc("/api/sendFeedback", &payload, &rpc_flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    if flags.truthy("json") || flags.truthy("dryRun") {
        print_json(&result);
    } else {
        let number = result["issueNumber"].as_u64().unwrap_or_default();
        let url = result["issueUrl"].as_str().unwrap_or_default();
        println!("Filed issue #{number}: {url}");
    }
    Ok(())
}
