//! Which single newlines in an agent's reply or a user's prompt stay line breaks.
//!
//! Markdown joins the lines of a paragraph, and so do both renderers (the GPUI `TextView` and the
//! phone's parser in `apps/mobile/app/src/chat/native/transcript/markdown/`): a question and the
//! lettered options under it read as one run of text, and so did a multi-line prompt. The decision
//! is made here and travels inside the Markdown as a hard break, which every renderer draws as a
//! new line.
//!
//! CDXC:SessionChat 2026-09-27 DECISION:
//! User: "i dont want joined lines when agent sends stuff with newlines". Every agent's reply keeps each single newline as a line break, and so does every other place an agent's own words are drawn: thinking, side-question answers, a subagent's message card, and a message another agent sent into the session. Supersedes the 2026-09-26 decision, under which Claude, Codex, Grok Build and Cursor followed Markdown's joining except before a lettered option line (only Pi, OMP, Hermes, Antigravity and ZCode kept every newline).

use markdown::mdast::Node;
use markdown::ParseOptions;

/// How an agent's reply treats a newline inside a paragraph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentLineBreaks {
    /// Markdown's joining, except before a lettered option line.
    #[default]
    Markdown,
    /// Every newline is a line break.
    Every,
}

impl AgentLineBreaks {
    /// The rule for a session: every agent keeps its newlines (the 2026-09-27 decision above).
    pub fn for_agent(_agent: Option<&str>, _session_agent: Option<&str>) -> Self {
        Self::Every
    }
}

/// The reply with the chosen soft line breaks turned into hard breaks. Code, tables, headings and
/// HTML are untouched: only the text of paragraphs is looked at.
pub fn agent_line_breaks(markdown: &str, mode: AgentLineBreaks) -> String {
    hard_breaks(markdown, mode, "\\")
}

/// A user's prompt with every newline inside a paragraph kept as a line break: it is text somebody
/// typed, so a pasted block or a list of lines shows the way it was sent.
///
/// CDXC:SessionChat 2026-09-27 WHY: the break is two trailing spaces, not the backslash agent replies get, because the prompt then goes through the typed-path linker, whose path token runs to the next whitespace: a prompt line ending in `/Users/me/project` would become a link to `/Users/me/project\` and lose the break with it. Agent replies stay on the backslash because the phone trims trailing spaces from agent paragraph lines.
pub fn user_line_breaks(markdown: &str) -> String {
    hard_breaks(markdown, AgentLineBreaks::Every, "  ")
}

/// A change to the source: the bytes `start..end` replaced by `text`.
struct Edit {
    start: usize,
    end: usize,
    text: String,
}

fn hard_breaks(markdown: &str, mode: AgentLineBreaks, marker: &str) -> String {
    if !markdown.contains('\n') {
        return markdown.to_string();
    }
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::gfm()) else {
        return markdown.to_string();
    };
    let mut edits = Vec::new();
    collect(&tree, None, markdown, mode, marker, &mut edits);
    if edits.is_empty() {
        return markdown.to_string();
    }
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut out = String::with_capacity(markdown.len() + edits.len() * 4);
    let mut cursor = 0;
    for edit in edits {
        if edit.start < cursor {
            continue;
        }
        out.push_str(&markdown[cursor..edit.start]);
        out.push_str(&edit.text);
        cursor = edit.end;
    }
    out.push_str(&markdown[cursor..]);
    out
}

/// The paragraph a text span sits in: the width of what precedes its first line on that line
/// (list markers, quote markers, indentation), and whether its line prefix holds a quote marker.
#[derive(Clone, Copy)]
struct Paragraph {
    base: usize,
    quoted: bool,
}

fn collect(
    node: &Node,
    paragraph: Option<Paragraph>,
    source: &str,
    mode: AgentLineBreaks,
    marker: &str,
    edits: &mut Vec<Edit>,
) {
    if let Some(paragraph) = paragraph {
        if let Node::Text(text) = node {
            if let Some(position) = &text.position {
                soft_breaks(
                    source,
                    position.start.offset,
                    position.end.offset,
                    paragraph,
                    mode,
                    marker,
                    edits,
                );
            }
            return;
        }
        // A hard break the reply already wrote: the next line keeps its indentation too.
        if let Node::Break(node) = node {
            if let Some(position) = &node.position {
                let span = source.get(position.start.offset..position.end.offset);
                if let Some(index) = span.and_then(|span| span.rfind('\n')) {
                    continuation_indent(
                        source,
                        position.start.offset + index + 1,
                        paragraph,
                        edits,
                    );
                }
            }
            return;
        }
    }
    let mut paragraph = paragraph;
    if let Node::Paragraph(node) = node {
        if let Some(position) = &node.position {
            let start = position.start.offset;
            let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
            let prefix = &source[line_start..start];
            paragraph = Some(Paragraph {
                base: width(prefix),
                quoted: prefix.contains('>'),
            });
        }
    }
    if let Node::Root(root) = node {
        for child in &root.children {
            if let Node::Paragraph(child) = child {
                if let Some(position) = &child.position {
                    first_line_indent(source, position.start.offset, mode, edits);
                }
            }
        }
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect(child, paragraph, source, mode, marker, edits);
    }
}

/// The insertion points, one per chosen newline in a text span: right before its line ending. A
/// line kept apart this way also keeps the spaces it is indented by.
fn soft_breaks(
    source: &str,
    start: usize,
    end: usize,
    paragraph: Paragraph,
    mode: AgentLineBreaks,
    marker: &str,
    edits: &mut Vec<Edit>,
) {
    let Some(span) = source.get(start..end) else {
        return;
    };
    for (index, _) in span.match_indices('\n') {
        let newline = start + index;
        let line_end = if source[..newline].ends_with('\r') {
            newline - 1
        } else {
            newline
        };
        let line = &source[..line_end];
        let next_line = source[newline + 1..].split('\n').next().unwrap_or_default();
        // Already a hard break: two trailing spaces, or an odd run of backslashes.
        let backslashes = line.len() - line.trim_end_matches('\\').len();
        if !(line.ends_with("  ") || backslashes % 2 == 1) {
            if mode != AgentLineBreaks::Every && !starts_with_option(next_line) {
                continue;
            }
            edits.push(Edit {
                start: line_end,
                end: line_end,
                text: marker.to_string(),
            });
        }
        continuation_indent(source, newline + 1, paragraph, edits);
    }
}

/// Keeps the indentation of a paragraph line that follows a line break.
///
/// CDXC:SessionChat 2026-10-18 WHY: Markdown strips the leading spaces of a paragraph's continuation lines, so a reply that shows hierarchy by indenting plain lines (no list markers) rendered flush left in chat while the Terminal view showed it indented (GitHub issue #252). The spaces past the paragraph's own container indent (list item content column, quote marker) become no-break spaces, which Markdown keeps as text and every renderer draws as spaces. Indented code blocks, lists and nested list items are not paragraph continuations and are left to Markdown.
fn continuation_indent(
    source: &str,
    line_start: usize,
    paragraph: Paragraph,
    edits: &mut Vec<Edit>,
) {
    let line = source[line_start..].split('\n').next().unwrap_or_default();
    let mut consumed = 0;
    let mut split = 0;
    for (at, character) in line.char_indices() {
        if consumed >= paragraph.base {
            break;
        }
        match character {
            ' ' | '\t' => consumed += char_width(character),
            '>' if paragraph.quoted => consumed += 1,
            _ => return,
        }
        split = at + character.len_utf8();
    }
    indent_edit(line_start + split, &line[split..], edits);
}

/// Keeps the one to three spaces a top-level paragraph's first line is indented by, which Markdown
/// drops the same way (four or more make an indented code block, which stays one).
fn first_line_indent(source: &str, start: usize, mode: AgentLineBreaks, edits: &mut Vec<Edit>) {
    if mode != AgentLineBreaks::Every {
        return;
    }
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let line = source[line_start..].split('\n').next().unwrap_or_default();
    indent_edit(line_start, line, edits);
}

/// Replaces the run of spaces and tabs `rest` opens with by as many no-break spaces.
fn indent_edit(at: usize, rest: &str, edits: &mut Vec<Edit>) {
    let run = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if run == 0 || rest[run..].trim().is_empty() {
        return;
    }
    edits.push(Edit {
        start: at,
        end: at + run,
        text: "\u{a0}".repeat(width(&rest[..run])),
    });
}

/// The columns a line prefix takes, a tab counting as four.
fn width(prefix: &str) -> usize {
    prefix.chars().map(char_width).sum()
}

fn char_width(character: char) -> usize {
    if character == '\t' {
        4
    } else {
        1
    }
}

/// `A. `, `B) `, `A1. ` after indentation or a quote prefix.
fn starts_with_option(line: &str) -> bool {
    let text = line.trim_start_matches([' ', '\t', '>']);
    let mut characters = text.chars();
    if !characters
        .next()
        .is_some_and(|letter| letter.is_ascii_uppercase())
    {
        return false;
    }
    let rest = characters
        .as_str()
        .trim_start_matches(|digit: char| digit.is_ascii_digit());
    rest.strip_prefix(['.', ')'])
        .is_some_and(|after| after.starts_with([' ', '\t']))
}
