//! What the Add Project dialog makes of a pasted or typed line: the Rust twin of
//! packages/core-ui/add-project-modal/add-project-input.ts (deleted 2026-10-01) (`classifyAddProjectInput`,
//! `normalizePastedProjectPath`, `parseAddProjectCloneInput`). URLs go through the `url` crate,
//! which implements the same WHATWG parser as the browser's `URL`.
use super::model::{AddProjectMachineOption, AddProjectSourceId};
use super::paths::is_filesystem_browse_query;

/// `DetectedCloneInput`: a clone the form can express, with the options it carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetectedCloneInput {
    pub(crate) query: String,
    pub(crate) source: AddProjectSourceId,
    pub(crate) remote_url: String,
    pub(crate) branch_name: String,
    pub(crate) destination: String,
    pub(crate) clone_main_only: bool,
    pub(crate) shallow_clone: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetectedProjectInput {
    Browse {
        query: String,
        machine_id: Option<String>,
    },
    Ambiguous {
        query: String,
        clone: DetectedCloneInput,
    },
    Error {
        message: String,
    },
    Clone(DetectedCloneInput),
}

/// A `parseAddProjectCloneInput` answer: a clone, or a message about the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParsedCloneInput {
    Clone(DetectedCloneInput),
    Error(String),
}

/// `[\w.-]` with the ASCII `\w` of a `u`-flag JavaScript regex.
fn is_repository_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
}

/// `/^[\w.-]+\/[\w.-]+\/?$/u` (two segments) or, for GitLab, `/^[\w.-]+(?:\/[\w.-]+)+\/?$/u`.
fn is_repository_shorthand(value: &str, allow_subgroups: bool) -> bool {
    let body = value.strip_suffix('/').unwrap_or(value);
    let segments: Vec<&str> = body.split('/').collect();
    let count_ok = if allow_subgroups {
        segments.len() >= 2
    } else {
        segments.len() == 2
    };
    count_ok
        && segments
            .iter()
            .all(|segment| !segment.is_empty() && segment.chars().all(is_repository_word))
}

/// CDXC:AddProject 2026-09-11 DECISION:
/// User: use the local machine for pasted paths unless a machine was selected; detect cd commands, escaped spaces, provider URLs, clone options, and saved-machine paths; offer a choice when a folder and repository shorthand both make sense.
/// SEE-ALSO: packages/core-ui/add-project-modal/add-project-input.ts (deleted 2026-10-01) (the React twin).
pub(crate) fn classify_add_project_input(
    input: &str,
    machines: &[AddProjectMachineOption],
) -> Option<DetectedProjectInput> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();
    let matches: Vec<&AddProjectMachineOption> = machines
        .iter()
        .filter(|machine| {
            [&machine.machine_id, &machine.label]
                .iter()
                .any(|name| lower.starts_with(&format!("{}:", name.to_lowercase())))
        })
        .collect();
    if matches.len() > 1 {
        return Some(DetectedProjectInput::Error {
            message: "More than one machine has that name. Select the machine first.".to_string(),
        });
    }
    if let Some(machine) = matches.first() {
        let rest = trimmed
            .find(':')
            .map(|index| &trimmed[index + 1..])
            .unwrap_or("");
        return normalize_pasted_project_path(rest).map(|query| DetectedProjectInput::Browse {
            query,
            machine_id: Some(machine.machine_id.clone()),
        });
    }
    if let Some(path) = normalize_pasted_project_path(trimmed) {
        return Some(DetectedProjectInput::Browse {
            query: path,
            machine_id: None,
        });
    }
    let clone = match parse_add_project_clone_input(trimmed, AddProjectSourceId::Github)? {
        ParsedCloneInput::Error(message) => return Some(DetectedProjectInput::Error { message }),
        ParsedCloneInput::Clone(clone) => clone,
    };
    let first_segment = trimmed.split('/').next().unwrap_or("");
    if is_repository_shorthand(trimmed, false) && !first_segment.contains('.') {
        return Some(DetectedProjectInput::Ambiguous {
            query: trimmed.to_string(),
            clone,
        });
    }
    Some(DetectedProjectInput::Clone(clone))
}

/// `/^cd\s+(?:--\s+)?([\s\S]+)$/u`.
fn cd_argument(value: &str) -> Option<&str> {
    let rest = value.strip_prefix("cd")?;
    let after_space = rest.trim_start();
    if after_space.len() == rest.len() {
        return None;
    }
    if after_space.is_empty() {
        // `\s+` gives its last character back to `[\s\S]+`.
        let last = rest.char_indices().last()?.0;
        return Some(&rest[last..]);
    }
    // `(?:--\s+)?` only matches when an argument follows it (the value arrives trimmed).
    if let Some(after_dashes) = after_space.strip_prefix("--") {
        let argument = after_dashes.trim_start();
        if argument.len() != after_dashes.len() && !argument.is_empty() {
            return Some(argument);
        }
    }
    Some(after_space)
}

/// `/^(["'`])[\s\S]*\1$/u`: the value is wrapped in one kind of quote.
fn is_quoted(value: &str) -> bool {
    let mut characters = value.chars();
    match (characters.next(), characters.next_back()) {
        (Some(first), Some(last)) => matches!(first, '"' | '\'' | '`') && first == last,
        _ => false,
    }
}

fn strip_quotes(value: &str) -> &str {
    let first = value.chars().next().map(char::len_utf8).unwrap_or(0);
    let last = value.chars().next_back().map(char::len_utf8).unwrap_or(0);
    &value[first..value.len() - last]
}

/// `/^(?:[a-z]:[\\/]|\\\\)/iu`.
fn starts_with_windows_root(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'))
}

/// `value.replace(/\\([ \t()'"\[\]&#;$`])/gu, '$1')`: shell-escaped characters lose their backslash.
fn unescape_shell_characters(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(&next) = characters.peek()
            && matches!(
                next,
                ' ' | '\t' | '(' | ')' | '\'' | '"' | '[' | ']' | '&' | '#' | ';' | '$' | '`'
            )
        {
            out.push(next);
            characters.next();
            continue;
        }
        out.push(character);
    }
    out
}

/// `decodeURIComponent`, which throws on a malformed escape.
fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let valid = bytes.len() >= index + 3
                && bytes[index + 1].is_ascii_hexdigit()
                && bytes[index + 2].is_ascii_hexdigit();
            if !valid {
                return None;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|decoded| decoded.into_owned())
}

/// `normalizePastedProjectPath`: a pasted path, `cd` command, quoted or shell-escaped path, or
/// `file://` URL as a browse query, or `None` when it is not a path.
pub(crate) fn normalize_pasted_project_path(input: &str) -> Option<String> {
    let mut value = input.trim().to_string();
    let cd = cd_argument(&value).map(str::to_string);
    if let Some(argument) = &cd {
        value = argument.clone();
    }
    if is_quoted(&value) {
        value = strip_quotes(&value).to_string();
    } else if !starts_with_windows_root(&value) {
        value = unescape_shell_characters(&value);
    }
    if value
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"))
    {
        let url = url::Url::parse(&value).ok()?;
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return None;
        }
        let decoded = decode_uri_component(url.path())?;
        let bytes = decoded.as_bytes();
        value = if bytes.len() >= 4
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
            && bytes[3] == b'/'
        {
            decoded[1..].to_string()
        } else {
            decoded
        };
    }
    if value == "~" {
        value = "~/".to_string();
    }
    if value == "." || value == ".." {
        value.push('/');
    }
    if cd.is_some() && !is_filesystem_browse_query(&value, "Win32") {
        value = format!("./{value}");
    }
    is_filesystem_browse_query(&value, "Win32").then_some(value)
}

/// `commandWords`: shell-style words, or `None` for an unterminated quote.
fn command_words(input: &str) -> Option<Vec<String>> {
    let characters: Vec<char> = input.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '\\'
            && quote != Some('\'')
            && index + 1 < characters.len()
            && (characters[index + 1].is_whitespace()
                || matches!(characters[index + 1], '\\' | '"' | '\''))
        {
            index += 1;
            word.push(characters[index]);
        } else if let Some(open) = quote {
            if character == open {
                quote = None;
            } else {
                word.push(character);
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(character);
        }
        index += 1;
    }
    if quote.is_some() {
        return None;
    }
    if !word.is_empty() {
        words.push(word);
    }
    Some(words)
}

/// `/^([^@\s]+)@([^:/\s]+):(.+)$/u`: an scp-style `user@host:path` remote.
fn scp_remote(token: &str) -> Option<(&str, &str, &str)> {
    let at = token.find('@')?;
    let user = &token[..at];
    if user.is_empty() || user.chars().any(char::is_whitespace) {
        return None;
    }
    let rest = &token[at + 1..];
    let colon = rest.find(':')?;
    let host = &rest[..colon];
    if host.is_empty()
        || host
            .chars()
            .any(|character| character == '/' || character.is_whitespace())
    {
        return None;
    }
    let path = &rest[colon + 1..];
    if path.is_empty() || path.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    Some((user, host, path))
}

fn decode_part(value: &str) -> String {
    decode_uri_component(value).unwrap_or_else(|| value.to_string())
}

/// `parseAddProjectCloneInput`: a repository, URL, or `git clone` / `gh repo clone` /
/// `glab repo clone` command, with the options the review form can express.
pub(crate) fn parse_add_project_clone_input(
    input: &str,
    default_source: AddProjectSourceId,
) -> Option<ParsedCloneInput> {
    let trimmed = input.trim();
    let words = command_words(trimmed)?;
    if words.is_empty() {
        return None;
    }
    let word = |index: usize| words.get(index).map(String::as_str);
    let command_length = if word(0) == Some("git") && word(1) == Some("clone") {
        2
    } else if matches!(word(0), Some("gh" | "glab"))
        && word(1) == Some("repo")
        && word(2) == Some("clone")
    {
        3
    } else {
        0
    };
    let mut branch_name = String::new();
    let mut destination = String::new();
    let mut clone_main_only = false;
    let mut shallow_clone = false;
    let mut positional: Vec<String> = Vec::new();
    if command_length > 0 {
        let mut options = true;
        let mut index = command_length;
        while index < words.len() {
            let current = words[index].as_str();
            if options && current == "--" {
                options = command_length == 3;
                index += 1;
                continue;
            }
            if options && (current == "-b" || current == "--branch") {
                index += 1;
                branch_name = words.get(index).cloned().unwrap_or_default();
                if branch_name.is_empty() {
                    return None;
                }
            } else if options
                && (current.starts_with("--branch=")
                    || (current.starts_with("-b") && current.len() > 2))
            {
                branch_name = if let Some(value) = current.strip_prefix("--branch=") {
                    value.to_string()
                } else {
                    current[2..].to_string()
                };
            } else if options && current == "--single-branch" {
                clone_main_only = true;
            } else if options && (current == "--depth" || current.starts_with("--depth=")) {
                let depth = if current == "--depth" {
                    index += 1;
                    words.get(index).map(String::as_str)
                } else {
                    Some(&current[8..])
                };
                if depth != Some("1") {
                    return Some(ParsedCloneInput::Error(
                        "The clone form supports --depth 1. Remove the depth option or use 1."
                            .to_string(),
                    ));
                }
                shallow_clone = true;
            } else if options && current.starts_with('-') {
                return Some(ParsedCloneInput::Error(format!(
                    "The clone form does not support {current}. Remove it to continue."
                )));
            } else {
                positional.push(current.to_string());
            }
            index += 1;
        }
        if positional.len() > 2 {
            return Some(ParsedCloneInput::Error(
                "Enter one repository and an optional destination folder.".to_string(),
            ));
        }
        destination = positional.get(1).cloned().unwrap_or_default();
    } else {
        positional.push(if is_quoted(trimmed) {
            strip_quotes(trimmed).to_string()
        } else {
            trimmed.to_string()
        });
    }
    let mut token = positional.first().cloned().unwrap_or_default();
    if token.is_empty() {
        return None;
    }
    let is_gitlab_shorthand = word(0) == Some("glab")
        || (command_length == 0 && default_source == AddProjectSourceId::Gitlab);
    let first_segment = token.split('/').next().unwrap_or("").to_string();
    if is_repository_shorthand(&token, is_gitlab_shorthand) && !first_segment.contains('.') {
        let host = if is_gitlab_shorthand {
            "gitlab.com"
        } else if command_length == 0 && default_source == AddProjectSourceId::Bitbucket {
            "bitbucket.org"
        } else {
            "github.com"
        };
        token = format!("https://{host}/{token}");
    }
    let scp = scp_remote(&token)
        .map(|(user, host, path)| (user.to_string(), host.to_string(), path.to_string()));
    let lower_token = token.to_ascii_lowercase();
    let candidate = match &scp {
        Some((user, host, path)) => format!("ssh://{user}@{host}/{path}"),
        None if lower_token.starts_with("http://")
            || lower_token.starts_with("https://")
            || lower_token.starts_with("ssh://") =>
        {
            token.clone()
        }
        None => format!("https://{token}"),
    };
    let mut url = url::Url::parse(&candidate).ok()?;
    let host = url.host_str().unwrap_or("").to_string();
    if !matches!(url.scheme(), "https" | "http" | "ssh") || !host.contains('.') {
        return None;
    }
    let host = host.to_lowercase();
    let source = if host == "github.com" {
        AddProjectSourceId::Github
    } else if host == "gitlab.com" {
        AddProjectSourceId::Gitlab
    } else if host == "bitbucket.org" {
        AddProjectSourceId::Bitbucket
    } else if host == "dev.azure.com"
        || host == "ssh.dev.azure.com"
        || host.ends_with(".visualstudio.com")
    {
        AddProjectSourceId::AzureDevops
    } else {
        AddProjectSourceId::Url
    };
    let parts: Vec<String> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    let mut repository_parts: Vec<String> = parts.clone();
    match source {
        AddProjectSourceId::Github | AddProjectSourceId::Bitbucket => {
            if parts.len() < 2 {
                return None;
            }
            repository_parts = parts[..2].to_vec();
            // A single tree segment (including an encoded slash) unambiguously names the ref.
            if branch_name.is_empty()
                && parts.get(2).map(String::as_str) == Some("tree")
                && parts.len() == 4
            {
                branch_name = decode_part(&parts[3]);
            }
        }
        AddProjectSourceId::Gitlab => {
            if let Some(separator) = parts.iter().position(|part| part == "-") {
                repository_parts = parts[..separator].to_vec();
                if branch_name.is_empty()
                    && parts.get(separator + 1).map(String::as_str) == Some("tree")
                    && parts.len() == separator + 3
                {
                    branch_name = decode_part(&parts[separator + 2]);
                }
            }
            if repository_parts.len() < 2 {
                return None;
            }
        }
        AddProjectSourceId::AzureDevops => {
            if let Some(git) = parts.iter().position(|part| part == "_git") {
                repository_parts = parts[..(git + 2).min(parts.len())].to_vec();
            } else if host == "ssh.dev.azure.com"
                && parts.first().map(String::as_str) == Some("v3")
                && parts.len() == 4
            {
                repository_parts = parts.clone();
            } else {
                return None;
            }
            match repository_parts.last().map(String::as_str) {
                None | Some("") | Some("_git") => return None,
                _ => {}
            }
            let version = url
                .query_pairs()
                .find(|(key, _)| key == "version")
                .map(|(_, value)| value.into_owned());
            if branch_name.is_empty()
                && let Some(version) = version.filter(|version| version.starts_with("GB"))
            {
                branch_name = version[2..].to_string();
            }
        }
        AddProjectSourceId::Url => {
            if parts.is_empty() {
                return None;
            }
        }
    }
    if source != AddProjectSourceId::AzureDevops
        && let Some(last) = repository_parts.last_mut()
        && !last.to_lowercase().ends_with(".git")
    {
        last.push_str(".git");
    }
    let joined = repository_parts.join("/");
    url.set_path(&format!("/{joined}"));
    url.set_query(None);
    url.set_fragment(None);
    let remote_url = match &scp {
        Some((user, host, _)) => format!("{user}@{host}:{joined}"),
        None => url.to_string(),
    };
    Some(ParsedCloneInput::Clone(DetectedCloneInput {
        query: trimmed.to_string(),
        source,
        remote_url,
        branch_name,
        destination,
        clone_main_only,
        shallow_clone,
    }))
}
