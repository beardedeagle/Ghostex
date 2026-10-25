//! The text one diff cell draws: tabs expanded to the React cell's `tab-size: 2` stops and, when
//! lines wrap, a `\n` wherever Chrome breaks the React cell's `white-space: pre-wrap;
//! overflow-wrap: anywhere` text, with the maps between that text and the patch line.

/// What a wrapped cell's text is measured against: the monospace advance, the width of a wide
/// (CJK) glyph, and the width the text may take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WrapMetrics {
    pub(crate) advance: f32,
    pub(crate) wide: f32,
    pub(crate) width: f32,
}

/// A cell's text as drawn and how it maps back to the patch line it came from.
pub(crate) struct CellText {
    pub(crate) display: String,
    /// The source byte each display byte came from, plus one entry for the end.
    display_to_source: Vec<usize>,
    /// The display byte each source byte starts at, plus one entry for the end.
    source_to_display: Vec<usize>,
    /// The text wraps onto more than one row.
    pub(crate) multi_row: bool,
}

impl CellText {
    /// The one space `renderDiffLineContent` draws for an empty cell.
    pub(crate) fn blank() -> Self {
        Self {
            display: " ".to_string(),
            display_to_source: vec![0, 0],
            source_to_display: vec![0],
            multi_row: false,
        }
    }

    /// Adds the line-break box Chrome highlights after the last character of a selected line.
    pub(crate) fn push_line_end(&mut self) {
        let end = self.display_to_source.last().copied().unwrap_or(0);
        self.display.push(' ');
        self.display_to_source.push(end);
    }

    pub(crate) fn display_of(&self, source: usize) -> usize {
        self.source_to_display
            .get(source)
            .or(self.source_to_display.last())
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn into_source_map(self) -> Vec<usize> {
        self.display_to_source
    }
}

/// Glyphs the fallback font draws a full em wide.
fn is_wide(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115f | 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xfe30..=0xfe4f
            | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x20000..=0x3fffd
    )
}

/// Whether Chrome may end a row between `prev` and `next` in a pre-wrap cell.
///
/// CDXC:Git 2026-09-28 WHY: Chrome's line breaker, not UAX #14 and not gpui's wrapper, decides where the React diff wraps: gpui breaks before `/`, `.` or `(` and hangs the indent, UAX #14 allows a break after `/`, and Chrome does neither. The pairs were measured in Chrome for every printable ASCII pair (a break after spaces and tabs, after `-` and `?` unless closing punctuation follows, and before an opening bracket after other punctuation) and the result matched Chrome on 2,816 real diff lines at four widths.
fn break_between(prev: char, next: char) -> bool {
    if matches!(next, ' ' | '\t') {
        return false;
    }
    match prev {
        ' ' | '\t' => true,
        '-' => !matches!(
            next,
            '?' | '!' | '$' | ')' | ',' | '.' | '/' | ':' | ';' | ']' | '}'
        ),
        '?' => !matches!(
            next,
            '?' | '!' | '"' | '\'' | ')' | ',' | '.' | '/' | ':' | ';' | ']' | '}'
        ),
        '!' | '"' | '#' | '%' | '&' | ')' | '*' | '+' | ',' | '.' | ':' | ';' | '=' | '>'
        | '\\' | ']' | '|' | '}' | '~' => matches!(next, '(' | '<' | '[' | '{'),
        _ => is_wide(prev) || is_wide(next),
    }
}

/// The source bytes where each row after the first starts. Rows break at the last break
/// opportunity that fits; a word longer than a row breaks anywhere (`overflow-wrap: anywhere`);
/// spaces at the end of a row hang past it (`pre-wrap`).
fn soft_breaks(source: &str, metrics: WrapMetrics) -> Vec<usize> {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let width_of = |ch: char, x: f32| {
        if ch == '\t' {
            let column = (x / metrics.advance).round() as usize;
            (2 - column % 2) as f32 * metrics.advance
        } else if is_wide(ch) {
            metrics.wide
        } else {
            metrics.advance
        }
    };
    let mut breaks = Vec::new();
    let mut line_start = 0;
    let mut opportunity = None;
    let mut x = 0.0;
    for index in 0..chars.len() {
        let ch = chars[index].1;
        if index > line_start && break_between(chars[index - 1].1, ch) {
            opportunity = Some(index);
        }
        let mut advance = width_of(ch, x);
        if matches!(ch, ' ' | '\t') {
            x += advance;
            continue;
        }
        while x + advance > metrics.width + 0.01 && index > line_start {
            line_start = match opportunity {
                Some(at) if at > line_start => at,
                _ => index,
            };
            breaks.push(chars[line_start].0);
            opportunity = None;
            x = 0.0;
            for &(_, moved) in &chars[line_start..index] {
                x += width_of(moved, x);
            }
            advance = width_of(ch, x);
        }
        x += advance;
    }
    breaks
}

/// Lays `source` out as the cell draws it: unwrapped when `wrap` is `None`.
pub(crate) fn lay_out_cell(source: &str, wrap: Option<WrapMetrics>) -> CellText {
    let breaks = wrap
        .map(|metrics| soft_breaks(source, metrics))
        .unwrap_or_default();
    let mut display = String::with_capacity(source.len() + breaks.len() + 4);
    let mut display_to_source = Vec::with_capacity(source.len() + breaks.len() + 5);
    let mut source_to_display = vec![0; source.len() + 1];
    let mut breaks_left = breaks.iter().copied().peekable();
    let mut column = 0usize;
    for (index, ch) in source.char_indices() {
        if breaks_left.peek() == Some(&index) {
            breaks_left.next();
            display_to_source.push(index);
            display.push('\n');
            column = 0;
        }
        for slot in &mut source_to_display[index..index + ch.len_utf8()] {
            *slot = display.len();
        }
        if ch == '\t' {
            let spaces = 2 - column % 2;
            for _ in 0..spaces {
                display_to_source.push(index);
                display.push(' ');
            }
            column += spaces;
        } else {
            for _ in 0..ch.len_utf8() {
                display_to_source.push(index);
            }
            display.push(ch);
            column += if is_wide(ch) { 2 } else { 1 };
        }
    }
    source_to_display[source.len()] = display.len();
    display_to_source.push(source.len());
    CellText {
        display,
        display_to_source,
        source_to_display,
        multi_row: !breaks.is_empty(),
    }
}
