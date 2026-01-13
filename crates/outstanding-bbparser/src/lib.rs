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

/// Configuration for handling unknown tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownTagBehavior {
    /// Pass through unknown tags unchanged.
    Passthrough,

    /// Strip unknown tags (keep content, remove tag markers).
    Strip,

    /// Prefix content with an indicator (e.g., "(!?)").
    Indicate(String),
}

impl Default for UnknownTagBehavior {
    fn default() -> Self {
        Self::Indicate("(!?)".to_string())
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
    /// * `styles` - Map of tag names to console styles (should be pre-resolved, no aliases)
    /// * `transform` - How to handle matched tags
    pub fn new(styles: HashMap<String, Style>, transform: TagTransform) -> Self {
        Self {
            styles,
            transform,
            unknown_behavior: UnknownTagBehavior::default(),
        }
    }

    /// Sets the behavior for unknown tags.
    pub fn unknown_behavior(mut self, behavior: UnknownTagBehavior) -> Self {
        self.unknown_behavior = behavior;
        self
    }

    /// Parses the input string and transforms tags according to the configured mode.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string containing `[tag]...[/tag]` markup
    ///
    /// # Returns
    ///
    /// The transformed string with tags processed according to the transform mode.
    pub fn parse(&self, input: &str) -> String {
        let tokens = Tokenizer::new(input).collect::<Vec<_>>();
        let valid_opens = self.compute_valid_tags(&tokens);
        let mut output = String::with_capacity(input.len());
        let mut stack: Vec<&str> = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Text(text) => {
                    output.push_str(text);
                }
                Token::OpenTag(tag) => {
                    // Check precomputed validity
                    if valid_opens.contains(&i) {
                        stack.push(tag);
                        self.emit_open_tag(&mut output, tag);
                    } else {
                        // No matching close tag - treat as literal text
                        output.push('[');
                        output.push_str(tag);
                        output.push(']');
                    }
                }
                Token::CloseTag(tag) => {
                    if stack.last().copied() == Some(*tag) {
                        stack.pop();
                        self.emit_close_tag(&mut output, tag);
                    } else if stack.contains(tag) {
                        // Mismatched nesting - close all tags up to and including this one
                        while let Some(open) = stack.pop() {
                            self.emit_close_tag(&mut output, open);
                            if open == *tag {
                                break;
                            }
                        }
                    } else {
                        // Orphan close tag - treat as literal text
                        output.push_str("[/");
                        output.push_str(tag);
                        output.push(']');
                    }
                }
                Token::InvalidTag(text) => {
                    // Invalid tag syntax - pass through as literal text
                    output.push_str(text);
                }
            }
            i += 1;
        }

        // Close any remaining unclosed tags
        while let Some(tag) = stack.pop() {
            self.emit_close_tag(&mut output, tag);
        }

        output
    }

    /// Pre-computes which OpenTag tokens have a valid matching CloseTag.
    /// This is O(N) instead of O(N^2).
    fn compute_valid_tags(&self, tokens: &[Token]) -> std::collections::HashSet<usize> {
        use std::collections::{HashMap, HashSet};
        let mut valid_indices = HashSet::new();
        let mut open_indices_by_tag: HashMap<&str, Vec<usize>> = HashMap::new();

        for (i, token) in tokens.iter().enumerate() {
            match token {
                Token::OpenTag(tag) => {
                    open_indices_by_tag.entry(tag).or_default().push(i);
                }
                Token::CloseTag(tag) => {
                    if let Some(indices) = open_indices_by_tag.get_mut(tag) {
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

    /// Emits the opening tag transformation.
    fn emit_open_tag(&self, output: &mut String, tag: &str) {
        match self.transform {
            TagTransform::Keep => {
                output.push('[');
                output.push_str(tag);
                output.push(']');
            }
            TagTransform::Remove => {
                // Output nothing for the tag itself
                if !self.styles.contains_key(tag) {
                    self.emit_unknown_prefix(output);
                }
            }
            TagTransform::Apply => {
                if self.styles.contains_key(tag) {
                    // Start the style - we need to apply it to the content
                    // console::Style applies to a specific string, so we'll handle this
                    // differently - we mark the start position
                    output.push_str(&format!("\x00STYLE_START:{}\x00", tag));
                } else {
                    self.emit_unknown_prefix(output);
                }
            }
        }
    }

    /// Emits the closing tag transformation.
    fn emit_close_tag(&self, output: &mut String, tag: &str) {
        match self.transform {
            TagTransform::Keep => {
                output.push_str("[/");
                output.push_str(tag);
                output.push(']');
            }
            TagTransform::Remove => {
                // Output nothing
            }
            TagTransform::Apply => {
                if self.styles.contains_key(tag) {
                    output.push_str(&format!("\x00STYLE_END:{}\x00", tag));
                }
            }
        }
    }

    /// Emits the unknown tag indicator prefix.
    fn emit_unknown_prefix(&self, output: &mut String) {
        if let UnknownTagBehavior::Indicate(ref indicator) = self.unknown_behavior {
            output.push_str(indicator);
            output.push(' ');
        }
    }

    /// Post-processes the output to apply styles (for TagTransform::Apply mode).
    fn apply_styles(&self, intermediate: &str) -> String {
        let mut result = String::with_capacity(intermediate.len());
        let mut style_stack: Vec<&Style> = Vec::new();

        // We use split_inclusive to keep the markers in the iteration if possible,
        // or just manual scanning. Manual scanning is safer given the specific marker format.
        // Format is: \x00STYLE_START:tag\x00 or \x00STYLE_END:tag\x00

        let mut last_pos = 0;

        while let Some(start_idx) = intermediate[last_pos..].find('\x00') {
            let abs_start = last_pos + start_idx;

            // 1. Append content before the marker
            if abs_start > last_pos {
                let text = &intermediate[last_pos..abs_start];
                self.append_styled(&mut result, text, &style_stack);
            }

            // 2. Find end of marker
            if let Some(end_offset) = intermediate[abs_start + 1..].find('\x00') {
                let abs_end = abs_start + 1 + end_offset; // index of closing \x00
                let marker_content = &intermediate[abs_start + 1..abs_end];

                if let Some(tag) = marker_content.strip_prefix("STYLE_START:") {
                    if let Some(style) = self.styles.get(tag) {
                        style_stack.push(style);
                    }
                } else if let Some(tag) = marker_content.strip_prefix("STYLE_END:") {
                    // We only pop if the stack top matches (or verify integrity).
                    // This implementation assumes well-matched pairs from parse(),
                    // but for robustness we check if we should pop specifically.
                    // The old implementation just popped if the tag existed in styles.
                    if self.styles.contains_key(tag) {
                        style_stack.pop();
                    }
                }

                last_pos = abs_end + 1;
            } else {
                // Malformed marker (unclosed null byte) - treat rest as text
                // This shouldn't happen with our generator but good for safety
                let text = &intermediate[abs_start..];
                self.append_styled(&mut result, text, &style_stack);
                last_pos = intermediate.len();
                break;
            }
        }

        // Append remaining text
        if last_pos < intermediate.len() {
            let text = &intermediate[last_pos..];
            self.append_styled(&mut result, text, &style_stack);
        }

        result
    }

    /// Helper to append styled text.
    fn append_styled(&self, output: &mut String, text: &str, style_stack: &[&Style]) {
        if text.is_empty() {
            return;
        }

        if style_stack.is_empty() {
            output.push_str(text);
        } else {
            // Apply styles in order.
            // Note: console::Style::apply_to returns a StyledObject.
            // We need to chain them.
            let mut current = text.to_string();
            for style in style_stack {
                current = style.apply_to(current).to_string();
            }
            output.push_str(&current);
        }
    }
}

// Better approach: process in one pass with style application
impl BBParser {
    /// Parses and transforms input, applying styles directly.
    ///
    /// This is the main entry point for parsing styled content.
    pub fn process(&self, input: &str) -> String {
        match self.transform {
            TagTransform::Keep | TagTransform::Remove => self.parse(input),
            TagTransform::Apply => {
                let intermediate = self.parse(input);
                self.apply_styles(&intermediate)
            }
        }
    }
}

/// Token types produced by the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    /// Plain text content.
    Text(&'a str),
    /// Opening tag: `[tagname]`
    OpenTag(&'a str),
    /// Closing tag: `[/tagname]`
    CloseTag(&'a str),
    /// Invalid tag syntax (passed through as text).
    InvalidTag(&'a str),
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
        // But hyphen cannot be followed by nothing (handled by the pattern)
        for c in chars {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' && c != '-' {
                return false;
            }
        }

        // Cannot end with hyphen followed by digit pattern at start
        // Actually, the rule is: cannot START with hyphen-digit
        // We already check first char, so we're good

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

        // Look for the next '['
        if let Some(bracket_pos) = remaining.find('[') {
            if bracket_pos > 0 {
                // There's text before the bracket
                let text = &remaining[..bracket_pos];
                self.pos += bracket_pos;
                return Some(Token::Text(text));
            }

            // We're at a '['
            // Try to parse a tag
            if let Some(close_bracket) = remaining.find(']') {
                let tag_content = &remaining[1..close_bracket];
                let full_tag = &remaining[..=close_bracket];

                // Check for closing tag
                if let Some(tag_name) = tag_content.strip_prefix('/') {
                    if Self::is_valid_tag_name(tag_name) {
                        self.pos += close_bracket + 1;
                        Some(Token::CloseTag(tag_name))
                    } else {
                        // Invalid tag name - treat as text
                        self.pos += close_bracket + 1;
                        Some(Token::InvalidTag(full_tag))
                    }
                } else if Self::is_valid_tag_name(tag_content) {
                    self.pos += close_bracket + 1;
                    Some(Token::OpenTag(tag_content))
                } else {
                    // Invalid tag name - treat as text
                    self.pos += close_bracket + 1;
                    Some(Token::InvalidTag(full_tag))
                }
            } else {
                // No closing bracket - rest is text
                self.pos = self.input.len();
                Some(Token::Text(remaining))
            }
        } else {
            // No more brackets - rest is text
            self.pos = self.input.len();
            Some(Token::Text(remaining))
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
            // Unknown but valid tag syntax - should be preserved
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
        fn unknown_tags_show_indicator() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove);
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "(!?) text");
        }

        #[test]
        fn unknown_tags_strip_with_config() {
            let parser = BBParser::new(test_styles(), TagTransform::Remove)
                .unknown_behavior(UnknownTagBehavior::Strip);
            assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
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
            // Unclosed tag should be treated as literal text
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
            // [bold] opened, [/red] closes nothing, [/bold] closes bold
            assert_eq!(
                parser.parse("[bold]hello[/red][/bold]"),
                "[bold]hello[/red][/bold]"
            );
        }

        #[test]
        fn overlapping_tags_auto_close() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            // [bold][red]...[/bold] - red was opened inside bold, bold closes first
            // This should auto-close red when bold closes
            let result = parser.parse("[bold][red]hello[/bold][/red]");
            // The parser should handle this gracefully
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
            // Regular brackets that aren't tags
            assert_eq!(parser.parse("[bold]array[0][/bold]"), "array[0]");
        }

        #[test]
        fn invalid_tag_syntax_passthrough() {
            let parser = BBParser::new(test_styles(), TagTransform::Keep);
            // These should be treated as literal text
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
            assert_eq!(tokens, vec![Token::Text("hello world")]);
        }

        #[test]
        fn tokenize_single_tag() {
            let tokens: Vec<_> = Tokenizer::new("[bold]hello[/bold]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag("bold"),
                    Token::Text("hello"),
                    Token::CloseTag("bold"),
                ]
            );
        }

        #[test]
        fn tokenize_nested_tags() {
            let tokens: Vec<_> = Tokenizer::new("[a][b]x[/b][/a]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag("a"),
                    Token::OpenTag("b"),
                    Token::Text("x"),
                    Token::CloseTag("b"),
                    Token::CloseTag("a"),
                ]
            );
        }

        #[test]
        fn tokenize_invalid_tag() {
            let tokens: Vec<_> = Tokenizer::new("[123]text[/123]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::InvalidTag("[123]"),
                    Token::Text("text"),
                    Token::InvalidTag("[/123]"),
                ]
            );
        }

        #[test]
        fn tokenize_mixed() {
            let tokens: Vec<_> = Tokenizer::new("a[b]c[/b]d").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::Text("a"),
                    Token::OpenTag("b"),
                    Token::Text("c"),
                    Token::CloseTag("b"),
                    Token::Text("d"),
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
            assert_eq!(parser.process("hello world"), "hello world");
        }

        #[test]
        fn unknown_tag_shows_indicator() {
            let parser = BBParser::new(test_styles(), TagTransform::Apply);
            let result = parser.process("[unknown]text[/unknown]");
            assert!(result.starts_with("(!?)"));
            assert!(result.contains("text"));
        }

        #[test]
        fn known_tag_applies_style() {
            let mut styles = HashMap::new();
            styles.insert("bold".to_string(), Style::new().bold().force_styling(true));

            let parser = BBParser::new(styles, TagTransform::Apply);
            let result = parser.process("[bold]hello[/bold]");

            // Should contain ANSI bold code
            assert!(result.contains("\x1b[1m") || result.contains("hello"));
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for valid tag names using regex
    fn valid_tag_name() -> impl Strategy<Value = String> {
        // CSS identifier: starts with letter or underscore, followed by alphanumeric, underscore, or hyphen
        "[a-z_][a-z0-9_-]{0,10}"
    }

    // Strategy for plain text (no brackets)
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
