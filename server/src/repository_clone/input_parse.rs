use serde_json::Value;

use super::*;

pub(crate) fn canonical_repository_lookup_url(input: &str) -> Option<String> {
    let parsed = parse_repository_clone_input(input)?;
    if let Some(rest) = parsed.clone_url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!("https://{host}/{path}"));
    }
    if let Some((host, path)) = parse_ssh_repository_token(&parsed.clone_url) {
        return Some(format!("https://{host}/{path}"));
    }
    Some(parsed.clone_url)
}

pub(super) fn parse_repository_clone_input(input: &str) -> Option<ParsedRepositoryCloneInput> {
    let token = extract_repository_input_token(input)?;
    if token
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("git@"))
    {
        let (host, repository_path) = token[4..].split_once(':')?;
        let repository_path = normalize_repository_clone_path_for_host(host, repository_path);
        if host.trim().is_empty() || repository_path.is_empty() {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("git@{}:{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    if let Some((host, path)) = parse_ssh_repository_token(&token) {
        let repository_path = normalize_repository_clone_path_for_host(host, path);
        if host.trim().is_empty() || repository_path.is_empty() {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("ssh://git@{}/{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    if let Some((host, path)) = parse_http_repository_token(&token) {
        let repository_path = normalize_repository_clone_path_for_host(host, path);
        if host.trim().is_empty() || repository_path.is_empty() || !host.contains('.') {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("https://{}/{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    let shorthand_path = normalize_repository_path(&token);
    if shorthand_path.split('/').count() < 2 {
        return None;
    }
    Some(ParsedRepositoryCloneInput {
        clone_url: format!("https://{DEFAULT_REPOSITORY_HOST}/{shorthand_path}"),
        repository_name: repository_name_from_path(&shorthand_path),
    })
}

fn extract_repository_input_token(input: &str) -> Option<String> {
    let tokens: Vec<String> = input
        .split_whitespace()
        .map(clean_repository_input_token)
        .filter(|token| !token.is_empty())
        .collect();
    let gh_clone_index = tokens
        .windows(3)
        .position(|window| window == ["gh", "repo", "clone"]);
    if let Some(index) = gh_clone_index {
        return tokens[index + 3..]
            .iter()
            .find(|token| is_repository_like_token(token))
            .cloned();
    }
    tokens
        .into_iter()
        .find(|token| is_repository_like_token(token))
}

fn clean_repository_input_token(token: &str) -> String {
    token
        .trim()
        .trim_start_matches(['<', '(', '"', '\'', '`'])
        .trim_end_matches(['>', ')', ',', '.', '"', '\'', '`'])
        .to_string()
}

fn is_repository_like_token(token: &str) -> bool {
    if token.is_empty() || token.starts_with('-') {
        return false;
    }
    if token
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("git@"))
    {
        return token[4..]
            .split_once(':')
            .is_some_and(|(host, path)| !host.is_empty() && !path.is_empty());
    }
    if let Some(rest) = strip_ascii_case_prefix(token, "ssh://") {
        return has_repository_like_scheme_path(rest);
    }
    if let Some(rest) = strip_ascii_case_prefix(token, "http://")
        .or_else(|| strip_ascii_case_prefix(token, "https://"))
    {
        return has_repository_like_scheme_path(rest);
    }
    if looks_like_host_path(token) {
        return true;
    }
    token.split_once('/').is_some_and(|(owner, path)| {
        !owner.is_empty() && !path.is_empty() && !path.starts_with('/')
    })
}

fn looks_like_host_path(token: &str) -> bool {
    token
        .split_once('/')
        .is_some_and(|(host, path)| host.contains('.') && !path.is_empty())
}

fn has_repository_like_scheme_path(rest: &str) -> bool {
    rest.char_indices()
        .any(|(index, ch)| ch == '/' && index > 0 && index < rest.len() - 1)
}

fn parse_ssh_repository_token(token: &str) -> Option<(&str, &str)> {
    let rest = strip_ascii_case_prefix(token, "ssh://")?;
    let slash_index = rest.find('/')?;
    let authority = &rest[..slash_index];
    let path = &rest[slash_index + 1..];
    if authority.is_empty() || path.is_empty() {
        return None;
    }
    let host = match authority.find('@') {
        Some(index) if index > 0 => &authority[index + 1..],
        _ => authority,
    };
    if host.is_empty() {
        return None;
    }
    Some((host, path))
}

fn parse_http_repository_token(token: &str) -> Option<(&str, &str)> {
    let without_scheme = strip_ascii_case_prefix(token, "https://")
        .or_else(|| strip_ascii_case_prefix(token, "http://"))
        .unwrap_or(token);
    let (host, path) = without_scheme.split_once('/')?;
    if host.contains('.') && !path.is_empty() {
        Some((host, path))
    } else {
        None
    }
}

/// CDXC:AddProject 2026-09-11 WHY:
/// Azure clone URLs use _git/repository or v3/organization/project/repository; appending .git changes the repository name.
/// SEE-ALSO: packages/core-ui/add-project-modal/add-project-input.ts (deleted 2026-10-01).
fn normalize_repository_clone_path_for_host(host: &str, path: &str) -> String {
    let host = host.rsplit('@').next().unwrap_or(host).to_ascii_lowercase();
    if host == "dev.azure.com" || host == "ssh.dev.azure.com" || host.ends_with(".visualstudio.com")
    {
        return path
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string();
    }
    normalize_repository_path(path)
}

fn normalize_repository_path(repository_path: &str) -> String {
    let before_hash = repository_path.split('#').next().unwrap_or_default();
    let before_query = before_hash.split('?').next().unwrap_or_default();
    let before_git_suffix = normalize_git_suffix(before_query);
    let is_clone_path = before_git_suffix.to_ascii_lowercase().ends_with(".git");
    let mut segments: Vec<String> = Vec::new();
    for segment in before_git_suffix.split('/') {
        let segment = decode_repository_path_segment(segment);
        if segment.is_empty() {
            continue;
        }
        let lower = segment.to_ascii_lowercase();
        if !is_clone_path
            && segments.len() >= 2
            && matches!(
                lower.as_str(),
                "-" | "branches"
                    | "commit"
                    | "commits"
                    | "issues"
                    | "pull"
                    | "pulls"
                    | "releases"
                    | "src"
                    | "tree"
                    | "wiki"
            )
        {
            break;
        }
        segments.push(segment.to_string());
    }
    let mut normalized = segments.join("/");
    if let Some(stripped) = normalized.strip_suffix(".git") {
        normalized = format!("{stripped}.git");
    } else if !normalized.is_empty() {
        normalized.push_str(".git");
    }
    normalized
}

fn repository_name_from_path(path: &str) -> String {
    let repository_name = path
        .split('/')
        .rfind(|segment| !segment.is_empty())
        .unwrap_or("repository");
    repository_name
        .strip_suffix(".git")
        .unwrap_or(repository_name)
        .to_string()
}

fn strip_ascii_case_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then(|| &input[prefix.len()..])
}

fn normalize_git_suffix(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let mut search_start = 0usize;
    while let Some(relative_index) = lower[search_start..].find(".git") {
        let index = search_start + relative_index;
        let rest = &path[index + 4..];
        if rest.is_empty() || rest.starts_with('/') {
            return format!("{}.git", &path[..index]);
        }
        search_start = index + 4;
    }
    path.to_string()
}

fn decode_repository_path_segment(segment: &str) -> String {
    let segment = segment.trim();
    percent_decode_utf8(segment).unwrap_or_else(|| segment.to_string())
}

fn percent_decode_utf8(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn normalize_repository_destination_folder_name(
    input: Option<&Value>,
    fallback: &str,
) -> Result<String, RepositoryCloneError> {
    let raw_name = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);
    let mut normalized = String::new();
    let mut previous_space = false;
    let mut previous_separator = false;
    for ch in raw_name.chars() {
        if matches!(ch, '/' | ':' | '\\') {
            if !previous_separator {
                normalized.push('-');
            }
            previous_separator = true;
            previous_space = false;
        } else if ch.is_whitespace() {
            if !previous_space {
                normalized.push(' ');
                previous_space = true;
            }
            previous_separator = false;
        } else {
            normalized.push(ch);
            previous_space = false;
            previous_separator = false;
        }
    }
    let normalized = normalized.trim().to_string();
    if normalized.is_empty() || normalized.chars().all(|ch| ch == '.') {
        return Err(RepositoryCloneError::bad_request(
            "newFolderName must be a valid folder name.",
        ));
    }
    Ok(normalized)
}

pub(super) fn normalize_repository_branch_name(
    input: Option<&Value>,
) -> Result<Option<String>, RepositoryCloneError> {
    let Some(value) = input else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let branch_name = value
        .as_str()
        .ok_or_else(|| RepositoryCloneError::bad_request("branchName must be a string."))?;
    let branch_name = branch_name.trim();
    if branch_name.is_empty() {
        return Ok(None);
    }
    if !is_repository_branch_name_valid(branch_name) {
        return Err(RepositoryCloneError::bad_request(
            "branchName must be a valid Git branch name.",
        ));
    }
    Ok(Some(branch_name.to_string()))
}

pub(super) fn is_repository_branch_name_valid(branch_name: &str) -> bool {
    branch_name.encode_utf16().count() <= 255
        && branch_name != "@"
        && !branch_name.starts_with(['-', '/'])
        && !branch_name.ends_with(['/', '.'])
        && !branch_name.contains("..")
        && !branch_name.contains("@{")
        && !branch_name.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
                || matches!(ch, '\u{0}'..='\u{1F}' | '\u{7F}')
        })
        && branch_name.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        })
}
