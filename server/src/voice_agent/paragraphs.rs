//! The paragraphs a long agent caption is drawn in: its `text` split at sentence ends, sent beside
//! the text as the caption frame's `paragraphs`.
//!
//! CDXC:VoiceMode 2026-10-24 DECISION: User, asked what the unreadable voice text looked like, picked "A wall of text", and for the fix picked "Both": long agent replies split into short paragraphs at sentence ends on display only, plus more room between rows and a run of the same tool shown as one row. The caption's `text`, the call log and the memory summary keep the reply as one string; only the drawing breaks it.
//! CDXC:VoiceMode 2026-10-24 WHY: gxserver computes the paragraphs once for every client (the desktop panel, the GPUI web build, the phone) so the break rule cannot drift between two copies. Spoken replies carry no line breaks (the caption is a transcript of the audio; a live Grok call showed each delta brings its own leading space, " started writing a small client…"), so sentence ends are the only place to break. Breaks depend only on the text before them, so a growing caption never moves a break it already showed.

/// Shorter replies stay one paragraph.
const SPLIT_FROM_CHARS: usize = 240;
/// A paragraph ends at the first sentence end after this many characters.
const PARAGRAPH_MIN_CHARS: usize = 100;
/// Words ending in a dot that do not end a sentence.
const TITLES: [&str; 9] = ["Mr", "Mrs", "Ms", "Dr", "Prof", "Sr", "Jr", "St", "vs"];

/// `text` in paragraphs, or one paragraph when it is short or has no sentence end to break at.
pub(super) fn paragraphs(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.chars().count() < SPLIT_FROM_CHARS {
        return vec![text.to_string()];
    }
    let mut paragraphs = Vec::new();
    let mut start = 0;
    for end in sentence_ends(text) {
        if text[start..end].trim().chars().count() >= PARAGRAPH_MIN_CHARS {
            paragraphs.push(text[start..end].trim().to_string());
            start = end;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        paragraphs.push(rest.to_string());
    }
    paragraphs
}

/// Byte offsets just after each sentence end that the next sentence is known to follow: `.`, `!`,
/// `?` or `…` (and any closing quote or bracket), then a space, then a capital, digit or opening
/// quote.
fn sentence_ends(text: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut ends = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (_, c) = chars[i];
        if !matches!(c, '.' | '!' | '?' | '…') || (c == '.' && is_abbreviation(text, chars[i].0))
        {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len()
            && matches!(
                chars[j].1,
                '.' | '!' | '?' | '"' | '\'' | '”' | '’' | ')' | ']'
            )
        {
            j += 1;
        }
        let end = chars.get(j).map_or(text.len(), |(at, _)| *at);
        let mut k = j;
        while k < chars.len() && chars[k].1.is_whitespace() {
            k += 1;
        }
        let starts_sentence = chars.get(k).is_some_and(|(_, next)| {
            next.is_uppercase()
                || next.is_ascii_digit()
                || matches!(next, '"' | '“' | '‘' | '\'' | '(')
        });
        if k > j && starts_sentence {
            ends.push(end);
        }
        i = j.max(i + 1);
    }
    ends
}

/// The dot at `dot` ends a title ("Dr."), an initial ("J."), or a dotted short form ("e.g.").
fn is_abbreviation(text: &str, dot: usize) -> bool {
    let word = text[..dot]
        .rsplit(|c: char| c.is_whitespace() || c == '(' || c == '"')
        .next()
        .unwrap_or_default();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    TITLES.contains(&word)
        || (letters == 1 && word.chars().count() == 1)
        || (word.contains('.') && word.split('.').all(|part| part.chars().count() <= 2))
}

#[cfg(test)]
mod tests {
    use super::paragraphs;

    /// The reply from the call the user found unreadable (2026-10-24), in full.
    const WALL: &str = "One is still working: Cap CLI demo video cleanup, which is scanning a video for QR codes right now, so leave that one. Of the twenty sleeping, five have no messages at all: Diagnose MacBook overheating, Simplify terminal prompt styling, Create vpm-uae project, Reduce overlapping window shadows, and Diagnose macOS hover issue. Those are safe to close. Another batch finished cleanly with nothing pending: Clipboard Screenshot Description, Sonobus audio routing, Time-series database comparison, Windows game launch on Mac, Agentation iframe selection, Tart VM performance optimization, Tart RAM GPU heat alternatives, Main.md questions and notes, fix-files-drive-detection-mac, Monthly doc creation for July, and Publish package to Microsoft Store. All done, close them. Three still have open questions you never answered: CPU usage investigation, VM Optimization Tips, and Disk cleanup. You can close those too if you've moved on, or I can read you the options. Cleanshot crashes replied an hour ago offering to clear a quarantine flag, so that one's worth a look before closing.";

    #[test]
    fn a_long_reply_breaks_at_sentence_ends() {
        let parts = paragraphs(WALL);
        assert_eq!(
            parts,
            vec![
                "One is still working: Cap CLI demo video cleanup, which is scanning a video for QR codes right now, so leave that one.",
                "Of the twenty sleeping, five have no messages at all: Diagnose MacBook overheating, Simplify terminal prompt styling, Create vpm-uae project, Reduce overlapping window shadows, and Diagnose macOS hover issue.",
                "Those are safe to close. Another batch finished cleanly with nothing pending: Clipboard Screenshot Description, Sonobus audio routing, Time-series database comparison, Windows game launch on Mac, Agentation iframe selection, Tart VM performance optimization, Tart RAM GPU heat alternatives, Main.md questions and notes, fix-files-drive-detection-mac, Monthly doc creation for July, and Publish package to Microsoft Store.",
                "All done, close them. Three still have open questions you never answered: CPU usage investigation, VM Optimization Tips, and Disk cleanup.",
                "You can close those too if you've moved on, or I can read you the options. Cleanshot crashes replied an hour ago offering to clear a quarantine flag, so that one's worth a look before closing.",
            ]
        );
        assert_eq!(parts.join(" "), WALL, "only the breaks are new");
    }

    #[test]
    fn a_short_reply_stays_one_paragraph() {
        let text = "Main Coordinator is back with that same question: deploy ghostex.dev now with the pairing page and the Android and iPhone app-link files, recommended yes. Want me to answer it?";
        assert_eq!(paragraphs(text), vec![text]);
    }

    #[test]
    fn a_growing_caption_keeps_the_breaks_it_showed() {
        let mut shown: Vec<String> = Vec::new();
        for end in (1..=WALL.len()).filter(|at| WALL.is_char_boundary(*at)) {
            let parts = paragraphs(&WALL[..end]);
            // Every finished paragraph shown before stays exactly as it was.
            for (before, now) in shown.iter().zip(&parts).take(shown.len().saturating_sub(1)) {
                assert_eq!(before, now, "at {end}");
            }
            shown = parts;
        }
    }

    #[test]
    fn abbreviations_and_dotted_names_do_not_break() {
        let text = "Dr. Smith said the page is on ghostex.dev now, and e.g. the pairing link works for J. Doe already, which took a long while to set up and test on both phones properly today, with the camera opening Ghostex straight away on each of them once the two app-link files were live. It is done.";
        let parts = paragraphs(text);
        assert_eq!(parts.len(), 2, "{parts:?}");
        assert!(parts[0].starts_with("Dr. Smith said"));
        assert_eq!(parts[1], "It is done.");
    }

    #[test]
    fn no_sentence_end_means_one_paragraph() {
        let text = "and then ".repeat(40);
        assert_eq!(paragraphs(&text), vec![text.trim().to_string()]);
    }
}
