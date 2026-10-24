//! Just enough of a POSIX shell reader to see what a command line an agent ran hands to whom: its
//! simple commands with their words unquoted, the file each one writes, and the here-document each
//! one reads.
//!
//! It never runs or expands anything. A `$(cat <<'EOF' … EOF)` substitution reads as the text of its
//! here-document, because that is how agents pass a multi-line argument; any other substitution
//! stays as the text the agent wrote.

/// One simple command: `cat > /tmp/note.md <<'EOF'`, `ghostex agents send S1:P2:G3 "hi"`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellCommand {
    /// The command's words, unquoted, without its redirections.
    pub words: Vec<String>,
    /// The file standard output goes to (`> path`, `>> path`, `&> path`), when one is named.
    pub stdout_file: Option<String>,
    /// The here-document fed to standard input, without its delimiter lines.
    pub heredoc: Option<String>,
    /// True when a single `|` feeds this command the previous one's output (`… | head`), so a
    /// summary of the line can tell a pipeline's filters from the command doing the work.
    pub piped: bool,
}

/// The simple commands of a script, in the order they appear.
pub fn parse_shell(script: &str) -> Vec<ShellCommand> {
    let chars: Vec<char> = script.chars().collect();
    Reader {
        chars: &chars,
        at: 0,
    }
    .script(false)
}

enum Redirect {
    Stdout,
    Other,
}

#[derive(Default)]
struct Builder {
    commands: Vec<(ShellCommand, Option<usize>)>,
    current: ShellCommand,
    heredoc_slot: Option<usize>,
    word: Option<String>,
    redirect: Option<Redirect>,
}

impl Builder {
    fn word_mut(&mut self) -> &mut String {
        self.word.get_or_insert_with(String::new)
    }

    fn finish_word(&mut self) {
        let Some(word) = self.word.take() else {
            return;
        };
        match self.redirect.take() {
            Some(Redirect::Stdout) => self.current.stdout_file = Some(word),
            Some(Redirect::Other) => {}
            None => self.current.words.push(word),
        }
    }

    fn end_command(&mut self) {
        self.finish_word();
        self.redirect = None;
        let command = std::mem::take(&mut self.current);
        let slot = self.heredoc_slot.take();
        if !command.words.is_empty() || command.stdout_file.is_some() || slot.is_some() {
            self.commands.push((command, slot));
        }
    }
}

/// A here-document whose body starts on the line after the one that opened it.
struct PendingHeredoc {
    delimiter: String,
    strip_tabs: bool,
    slot: usize,
}

struct Reader<'a> {
    chars: &'a [char],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    /// A script, or the inside of a `$( … )` when `nested`, which ends at its unmatched `)`.
    fn script(&mut self, nested: bool) -> Vec<ShellCommand> {
        let mut builder = Builder::default();
        let mut bodies: Vec<String> = Vec::new();
        let mut pending: Vec<PendingHeredoc> = Vec::new();
        let mut depth = 0usize;
        while let Some(character) = self.peek() {
            match character {
                ' ' | '\t' | '\r' => {
                    builder.finish_word();
                    self.at += 1;
                }
                '\n' => {
                    builder.end_command();
                    self.at += 1;
                    self.read_heredocs(&mut pending, &mut bodies, nested);
                }
                '#' if builder.word.is_none() => {
                    while self.peek().is_some_and(|next| next != '\n') {
                        self.at += 1;
                    }
                }
                '\\' => {
                    match self.peek_at(1) {
                        Some('\n') => {}
                        Some(next) => builder.word_mut().push(next),
                        None => builder.word_mut().push('\\'),
                    }
                    self.at += 2;
                }
                '\'' => {
                    self.at += 1;
                    let text = self.until('\'');
                    builder.word_mut().push_str(&text);
                }
                '"' => {
                    self.at += 1;
                    let text = self.double_quoted();
                    builder.word_mut().push_str(&text);
                }
                '$' if self.peek_at(1) == Some('\'') => {
                    self.at += 2;
                    let text = self.ansi_c_quoted();
                    builder.word_mut().push_str(&text);
                }
                '$' if self.peek_at(1) == Some('(') && self.peek_at(2) != Some('(') => {
                    let text = self.substitution();
                    builder.word_mut().push_str(&text);
                }
                '`' => {
                    let start = self.at;
                    self.at += 1;
                    self.until('`');
                    let raw: String = self.chars[start..self.at].iter().collect();
                    builder.word_mut().push_str(&raw);
                }
                '&' if self.peek_at(1) == Some('>') => {
                    builder.finish_word();
                    self.at += 2;
                    if self.peek() == Some('>') {
                        self.at += 1;
                    }
                    builder.redirect = Some(Redirect::Stdout);
                }
                ';' | '&' | '|' => {
                    builder.end_command();
                    if character == '|' {
                        if self.peek_at(1) == Some('|') {
                            self.at += 1;
                        } else {
                            builder.current.piped = true;
                        }
                    }
                    self.at += 1;
                }
                '(' => {
                    builder.end_command();
                    depth += 1;
                    self.at += 1;
                }
                ')' => {
                    builder.end_command();
                    self.at += 1;
                    if depth == 0 {
                        if nested {
                            break;
                        }
                    } else {
                        depth -= 1;
                    }
                }
                '>' | '<' => {
                    // `2>&1`: an unspaced number before a redirection is a file descriptor.
                    if builder.word.as_deref().is_some_and(|word| {
                        !word.is_empty() && word.chars().all(|digit| digit.is_ascii_digit())
                    }) {
                        builder.word = None;
                    } else {
                        builder.finish_word();
                    }
                    self.redirection(&mut builder, &mut bodies, &mut pending);
                }
                _ => {
                    builder.word_mut().push(character);
                    self.at += 1;
                }
            }
        }
        builder.end_command();
        builder
            .commands
            .into_iter()
            .map(|(mut command, slot)| {
                command.heredoc = slot.and_then(|slot| bodies.get(slot).cloned());
                command
            })
            .collect()
    }

    /// Everything up to the closing `quote`, which is consumed.
    fn until(&mut self, quote: char) -> String {
        let mut text = String::new();
        while let Some(character) = self.peek() {
            self.at += 1;
            if character == quote {
                break;
            }
            text.push(character);
        }
        text
    }

    fn double_quoted(&mut self) -> String {
        let mut text = String::new();
        while let Some(character) = self.peek() {
            match character {
                '"' => {
                    self.at += 1;
                    break;
                }
                '\\' => match self.peek_at(1) {
                    Some(next @ ('"' | '\\' | '$' | '`')) => {
                        text.push(next);
                        self.at += 2;
                    }
                    Some('\n') => self.at += 2,
                    _ => {
                        text.push('\\');
                        self.at += 1;
                    }
                },
                '$' if self.peek_at(1) == Some('(') && self.peek_at(2) != Some('(') => {
                    text.push_str(&self.substitution());
                }
                _ => {
                    text.push(character);
                    self.at += 1;
                }
            }
        }
        text
    }

    fn ansi_c_quoted(&mut self) -> String {
        let mut text = String::new();
        while let Some(character) = self.peek() {
            self.at += 1;
            match character {
                '\'' => break,
                '\\' => {
                    let Some(next) = self.peek() else {
                        break;
                    };
                    self.at += 1;
                    text.push(match next {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                }
                other => text.push(other),
            }
        }
        text
    }

    /// `$( … )` starting at `$`: the here-document a lone `cat` reads, else the text as written.
    fn substitution(&mut self) -> String {
        let start = self.at;
        self.at += 2;
        let inner = self.script(true);
        if let [command] = inner.as_slice() {
            if command.words == ["cat"] {
                if let Some(body) = &command.heredoc {
                    return body.trim_end_matches('\n').to_string();
                }
            }
        }
        self.chars[start..self.at.min(self.chars.len())]
            .iter()
            .collect()
    }

    fn redirection(
        &mut self,
        builder: &mut Builder,
        bodies: &mut Vec<String>,
        pending: &mut Vec<PendingHeredoc>,
    ) {
        let character = self.peek();
        self.at += 1;
        if character == Some('>') {
            match self.peek() {
                Some('>') | Some('|') => self.at += 1,
                Some('&') => {
                    self.at += 1;
                    if self
                        .peek()
                        .is_some_and(|next| next.is_ascii_digit() || next == '-')
                    {
                        while self
                            .peek()
                            .is_some_and(|next| next.is_ascii_digit() || next == '-')
                        {
                            self.at += 1;
                        }
                        return;
                    }
                }
                _ => {}
            }
            builder.redirect = Some(Redirect::Stdout);
            return;
        }
        match self.peek() {
            Some('<') => {
                self.at += 1;
                if self.peek() == Some('<') {
                    // A here-string: its word is input, not an argument.
                    self.at += 1;
                    builder.redirect = Some(Redirect::Other);
                    return;
                }
                let strip_tabs = self.peek() == Some('-');
                if strip_tabs {
                    self.at += 1;
                }
                while matches!(self.peek(), Some(' ' | '\t')) {
                    self.at += 1;
                }
                let delimiter = self.delimiter();
                let slot = bodies.len();
                bodies.push(String::new());
                pending.push(PendingHeredoc {
                    delimiter,
                    strip_tabs,
                    slot,
                });
                builder.heredoc_slot = Some(slot);
            }
            Some('&') => {
                self.at += 1;
                while self
                    .peek()
                    .is_some_and(|next| next.is_ascii_digit() || next == '-')
                {
                    self.at += 1;
                }
            }
            Some('>') => {
                self.at += 1;
                builder.redirect = Some(Redirect::Other);
            }
            _ => builder.redirect = Some(Redirect::Other),
        }
    }

    /// A here-document's delimiter word, with the quotes that only stop expansion removed.
    fn delimiter(&mut self) -> String {
        let mut delimiter = String::new();
        while let Some(character) = self.peek() {
            if character.is_whitespace() || ";&|<>()".contains(character) {
                break;
            }
            self.at += 1;
            if !matches!(character, '\'' | '"' | '\\') {
                delimiter.push(character);
            }
        }
        delimiter
    }

    /// The bodies of the here-documents the line just ended opened, in order.
    fn read_heredocs(
        &mut self,
        pending: &mut Vec<PendingHeredoc>,
        bodies: &mut [String],
        nested: bool,
    ) {
        for heredoc in pending.drain(..) {
            let mut body = String::new();
            while self.at < self.chars.len() {
                let end = self.chars[self.at..]
                    .iter()
                    .position(|character| *character == '\n')
                    .map_or(self.chars.len(), |offset| self.at + offset);
                let raw: String = self.chars[self.at..end].iter().collect();
                let line = if heredoc.strip_tabs {
                    raw.trim_start_matches('\t').to_string()
                } else {
                    raw
                };
                if line == heredoc.delimiter {
                    self.at = (end + 1).min(self.chars.len());
                    break;
                }
                // `EOF)` closes the substitution on the delimiter's own line.
                if nested && line.strip_suffix(')') == Some(heredoc.delimiter.as_str()) {
                    self.at = end - 1;
                    break;
                }
                body.push_str(&line);
                body.push('\n');
                self.at = (end + 1).min(self.chars.len());
            }
            if let Some(slot) = bodies.get_mut(heredoc.slot) {
                *slot = body;
            }
        }
    }
}
