//! Loops in a script an agent ran, unrolled so each send inside reads the value it ran with.
//!
//! CDXC:SessionChat 2026-10-12 WHY:
//! A coordinator messages its threads in a loop (`for t in A B; do ghostex agents send $t "…" | grep -o status; done`, or PowerShell's `foreach($t in @("A","B")){ … }`) and filters away the JSON that names each recipient, so the card read "To S8g:P2uwo:$t". A loop is unrolled into its body once per value, after an assignment that sets the loop variable, and the reader in sent_message_script.rs reads `$t` (whole or inside a word) from that assignment.
//! SEE-ALSO: sent_message_script.rs parses the script and reads the variables; sent_message.rs turns the sends into cards.

use crate::transcript::sent_message_script::{assignment, is_expansion, variable_name};
use crate::transcript::shell_script::ShellCommand;

/// The most values one loop is unrolled into; a longer loop is read as written.
const MAX_LOOP_VALUES: usize = 64;

/// `for <var> in <words>` loops unrolled: the body once per value, after the assignment that sets
/// the loop variable. A loop whose values the script does not show (`for t in "${targets[@]}"`)
/// keeps its body once, with the variable unset.
pub fn unroll_loops(commands: Vec<ShellCommand>) -> Vec<ShellCommand> {
    let mut unrolled: Vec<ShellCommand> = Vec::with_capacity(commands.len());
    let mut at = 0;
    while at < commands.len() {
        let Some((variable, words)) = loop_header(&commands[at]) else {
            unrolled.push(commands[at].clone());
            at += 1;
            continue;
        };
        let mut depth = 1;
        let mut end = at + 1;
        while end < commands.len() {
            match commands[end].words.first().map(String::as_str) {
                Some("for" | "while" | "until" | "select") => depth += 1,
                Some("done") => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        let body = unroll_loops(commands[at + 1..end].to_vec());
        match loop_values(&words, &unrolled) {
            Some(values) => {
                for value in values {
                    unrolled.push(ShellCommand {
                        words: vec![format!("{variable}={value}")],
                        ..ShellCommand::default()
                    });
                    unrolled.extend(body.iter().cloned());
                }
            }
            None => unrolled.extend(body),
        }
        at = end + 1;
    }
    unrolled
}

/// `for t in A B C`: the variable and the words it walks.
fn loop_header(shell: &ShellCommand) -> Option<(String, Vec<String>)> {
    match shell.words.as_slice() {
        [keyword, variable, within, words @ ..] if keyword == "for" && within == "in" => {
            let valid = !variable.is_empty()
                && variable
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_');
            valid.then(|| (variable.clone(), words.to_vec()))
        }
        _ => None,
    }
}

/// The values a loop walks: its words, a whole-word variable read from an earlier assignment and
/// split on white space the way the shell splits it. `None` when one is not shown.
fn loop_values(words: &[String], before: &[ShellCommand]) -> Option<Vec<String>> {
    let mut values = Vec::new();
    for word in words {
        if let Some(name) = variable_name(word) {
            let (_, assigned) = before
                .iter()
                .rev()
                .find_map(|shell| assignment(shell).filter(|(assigned, _)| *assigned == name))?;
            match assigned.as_slice() {
                [value] if !is_expansion(value) && !value.contains('$') => {
                    values.extend(value.split_whitespace().map(str::to_string))
                }
                _ => return None,
            }
        } else if word.contains(['$', '`', '*']) {
            return None;
        } else {
            values.push(word.clone());
        }
    }
    (!values.is_empty() && values.len() <= MAX_LOOP_VALUES).then_some(values)
}

/// PowerShell loops rewritten as the POSIX `for` the shell reader understands:
/// `foreach ($t in @("A","B")) { … }` and `"A","B" | ForEach-Object { … $_ … }`, where the list is
/// spelled out or is a `$name` the script set to one. Any other loop is left as written.
pub fn powershell_loops(script: &str) -> String {
    let lower = script.to_ascii_lowercase();
    let mut out = String::with_capacity(script.len());
    let mut copied = 0;
    let mut search = 0;
    while let Some(found) = lower[search..].find("foreach").map(|at| search + at) {
        search = found + "foreach".len();
        let starts_word = lower[..found].chars().next_back().is_none_or(|character| {
            !(character.is_alphanumeric() || matches!(character, '_' | '$' | '-'))
        });
        if !starts_word {
            continue;
        }
        let rewritten = if lower[found..].starts_with("foreach-object") {
            pipeline_loop(script, found, copied)
        } else {
            statement_loop(script, found)
        };
        if let Some((start, end, replacement)) = rewritten {
            out.push_str(&script[copied..start]);
            out.push_str(&replacement);
            copied = end;
            search = end;
        }
    }
    out.push_str(&script[copied..]);
    out
}

/// `foreach ($t in <list>) { <body> }` starting at `found`: the span it covers and its rewrite.
fn statement_loop(script: &str, found: usize) -> Option<(usize, usize, String)> {
    let rest = script[found + "foreach".len()..].trim_start();
    let open = script.len() - rest.len();
    let close = open + 1 + matching(rest.strip_prefix('(')?, '(', ')')?;
    let header = script[open + 1..close].trim().strip_prefix('$')?;
    let split = header.find(char::is_whitespace)?;
    let variable = &header[..split];
    let list = header[split..].trim_start();
    if !list.get(..2)?.eq_ignore_ascii_case("in") {
        return None;
    }
    let values = list_values(&list[2..], script)?;
    let (body, end) = braced_body(script, close + 1)?;
    Some((found, end, posix_loop(variable, &values, &body)))
}

/// `<list> | ForEach-Object { <body> }` with `ForEach-Object` at `found`: the list runs back to the
/// start of its statement, no further than `floor` (what was already rewritten).
fn pipeline_loop(script: &str, found: usize, floor: usize) -> Option<(usize, usize, String)> {
    let list_end = floor + script[floor..found].trim_end().strip_suffix('|')?.len();
    let start = script[floor..list_end]
        .rfind(['\n', ';', '{'])
        .map_or(floor, |at| floor + at + 1);
    let values = list_values(&script[start..list_end], script)?;
    let (body, end) = braced_body(script, found + "foreach-object".len())?;
    Some((start, end, posix_loop("_", &values, &body)))
}

/// The `{ … }` that follows `from` (after white space): its inside, loops rewritten, and the offset
/// just past its `}`.
fn braced_body(script: &str, from: usize) -> Option<(String, usize)> {
    let after = script[from..].trim_start();
    let brace = script.len() - after.len();
    let end = brace + 1 + matching(after.strip_prefix('{')?, '{', '}')?;
    Some((powershell_loops(&script[brace + 1..end]), end + 1))
}

fn posix_loop(variable: &str, values: &[String], body: &str) -> String {
    let quoted: Vec<String> = values
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', r"'\''")))
        .collect();
    format!("for {variable} in {}; do\n{body}\ndone", quoted.join(" "))
}

/// The byte offset in `text` of the `close` that matches an `open` just before it, skipping quoted
/// strings.
fn matching(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, character) in text.char_indices() {
        if let Some(quoting) = quote {
            if escaped {
                escaped = false;
            } else if character == '`' && quoting == '"' {
                escaped = true;
            } else if character == quoting {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            _ if character == open => depth += 1,
            _ if character == close => {
                if depth == 0 {
                    return Some(at);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// A PowerShell list spelled out (`@("A","B")`, `"A","B"`, `@('A';'B')`), or a `$name` the script set
/// to one of those.
fn list_values(list: &str, script: &str) -> Option<Vec<String>> {
    let list = list.trim();
    if let Some(name) = list.strip_prefix('$') {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_');
        if !valid {
            return None;
        }
        let lower = script.to_ascii_lowercase();
        let marker = format!("${}", name.to_ascii_lowercase());
        let assigned = lower.match_indices(&marker).find_map(|(at, _)| {
            let after = &lower[at + marker.len()..];
            let value = after.trim_start().strip_prefix('=')?;
            let start = lower.len() - value.len();
            let end = script[start..]
                .find(['\n', ';'])
                .map_or(script.len(), |offset| start + offset);
            Some(script[start..end].to_string())
        })?;
        return list_values(&assigned, "");
    }
    let list = list
        .strip_prefix("@(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(list);
    let mut values = Vec::new();
    let mut item = String::new();
    let mut quote: Option<char> = None;
    for character in list.chars() {
        match quote {
            Some(quoting) if character == quoting => quote = None,
            Some(_) => item.push(character),
            None if matches!(character, '\'' | '"') => quote = Some(character),
            None if matches!(character, ',' | ';' | '\n') => values.push(std::mem::take(&mut item)),
            None => item.push(character),
        }
    }
    values.push(item);
    let values: Vec<String> = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let literal = values
        .iter()
        .all(|value| !value.contains(['$', '(', ')', '`', '@', '{', '}', ' ', '\t', '|']));
    (literal && !values.is_empty() && values.len() <= MAX_LOOP_VALUES).then_some(values)
}
