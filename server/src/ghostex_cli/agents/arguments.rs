use crate::ghostex_cli::{
    args::Flags,
    rpc::{CliError, CliResult},
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Delivery {
    Normal,
    Interrupt,
    Queue,
}

pub(super) struct Arguments {
    pub command: String,
    pub positional: Vec<String>,
    pub body_file: Option<String>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub project_id: Option<String>,
    pub delivery: Delivery,
    /// `--request-id`: the send's id, reused by a caller that retries it.
    pub request_id: Option<String>,
    pub all: bool,
    /// `--allow-hidden`: create an agent that is turned off in Settings > Agents.
    pub allow_hidden: bool,
    pub json: bool,
    pub help: bool,
    pub flags: Flags,
}

pub(super) fn parse(args: &[String]) -> CliResult<Arguments> {
    let mut parsed = Arguments {
        command: String::new(),
        positional: Vec::new(),
        body_file: None,
        task: None,
        title: None,
        project_id: None,
        delivery: Delivery::Normal,
        request_id: None,
        all: false,
        allow_hidden: false,
        json: false,
        help: args.is_empty(),
        flags: Flags::default(),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                parsed.positional.extend(args.cloned());
                break;
            }
            "-h" | "--help" => parsed.help = true,
            "--json" => parsed.json = true,
            "--all" => parsed.all = true,
            "--allow-hidden" => parsed.allow_hidden = true,
            "--interrupt" | "--queue" => {
                if parsed.delivery != Delivery::Normal {
                    return Err(CliError::Other(
                        "Choose only one delivery flag: --interrupt or --queue.".into(),
                    ));
                }
                parsed.delivery = if arg == "--interrupt" {
                    Delivery::Interrupt
                } else {
                    Delivery::Queue
                };
            }
            "--body-file" | "--server" | "--task" | "--title" | "--project-id" | "--request-id" => {
                let value = args
                    .next()
                    .filter(|value| !value.starts_with("--") && !value.is_empty())
                    .ok_or_else(|| CliError::Other(format!("{arg} requires a value.")))?;
                let slot = match arg.as_str() {
                    "--body-file" => Some(&mut parsed.body_file),
                    "--task" => Some(&mut parsed.task),
                    "--title" => Some(&mut parsed.title),
                    "--project-id" => Some(&mut parsed.project_id),
                    "--request-id" => Some(&mut parsed.request_id),
                    _ => None,
                };
                if let Some(slot) = slot {
                    if slot.replace(value.clone()).is_some() {
                        return Err(CliError::Other(format!("Provide {arg} only once.")));
                    }
                } else {
                    parsed.flags.insert_text("server", value);
                }
            }
            value if value.starts_with('-') => {
                return Err(CliError::Other(format!(
                    "Unknown option: {value}. See ghostex agents --help."
                )))
            }
            value if parsed.command.is_empty() => parsed.command = value.to_owned(),
            value => parsed.positional.push(value.to_owned()),
        }
    }
    if parsed.help {
        return Ok(parsed);
    }
    if parsed.request_id.is_some() && parsed.command != "send" {
        return Err(CliError::Other(
            "--request-id is only available for send.".into(),
        ));
    }
    if parsed
        .request_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty() || id.chars().count() > 128)
    {
        return Err(CliError::Other(
            "--request-id must be 1 to 128 characters.".into(),
        ));
    }
    if parsed.command != "create"
        && (parsed.task.is_some()
            || parsed.title.is_some()
            || parsed.project_id.is_some()
            || parsed.allow_hidden)
    {
        return Err(CliError::Other(
            "--task, --title, --project-id, and --allow-hidden are only available for create."
                .into(),
        ));
    }
    match parsed.command.as_str() {
        "whoami" | "list" | "types" => {
            if !parsed.positional.is_empty()
                || parsed.body_file.is_some()
                || parsed.delivery != Delivery::Normal
            {
                return Err(CliError::Other(format!(
                    "{} does not accept message arguments.",
                    parsed.command
                )));
            }
            if parsed.command == "whoami" && (parsed.all || parsed.flags.contains("server")) {
                return Err(CliError::Other(
                    "whoami identifies the caller; --all and --server are not accepted.".into(),
                ));
            }
            if parsed.command == "list" && parsed.flags.contains("server") && !parsed.all {
                return Err(CliError::Other(
                    "Use list --all --server <profile> to list another server.".into(),
                ));
            }
            if parsed.command == "types" && parsed.all {
                return Err(CliError::Other("--all is only available for list.".into()));
            }
        }
        "send" => {
            if parsed.all {
                return Err(CliError::Other("--all is only available for list.".into()));
            }
            let count = if parsed.body_file.is_some() { 1 } else { 2 };
            if parsed.positional.len() != count {
                return Err(CliError::Other("Usage: ghostex agents send <session-ref> <text> OR <session-ref> --body-file <path>. Quote inline text as one argument.".into()));
            }
        }
        "create" | "close" => {
            if parsed.positional.len() != 1 || parsed.all || parsed.delivery != Delivery::Normal {
                return Err(CliError::Other(format!(
                    "Usage: ghostex agents {} <{}>. See ghostex agents --help.",
                    parsed.command,
                    if parsed.command == "create" {
                        "agent-id"
                    } else {
                        "session-ref"
                    }
                )));
            }
            if parsed.command == "close" && parsed.body_file.is_some() {
                return Err(CliError::Other("close does not accept a message.".into()));
            }
            if parsed.command == "create" && parsed.body_file.is_some() && parsed.task.is_some() {
                return Err(CliError::Other(
                    "Choose --task or --body-file, not both.".into(),
                ));
            }
            if parsed.command == "create"
                && parsed.flags.contains("server")
                && parsed.project_id.is_none()
            {
                return Err(CliError::Other(
                    "Creating on --server requires --project-id.".into(),
                ));
            }
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown agents command: {other}. See ghostex agents --help."
            )))
        }
    }
    Ok(parsed)
}
