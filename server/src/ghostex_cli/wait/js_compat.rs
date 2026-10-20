use serde_json::Value;

/// JS truthiness of an optional JSON value (missing/undefined → false).
pub(super) fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// JS String(value) coercion for JSON values.
pub(super) fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int.to_string()
            } else if let Some(int) = number.as_u64() {
                int.to_string()
            } else {
                js_f64_string(number.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — null/undefined become "".
pub(super) fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(other) => js_string(other),
    }
}

/// Template interpolation of a possibly-missing value (undefined → "undefined").
pub(super) fn js_display(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(other) => js_string(other),
    }
}

/// JS number-to-string for finite doubles (integers print without a decimal).
pub(super) fn js_f64_string(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

pub(super) mod js_regex {
    /*!
    Private ECMAScript-flavored regex engine for `wait-for-text` (the `regex`
    crate is not a dependency and `new RegExp(pattern)` accepts user-supplied
    JS patterns). Backtracking matcher supporting the subset agents actually
    use in sentinel patterns: literals and escaped literals, `.`, `^`, `$`,
    `\b`/`\B`, `\d \D \w \W \s \S`, character classes with ranges and negation,
    groups `( )`, `(?: )`, named groups, lookahead `(?= ) (?! )`, alternation
    `|`, and quantifiers `* + ? {n} {n,} {n,m}` with lazy `?` variants.
    Unsupported (compile error): backreferences `\1`-`\9` and lookbehind.
    `test()` is unanchored per-line search like JS RegExp.prototype.test.
    */

    #[derive(Debug)]
    pub struct Regex {
        root: Node,
    }

    #[derive(Debug)]
    enum Node {
        Char(char),
        Any,
        Class {
            negated: bool,
            items: Vec<ClassItem>,
        },
        StartAnchor,
        EndAnchor,
        WordBoundary {
            negated: bool,
        },
        Seq(Vec<Node>),
        Alt(Vec<Node>),
        Repeat {
            node: Box<Node>,
            min: u32,
            max: Option<u32>,
            lazy: bool,
        },
        Look {
            negated: bool,
            node: Box<Node>,
        },
    }

    #[derive(Debug, Clone, Copy)]
    enum ClassItem {
        Char(char),
        Range(char, char),
        Digit,
        NotDigit,
        Word,
        NotWord,
        Space,
        NotSpace,
    }

    impl Regex {
        pub fn new(pattern: &str) -> Result<Regex, String> {
            let mut parser = Parser {
                chars: pattern.chars().collect(),
                pos: 0,
            };
            let root = parser
                .parse_alternation()
                .map_err(|detail| format!("Invalid regular expression: /{pattern}/: {detail}"))?;
            if parser.pos < parser.chars.len() {
                // Only an unmatched ')' can stop the top-level parse.
                return Err(format!(
                    "Invalid regular expression: /{pattern}/: Unmatched ')'"
                ));
            }
            Ok(Regex { root })
        }

        pub fn test(&self, text: &str) -> bool {
            let chars: Vec<char> = text.chars().collect();
            (0..=chars.len()).any(|start| match_node(&self.root, &chars, start, &mut |_| true))
        }
    }

    struct Parser {
        chars: Vec<char>,
        pos: usize,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.pos).copied()
        }

        fn advance(&mut self) -> Option<char> {
            let character = self.peek();
            if character.is_some() {
                self.pos += 1;
            }
            character
        }

        fn eat(&mut self, expected: char) -> bool {
            if self.peek() == Some(expected) {
                self.pos += 1;
                true
            } else {
                false
            }
        }

        fn parse_alternation(&mut self) -> Result<Node, String> {
            let mut branches = vec![self.parse_sequence()?];
            while self.eat('|') {
                branches.push(self.parse_sequence()?);
            }
            if branches.len() == 1 {
                Ok(branches.pop().expect("one branch"))
            } else {
                Ok(Node::Alt(branches))
            }
        }

        fn parse_sequence(&mut self) -> Result<Node, String> {
            let mut nodes: Vec<Node> = Vec::new();
            loop {
                match self.peek() {
                    None | Some('|') | Some(')') => break,
                    _ => {}
                }
                let atom = self.parse_atom()?;
                let node = match self.parse_quantifier()? {
                    Some((min, max)) => {
                        // V8: quantifiers on ^ $ \b \B are "Nothing to repeat"
                        // (lookaheads stay quantifiable per Annex B).
                        if matches!(
                            atom,
                            Node::StartAnchor | Node::EndAnchor | Node::WordBoundary { .. }
                        ) {
                            return Err("Nothing to repeat".to_string());
                        }
                        let lazy = self.eat('?');
                        Node::Repeat {
                            node: Box::new(atom),
                            min,
                            max,
                            lazy,
                        }
                    }
                    None => atom,
                };
                nodes.push(node);
            }
            Ok(Node::Seq(nodes))
        }

        fn parse_atom(&mut self) -> Result<Node, String> {
            match self.advance().expect("caller checked peek") {
                '(' => self.parse_group(),
                '^' => Ok(Node::StartAnchor),
                '$' => Ok(Node::EndAnchor),
                '.' => Ok(Node::Any),
                '[' => self.parse_class(),
                '\\' => self.parse_escape_atom(),
                '*' | '+' | '?' => Err("Nothing to repeat".to_string()),
                '{' => {
                    // V8: a brace that forms a valid quantifier with nothing
                    // before it is "Nothing to repeat"; otherwise it is a
                    // literal '{' (e.g. "a{", "{x}", "{,3}").
                    self.pos -= 1;
                    if self.parse_quantifier()?.is_some() {
                        return Err("Nothing to repeat".to_string());
                    }
                    self.pos += 1;
                    Ok(Node::Char('{'))
                }
                // '}' and ']' fall through as literals like Annex B.
                other => Ok(Node::Char(other)),
            }
        }

        fn parse_group(&mut self) -> Result<Node, String> {
            if self.eat('?') {
                match self.peek() {
                    Some(':') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(inner)
                    }
                    Some('=') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(Node::Look {
                            negated: false,
                            node: Box::new(inner),
                        })
                    }
                    Some('!') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(Node::Look {
                            negated: true,
                            node: Box::new(inner),
                        })
                    }
                    Some('<') => {
                        self.pos += 1;
                        match self.peek() {
                            Some('=') | Some('!') => Err(
                                "lookbehind assertions are not supported by this CLI's regex engine"
                                    .to_string(),
                            ),
                            _ => {
                                // (?<name>...) named capture: treat as a plain group.
                                while let Some(character) = self.advance() {
                                    if character == '>' {
                                        break;
                                    }
                                }
                                let inner = self.parse_alternation()?;
                                self.expect_group_close()?;
                                Ok(inner)
                            }
                        }
                    }
                    _ => Err("Invalid group".to_string()),
                }
            } else {
                let inner = self.parse_alternation()?;
                self.expect_group_close()?;
                Ok(inner)
            }
        }

        fn expect_group_close(&mut self) -> Result<(), String> {
            if self.eat(')') {
                Ok(())
            } else {
                Err("Unterminated group".to_string())
            }
        }

        fn parse_quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, String> {
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    Ok(Some((0, None)))
                }
                Some('+') => {
                    self.pos += 1;
                    Ok(Some((1, None)))
                }
                Some('?') => {
                    self.pos += 1;
                    Ok(Some((0, Some(1))))
                }
                Some('{') => {
                    let saved = self.pos;
                    self.pos += 1;
                    let Some(min) = self.parse_decimal() else {
                        self.pos = saved;
                        return Ok(None);
                    };
                    let max = if self.eat(',') {
                        self.parse_decimal()
                    } else {
                        Some(min)
                    };
                    if !self.eat('}') {
                        self.pos = saved;
                        return Ok(None);
                    }
                    if let Some(upper) = max {
                        if upper < min {
                            return Err("numbers out of order in {} quantifier".to_string());
                        }
                    }
                    Ok(Some((min, max)))
                }
                _ => Ok(None),
            }
        }

        fn parse_decimal(&mut self) -> Option<u32> {
            let start = self.pos;
            let mut value: u64 = 0;
            while let Some(digit) = self.peek().and_then(|c| c.to_digit(10)) {
                value = (value * 10 + digit as u64).min(u32::MAX as u64);
                self.pos += 1;
            }
            if self.pos == start {
                None
            } else {
                Some(value as u32)
            }
        }

        fn parse_escape_atom(&mut self) -> Result<Node, String> {
            let Some(escaped) = self.advance() else {
                return Err("\\ at end of pattern".to_string());
            };
            match escaped {
                'd' => Ok(shorthand_class(ClassItem::Digit)),
                'D' => Ok(shorthand_class(ClassItem::NotDigit)),
                'w' => Ok(shorthand_class(ClassItem::Word)),
                'W' => Ok(shorthand_class(ClassItem::NotWord)),
                's' => Ok(shorthand_class(ClassItem::Space)),
                'S' => Ok(shorthand_class(ClassItem::NotSpace)),
                'b' => Ok(Node::WordBoundary { negated: false }),
                'B' => Ok(Node::WordBoundary { negated: true }),
                '1'..='9' => Err(
                    "backreferences (\\1-\\9) are not supported by this CLI's regex engine"
                        .to_string(),
                ),
                'c' => match self.peek() {
                    Some(letter) if letter.is_ascii_alphabetic() => {
                        self.pos += 1;
                        Ok(Node::Char((((letter as u8) & 0x1f) as u8) as char))
                    }
                    _ => Ok(Node::Seq(vec![Node::Char('\\'), Node::Char('c')])),
                },
                other => Ok(Node::Char(self.escaped_char(other))),
            }
        }

        fn escaped_char(&mut self, escaped: char) -> char {
            match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'f' => '\u{c}',
                'v' => '\u{b}',
                '0' => '\0',
                'x' => self.parse_hex_escape(2).unwrap_or('x'),
                'u' => self.parse_hex_escape(4).unwrap_or('u'),
                other => other,
            }
        }

        fn parse_hex_escape(&mut self, digits: usize) -> Option<char> {
            let saved = self.pos;
            let mut value: u32 = 0;
            for _ in 0..digits {
                let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) else {
                    self.pos = saved;
                    return None;
                };
                value = value * 16 + digit;
                self.pos += 1;
            }
            match char::from_u32(value) {
                Some(character) => Some(character),
                None => {
                    self.pos = saved;
                    None
                }
            }
        }

        fn parse_class(&mut self) -> Result<Node, String> {
            let negated = self.eat('^');
            let mut items: Vec<ClassItem> = Vec::new();
            loop {
                let Some(character) = self.peek() else {
                    return Err("Unterminated character class".to_string());
                };
                if character == ']' {
                    self.pos += 1;
                    break;
                }
                let first = self.parse_class_element()?;
                if let ClassItem::Char(low) = first {
                    let dash_next = self.peek() == Some('-')
                        && self.chars.get(self.pos + 1).copied() != Some(']')
                        && self.chars.get(self.pos + 1).is_some();
                    if dash_next {
                        self.pos += 1; // consume '-'
                        let second = self.parse_class_element()?;
                        match second {
                            ClassItem::Char(high) => {
                                if (high as u32) < (low as u32) {
                                    return Err("Range out of order in character class".to_string());
                                }
                                items.push(ClassItem::Range(low, high));
                                continue;
                            }
                            other => {
                                // Annex B: shorthand adjacent to '-' keeps '-' literal.
                                items.push(ClassItem::Char(low));
                                items.push(ClassItem::Char('-'));
                                items.push(other);
                                continue;
                            }
                        }
                    }
                }
                items.push(first);
            }
            Ok(Node::Class { negated, items })
        }

        fn parse_class_element(&mut self) -> Result<ClassItem, String> {
            let character = self.advance().expect("caller checked peek");
            if character != '\\' {
                return Ok(ClassItem::Char(character));
            }
            let Some(escaped) = self.advance() else {
                return Err("\\ at end of pattern".to_string());
            };
            Ok(match escaped {
                'd' => ClassItem::Digit,
                'D' => ClassItem::NotDigit,
                'w' => ClassItem::Word,
                'W' => ClassItem::NotWord,
                's' => ClassItem::Space,
                'S' => ClassItem::NotSpace,
                'b' => ClassItem::Char('\u{8}'),
                '0'..='7' => {
                    // Octal escape (Annex B), up to 3 octal digits total.
                    let mut value = escaped.to_digit(8).expect("octal digit");
                    let mut count = 1;
                    while count < 3 {
                        let Some(digit) = self.peek().and_then(|c| c.to_digit(8)) else {
                            break;
                        };
                        value = value * 8 + digit;
                        self.pos += 1;
                        count += 1;
                    }
                    ClassItem::Char(char::from_u32(value).unwrap_or('\0'))
                }
                other => ClassItem::Char(self.escaped_char(other)),
            })
        }
    }

    fn shorthand_class(item: ClassItem) -> Node {
        Node::Class {
            negated: false,
            items: vec![item],
        }
    }

    fn match_node(
        node: &Node,
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match node {
            Node::Char(expected) => pos < chars.len() && chars[pos] == *expected && cont(pos + 1),
            Node::Any => pos < chars.len() && !is_line_terminator(chars[pos]) && cont(pos + 1),
            Node::Class { negated, items } => {
                pos < chars.len() && (class_matches(items, chars[pos]) != *negated) && cont(pos + 1)
            }
            Node::StartAnchor => pos == 0 && cont(pos),
            Node::EndAnchor => pos == chars.len() && cont(pos),
            Node::WordBoundary { negated } => {
                let before = pos > 0 && is_word_char(chars[pos - 1]);
                let after = pos < chars.len() && is_word_char(chars[pos]);
                ((before != after) != *negated) && cont(pos)
            }
            Node::Seq(nodes) => match_seq(nodes, chars, pos, cont),
            Node::Alt(branches) => {
                for branch in branches {
                    if match_node(branch, chars, pos, &mut *cont) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat {
                node,
                min,
                max,
                lazy,
            } => match_repeat(node, *min, *max, *lazy, 0, chars, pos, cont),
            Node::Look { negated, node } => {
                let matched = match_node(node, chars, pos, &mut |_| true);
                (matched != *negated) && cont(pos)
            }
        }
    }

    fn match_seq(
        nodes: &[Node],
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match nodes.split_first() {
            None => cont(pos),
            Some((first, rest)) => match_node(first, chars, pos, &mut |next| {
                match_seq(rest, chars, next, cont)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_repeat(
        node: &Node,
        min: u32,
        max: Option<u32>,
        lazy: bool,
        count: u32,
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        let can_more = max.is_none_or(|limit| count < limit);
        if lazy {
            if count >= min && cont(pos) {
                return true;
            }
            if can_more {
                return match_node(node, chars, pos, &mut |next| {
                    if next == pos && count + 1 >= min {
                        // Zero-width iteration adds nothing once min is reached.
                        return false;
                    }
                    match_repeat(node, min, max, lazy, count + 1, chars, next, cont)
                });
            }
            false
        } else {
            if can_more
                && match_node(node, chars, pos, &mut |next| {
                    if next == pos {
                        // Zero-width progress: stop expanding to avoid loops,
                        // but finish through the continuation once min is met.
                        if count + 1 >= min {
                            return cont(pos);
                        }
                    }
                    match_repeat(node, min, max, lazy, count + 1, chars, next, cont)
                })
            {
                return true;
            }
            count >= min && cont(pos)
        }
    }

    fn class_matches(items: &[ClassItem], character: char) -> bool {
        items.iter().any(|item| match item {
            ClassItem::Char(expected) => character == *expected,
            ClassItem::Range(low, high) => (*low..=*high).contains(&character),
            ClassItem::Digit => character.is_ascii_digit(),
            ClassItem::NotDigit => !character.is_ascii_digit(),
            ClassItem::Word => is_word_char(character),
            ClassItem::NotWord => !is_word_char(character),
            ClassItem::Space => is_js_whitespace(character),
            ClassItem::NotSpace => !is_js_whitespace(character),
        })
    }

    fn is_word_char(character: char) -> bool {
        character.is_ascii_alphanumeric() || character == '_'
    }

    fn is_line_terminator(character: char) -> bool {
        matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}')
    }

    /// JS \s character class membership.
    fn is_js_whitespace(character: char) -> bool {
        matches!(
            character,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
        )
    }
}
