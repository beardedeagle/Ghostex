//! Reading an asking tool's result back onto the questions it asked.
//!
//! Port of the answer half of `packages/shared/session-chat-presentation/questions.ts`. Every
//! agent frames the answer differently and none of them echo a structured record, so each
//! envelope is peeled by hand and anything that does not parse falls back to the raw output.

use serde::{Deserialize, Serialize};

use crate::questions::exchange::ParsedQuestion;
use crate::questions::model::Question;
use crate::transcript::jsstr::{js_trim, js_trim_end};

/// One question's answer, matched back onto the options it was offered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeAnswer {
    /// Options whose labels the answer text matched, in answer order.
    pub selected_indices: Vec<usize>,
    /// Free text beyond the matched labels ("Other" answers, added notes).
    pub other_text: Option<String>,
    /// The user closed the question dialog without answering.
    pub dismissed: bool,
}

/// An answered question tool call, as the transcript draws it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionExchange {
    pub questions: Vec<Question>,
    /// One entry per question; `null` when that question went unanswered (skipped). The whole
    /// field is `null` when the result text did not parse per question, and `fallback_text` then
    /// carries the answer as one blob.
    pub answers: Option<Vec<Option<ExchangeAnswer>>>,
    pub fallback_text: Option<String>,
}

const RESULT_PREFIXES: [&str; 2] = ["The user answered: ", "Your questions have been answered: "];
/*
Pi's cursor_ask_question result envelope: a single question answers as `User answered: <label or
custom text>` (one line) and a cancel as the exact sentence below. The multi-question form
(`User answered:` + `- id: value` lines) keeps question ids we do not track, so it falls back to
the raw blob.
*/
const PI_RESULT_PREFIX: &str = "User answered: ";
const PI_CANCELLED_TEXT: &str = "User cancelled the question.";
/*
omp's `ask` result envelope. A single question answers as newline-joined parts
(`User selected: <labels>`, `User provided custom input: <text>`, multi-line bodies continue
two-space indented, and `User added note: <text>`), or one of the two closing sentences. The
multi-question form is `User answers:` plus one `<id>: <value>` line per question, where value is
a bare label, `[a, b]` for multi-select, `"text"` for custom input, or `(cancelled)`, each
optionally suffixed by ` (auto-selected after timeout)` and ` (note: …)`.
*/
const OMP_SELECTED_PREFIX: &str = "User selected: ";
const OMP_CUSTOM_PREFIX: &str = "User provided custom input:";
const OMP_NOTE_PREFIX: &str = "User added note:";
const OMP_CANCELLED_TEXT: &str = "User cancelled the selection";
const OMP_NO_SELECTION_TEXT: &str = "User did not select any options";
const OMP_MULTI_HEADER: &str = "User answers:";
const OMP_TIMEOUT_SUFFIX: &str = " (auto-selected after timeout)";
/// Substring-matched so the exact closing-sentence wording cannot break it. No leading quote: a
/// preview answer ends in its mockup, not in a closing `"`.
const RESULT_SUFFIX_MARKERS: [&str; 2] = [". Read the answers carefully", ". You can now continue"];
/// CDXC:SessionChat 2026-09-23 WHY: Claude Code 2.1.280 follows a preview option's answer with its mockup and any note: `"Q"="Grid" selected preview:\n<mockup> notes: <text>`.
const PREVIEW_ANSWER_MARKER: &str = "\" selected preview:";
const PREVIEW_NOTE_MARKER: &str = " notes: ";
const DISMISSED_PREFIX: &str = "[User dismissed";

/// The `"…"` body between the known prefix and the closing sentence, or `None`.
fn strip_answer_envelope(output: &str) -> Option<String> {
    let prefix = RESULT_PREFIXES
        .iter()
        .find(|candidate| output.starts_with(**candidate))?;
    let mut body = output[prefix.len()..].to_string();
    for marker in RESULT_SUFFIX_MARKERS {
        if let Some(at) = body.rfind(marker) {
            body.truncate(at);
            break;
        }
    }
    Some(body)
}

/// Match an answer's text back onto the question's options: consume full option labels (longest
/// first) separated by `", "` from the front; whatever remains is the user's own words.
///
/// Answers echo option labels verbatim, so exact prefix matching is safe; multi-select answers are
/// labels joined by `", "` with any free-text answer appended the same way.
pub fn match_answer_to_options(question: &Question, text: &str) -> ExchangeAnswer {
    if text.starts_with(DISMISSED_PREFIX) {
        return ExchangeAnswer {
            selected_indices: Vec::new(),
            other_text: None,
            dismissed: true,
        };
    }
    let mut selected_indices: Vec<usize> = Vec::new();
    let mut remaining = text;
    loop {
        let mut best: Option<usize> = None;
        let mut best_length = 0usize;
        for (index, option) in question.options.iter().enumerate() {
            if selected_indices.contains(&index) || option.label.is_empty() {
                continue;
            }
            let boundary = remaining.len() == option.label.len()
                || remaining.starts_with(&format!("{}, ", option.label));
            if boundary
                && remaining.starts_with(&option.label)
                && (best.is_none() || option.label.len() > best_length)
            {
                best = Some(index);
                best_length = option.label.len();
            }
        }
        let Some(best) = best else { break };
        selected_indices.push(best);
        remaining = &remaining[best_length..];
        if let Some(rest) = remaining.strip_prefix(", ") {
            remaining = rest;
        }
        if remaining.is_empty() {
            break;
        }
    }
    let other = js_trim(remaining);
    ExchangeAnswer {
        selected_indices,
        other_text: if other.is_empty() {
            None
        } else {
            Some(other.to_string())
        },
        dismissed: false,
    }
}

/// Peel omp's trailing ` (note: …)` and ` (auto-selected after timeout)` decorations.
fn strip_omp_decorations(raw: &str) -> (Option<String>, String) {
    let mut text = raw.to_string();
    let mut note = None;
    // `formatQuestionResult` appends the timeout suffix first and the note last.
    const NOTE_OPEN: &str = " (note: ";
    if text.ends_with(')') {
        if let Some(at) = text.find(NOTE_OPEN) {
            if at + NOTE_OPEN.len() < text.len() {
                note = Some(text[at + NOTE_OPEN.len()..text.len() - 1].to_string());
                text.truncate(at);
            }
        }
    }
    if let Some(stripped) = text.strip_suffix(OMP_TIMEOUT_SUFFIX) {
        text = stripped.to_string();
    }
    (note, text)
}

fn parse_omp_single_answer(question: &Question, trimmed: &str) -> Option<ExchangeAnswer> {
    if trimmed == OMP_CANCELLED_TEXT {
        return Some(ExchangeAnswer {
            selected_indices: Vec::new(),
            other_text: None,
            dismissed: true,
        });
    }
    if trimmed == OMP_NO_SELECTION_TEXT {
        return Some(ExchangeAnswer::default());
    }
    let lines: Vec<&str> = trimmed.split('\n').collect();
    let opens_as_omp = lines.first().is_some_and(|first| {
        first.starts_with(OMP_SELECTED_PREFIX)
            || first.starts_with(OMP_CUSTOM_PREFIX)
            || first.starts_with(OMP_NOTE_PREFIX)
    });
    if !opens_as_omp {
        return None;
    }
    let mut selected_indices: Vec<usize> = Vec::new();
    let mut extras: Vec<String> = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if let Some(rest) = line.strip_prefix(OMP_SELECTED_PREFIX) {
            let labels = rest.strip_suffix(OMP_TIMEOUT_SUFFIX).unwrap_or(rest);
            let matched = match_answer_to_options(question, labels);
            selected_indices = matched.selected_indices;
            if let Some(other) = matched.other_text {
                extras.push(other);
            }
            index += 1;
            continue;
        }
        if line.starts_with(OMP_CUSTOM_PREFIX) || line.starts_with(OMP_NOTE_PREFIX) {
            let prefix = if line.starts_with(OMP_CUSTOM_PREFIX) {
                OMP_CUSTOM_PREFIX
            } else {
                OMP_NOTE_PREFIX
            };
            let rest = &line[prefix.len()..];
            let mut body = rest.strip_prefix(' ').unwrap_or(rest).to_string();
            index += 1;
            if body.is_empty() {
                // Multi-line body: the following lines are two-space indented.
                let mut body_lines: Vec<&str> = Vec::new();
                while index < lines.len() && lines[index].starts_with("  ") {
                    body_lines.push(&lines[index][2..]);
                    index += 1;
                }
                body = body_lines.join("\n");
            }
            if !body.is_empty() {
                extras.push(body);
            }
            continue;
        }
        index += 1;
    }
    Some(ExchangeAnswer {
        selected_indices,
        other_text: if extras.is_empty() {
            None
        } else {
            Some(extras.join("\n"))
        },
        dismissed: false,
    })
}

fn parse_omp_answer_value(question: &Question, raw: &str) -> ExchangeAnswer {
    let (note, text) = strip_omp_decorations(raw);
    if text == "(cancelled)" {
        return ExchangeAnswer {
            selected_indices: Vec::new(),
            other_text: note,
            dismissed: true,
        };
    }
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        let custom = &text[1..text.len() - 1];
        let joined = match &note {
            Some(note) => format!("{custom}\n{note}"),
            None => custom.to_string(),
        };
        return ExchangeAnswer {
            selected_indices: Vec::new(),
            // `[custom, note].join('\n') || null`: an empty custom answer with no note is null.
            other_text: if joined.is_empty() {
                None
            } else {
                Some(joined)
            },
            dismissed: false,
        };
    }
    let list_body = if text.starts_with('[') && text.ends_with(']') && text.len() >= 2 {
        Some(text[1..text.len() - 1].to_string())
    } else {
        None
    };
    let matched = match_answer_to_options(question, list_body.as_deref().unwrap_or(&text));
    let mut other: Vec<String> = Vec::new();
    if let Some(text) = matched.other_text {
        other.push(text);
    }
    if let Some(note) = note {
        other.push(note);
    }
    ExchangeAnswer {
        selected_indices: matched.selected_indices,
        other_text: if other.is_empty() {
            None
        } else {
            Some(other.join("\n"))
        },
        dismissed: false,
    }
}

fn parse_omp_multi_answers(
    entries: &[ParsedQuestion],
    trimmed: &str,
) -> Option<Vec<Option<ExchangeAnswer>>> {
    let body = trimmed.strip_prefix(OMP_MULTI_HEADER)?;
    // Locate each question's `\n<id>: ` marker in order; custom inputs are quoted but not
    // escaped, so marker slicing is the only reliable read.
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    let mut from = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        let Some(id) = entry.id.as_ref() else {
            continue;
        };
        let marker = format!("\n{id}: ");
        if let Some(at) = body.get(from..).and_then(|tail| tail.find(&marker)) {
            let at = from + at;
            found.push((at, at + marker.len(), index));
            from = at + marker.len();
        }
    }
    if found.is_empty() {
        return None;
    }
    let mut answers: Vec<Option<ExchangeAnswer>> = entries.iter().map(|_| None).collect();
    for (position, (_, end, index)) in found.iter().enumerate() {
        let raw = match found.get(position + 1) {
            Some((next_start, _, _)) => &body[*end..*next_start],
            None => &body[*end..],
        };
        answers[*index] = Some(parse_omp_answer_value(
            &entries[*index].question,
            js_trim_end(raw),
        ));
    }
    Some(answers)
}

/*
Hermes' clarify results are JSON (rendered verbatim into the tool output):
`{"question", "choices_offered", "user_response"}` for a single question, where `user_response` is
a bare label, custom text, or a list for multi-select, and
`{"responses": [{id?, question, choices_offered, user_response}, …], "timed_out"?}` for a batch, in
question order with `""` marking a skip.
*/
fn hermes_response_to_answer(
    question: &Question,
    response: Option<&serde_json::Value>,
    timed_out: bool,
) -> ExchangeAnswer {
    if let Some(serde_json::Value::Array(items)) = response {
        let mut selected_indices: Vec<usize> = Vec::new();
        let mut extras: Vec<String> = Vec::new();
        for item in items {
            let Some(text) = item.as_str().filter(|text| !text.is_empty()) else {
                continue;
            };
            let option_index = question
                .options
                .iter()
                .enumerate()
                .position(|(index, option)| {
                    option.label == text && !selected_indices.contains(&index)
                });
            match option_index {
                Some(index) => selected_indices.push(index),
                None => extras.push(text.to_string()),
            }
        }
        return ExchangeAnswer {
            selected_indices,
            other_text: if extras.is_empty() {
                None
            } else {
                Some(extras.join(", "))
            },
            dismissed: false,
        };
    }
    let text = response
        .and_then(serde_json::Value::as_str)
        .map(js_trim)
        .unwrap_or_default();
    if text.is_empty() {
        return ExchangeAnswer {
            selected_indices: Vec::new(),
            other_text: None,
            dismissed: timed_out,
        };
    }
    match_answer_to_options(question, text)
}

fn parse_hermes_answers(
    entries: &[ParsedQuestion],
    trimmed: &str,
) -> Option<Vec<Option<ExchangeAnswer>>> {
    if !trimmed.starts_with('{') {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let record = parsed.as_object()?;
    let timed_out = record.get("timed_out") == Some(&serde_json::Value::Bool(true));
    if let Some(serde_json::Value::Array(rows)) = record.get("responses") {
        let mut answers: Vec<Option<ExchangeAnswer>> = entries.iter().map(|_| None).collect();
        for (index, row) in rows.iter().enumerate() {
            let Some(entry) = entries.get(index) else {
                continue;
            };
            if !matches!(
                row,
                serde_json::Value::Object(_) | serde_json::Value::Array(_)
            ) {
                continue;
            }
            answers[index] = Some(hermes_response_to_answer(
                &entry.question,
                row.get("user_response"),
                timed_out,
            ));
        }
        return answers.iter().any(Option::is_some).then_some(answers);
    }
    if record.contains_key("user_response") && entries.len() == 1 {
        let entry = entries.first()?;
        return Some(vec![Some(hermes_response_to_answer(
            &entry.question,
            record.get("user_response"),
            false,
        ))]);
    }
    None
}

/// The `A<n>: ` marker that opens question `n`'s answer line, as a byte range in `text`.
pub(crate) fn antigravity_answer_markers(
    text: &str,
    question_count: usize,
) -> Option<Vec<(usize, usize)>> {
    let mut found = Vec::with_capacity(question_count);
    let mut from = 0usize;
    for number in 1..=question_count {
        let marker = format!("A{number}: ");
        let at = if number == 1 {
            text.starts_with(&marker).then_some(0)?
        } else {
            from + text.get(from..)?.find(&format!("\n{marker}"))? + 1
        };
        found.push((at, at + marker.len()));
        from = at + marker.len();
    }
    Some(found)
}

/*
Antigravity's ask_question answers one line per question, in order: `A1: Apple, Cherry`, with a
write-in answer appended to the picked labels the same way and `User Skipped` for a question the
user skipped.
*/
const ANTIGRAVITY_SKIPPED_TEXT: &str = "User Skipped";

fn parse_antigravity_answers(
    entries: &[ParsedQuestion],
    trimmed: &str,
) -> Option<Vec<Option<ExchangeAnswer>>> {
    let markers = antigravity_answer_markers(trimmed, entries.len())?;
    Some(
        markers
            .iter()
            .enumerate()
            .map(|(index, (_, end))| {
                let stop = markers.get(index + 1).map_or(trimmed.len(), |next| next.0);
                let value = js_trim(&trimmed[*end..stop]);
                (value != ANTIGRAVITY_SKIPPED_TEXT)
                    .then(|| match_answer_to_options(&entries[index].question, value))
            })
            .collect(),
    )
}

fn parse_answers(entries: &[ParsedQuestion], output: &str) -> Option<Vec<Option<ExchangeAnswer>>> {
    let trimmed = js_trim(output);
    if entries.len() == 1 {
        let first = &entries[0].question;
        if trimmed == PI_CANCELLED_TEXT {
            return Some(vec![Some(ExchangeAnswer {
                selected_indices: Vec::new(),
                other_text: None,
                dismissed: true,
            })]);
        }
        if trimmed.starts_with(PI_RESULT_PREFIX) && !trimmed.contains('\n') {
            return Some(vec![Some(match_answer_to_options(
                first,
                &trimmed[PI_RESULT_PREFIX.len()..],
            ))]);
        }
        if let Some(single) = parse_omp_single_answer(first, trimmed) {
            return Some(vec![Some(single)]);
        }
    }
    if let Some(multi) = parse_omp_multi_answers(entries, trimmed) {
        return Some(multi);
    }
    if let Some(hermes) = parse_hermes_answers(entries, trimmed) {
        return Some(hermes);
    }
    if let Some(antigravity) = parse_antigravity_answers(entries, trimmed) {
        return Some(antigravity);
    }
    let body = strip_answer_envelope(trimmed)?;
    // Locate each question's `"question"="` marker in order; a question whose marker is missing
    // was skipped and keeps a null entry.
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    let mut from = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if entry.question.question.is_empty() {
            continue;
        }
        let marker = format!("\"{}\"=\"", entry.question.question);
        if let Some(at) = body.get(from..).and_then(|tail| tail.find(&marker)) {
            let at = from + at;
            found.push((at, at + marker.len(), index));
            from = at + marker.len();
        }
    }
    if found.is_empty() {
        return None;
    }
    let mut answers: Vec<Option<ExchangeAnswer>> = entries.iter().map(|_| None).collect();
    for (position, (_, end, index)) in found.iter().enumerate() {
        let slice = match found.get(position + 1) {
            Some((next_start, _, _)) => &body[*end..*next_start],
            None => &body[*end..],
        };
        let mut raw = js_trim_end(slice);
        raw = raw.strip_suffix(',').unwrap_or(raw);
        raw = raw.strip_suffix('"').unwrap_or(raw);
        answers[*index] = Some(match_claude_answer(&entries[*index].question, raw));
    }
    Some(answers)
}

/// The one entry point: an answered question tool pair becomes an exchange the card can render;
/// anything else (pending question, error result, unparseable input) returns `None` and keeps the
/// generic tool row.
pub fn answered_question_exchange(
    name: &str,
    input: &serde_json::Value,
    output: &str,
    is_error: bool,
) -> Option<QuestionExchange> {
    if is_error || !crate::questions::exchange::is_question_tool_name(name) {
        return None;
    }
    let entries = crate::questions::exchange::parse_questions_with_ids(input, Some(name))?;
    let output = js_trim(output);
    if output.is_empty() {
        return None;
    }
    let answers = parse_answers(&entries, output);
    Some(QuestionExchange {
        questions: entries.into_iter().map(|entry| entry.question).collect(),
        fallback_text: if answers.is_none() {
            Some(output.to_string())
        } else {
            None
        },
        answers,
    })
}

/// A Claude answer, with a preview option's mockup dropped and its note kept as the user's words.
fn match_claude_answer(question: &Question, raw: &str) -> ExchangeAnswer {
    let Some(preview_at) = raw.find(PREVIEW_ANSWER_MARKER) else {
        return match_answer_to_options(question, raw);
    };
    let preview = &raw[preview_at + PREVIEW_ANSWER_MARKER.len()..];
    let note = preview
        .rfind(PREVIEW_NOTE_MARKER)
        .map(|at| js_trim(&preview[at + PREVIEW_NOTE_MARKER.len()..]))
        .unwrap_or_default();
    let mut matched = match_answer_to_options(question, &raw[..preview_at]);
    let other: Vec<&str> = matched
        .other_text
        .as_deref()
        .into_iter()
        .chain((!note.is_empty()).then_some(note))
        .collect();
    matched.other_text = (!other.is_empty()).then(|| other.join("\n"));
    matched
}
