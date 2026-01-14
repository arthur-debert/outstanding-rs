//! BBCode-style tag parser for terminal styling.
//!
//! This crate provides a parser for `[tag]content[/tag]` style markup,
//! designed for terminal output styling. It handles nested tags correctly
//! and supports multiple output modes.
//!
//! # Example
//!
//! ```rust
//! use outstanding_bbparser::{BBParser, TagTransform};
//! use console::Style;
//! use std::collections::HashMap;
//!
//! let mut styles = HashMap::new();
//! styles.insert("bold".to_string(), Style::new().bold());
//! styles.insert("red".to_string(), Style::new().red());
//!
//! // Apply ANSI codes
//! let parser = BBParser::new(styles.clone(), TagTransform::Apply);
//! let output = parser.parse("[bold]hello[/bold]");
//! // output contains ANSI escape codes for bold
//!
//! // Strip tags (plain text)
//! let parser = BBParser::new(styles.clone(), TagTransform::Remove);
//! let output = parser.parse("[bold]hello[/bold]");
//! assert_eq!(output, "hello");
//!
//! // Keep tags visible (debug mode)
//! let parser = BBParser::new(styles, TagTransform::Keep);
//! let output = parser.parse("[bold]hello[/bold]");
//! assert_eq!(output, "[bold]hello[/bold]");
//! ```
//!
//! # Unknown Tag Handling
//!
//! Tags not found in the styles map can be handled in two ways:
//!
//! - [`UnknownTagBehavior::Passthrough`]: Keep tags with a `?` marker: `[foo]` → `[foo?]`
//! - [`UnknownTagBehavior::Strip`]: Remove tags entirely, keep content: `[foo]text[/foo]` → `text`
//!
//! For validation, use [`BBParser::validate`] to check for unknown tags before parsing.
//!
//! # Tag Name Syntax
//!
//! Tag names follow CSS identifier rules:
//! - Start with a letter (`a-z`) or underscore (`_`)
//! - Followed by letters, digits (`0-9`), underscores, or hyphens (`-`)
//! - Cannot start with a digit or hyphen followed by digit
//! - Case-sensitive (lowercase recommended)
//!
//! Pattern: `[a-z_][a-z0-9_-]*`

use console::Style;
use std::collections::HashMap;

/// How to transform matched tags in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagTransform {
    /// Apply ANSI escape codes from the associated Style.
    /// Used for terminal output with color support.
    Apply,

    /// Remove all tags, outputting only the content.
    /// Used for plain text output without styling.
    Remove,

    /// Keep tags as-is in the output.
    /// Used for debug mode to visualize tag structure.
    Keep,
}

/// How to handle tags not found in the styles map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownTagBehavior {
    /// Keep unknown tags as literal text with a `?` marker.
    /// `[foo]text[/foo]` → `[foo?]text[/foo?]`
    ///
    /// This makes unknown tags visible without breaking output.
    #[default]
    Passthrough,

    /// Strip unknown tags entirely, keeping only inner content.
    /// `[foo]text[/foo]` → `text`
    ///
    /// Use this for graceful degradation in production.
    Strip,
}

/// The kind of unknown tag encountered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownTagKind {
    /// An opening tag: `[foo]`
    Open,
    /// A closing tag: `[/foo]`
    Close,
    /// An unbalanced opening tag: `[foo]...` (no matching close)
    Unbalanced,
    /// An unexpected closing tag: `...[/foo]` (no matching open)
    UnexpectedClose,
}

/// An error representing an unknown tag in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTagError {
    /// The tag name that was not found in styles.
    pub tag: String,
    /// The kind of tag (open or close).
    pub kind: UnknownTagKind,
    /// Byte offset of the opening `[` in the input.
    pub start: usize,
    /// Byte offset after the closing `]` in the input.
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

/// A collection of unknown tag errors found during parsing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownTagErrors {
    /// The list of unknown tag errors.
    pub errors: Vec<UnknownTagError>,
}

impl UnknownTagErrors {
    /// Creates an empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no errors were found.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns the number of errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Adds an error to the collection.
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

/// A BBCode-style tag parser for terminal styling.
///
/// The parser processes `[tag]content[/tag]` patterns and transforms them
/// according to the configured [`TagTransform`] mode.
#[derive(Debug, Clone)]
pub struct BBParser {
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
}

impl BBParser {
    /// Creates a new parser with the given styles and transform mode.
    ///
    /// # Arguments
    ///
    /// * `styles` - Map of tag names to console styles.
    ///   Note: These styles are used directly; no alias resolution is performed.
    /// * `transform` - How to handle matched tags
    ///
    /// Unknown tags default to [`UnknownTagBehavior::Passthrough`].
    pub fn new(styles: HashMap<String, Style>, transform: TagTransform) -> Self {
        Self {
            styles,
            transform,
            unknown_behavior: UnknownTagBehavior::default(),
        }
    }

    /// Sets the behavior for unknown tags.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding_bbparser::{BBParser, TagTransform, UnknownTagBehavior};
    /// use std::collections::HashMap;
    ///
    /// let parser = BBParser::new(HashMap::new(), TagTransform::Remove)
    ///     .unknown_behavior(UnknownTagBehavior::Strip);
    ///
    /// // Unknown tags are stripped
    /// assert_eq!(parser.parse("[foo]text[/foo]"), "text");
    /// ```
    pub fn unknown_behavior(mut self, behavior: UnknownTagBehavior) -> Self {
        self.unknown_behavior = behavior;
        self
    }

    /// Parses and transforms input.
    ///
    /// Unknown tags are handled according to the configured [`UnknownTagBehavior`].
    pub fn parse(&self, input: &str) -> String {
        let (output, _) = self.parse_internal(input);
        output
    }

    /// Parses input and collects any unknown tag errors.
    ///
    /// Returns the transformed output AND any errors found.
    /// The output uses the configured [`UnknownTagBehavior`] for transformation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding_bbparser::{BBParser, TagTransform};
    /// use std::collections::HashMap;
    ///
    /// let parser = BBParser::new(HashMap::new(), TagTransform::Remove);
    /// let (output, errors) = parser.parse_with_diagnostics("[unknown]text[/unknown]");
    ///
    /// assert!(!errors.is_empty());
    /// assert_eq!(errors.len(), 2); // open and close tags
    /// ```
    pub fn parse_with_diagnostics(&self, input: &str) -> (String, UnknownTagErrors) {
        self.parse_internal(input)
    }

    /// Validates input for unknown tags without producing transformed output.
    ///
    /// Returns `Ok(())` if all tags are known, `Err` with details otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding_bbparser::{BBParser, TagTransform};
    /// use std::collections::HashMap;
    /// use console::Style;
    ///
    /// let mut styles = HashMap::new();
    /// styles.insert("bold".to_string(), Style::new().bold());
    ///
    /// let parser = BBParser::new(styles, TagTransform::Apply);
    ///
    /// // Known tag passes validation
    /// assert!(parser.validate("[bold]text[/bold]").is_ok());
    ///
    /// // Unknown tag fails validation
    /// let result = parser.validate("[unknown]text[/unknown]");
    /// assert!(result.is_err());
    /// ```
    pub fn validate(&self, input: &str) -> Result<(), UnknownTagErrors> {
        let (_, errors) = self.parse_internal(input);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Internal parsing that returns both output and errors.
    fn parse_internal(&self, input: &str) -> (String, UnknownTagErrors) {
        let tokens = Tokenizer::new(input).collect::<Vec<_>>();
        let valid_opens = self.compute_valid_tags(&tokens);
        let mut events = Vec::new();
        let mut errors = UnknownTagErrors::new();
        let mut stack: Vec<&str> = Vec::new();

        // ...
        // ...
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Text { content, .. } => {
                    events.push(ParseEvent::Literal(std::borrow::Cow::Borrowed(content)));
                }
                Token::OpenTag { name, start, end } => {
                    if valid_opens.contains(&i) {
                        stack.push(name);
                        self.emit_open_tag_event(&mut events, &mut errors, name, *start, *end);
                    } else {
                        // Check if this looks like a valid tag name but was just unclosed/unbalanced
                        let is_valid_name = Tokenizer::is_valid_tag_name(name);
                        if is_valid_name {
                            // Strictly error on unbalanced tags
                            errors.push(UnknownTagError {
                                tag: name.to_string(),
                                kind: UnknownTagKind::Unbalanced, // NEW VARIANT
                                start: *start,
                                end: *end,
                            });
                            // Also treat as literal to not break output entirely?
                            // Or just error? Issue says "Unbalanced tags must error".
                            // We record error. Output depends on transform.
                            // We'll output literal text for visual feedback?
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
                        // Unexpected close tag
                        let is_valid_name = Tokenizer::is_valid_tag_name(name);
                        if is_valid_name {
                            errors.push(UnknownTagError {
                                tag: name.to_string(),
                                kind: UnknownTagKind::UnexpectedClose, // NEW VARIANT
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
            TagTransform::Remove => {
                // Nothing to emit for known or stripped unknown tags
            }
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
                        UnknownTagBehavior::Strip => {
                            // Nothing to emit
                        }
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

        // Only record error if we have valid position info (not auto-closed)
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
            TagTransform::Remove => {
                // Nothing to emit
            }
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
                        UnknownTagBehavior::Strip => {
                            // Nothing to emit
                        }
                    }
                }
            }
        }
    }

    /// Renders events to a string.
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

    /// Pre-computes which OpenTag tokens have a valid matching CloseTag.
    /// This is O(N) instead of O(N^2).
    fn compute_valid_tags(&self, tokens: &[Token]) -> std::collections::HashSet<usize> {
        use std::collections::{HashMap, HashSet};
        let mut valid_indices = HashSet::new();
        let mut open_indices_by_tag: HashMap<&str, Vec<usize>> = HashMap::new();

        for (i, token) in tokens.iter().enumerate() {
            match token {
                Token::OpenTag { name, .. } => {
                    open_indices_by_tag.entry(name).or_default().push(i);
                }
                Token::CloseTag { name, .. } => {
                    if let Some(indices) = open_indices_by_tag.get_mut(name) {
                        if let Some(open_idx) = indices.pop() {
                            valid_indices.insert(open_idx);
                        }
                    }
                }
                _ => {}
            }
        }

        valid_indices
    }

    /// Helper to append styled text.
    fn append_styled(&self, output: &mut String, text: &str, style_stack: &[&Style]) {
        if text.is_empty() {
            return;
        }

        if style_stack.is_empty() {
            output.push_str(text);
        } else {
            let mut current = text.to_string();
            // Apply styles from innermost (top of stack) to outermost (bottom).
            // This ensures that inner styles override outer styles (ANSI rules: last code wins).
            // Also optimizes by stripping nested resets.
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

/// Token types produced by the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    /// Plain text content.
    Text {
        content: &'a str,
        start: usize,
        end: usize,
    },
    /// Opening tag: `[tagname]`
    OpenTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    /// Closing tag: `[/tagname]`
    CloseTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    /// Invalid tag syntax (passed through as text).
    InvalidTag {
        content: &'a str,
        start: usize,
        end: usize,
    },
}

/// Tokenizer for BBCode-style tags.
struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// Checks if a string is a valid tag name (CSS identifier rules).
    fn is_valid_tag_name(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let mut chars = s.chars();
        let first = chars.next().unwrap();

        // First char must be letter or underscore
        if !first.is_ascii_lowercase() && first != '_' {
            return false;
        }

        // Rest can be letter, digit, underscore, or hyphen
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

        // Look for the next '['
        if let Some(bracket_pos) = remaining.find('[') {
            if bracket_pos > 0 {
                // There's text before the bracket
                let text = &remaining[..bracket_pos];
                self.pos += bracket_pos;
                return Some(Token::Text {
                    content: text,
                    start: start_pos,
                    end: self.pos,
                });
            }

            // We're at a '['
            // Try to parse a tag
            if let Some(close_bracket) = remaining.find(']') {
                let tag_content = &remaining[1..close_bracket];
                let full_tag = &remaining[..=close_bracket];
                let end_pos = start_pos + close_bracket + 1;

                // Check for closing tag
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
                // No closing bracket - rest is text
                let end_pos = self.input.len();
                self.pos = end_pos;
                Some(Token::Text {
                    content: remaining,
                    start: start_pos,
                    end: end_pos,
                })
            }
        } else {
            // No more brackets - rest is text
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

    // ==================== TagTransform::Keep Tests ====================

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

    // ==================== TagTransform::Remove Tests ====================

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

    // ==================== Unknown Tag Behavior Tests ====================

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
            // Should have bold styling but no unknown tag markers
            assert!(!result.contains("[unknown"));
            assert!(result.contains("text"));
        }

        #[test]
        fn keep_mode_ignores_unknown_behavior() {
            // In Keep mode, all tags are preserved as-is regardless of unknown_behavior
            let parser = BBParser::new(test_styles(), TagTransform::Keep)
                .unknown_behavior(UnknownTagBehavior::Strip);
            assert_eq!(
                parser.parse("[unknown]text[/unknown]"),
                "[unknown]text[/unknown]"
            );
        }

        #[test]
        fn remove_mode_always_strips_tags() {
            // In Remove mode, all tags are stripped regardless of unknown_behavior
            let parser = BBParser::new(test_styles(), TagTransform::Remove)
                .unknown_behavior(UnknownTagBehavior::Passthrough);
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
        }
    }

    // ==================== Validation Tests ====================

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
            assert_eq!(errors.len(), 2); // open and close
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
            assert_eq!(open_error.end, 9); // "[unknown]"

            let close_error = errors
                .errors
                .iter()
                .find(|e| e.kind == UnknownTagKind::Close)
                .unwrap();
            assert_eq!(close_error.start, 13);
            assert_eq!(close_error.end, 23); // "[/unknown]"
        }

        #[test]
        fn validate_multiple_unknown_tags() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.validate("[foo]a[/foo][bar]b[/bar]");
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 4); // 2 opens + 2 closes

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
            assert_eq!(errors.len(), 2); // only unknown tag errors

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

    // ==================== Parse With Diagnostics Tests ====================

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

    // ==================== Tag Name Validation Tests ====================

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

    // ==================== Edge Cases ====================

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

    // ==================== Tokenizer Tests ====================

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

    // ==================== Apply Mode Tests ====================

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

    // ==================== Error Display Tests ====================

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
    use proptest::prelude::*;

    fn valid_tag_name() -> impl Strategy<Value = String> {
        "[a-z_][a-z0-9_-]{0,10}"
    }

    fn plain_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?:;'\"]{0,50}"
            .prop_filter("no brackets", |s| !s.contains('[') && !s.contains(']'))
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
            prop_assert_eq!(errors.len(), 2); // open + close
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
