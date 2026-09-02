//! BBCode-style tag parser for terminal styling: `[tag]content[/tag]`
//! markup with nested tags and multiple output modes (see [`TagTransform`]).
//! Tag names follow CSS identifier rules (`[a-z_][a-z0-9_-]*`); a literal
//! `[`/`]` is written `\[`/`\]`.
//!
//! ```rust
//! use standout_bbparser::{BBParser, TagTransform};
//! use console::Style;
//! use std::collections::HashMap;
//!
//! let mut styles = HashMap::new();
//! styles.insert("bold".to_string(), Style::new().bold());
//!
//! let parser = BBParser::new(styles, TagTransform::Remove);
//! assert_eq!(parser.parse("[bold]hello[/bold]"), "hello");
//! ```

use console::{AnsiCodeIterator, Style};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagTransform {
    Apply,
    Remove,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownTagBehavior {
    #[default]
    Passthrough,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownTagKind {
    Open,
    Close,
    Unbalanced,
    UnexpectedClose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTagError {
    pub tag: String,
    pub kind: UnknownTagKind,
    pub start: usize,
    pub end: usize,
}

impl std::fmt::Display for UnknownTagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            UnknownTagKind::Open => "unknown opening",
            UnknownTagKind::Close => "unknown closing",
            UnknownTagKind::Unbalanced => "unbalanced",
            UnknownTagKind::UnexpectedClose => "unexpected closing",
        };
        write!(
            f,
            "{} tag '{}' at position {}..{}",
            kind, self.tag, self.start, self.end
        )
    }
}

impl std::error::Error for UnknownTagError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownTagErrors {
    pub errors: Vec<UnknownTagError>,
}

impl UnknownTagErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    pub fn push(&mut self, error: UnknownTagError) {
        self.errors.push(error);
    }
}

impl std::fmt::Display for UnknownTagErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "found {} unknown tag(s):", self.errors.len())?;
        for error in &self.errors {
            writeln!(f, "  - {}", error)?;
        }
        Ok(())
    }
}

impl std::error::Error for UnknownTagErrors {}

impl IntoIterator for UnknownTagErrors {
    type Item = UnknownTagError;
    type IntoIter = std::vec::IntoIter<UnknownTagError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl<'a> IntoIterator for &'a UnknownTagErrors {
    type Item = &'a UnknownTagError;
    type IntoIter = std::slice::Iter<'a, UnknownTagError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.iter()
    }
}

pub fn strip_tags(input: &str) -> String {
    let parser = BBParser::new(HashMap::new(), TagTransform::Remove)
        .unknown_behavior(UnknownTagBehavior::Strip);
    parser.parse(input)
}

// Style tags stay out of the visible character stream; selecting a range
// preserves covering styles and closes every emitted tag.
#[derive(Debug, Clone)]
pub struct StyledText<'a> {
    events: Vec<StyledEvent<'a>>,
}

#[derive(Debug, Clone)]
enum StyledEvent<'a> {
    Text {
        source: &'a str,
        unescape_brackets: bool,
    },
    OpenTag(&'a str),
    CloseTag(&'a str),
}

impl<'a> StyledText<'a> {
    pub fn parse(input: &'a str) -> Self {
        let tokens = Tokenizer::new(input).collect::<Vec<_>>();
        let valid_opens = compute_valid_tags(&tokens);
        let mut events = Vec::new();
        let mut stack: Vec<&str> = Vec::new();

        for (index, token) in tokens.iter().enumerate() {
            match token {
                Token::Text { content, .. } => events.push(StyledEvent::Text {
                    source: content,
                    unescape_brackets: true,
                }),
                Token::OpenTag { name, .. } if valid_opens.contains(&index) => {
                    stack.push(name);
                    events.push(StyledEvent::OpenTag(name));
                }
                Token::OpenTag { start, end, .. } => events.push(StyledEvent::Text {
                    source: &input[*start..*end],
                    unescape_brackets: false,
                }),
                Token::CloseTag { name, .. } if stack.last().copied() == Some(*name) => {
                    stack.pop();
                    events.push(StyledEvent::CloseTag(name));
                }
                Token::CloseTag { name, .. } if stack.contains(name) => {
                    while let Some(open) = stack.pop() {
                        events.push(StyledEvent::CloseTag(open));
                        if open == *name {
                            break;
                        }
                    }
                }
                Token::CloseTag { start, end, .. } => events.push(StyledEvent::Text {
                    source: &input[*start..*end],
                    unescape_brackets: false,
                }),
                Token::InvalidTag { content, .. } => events.push(StyledEvent::Text {
                    source: content,
                    unescape_brackets: false,
                }),
            }
        }

        while let Some(tag) = stack.pop() {
            events.push(StyledEvent::CloseTag(tag));
        }

        Self { events }
    }

    pub fn visit_visible_chars(&self, mut visitor: impl FnMut(char)) {
        for event in &self.events {
            if let StyledEvent::Text {
                source,
                unescape_brackets,
            } = event
            {
                visit_text_units(source, *unescape_brackets, |character, _| {
                    if let Some(character) = character {
                        visitor(character);
                    }
                });
            }
        }
    }

    // `separator` is inserted outside styling between non-empty ranges.
    pub fn select(&self, ranges: &[Range<usize>], separator: &str) -> String {
        let mut result = String::new();
        let mut rendered_any = false;

        for range in ranges.iter().filter(|range| range.start < range.end) {
            if rendered_any {
                result.push_str(separator);
            }
            result.push_str(&self.render_range(range.clone()));
            rendered_any = true;
        }

        result
    }

    pub fn select_range(&self, range: Range<usize>) -> String {
        self.render_range(range)
    }

    fn render_range(&self, range: Range<usize>) -> String {
        let mut result = String::new();
        let mut source_stack: Vec<&str> = Vec::new();
        let mut output_stack: Vec<&str> = Vec::new();
        let mut visible_index = 0;
        let mut started = false;

        for event in &self.events {
            match event {
                StyledEvent::OpenTag(tag) => {
                    source_stack.push(tag);
                    if started && visible_index < range.end {
                        push_open_tag(&mut result, tag);
                        output_stack.push(tag);
                    }
                }
                StyledEvent::CloseTag(tag) => {
                    source_stack.pop();
                    if started && output_stack.last().copied() == Some(*tag) {
                        push_close_tag(&mut result, tag);
                        output_stack.pop();
                    }
                }
                StyledEvent::Text {
                    source,
                    unescape_brackets,
                } => {
                    visit_text_units(source, *unescape_brackets, |character, raw| {
                        if character.is_none() {
                            if (started && visible_index <= range.end)
                                || (!started
                                    && visible_index == range.start
                                    && range.start < range.end)
                            {
                                if !started {
                                    for tag in &source_stack {
                                        push_open_tag(&mut result, tag);
                                        output_stack.push(tag);
                                    }
                                    started = true;
                                }
                                result.push_str(raw);
                            }
                            return;
                        }
                        if visible_index >= range.start && visible_index < range.end {
                            if !started {
                                for tag in &source_stack {
                                    push_open_tag(&mut result, tag);
                                    output_stack.push(tag);
                                }
                                started = true;
                            }
                            result.push_str(raw);
                        }
                        visible_index += 1;
                    });
                }
            }
        }

        for tag in output_stack.into_iter().rev() {
            push_close_tag(&mut result, tag);
        }
        result
    }
}

fn push_open_tag(output: &mut String, tag: &str) {
    output.push('[');
    output.push_str(tag);
    output.push(']');
}

fn push_close_tag(output: &mut String, tag: &str) {
    output.push_str("[/");
    output.push_str(tag);
    output.push(']');
}

fn visit_text_units<'a>(
    source: &'a str,
    unescape_brackets: bool,
    mut visitor: impl FnMut(Option<char>, &'a str),
) {
    for (unit, is_ansi) in AnsiCodeIterator::new(source) {
        if is_ansi {
            visitor(None, unit);
        } else {
            visit_plain_text_units(unit, unescape_brackets, &mut visitor);
        }
    }
}

fn visit_plain_text_units<'a>(
    source: &'a str,
    unescape_brackets: bool,
    visitor: &mut impl FnMut(Option<char>, &'a str),
) {
    let mut indices = source.char_indices().peekable();
    while let Some((start, character)) = indices.next() {
        if unescape_brackets && character == '\\' {
            if let Some(&(next_start, next)) = indices.peek() {
                if next == '[' || next == ']' {
                    indices.next();
                    let end = next_start + next.len_utf8();
                    visitor(Some(next), &source[start..end]);
                    continue;
                }
            }
        }
        let end = start + character.len_utf8();
        visitor(Some(character), &source[start..end]);
    }
}

#[derive(Debug, Clone)]
pub struct BBParser {
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
}

impl BBParser {
    // Styles are used directly; no alias resolution is performed. Unknown
    // tags default to `UnknownTagBehavior::Passthrough`.
    pub fn new(styles: HashMap<String, Style>, transform: TagTransform) -> Self {
        Self {
            styles,
            transform,
            unknown_behavior: UnknownTagBehavior::default(),
        }
    }

    pub fn styles(&self) -> &HashMap<String, Style> {
        &self.styles
    }

    pub fn unknown_behavior(mut self, behavior: UnknownTagBehavior) -> Self {
        self.unknown_behavior = behavior;
        self
    }

    pub fn parse(&self, input: &str) -> String {
        let (output, _) = self.parse_internal(input);
        output
    }

    pub fn parse_with_diagnostics(&self, input: &str) -> (String, UnknownTagErrors) {
        self.parse_internal(input)
    }

    pub fn validate(&self, input: &str) -> Result<(), UnknownTagErrors> {
        let (_, errors) = self.parse_internal(input);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn parse_internal(&self, input: &str) -> (String, UnknownTagErrors) {
        let tokens = Tokenizer::new(input).collect::<Vec<_>>();
        let valid_opens = compute_valid_tags(&tokens);
        let mut events = Vec::new();
        let mut errors = UnknownTagErrors::new();
        let mut stack: Vec<&str> = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Text { content, .. } => {
                    events.push(ParseEvent::Literal(unescape(content)));
                }
                Token::OpenTag { name, start, end } => {
                    if valid_opens.contains(&i) {
                        stack.push(name);
                        self.emit_open_tag_event(&mut events, &mut errors, name, *start, *end);
                    } else {
                        let is_valid_name = Tokenizer::is_valid_tag_name(name);
                        if is_valid_name {
                            errors.push(UnknownTagError {
                                tag: name.to_string(),
                                kind: UnknownTagKind::Unbalanced,
                                start: *start,
                                end: *end,
                            });
                            events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                                "[{}]",
                                name
                            ))));
                        } else {
                            events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                                "[{}]",
                                name
                            ))));
                        }
                    }
                }
                Token::CloseTag { name, start, end } => {
                    if stack.last().copied() == Some(*name) {
                        stack.pop();
                        self.emit_close_tag_event(&mut events, &mut errors, name, *start, *end);
                    } else if stack.contains(name) {
                        while let Some(open) = stack.pop() {
                            self.emit_close_tag_event(&mut events, &mut errors, open, 0, 0);
                            if open == *name {
                                break;
                            }
                        }
                    } else {
                        let is_valid_name = Tokenizer::is_valid_tag_name(name);
                        if is_valid_name {
                            errors.push(UnknownTagError {
                                tag: name.to_string(),
                                kind: UnknownTagKind::UnexpectedClose,
                                start: *start,
                                end: *end,
                            });
                        }
                        events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                            "[/{}]",
                            name
                        ))));
                    }
                }
                Token::InvalidTag { content, .. } => {
                    events.push(ParseEvent::Literal(std::borrow::Cow::Borrowed(content)));
                }
            }
            i += 1;
        }

        while let Some(tag) = stack.pop() {
            self.emit_close_tag_event(&mut events, &mut errors, tag, 0, 0);
        }

        let output = self.render(events);
        (output, errors)
    }

    fn emit_open_tag_event<'a>(
        &self,
        events: &mut Vec<ParseEvent<'a>>,
        errors: &mut UnknownTagErrors,
        tag: &'a str,
        start: usize,
        end: usize,
    ) {
        let is_known = self.styles.contains_key(tag);

        if !is_known {
            errors.push(UnknownTagError {
                tag: tag.to_string(),
                kind: UnknownTagKind::Open,
                start,
                end,
            });
        }

        match self.transform {
            TagTransform::Keep => {
                events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                    "[{}]",
                    tag
                ))));
            }
            TagTransform::Remove => {}
            TagTransform::Apply => {
                if is_known {
                    events.push(ParseEvent::StyleStart(tag));
                } else {
                    match self.unknown_behavior {
                        UnknownTagBehavior::Passthrough => {
                            events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                                "[{}?]",
                                tag
                            ))));
                        }
                        UnknownTagBehavior::Strip => {}
                    }
                }
            }
        }
    }

    fn emit_close_tag_event<'a>(
        &self,
        events: &mut Vec<ParseEvent<'a>>,
        errors: &mut UnknownTagErrors,
        tag: &'a str,
        start: usize,
        end: usize,
    ) {
        let is_known = self.styles.contains_key(tag);

        // `end == 0` marks an auto-closed tag, which is not an error.
        if !is_known && end > 0 {
            errors.push(UnknownTagError {
                tag: tag.to_string(),
                kind: UnknownTagKind::Close,
                start,
                end,
            });
        }

        match self.transform {
            TagTransform::Keep => {
                events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                    "[/{}]",
                    tag
                ))));
            }
            TagTransform::Remove => {}
            TagTransform::Apply => {
                if is_known {
                    events.push(ParseEvent::StyleEnd(tag));
                } else {
                    match self.unknown_behavior {
                        UnknownTagBehavior::Passthrough => {
                            events.push(ParseEvent::Literal(std::borrow::Cow::Owned(format!(
                                "[/{}?]",
                                tag
                            ))));
                        }
                        UnknownTagBehavior::Strip => {}
                    }
                }
            }
        }
    }

    fn render(&self, events: Vec<ParseEvent>) -> String {
        let mut result = String::new();
        let mut style_stack: Vec<&Style> = Vec::new();

        for event in events {
            match event {
                ParseEvent::Literal(text) => {
                    self.append_styled(&mut result, &text, &style_stack);
                }
                ParseEvent::StyleStart(tag) => {
                    if let Some(style) = self.styles.get(tag) {
                        style_stack.push(style);
                    }
                }
                ParseEvent::StyleEnd(tag) => {
                    if self.styles.contains_key(tag) {
                        style_stack.pop();
                    }
                }
            }
        }
        result
    }

    fn append_styled(&self, output: &mut String, text: &str, style_stack: &[&Style]) {
        if text.is_empty() {
            return;
        }

        if style_stack.is_empty() {
            output.push_str(text);
        } else {
            let mut current = text.to_string();
            // Innermost to outermost, so inner styles win (ANSI: last code
            // wins), stripping nested resets along the way.
            for style in style_stack.iter().rev() {
                if current.ends_with("\x1b[0m") {
                    current.truncate(current.len() - 4);
                }
                current = style.apply_to(current).to_string();
            }
            output.push_str(&current);
        }
    }
}

enum ParseEvent<'a> {
    Literal(std::borrow::Cow<'a, str>),
    StyleStart(&'a str),
    StyleEnd(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    Text {
        content: &'a str,
        start: usize,
        end: usize,
    },
    OpenTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    CloseTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    InvalidTag {
        content: &'a str,
        start: usize,
        end: usize,
    },
}

// O(N) instead of O(N^2): pre-computes which OpenTag tokens have a matching
// CloseTag.
fn compute_valid_tags(tokens: &[Token<'_>]) -> std::collections::HashSet<usize> {
    use std::collections::{HashMap, HashSet};
    let mut valid_indices = HashSet::new();
    let mut open_indices_by_tag: HashMap<&str, Vec<usize>> = HashMap::new();

    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::OpenTag { name, .. } => {
                open_indices_by_tag.entry(name).or_default().push(index);
            }
            Token::CloseTag { name, .. } => {
                if let Some(indices) = open_indices_by_tag.get_mut(name) {
                    if let Some(open_index) = indices.pop() {
                        valid_indices.insert(open_index);
                    }
                }
            }
            _ => {}
        }
    }

    valid_indices
}

// ANSI controls are skipped as terminal syntax. Byte-level scanning is safe
// here: `\`, `[`, `]` are ASCII and cannot be UTF-8 continuation bytes.
fn find_unescaped_bracket(s: &str) -> Option<usize> {
    let mut source_offset = 0;
    for (unit, is_ansi) in AnsiCodeIterator::new(s) {
        if !is_ansi {
            let bytes = unit.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    let next = bytes[i + 1];
                    if next == b'[' || next == b']' {
                        i += 2;
                        continue;
                    }
                }
                if bytes[i] == b'[' {
                    return Some(source_offset + i);
                }
                i += 1;
            }
        }
        source_offset += unit.len();
    }
    None
}

// Returns `Cow::Borrowed` when no `\[`/`\]` escape is present, so
// escape-free inputs (Windows paths, regexes) stay allocation-free.
fn unescape(s: &str) -> std::borrow::Cow<'_, str> {
    let bytes = s.as_bytes();
    let has_escape = bytes
        .windows(2)
        .any(|w| w[0] == b'\\' && (w[1] == b'[' || w[1] == b']'));
    if !has_escape {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '[' || next == ']' {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    std::borrow::Cow::Owned(out)
}

struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn is_valid_tag_name(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let mut chars = s.chars();
        let first = chars.next().unwrap();

        if !first.is_ascii_lowercase() && first != '_' {
            return false;
        }

        for c in chars {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' && c != '-' {
                return false;
            }
        }

        true
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.input.len() {
            return None;
        }

        let remaining = &self.input[self.pos..];
        let start_pos = self.pos;

        if let Some(bracket_pos) = find_unescaped_bracket(remaining) {
            if bracket_pos > 0 {
                let text = &remaining[..bracket_pos];
                self.pos += bracket_pos;
                return Some(Token::Text {
                    content: text,
                    start: start_pos,
                    end: self.pos,
                });
            }

            if let Some(close_bracket) = remaining.find(']') {
                let tag_content = &remaining[1..close_bracket];
                let full_tag = &remaining[..=close_bracket];
                let end_pos = start_pos + close_bracket + 1;

                if let Some(tag_name) = tag_content.strip_prefix('/') {
                    if Self::is_valid_tag_name(tag_name) {
                        self.pos = end_pos;
                        Some(Token::CloseTag {
                            name: tag_name,
                            start: start_pos,
                            end: end_pos,
                        })
                    } else {
                        self.pos = end_pos;
                        Some(Token::InvalidTag {
                            content: full_tag,
                            start: start_pos,
                            end: end_pos,
                        })
                    }
                } else if Self::is_valid_tag_name(tag_content) {
                    self.pos = end_pos;
                    Some(Token::OpenTag {
                        name: tag_content,
                        start: start_pos,
                        end: end_pos,
                    })
                } else {
                    self.pos = end_pos;
                    Some(Token::InvalidTag {
                        content: full_tag,
                        start: start_pos,
                        end: end_pos,
                    })
                }
            } else {
                let end_pos = self.input.len();
                self.pos = end_pos;
                Some(Token::Text {
                    content: remaining,
                    start: start_pos,
                    end: end_pos,
                })
            }
        } else {
            let end_pos = self.input.len();
            self.pos = end_pos;
            Some(Token::Text {
                content: remaining,
                start: start_pos,
                end: end_pos,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_styles() -> HashMap<String, Style> {
        let mut styles = HashMap::new();
        styles.insert("bold".to_string(), Style::new().bold());
        styles.insert("red".to_string(), Style::new().red());
        styles.insert("dim".to_string(), Style::new().dim());
        styles.insert("title".to_string(), Style::new().cyan().bold());
        styles.insert("error".to_string(), Style::new().red().bold());
        styles.insert("my_style".to_string(), Style::new().green());
        styles.insert("style-with-dash".to_string(), Style::new().yellow());
        styles
    }

    mod strip_tags_tests {
        use super::super::strip_tags;

        #[test]
        fn strips_known_style_tags() {
            assert_eq!(strip_tags("[bold]hello[/bold]"), "hello");
        }

        #[test]
        fn strips_unknown_tags() {
            assert_eq!(strip_tags("[additions]+32[/additions]"), "+32");
        }

        #[test]
        fn strips_multiple_tags() {
            assert_eq!(
                strip_tags("[additions]+32[/additions]/[deletions]-0[/deletions]/32"),
                "+32/-0/32"
            );
        }

        #[test]
        fn plain_text_unchanged() {
            assert_eq!(strip_tags("no tags here"), "no tags here");
        }

        #[test]
        fn empty_string() {
            assert_eq!(strip_tags(""), "");
        }

        #[test]
        fn nested_tags() {
            assert_eq!(strip_tags("[a][b]text[/b][/a]"), "text");
        }
    }

    mod styled_text_tests {
        use super::super::StyledText;

        #[test]
        fn selected_range_rebuilds_nested_balanced_tags() {
            let text = StyledText::parse("[outer]ab[inner]cdef[/inner]gh[/outer]");

            assert_eq!(
                text.select_range(0..5),
                "[outer]ab[inner]cde[/inner][/outer]"
            );
            assert_eq!(
                text.select_range(3..8),
                "[outer][inner]def[/inner]gh[/outer]"
            );
        }

        #[test]
        fn separate_ranges_are_independently_balanced() {
            let text = StyledText::parse("[outer]ab[inner]cdef[/inner]gh[/outer]");

            assert_eq!(
                text.select(&[0..2, 6..8], "…"),
                "[outer]ab[/outer]…[outer]gh[/outer]"
            );
        }

        #[test]
        fn selected_escaped_brackets_remain_escaped_source() {
            let text = StyledText::parse(r"[outer]a\[inner\]z[/outer]");
            let mut visible = String::new();
            text.visit_visible_chars(|character| visible.push(character));

            assert_eq!(visible, "a[inner]z");
            assert_eq!(
                text.select_range(0..visible.chars().count()),
                r"[outer]a\[inner\]z[/outer]"
            );
        }

        #[test]
        fn ansi_sequences_are_zero_width_and_preserved_when_selected() {
            let text = StyledText::parse("\x1b[31m[outer]hello[/outer]\x1b[0m");
            let mut visible = String::new();
            text.visit_visible_chars(|character| visible.push(character));

            assert_eq!(visible, "hello");
            assert_eq!(
                text.select_range(0..5),
                "\x1b[31m[outer]hello[/outer]\x1b[0m"
            );
        }

        #[test]
        fn legacy_ansi_designation_sequences_are_zero_width_and_preserved() {
            let input = "\x1b(0[outer]hello[/outer]\x1b(B";
            let text = StyledText::parse(input);
            let mut visible = String::new();
            text.visit_visible_chars(|character| visible.push(character));

            assert_eq!(visible, "hello");
            assert_eq!(text.select_range(0..5), input);
        }

        #[test]
        fn c1_ansi_sequences_are_zero_width_and_preserved() {
            let input = "\u{9b}31m[outer]hello[/outer]\u{9b}0m";
            let text = StyledText::parse(input);
            let mut visible = String::new();
            text.visit_visible_chars(|character| visible.push(character));

            assert_eq!(visible, "hello");
            assert_eq!(text.select_range(0..5), input);
        }
    }

    mod keep_mode {
        use super::*;

        #[test]
        fn plain_text_unchanged() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("hello world"), "hello world");
        }

        #[test]
        fn single_tag_preserved() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("[bold]hello[/bold]"), "[bold]hello[/bold]");
        }

        #[test]
        fn nested_tags_preserved() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(
                parser.parse("[bold][red]hello[/red][/bold]"),
                "[bold][red]hello[/red][/bold]"
            );
        }

        #[test]
        fn adjacent_tags_preserved() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(
                parser.parse("[bold]a[/bold][red]b[/red]"),
                "[bold]a[/bold][red]b[/red]"
            );
        }

        #[test]
        fn text_around_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(
                parser.parse("before [bold]middle[/bold] after"),
                "before [bold]middle[/bold] after"
            );
        }

        #[test]
        fn unknown_tags_preserved() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(
                parser.parse("[unknown]text[/unknown]"),
                "[unknown]text[/unknown]"
            );
        }
    }

    mod remove_mode {
        use super::*;

        #[test]
        fn plain_text_unchanged() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("hello world"), "hello world");
        }

        #[test]
        fn single_tag_stripped() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[bold]hello[/bold]"), "hello");
        }

        #[test]
        fn nested_tags_stripped() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[bold][red]hello[/red][/bold]"), "hello");
        }

        #[test]
        fn adjacent_tags_stripped() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[bold]a[/bold][red]b[/red]"), "ab");
        }

        #[test]
        fn text_around_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("before [bold]middle[/bold] after"),
                "before middle after"
            );
        }

        #[test]
        fn unknown_tags_stripped() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            // Default is Passthrough, but Remove mode ignores unknown_behavior for output
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
        }
    }

    mod unknown_tag_behavior {
        use super::*;

        #[test]
        fn passthrough_adds_question_mark_in_apply_mode() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Passthrough);
            assert_eq!(
                parser.parse("[unknown]text[/unknown]"),
                "[unknown?]text[/unknown?]"
            );
        }

        #[test]
        fn passthrough_is_default() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert_eq!(
                parser.parse("[unknown]text[/unknown]"),
                "[unknown?]text[/unknown?]"
            );
        }

        #[test]
        fn strip_removes_unknown_tags_in_apply_mode() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Strip);
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
        }

        #[test]
        fn passthrough_nested_with_known() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Passthrough);
            let result = parser.parse("[bold][unknown]text[/unknown][/bold]");
            assert!(result.contains("[unknown?]"));
            assert!(result.contains("[/unknown?]"));
            assert!(result.contains("text"));
        }

        #[test]
        fn strip_nested_with_known() {
            let mut styles = HashMap::new();
            styles.insert("bold".to_string(), Style::new().bold().force_styling(true));
            let parser = BBParser::new(styles, TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Strip);
            let result = parser.parse("[bold][unknown]text[/unknown][/bold]");
            assert!(!result.contains("[unknown"));
            assert!(result.contains("text"));
        }

        #[test]
        fn keep_mode_ignores_unknown_behavior() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep)
                .unknown_behavior(UnknownTagBehavior::Strip);
            assert_eq!(
                parser.parse("[unknown]text[/unknown]"),
                "[unknown]text[/unknown]"
            );
        }

        #[test]
        fn remove_mode_always_strips_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove)
                .unknown_behavior(UnknownTagBehavior::Passthrough);
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn validate_all_known_tags_passes() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert!(parser.validate("[bold]text[/bold]").is_ok());
        }

        #[test]
        fn validate_nested_known_tags_passes() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert!(parser.validate("[bold][red]text[/red][/bold]").is_ok());
        }

        #[test]
        fn validate_unknown_tag_fails() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[unknown]text[/unknown]");
            assert!(result.is_err());
        }

        #[test]
        fn validate_returns_correct_error_count() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[unknown]text[/unknown]");
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 2);
        }

        #[test]
        fn validate_error_contains_tag_name() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[foobar]text[/foobar]");
            let errors = result.unwrap_err();
            assert!(errors.errors.iter().all(|e| e.tag == "foobar"));
        }

        #[test]
        fn validate_error_distinguishes_open_and_close() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[unknown]text[/unknown]");
            let errors = result.unwrap_err();

            let open_count = errors
                .errors
                .iter()
                .filter(|e| e.kind == UnknownTagKind::Open)
                .count();
            let close_count = errors
                .errors
                .iter()
                .filter(|e| e.kind == UnknownTagKind::Close)
                .count();

            assert_eq!(open_count, 1);
            assert_eq!(close_count, 1);
        }

        #[test]
        fn validate_error_has_correct_positions() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let input = "[unknown]text[/unknown]";
            let result = parser.validate(input);
            let errors = result.unwrap_err();

            let open_error = errors
                .errors
                .iter()
                .find(|e| e.kind == UnknownTagKind::Open)
                .unwrap();
            assert_eq!(open_error.start, 0);
            assert_eq!(open_error.end, 9);

            let close_error = errors
                .errors
                .iter()
                .find(|e| e.kind == UnknownTagKind::Close)
                .unwrap();
            assert_eq!(close_error.start, 13);
            assert_eq!(close_error.end, 23);
        }

        #[test]
        fn validate_multiple_unknown_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[foo]a[/foo][bar]b[/bar]");
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 4);

            let tags: std::collections::HashSet<_> =
                errors.errors.iter().map(|e| e.tag.as_str()).collect();
            assert!(tags.contains("foo"));
            assert!(tags.contains("bar"));
        }

        #[test]
        fn validate_mixed_known_and_unknown() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[bold][unknown]text[/unknown][/bold]");
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 2);

            for error in &errors.errors {
                assert_eq!(error.tag, "unknown");
            }
        }

        #[test]
        fn validate_plain_text_passes() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert!(parser.validate("plain text without tags").is_ok());
        }

        #[test]
        fn validate_empty_string_passes() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert!(parser.validate("").is_ok());
        }
    }

    mod parse_with_diagnostics {
        use super::*;

        #[test]
        fn returns_output_and_errors() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Passthrough);
            let (output, errors) = parser.parse_with_diagnostics("[unknown]text[/unknown]");

            assert_eq!(output, "[unknown?]text[/unknown?]");
            assert_eq!(errors.len(), 2);
        }

        #[test]
        fn output_uses_strip_behavior() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply)
                .unknown_behavior(UnknownTagBehavior::Strip);
            let (output, errors) = parser.parse_with_diagnostics("[unknown]text[/unknown]");

            assert_eq!(output, "text");
            assert_eq!(errors.len(), 2);
        }

        #[test]
        fn no_errors_for_known_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let (_, errors) = parser.parse_with_diagnostics("[bold]text[/bold]");
            assert!(errors.is_empty());
        }

        #[test]
        fn errors_iterable() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let (_, errors) = parser.parse_with_diagnostics("[a]x[/a][b]y[/b]");

            let mut count = 0;
            for error in &errors {
                assert!(error.tag == "a" || error.tag == "b");
                count += 1;
            }
            assert_eq!(count, 4);
        }
    }

    mod tag_names {
        use super::*;

        #[test]
        fn valid_simple_names() {
            assert!(Tokenizer::is_valid_tag_name("bold"));
            assert!(Tokenizer::is_valid_tag_name("red"));
            assert!(Tokenizer::is_valid_tag_name("a"));
        }

        #[test]
        fn valid_with_underscore() {
            assert!(Tokenizer::is_valid_tag_name("my_style"));
            assert!(Tokenizer::is_valid_tag_name("_private"));
            assert!(Tokenizer::is_valid_tag_name("a_b_c"));
        }

        #[test]
        fn valid_with_hyphen() {
            assert!(Tokenizer::is_valid_tag_name("my-style"));
            assert!(Tokenizer::is_valid_tag_name("font-bold"));
            assert!(Tokenizer::is_valid_tag_name("a-b-c"));
        }

        #[test]
        fn valid_with_numbers() {
            assert!(Tokenizer::is_valid_tag_name("h1"));
            assert!(Tokenizer::is_valid_tag_name("col2"));
            assert!(Tokenizer::is_valid_tag_name("style123"));
        }

        #[test]
        fn invalid_starts_with_digit() {
            assert!(!Tokenizer::is_valid_tag_name("1style"));
            assert!(!Tokenizer::is_valid_tag_name("123"));
        }

        #[test]
        fn invalid_starts_with_hyphen() {
            assert!(!Tokenizer::is_valid_tag_name("-style"));
            assert!(!Tokenizer::is_valid_tag_name("-1"));
        }

        #[test]
        fn invalid_uppercase() {
            assert!(!Tokenizer::is_valid_tag_name("Bold"));
            assert!(!Tokenizer::is_valid_tag_name("BOLD"));
            assert!(!Tokenizer::is_valid_tag_name("myStyle"));
        }

        #[test]
        fn invalid_special_chars() {
            assert!(!Tokenizer::is_valid_tag_name("my.style"));
            assert!(!Tokenizer::is_valid_tag_name("my@style"));
            assert!(!Tokenizer::is_valid_tag_name("my style"));
        }

        #[test]
        fn invalid_empty() {
            assert!(!Tokenizer::is_valid_tag_name(""));
        }
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn empty_input() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse(""), "");
        }

        #[test]
        fn unclosed_tag_passthrough() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("[bold]hello"), "[bold]hello");
        }

        #[test]
        fn orphan_close_tag_passthrough() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("hello[/bold]"), "hello[/bold]");
        }

        #[test]
        fn mismatched_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(
                parser.parse("[bold]hello[/red][/bold]"),
                "[bold]hello[/red][/bold]"
            );
        }

        #[test]
        fn overlapping_tags_auto_close() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            let result = parser.parse("[bold][red]hello[/bold][/red]");
            assert!(result.contains("hello"));
        }

        #[test]
        fn empty_tag_content() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[bold][/bold]"), "");
        }

        #[test]
        fn brackets_in_content() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[bold]array[0][/bold]"), "array[0]");
        }

        #[test]
        fn invalid_tag_syntax_passthrough() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("[123]text[/123]"), "[123]text[/123]");
            assert_eq!(parser.parse("[-bad]text[/-bad]"), "[-bad]text[/-bad]");
            assert_eq!(parser.parse("[Bad]text[/Bad]"), "[Bad]text[/Bad]");
        }

        #[test]
        fn deeply_nested() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("[bold][red][dim]deep[/dim][/red][/bold]"),
                "deep"
            );
        }

        #[test]
        fn many_adjacent_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("[bold]a[/bold][red]b[/red][dim]c[/dim]"),
                "abc"
            );
        }

        #[test]
        fn unclosed_bracket() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("hello [bold world"), "hello [bold world");
        }

        #[test]
        fn multiline_content() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("[bold]line1\nline2\nline3[/bold]"),
                "line1\nline2\nline3"
            );
        }

        #[test]
        fn style_with_underscore() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[my_style]text[/my_style]"), "text");
        }

        #[test]
        fn style_with_dash() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("[style-with-dash]text[/style-with-dash]"),
                "text"
            );
        }
    }

    mod escapes {
        use super::*;

        #[test]
        fn escaped_open_bracket_is_literal() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("\\[bold\\]"), "[bold]");
        }

        #[test]
        fn escaped_brackets_inside_known_tag() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(
                parser.parse("[bold]hello \\[world\\][/bold]"),
                "hello [world]"
            );
        }

        #[test]
        fn escapes_keep_mode_emits_literal_brackets() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("\\[bold\\]"), "[bold]");
        }

        #[test]
        fn escapes_apply_mode_styles_around_literals() {
            let mut styles = HashMap::new();
            styles.insert("bold".to_string(), Style::new().bold().force_styling(true));
            let parser = BBParser::new(styles, TagTransform::Apply);
            let result = parser.parse("[bold]\\[x\\][/bold]");
            assert!(result.contains("[x]"));
            assert!(!result.contains("[bold]"));
        }

        #[test]
        fn lone_backslash_is_literal() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("path C:\\foo\\bar"), "path C:\\foo\\bar");
        }

        #[test]
        fn unescape_borrows_when_no_bracket_escape_present() {
            assert!(matches!(
                unescape("plain text"),
                std::borrow::Cow::Borrowed(_)
            ));
            assert!(matches!(
                unescape("C:\\foo\\bar"),
                std::borrow::Cow::Borrowed(_)
            ));
            assert!(matches!(unescape("\\d+"), std::borrow::Cow::Borrowed(_)));
            assert!(matches!(
                unescape("trailing\\"),
                std::borrow::Cow::Borrowed(_)
            ));
            assert!(matches!(unescape("\\["), std::borrow::Cow::Owned(_)));
            assert!(matches!(unescape("\\]"), std::borrow::Cow::Owned(_)));
        }

        #[test]
        fn trailing_backslash_is_literal() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("end\\"), "end\\");
        }

        #[test]
        fn double_backslash_then_open_emits_backslash_then_literal_bracket() {
            // `\\` is not an escape sequence, so the first `\` is literal;
            // the second `\` pairs with `[` to emit a literal `[`.
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("\\\\[bold]"), "\\[bold]");
        }

        #[test]
        fn escaped_brackets_dont_create_unknown_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let (output, errors) = parser.parse_with_diagnostics("\\[unknown\\]");
            assert_eq!(output, "[unknown]");
            assert!(errors.is_empty());
        }

        #[test]
        fn escapes_pass_validation() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert!(parser.validate("\\[anything\\]").is_ok());
            assert!(parser.validate("[bold]a\\[b\\]c[/bold]").is_ok());
        }

        #[test]
        fn strip_tags_handles_escapes() {
            assert_eq!(strip_tags("\\[bold\\]"), "[bold]");
            assert_eq!(strip_tags("[bold]a\\[b\\]c[/bold]"), "a[b]c");
        }

        #[test]
        fn escape_does_not_apply_inside_tag_name() {
            // `\` is not a valid tag-name char, so this becomes InvalidTag.
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            assert_eq!(parser.parse("[bo\\ld]"), "[bo\\ld]");
        }

        #[test]
        fn escapes_with_multibyte_text() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("café \\[é\\] 🎉"), "café [é] 🎉");
        }

        #[test]
        fn only_open_escaped_leaves_close_unmatched() {
            // Escaping only the open leaves the close unmatched.
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let (output, errors) = parser.parse_with_diagnostics("\\[bold]hi[/bold]");
            assert!(output.contains("[bold]hi"));
            assert!(output.contains("[/bold]"));
            assert!(!errors.is_empty());
            assert!(errors
                .errors
                .iter()
                .any(|e| e.kind == UnknownTagKind::UnexpectedClose));
        }
    }

    mod tokenizer {
        use super::*;

        #[test]
        fn tokenize_plain_text() {
            let tokens: Vec<_> = Tokenizer::new("hello world").collect();
            assert_eq!(
                tokens,
                vec![Token::Text {
                    content: "hello world",
                    start: 0,
                    end: 11
                }]
            );
        }

        #[test]
        fn tokenize_single_tag() {
            let tokens: Vec<_> = Tokenizer::new("[bold]hello[/bold]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag {
                        name: "bold",
                        start: 0,
                        end: 6
                    },
                    Token::Text {
                        content: "hello",
                        start: 6,
                        end: 11
                    },
                    Token::CloseTag {
                        name: "bold",
                        start: 11,
                        end: 18
                    },
                ]
            );
        }

        #[test]
        fn tokenize_nested_tags() {
            let tokens: Vec<_> = Tokenizer::new("[a][b]x[/b][/a]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag {
                        name: "a",
                        start: 0,
                        end: 3
                    },
                    Token::OpenTag {
                        name: "b",
                        start: 3,
                        end: 6
                    },
                    Token::Text {
                        content: "x",
                        start: 6,
                        end: 7
                    },
                    Token::CloseTag {
                        name: "b",
                        start: 7,
                        end: 11
                    },
                    Token::CloseTag {
                        name: "a",
                        start: 11,
                        end: 15
                    },
                ]
            );
        }

        #[test]
        fn tokenize_invalid_tag() {
            let tokens: Vec<_> = Tokenizer::new("[123]text[/123]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::InvalidTag {
                        content: "[123]",
                        start: 0,
                        end: 5
                    },
                    Token::Text {
                        content: "text",
                        start: 5,
                        end: 9
                    },
                    Token::InvalidTag {
                        content: "[/123]",
                        start: 9,
                        end: 15
                    },
                ]
            );
        }

        #[test]
        fn tokenize_mixed() {
            let tokens: Vec<_> = Tokenizer::new("a[b]c[/b]d").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::Text {
                        content: "a",
                        start: 0,
                        end: 1
                    },
                    Token::OpenTag {
                        name: "b",
                        start: 1,
                        end: 4
                    },
                    Token::Text {
                        content: "c",
                        start: 4,
                        end: 5
                    },
                    Token::CloseTag {
                        name: "b",
                        start: 5,
                        end: 9
                    },
                    Token::Text {
                        content: "d",
                        start: 9,
                        end: 10
                    },
                ]
            );
        }
    }

    mod apply_mode {
        use super::*;

        #[test]
        fn plain_text_unchanged() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            assert_eq!(parser.parse("hello world"), "hello world");
        }

        #[test]
        fn unknown_tag_passthrough_with_marker() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.parse("[unknown]text[/unknown]");
            assert!(result.contains("[unknown?]"));
            assert!(result.contains("[/unknown?]"));
            assert!(result.contains("text"));
        }

        #[test]
        fn known_tag_applies_style() {
            let mut styles = HashMap::new();
            styles.insert("bold".to_string(), Style::new().bold().force_styling(true));

            let parser = BBParser::new(styles, TagTransform::Apply);
            let result = parser.parse("[bold]hello[/bold]");

            assert!(result.contains("\x1b[1m") || result.contains("hello"));
        }
    }

    mod error_display {
        use super::*;

        #[test]
        fn unknown_tag_error_display() {
            let error = UnknownTagError {
                tag: "foo".to_string(),
                kind: UnknownTagKind::Open,
                start: 0,
                end: 5,
            };
            let display = format!("{}", error);
            assert!(display.contains("foo"));
            assert!(display.contains("opening"));
            assert!(display.contains("0..5"));
        }

        #[test]
        fn unknown_tag_errors_display() {
            let mut errors = UnknownTagErrors::new();
            errors.push(UnknownTagError {
                tag: "foo".to_string(),
                kind: UnknownTagKind::Open,
                start: 0,
                end: 5,
            });
            errors.push(UnknownTagError {
                tag: "foo".to_string(),
                kind: UnknownTagKind::Close,
                start: 9,
                end: 15,
            });

            let display = format!("{}", errors);
            assert!(display.contains("2 unknown tag"));
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use console::strip_ansi_codes;
    use proptest::prelude::*;

    fn valid_tag_name() -> impl Strategy<Value = String> {
        "[a-z_][a-z0-9_-]{0,10}"
    }

    fn plain_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?:;'\"]{0,50}"
            .prop_filter("no brackets", |s| !s.contains('[') && !s.contains(']'))
    }

    fn ansi_control() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec![
            "\x1b[31m",
            "\x1b[0m",
            "\x1b(0",
            "\x1b(B",
            "\x1b)0",
            "\x1b)B",
            "\u{9b}31m",
            "\u{9b}0m",
        ])
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn keep_mode_roundtrip(content in plain_text()) {
            let parser = BBParser::new(HashMap::new(), TagTransform::Keep);
            prop_assert_eq!(parser.parse(&content), content);
        }

        #[test]
        fn remove_mode_plain_text_unchanged(content in plain_text()) {
            let parser = BBParser::new(HashMap::new(), TagTransform::Remove);
            prop_assert_eq!(parser.parse(&content), content);
        }

        #[test]
        fn styled_text_matches_established_ansi_stripping_semantics(
            prefix in prop::collection::vec(ansi_control(), 0..4),
            content in plain_text(),
            suffix in prop::collection::vec(ansi_control(), 0..4),
        ) {
            let input = format!(
                "{}[outer]{}[/outer]{}",
                prefix.concat(),
                content,
                suffix.concat()
            );
            let styled = StyledText::parse(&input);
            let mut visible = String::new();
            styled.visit_visible_chars(|character| visible.push(character));
            let expected = strip_tags(&strip_ansi_codes(&input));

            prop_assert_eq!(&visible, &expected);
            if !visible.is_empty() {
                prop_assert_eq!(styled.select_range(0..visible.chars().count()), input);
            }
        }

        #[test]
        fn valid_tag_names_accepted(tag in valid_tag_name()) {
            prop_assert!(Tokenizer::is_valid_tag_name(&tag));
        }

        #[test]
        fn remove_strips_known_tags(tag in valid_tag_name(), content in plain_text()) {
            let mut styles = HashMap::new();
            styles.insert(tag.clone(), Style::new());

            let parser = BBParser::new(styles, TagTransform::Remove);
            let input = format!("[{}]{}[/{}]", tag, content, tag);
            let result = parser.parse(&input);

            prop_assert_eq!(result, content);
        }

        #[test]
        fn keep_preserves_structure(tag in valid_tag_name(), content in plain_text()) {
            let parser = BBParser::new(HashMap::new(), TagTransform::Keep);
            let input = format!("[{}]{}[/{}]", tag, content, tag);
            let result = parser.parse(&input);

            prop_assert_eq!(result, input);
        }

        #[test]
        fn nested_tags_balanced(
            outer in valid_tag_name(),
            inner in valid_tag_name(),
            content in plain_text()
        ) {
            let mut styles = HashMap::new();
            styles.insert(outer.clone(), Style::new());
            styles.insert(inner.clone(), Style::new());

            let parser = BBParser::new(styles, TagTransform::Remove);
            let input = format!("[{}][{}]{}[/{}][/{}]", outer, inner, content, inner, outer);
            let result = parser.parse(&input);

            prop_assert_eq!(result, content);
        }

        #[test]
        fn validate_finds_unknown_tags(tag in valid_tag_name(), content in plain_text()) {
            let parser = BBParser::new(HashMap::new(), TagTransform::Apply);
            let input = format!("[{}]{}[/{}]", tag, content, tag);
            let result = parser.validate(&input);

            prop_assert!(result.is_err());
            let errors = result.unwrap_err();
            prop_assert_eq!(errors.len(), 2);
        }

        #[test]
        fn invalid_start_digit_rejected(n in 0..10u8, rest in "[a-z0-9_-]{0,5}") {
            let tag = format!("{}{}", n, rest);
            prop_assert!(!Tokenizer::is_valid_tag_name(&tag));
        }

        #[test]
        fn invalid_start_hyphen_rejected(rest in "[a-z0-9_-]{0,5}") {
            let tag = format!("-{}", rest);
            prop_assert!(!Tokenizer::is_valid_tag_name(&tag));
        }

        #[test]
        fn uppercase_rejected(tag in "[A-Z][a-zA-Z0-9_-]{0,5}") {
            prop_assert!(!Tokenizer::is_valid_tag_name(&tag));
        }
    }
}
