//! The words Read aloud speaks for one reply: its prose and list items, with every block a voice
//! cannot read turned into a short note.
//!
//! Input is the reply's prose Markdown as the agent wrote it (the same source as the transcript's
//! `copyText`, `transcript/presentation.rs`), never the renderer's marked-up text. It is parsed with
//! the same `markdown` crate the transcript uses, so a code block, a table or a link is the same
//! thing here as on screen.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: Read aloud speaks prose and list items only. Code blocks and tables become a short spoken note ("Code block skipped."), links are read by their label, and Markdown symbols are dropped. Headings and list items are sentences of their own, a run of skipped blocks is one note ("2 code blocks skipped."), and a reply with nothing else to say still speaks its notes, so the listener knows a reply arrived.
//!
//! CDXC:VoiceMode 2026-10-21 DECISION:
//! User (Q30): inline code that is a file path is read shortened to its file name ("turns.rs", not "packages slash gx-chat-core slash ..."). A path is what the transcript draws as a file pill (`resolve_inline_code_file_path`), and its line number is kept ("turns.rs line 213").
//!
//! CDXC:VoiceMode 2026-10-21 WHY:
//! A path the agent wrote in plain prose, and a link whose label is a path, become the same file pill on screen, so they are read by file name too; reading the whole path aloud is the problem the user's answer was about, whichever way the agent wrote it.

use markdown::mdast::Node;
use markdown::ParseOptions;
use serde::{Deserialize, Serialize};

use crate::transcript::file_paths::{bare_file_paths, resolve_inline_code_file_path, FilePathRef};

/// One utterance: what a speech engine is handed in one call.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSegment {
    pub text: String,
    pub kind: SegmentKind,
}

/// Whether a segment is the agent's own words or a note Read aloud says in their place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    #[default]
    Prose,
    Note,
}

/// The longest segment handed to an engine at once, in characters.
///
/// CDXC:VoiceMode 2026-10-20 WHY: engines choke on long input (Android's `getMaxSpeechInputLength`
/// is 4,000, and Chrome stops an utterance after about 15 seconds of speech), and a segment
/// boundary is where Stop takes effect fastest on some engines, so a long paragraph is cut at its
/// sentence ends.
pub const MAX_SEGMENT_CHARS: usize = 1000;

/// What auto-read says when a turn ends with a question the agent asks you.
pub const ASKING_LINE: &str = "The agent is asking you a question.";

/// What auto-read says when a turn ends waiting on a permission prompt.
pub const APPROVAL_LINE: &str = "The agent is waiting for your approval.";

/// The speakable segments of one reply's Markdown, in reading order.
pub fn speakable_segments(markdown: &str) -> Vec<SpeechSegment> {
    let mut options = ParseOptions::gfm();
    // `$$` blocks are formulas; single-dollar inline math stays off, because "$5 and $10" in a
    // reply is money, not a formula.
    options.constructs.math_flow = true;
    options.constructs.frontmatter = true;
    let Ok(tree) = markdown::to_mdast(markdown, &options) else {
        return prose_segments(&clean(markdown));
    };
    let mut pieces = Vec::new();
    blocks(children(&tree), &mut pieces);
    assemble(pieces)
}

/// One step of the reading, before runs of skipped blocks are merged.
enum Piece {
    Sentence(String),
    Skip(Skip),
    Note(String),
}

/// A block a voice does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Skip {
    Code(Option<&'static str>),
    Table,
    Diagram,
    Formula,
    Image,
    Page,
}

impl Skip {
    /// The noun a merged note counts, singular and plural.
    fn noun(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            Skip::Code(_) => ("a", "code block", "code blocks"),
            Skip::Table => ("a", "table", "tables"),
            Skip::Diagram => ("a", "diagram", "diagrams"),
            Skip::Formula => ("a", "formula", "formulas"),
            Skip::Image => ("an", "image", "images"),
            Skip::Page => ("a", "page", "pages"),
        }
    }

    fn alone(&self) -> String {
        match self {
            Skip::Code(Some(language)) => format!("{language} code block skipped."),
            Skip::Code(None) => "Code block skipped.".to_string(),
            Skip::Table => "Table skipped.".to_string(),
            Skip::Diagram => "Diagram skipped.".to_string(),
            Skip::Formula => "Formula skipped.".to_string(),
            Skip::Image => "Image skipped.".to_string(),
            Skip::Page => "Page skipped.".to_string(),
        }
    }
}

fn children(node: &Node) -> &[Node] {
    node.children().map(Vec::as_slice).unwrap_or(&[])
}

fn blocks(nodes: &[Node], out: &mut Vec<Piece>) {
    for node in nodes {
        block(node, out);
    }
}

fn block(node: &Node, out: &mut Vec<Piece>) {
    match node {
        Node::Paragraph(paragraph) => {
            // A paragraph that is only images is a skipped block, or the images' alt text.
            if paragraph.children.iter().all(is_image_or_blank) {
                let alts: Vec<String> = paragraph
                    .children
                    .iter()
                    .filter_map(image_alt)
                    .map(|alt| clean(&alt))
                    .filter(|alt| !alt.is_empty())
                    .collect();
                if alts.is_empty() {
                    if paragraph
                        .children
                        .iter()
                        .any(|child| image_alt(child).is_some())
                    {
                        out.push(Piece::Skip(Skip::Image));
                    }
                } else {
                    sentence(&alts.join(". "), out);
                }
                return;
            }
            sentence(&inline(&paragraph.children), out);
        }
        Node::Heading(heading) => sentence(&inline(&heading.children), out),
        Node::List(list) => {
            for item in &list.children {
                block(item, out);
            }
        }
        Node::ListItem(item) => {
            let mut inner = Vec::new();
            blocks(&item.children, &mut inner);
            if item.checked == Some(true) {
                // The tick is spoken after the item's first sentence.
                if let Some(Piece::Sentence(text)) = inner
                    .iter_mut()
                    .find(|piece| matches!(piece, Piece::Sentence(_)))
                {
                    *text = format!("{}, done.", without_end(text));
                }
            }
            out.extend(inner);
        }
        Node::Blockquote(quote) => {
            let start = out.len();
            blocks(&quote.children, out);
            if let Some(Piece::Sentence(first)) = out.get_mut(start) {
                if let Some(rest) = alert(first) {
                    *first = rest;
                }
            }
        }
        Node::Code(code) => out.push(code_note(code.lang.as_deref(), &code.value)),
        Node::Math(_) => out.push(Piece::Skip(Skip::Formula)),
        Node::Table(_) => out.push(Piece::Skip(Skip::Table)),
        // A rule between blocks is a pause, and every block is already a segment of its own.
        Node::ThematicBreak(_) => {}
        // Raw HTML, footnote definitions, link definitions and front matter are not read.
        Node::Html(_)
        | Node::FootnoteDefinition(_)
        | Node::Definition(_)
        | Node::Yaml(_)
        | Node::Toml(_) => {}
        other => blocks(children(other), out),
    }
}

/// A fenced or indented code block, by what its fence names.
fn code_note(lang: Option<&str>, value: &str) -> Piece {
    let lang = lang.unwrap_or_default().trim().to_ascii_lowercase();
    match lang.as_str() {
        // A question form the agent asked in its reply (`transcript/question_forms.rs`).
        "questions" => Piece::Note(ASKING_LINE.to_string()),
        "visual" => visual_note(value),
        "mermaid" | "dot" | "graphviz" | "plantuml" => Piece::Skip(Skip::Diagram),
        "math" | "latex" | "tex" | "katex" => Piece::Skip(Skip::Formula),
        _ => Piece::Skip(Skip::Code(language_name(&lang))),
    }
}

/// A ```visual block: a chart or card is a diagram; a page the agent published (`ghostex show`)
/// that asks questions is the question line, like a question form.
fn visual_note(value: &str) -> Piece {
    let parsed: Option<serde_json::Value> = serde_json::from_str(value).ok();
    let page = parsed.as_ref().and_then(|value| value.get("page"));
    match page {
        Some(page)
            if page
                .get("questions")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|questions| !questions.is_empty()) =>
        {
            Piece::Note(ASKING_LINE.to_string())
        }
        Some(_) => Piece::Skip(Skip::Page),
        None => Piece::Skip(Skip::Diagram),
    }
}

/// The spoken name of a fence's language, or `None` to say just "code block".
fn language_name(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "rust" | "rs" => "Rust",
        "ts" | "typescript" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "javascript" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" | "python" => "Python",
        "sh" | "bash" | "zsh" | "shell" | "console" | "fish" | "shellscript" => "Shell",
        "powershell" | "ps1" | "pwsh" => "PowerShell",
        "json" | "jsonc" | "json5" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "html" => "HTML",
        "css" | "scss" | "less" => "CSS",
        "sql" => "SQL",
        "go" | "golang" => "Go",
        "swift" => "Swift",
        "kotlin" | "kt" | "kts" => "Kotlin",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "c++" | "cc" | "hpp" => "C++",
        "cs" | "csharp" => "C sharp",
        "rb" | "ruby" => "Ruby",
        "php" => "PHP",
        "zig" => "Zig",
        "lua" => "Lua",
        "dart" => "Dart",
        "xml" => "XML",
        "diff" | "patch" => "Diff",
        "md" | "markdown" => "Markdown",
        "dockerfile" | "docker" => "Dockerfile",
        _ => return None,
    })
}

/// `[!NOTE]` and the other GitHub alert markers at the start of a quote become a spoken label.
fn alert(first: &str) -> Option<String> {
    let rest = first.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let label = match rest[..close].to_ascii_lowercase().as_str() {
        "note" => "Note",
        "tip" => "Tip",
        "important" => "Important",
        "warning" => "Warning",
        "caution" => "Caution",
        _ => return None,
    };
    let body = rest[close + 1..].trim();
    Some(if body.is_empty() {
        format!("{label}.")
    } else {
        format!("{label}: {body}")
    })
}

fn is_image_or_blank(node: &Node) -> bool {
    match node {
        Node::Image(_) | Node::ImageReference(_) | Node::Break(_) => true,
        Node::Text(text) => text.value.trim().is_empty(),
        _ => false,
    }
}

fn image_alt(node: &Node) -> Option<String> {
    match node {
        Node::Image(image) => Some(image.alt.clone()),
        Node::ImageReference(image) => Some(image.alt.clone()),
        _ => None,
    }
}

/// The words of an inline run, with every Markdown symbol gone.
fn inline(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        inline_node(node, &mut out);
    }
    clean(&out)
}

fn inline_node(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&prose_paths(&text.value)),
        Node::InlineCode(code) => out.push_str(&spoken_code(&code.value)),
        Node::Link(link) => {
            let label = inline(&link.children);
            out.push_str(&link_label(&label, &link.url));
        }
        Node::LinkReference(link) => {
            let label = inline(&link.children);
            out.push_str(if label.is_empty() { "link" } else { &label });
        }
        Node::Image(image) => out.push_str(&image.alt),
        Node::ImageReference(image) => out.push_str(&image.alt),
        Node::Break(_) => out.push(' '),
        Node::InlineMath(_) => out.push_str("formula"),
        Node::Html(_) | Node::FootnoteReference(_) => {}
        other => {
            for child in children(other) {
                inline_node(child, out);
            }
        }
    }
}

/// A link is read by its label; a bare URL (or a label that is the URL) is "link".
fn link_label(label: &str, url: &str) -> String {
    let trimmed = label.trim();
    let bare = trimmed.is_empty()
        || trimmed == url
        || url.strip_prefix("mailto:") == Some(trimmed)
        || ["http://", "https://", "www.", "mailto:", "file://"]
            .iter()
            .any(|scheme| trimmed.starts_with(scheme));
    if bare {
        return "link".to_string();
    }
    match resolve_inline_code_file_path(trimmed) {
        Some(path) => spoken_path(&path),
        None => trimmed.to_string(),
    }
}

/// Inline code: a file path by its file name, a folder path by its last folder, and a
/// `snake_case` identifier as words.
fn spoken_code(code: &str) -> String {
    if let Some(path) = resolve_inline_code_file_path(code) {
        return spoken_path(&path);
    }
    if let Some(name) = announced_path_name(code.trim()) {
        return name.to_string();
    }
    let bytes = code.as_bytes();
    code.char_indices()
        .map(|(at, character)| {
            let between_words = character == '_'
                && at > 0
                && bytes[at - 1].is_ascii_alphanumeric()
                && bytes.get(at + 1).is_some_and(u8::is_ascii_alphanumeric);
            if between_words {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// The last name of a path that announces itself as one (`/Users/me/dev/app`, `~/dev/app/`,
/// `./build`, `C:\Users\me\dev`) and has at least two parts, or `None`.
///
/// A folder is no file pill on screen (it has no extension), but read in full it is the same
/// "slash Users slash ..." the user's answer was about, so it is shortened the same way.
fn announced_path_name(token: &str) -> Option<&str> {
    let bytes = token.as_bytes();
    let drive = bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && matches!(bytes.get(2), Some(b'\\' | b'/'));
    let announced = drive
        || (token.starts_with('/') && !token.starts_with("//"))
        || token.starts_with("~/")
        || token.starts_with("./")
        || token.starts_with("../");
    if !announced {
        return None;
    }
    let body = if drive { &token[3..] } else { token };
    let parts: Vec<&str> = body
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != "~" && *part != "." && *part != "..")
        .collect();
    let path_like = parts.iter().all(|part| {
        part.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'@' | b'-' | b'%')
        })
    });
    (path_like && parts.len() >= 2).then(|| *parts.last().unwrap_or(&""))
}

/// The folder paths of a prose run (after its file paths were shortened), by their last folder.
fn prose_folder_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let space = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let token = &rest[..space];
        let lead = token.len()
            - token
                .trim_start_matches(['(', '[', '{', '\'', '"', '<'])
                .len();
        let core_end = token
            .trim_end_matches([')', ']', '}', ',', '.', '!', '?', ';', ':', '\'', '"', '>'])
            .len();
        match (lead < core_end)
            .then(|| announced_path_name(&token[lead..core_end]))
            .flatten()
        {
            Some(name) => {
                out.push_str(&token[..lead]);
                out.push_str(name);
                out.push_str(&token[core_end..]);
            }
            None => out.push_str(token),
        }
        let after_space = rest[space..]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(rest.len() - space, |(at, _)| at);
        out.push_str(&rest[space..space + after_space]);
        rest = &rest[space + after_space..];
    }
    out
}

/// "turns.rs", or "turns.rs line 213" when the path names a line.
fn spoken_path(path: &FilePathRef) -> String {
    match path.position {
        Some(position) => format!("{} line {}", path.basename, position.line),
        None => path.basename.clone(),
    }
}

/// The plain-prose paths of a text run, shortened to their file names.
fn prose_paths(text: &str) -> String {
    let found = bare_file_paths(text);
    if found.is_empty() {
        return prose_folder_paths(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for path in found {
        if path.start < copied {
            continue;
        }
        out.push_str(&text[copied..path.start]);
        // An `@mention`'s path has already lost its marker and quotes.
        match resolve_inline_code_file_path(&path.path) {
            Some(reference) => out.push_str(&spoken_path(&reference)),
            None => out.push_str(crate::transcript::jsstr::last_path_segment(&path.path)),
        }
        copied = path.end;
    }
    out.push_str(&text[copied..]);
    prose_folder_paths(&out)
}

/// Emoji dropped, arrows and approximations put into words, whitespace collapsed.
///
/// CDXC:VoiceMode 2026-10-20 WHY: engines read an emoji's name aloud ("white heavy check mark"),
/// which turns a handover's ✅ Done into noise, so emoji are dropped; an arrow between two words
/// reads as "to" and "~5" as "about 5", the way a person would say them.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if is_emoji(character) {
            continue;
        }
        match character {
            '\u{2192}' | '\u{27f6}' | '\u{21d2}' => out.push_str(" to "),
            // "Settings › Chat › Voice" is a menu path: a pause between its steps.
            '\u{203a}' | '\u{bb}' => out.push(','),
            '~' if chars.peek().is_some_and(char::is_ascii_digit) => out.push_str("about "),
            _ => out.push(character),
        }
    }
    let collapsed = crate::transcript::jsstr::collapse_whitespace(&out).replace(" ,", ",");
    collapsed.trim().to_string()
}

/// Emoji and their joiners, by the Unicode blocks they live in.
fn is_emoji(character: char) -> bool {
    matches!(
        character as u32,
        0x1F000..=0x1FAFF
            | 0x2600..=0x27BF
            | 0x2B00..=0x2BFF
            | 0xFE00..=0xFE0F
            | 0x200D
            | 0x20E3
            | 0xE0020..=0xE007F
            | 0x231A..=0x231B
            | 0x2328
            | 0x23CF
            | 0x23E9..=0x23F3
            | 0x23F8..=0x23FA
            | 0x25AA..=0x25AB
            | 0x25B6
            | 0x25C0
            | 0x25FB..=0x25FE
            | 0x2934..=0x2935
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
    )
}

fn sentence(text: &str, out: &mut Vec<Piece>) {
    let text = clean(text);
    if text.is_empty() || !text.chars().any(char::is_alphanumeric) {
        return;
    }
    out.push(Piece::Sentence(with_end(&text)));
}

/// A sentence ends in punctuation, so the engine lets its voice fall at the end of a heading or a
/// list item instead of running it into the next one.
fn with_end(text: &str) -> String {
    // `…suspended."` already ends its sentence inside the quote.
    let closed = text.trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}']);
    if closed.ends_with(['.', '!', '?', ':', ';', '\u{2026}']) {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

fn without_end(text: &str) -> &str {
    text.trim_end_matches(['.', '!', '?', ':', ';', '\u{2026}'])
}

/// Merges runs of skipped blocks into one note and cuts long prose at its sentence ends.
fn assemble(pieces: Vec<Piece>) -> Vec<SpeechSegment> {
    let mut segments = Vec::new();
    let mut run: Vec<Skip> = Vec::new();
    let flush = |run: &mut Vec<Skip>, segments: &mut Vec<SpeechSegment>| {
        if !run.is_empty() {
            segments.push(SpeechSegment {
                text: skipped_note(run),
                kind: SegmentKind::Note,
            });
            run.clear();
        }
    };
    for piece in pieces {
        match piece {
            Piece::Skip(skip) => run.push(skip),
            Piece::Sentence(text) => {
                flush(&mut run, &mut segments);
                segments.extend(prose_segments(&text));
            }
            Piece::Note(text) => {
                flush(&mut run, &mut segments);
                segments.push(SpeechSegment {
                    text,
                    kind: SegmentKind::Note,
                });
            }
        }
    }
    flush(&mut run, &mut segments);
    segments
}

/// "Code block skipped.", or one note for a run: "2 code blocks and a table skipped."
fn skipped_note(run: &[Skip]) -> String {
    if let [only] = run {
        return only.alone();
    }
    // Counted per noun, in the order the nouns first appear.
    let mut counts: Vec<(Skip, usize, bool)> = Vec::new();
    for skip in run {
        let noun = skip.noun().1;
        match counts.iter_mut().find(|(seen, ..)| seen.noun().1 == noun) {
            Some((seen, count, same_language)) => {
                *count += 1;
                *same_language &= *seen == *skip;
            }
            None => counts.push((skip.clone(), 1, true)),
        }
    }
    let phrases: Vec<String> = counts
        .iter()
        .map(|(skip, count, same_language)| {
            let (article, one, many) = skip.noun();
            let language = match skip {
                Skip::Code(Some(language)) if *same_language => format!("{language} "),
                _ => String::new(),
            };
            if *count == 1 {
                if language.is_empty() {
                    format!("{article} {one}")
                } else {
                    format!(
                        "{} {language}{one}",
                        if language.starts_with(['A', 'E', 'I', 'O', 'U']) {
                            "an"
                        } else {
                            "a"
                        }
                    )
                }
            } else {
                format!("{count} {language}{many}")
            }
        })
        .collect();
    let joined = match phrases.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    };
    let mut note = format!("{joined} skipped.");
    if let Some(first) = note.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    note
}

/// One block's prose, cut into segments of at most [`MAX_SEGMENT_CHARS`] at sentence ends.
fn prose_segments(text: &str) -> Vec<SpeechSegment> {
    if text.is_empty() {
        return Vec::new();
    }
    let prose = |text: String| SpeechSegment {
        text,
        kind: SegmentKind::Prose,
    };
    if text.chars().count() <= MAX_SEGMENT_CHARS {
        return vec![prose(text.to_string())];
    }
    let mut segments = Vec::new();
    let mut current = String::new();
    for sentence in sentences(text) {
        for piece in hard_split(sentence) {
            let joined = current.chars().count() + piece.chars().count() + 1;
            if !current.is_empty() && joined > MAX_SEGMENT_CHARS {
                segments.push(prose(std::mem::take(&mut current)));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(piece.trim());
        }
    }
    if !current.is_empty() {
        segments.push(prose(current));
    }
    segments
}

/// The sentences of a paragraph: each ends after `.`, `!` or `?` followed by a space.
fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'.' | b'!' | b'?') && bytes.get(at + 1) == Some(&b' ') {
            found.push(&text[start..=at]);
            start = at + 2;
        }
    }
    if start < text.len() {
        found.push(&text[start..]);
    }
    found
}

/// A sentence longer than a segment, cut at the last space that fits (or anywhere, for one word
/// that long).
fn hard_split(sentence: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = sentence.trim();
    while rest.chars().count() > MAX_SEGMENT_CHARS {
        let limit = rest
            .char_indices()
            .nth(MAX_SEGMENT_CHARS)
            .map_or(rest.len(), |(at, _)| at);
        let cut = rest[..limit]
            .rfind(' ')
            .filter(|at| *at > 0)
            .unwrap_or(limit);
        pieces.push(&rest[..cut]);
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}
