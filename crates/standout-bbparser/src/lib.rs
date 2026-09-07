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

pub mod ansi;
mod styled_text;
mod tokenizer;

pub use styled_text::{StyledText, StyledTextEvent};
pub use tokenizer::is_valid_tag_name;
use tokenizer::{compute_valid_tags, unescape, Token, Tokenizer};

use console::Style;
use std::collections::HashMap;

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
                        let is_valid_name = is_valid_tag_name(name);
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
                        let is_valid_name = is_valid_tag_name(name);
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

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;
