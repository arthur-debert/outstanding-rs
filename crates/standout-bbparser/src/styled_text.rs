use crate::ansi::{ansi_units, AnsiBalance};
use crate::tokenizer::{compute_valid_tags, unescape, Token, Tokenizer};
use std::ops::Range;

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

#[derive(Debug)]
pub enum StyledTextEvent<'a> {
    Text(std::borrow::Cow<'a, str>),
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

    pub fn visit(&self, mut visitor: impl FnMut(StyledTextEvent<'_>)) {
        for event in &self.events {
            visitor(match event {
                StyledEvent::Text {
                    source,
                    unescape_brackets,
                } => StyledTextEvent::Text(if *unescape_brackets {
                    unescape(source)
                } else {
                    std::borrow::Cow::Borrowed(source)
                }),
                StyledEvent::OpenTag(name) => StyledTextEvent::OpenTag(name),
                StyledEvent::CloseTag(name) => StyledTextEvent::CloseTag(name),
            });
        }
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
        let mut ansi = AnsiBalance::default();
        let mut visible_index = 0;
        let mut started = false;
        let mut cut = false;

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
                                ansi.observe(raw);
                            }
                            return;
                        }
                        // A range covering the whole text never reaches here, so
                        // untruncated source passes through unbalanced.
                        if started && !cut && visible_index >= range.end {
                            result.push_str(ansi.closing());
                            cut = true;
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
    for unit in ansi_units(source) {
        if unit.is_escape {
            visitor(None, unit.text);
        } else {
            visit_plain_text_units(unit.text, unescape_brackets, &mut visitor);
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
                if matches!(next, '[' | ']' | '\\') {
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

#[cfg(test)]
mod styled_text_tests {
    use super::StyledText;

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
    fn a_cut_closes_the_ansi_style_it_leaves_open() {
        let text = StyledText::parse("\x1b[31malpha beta gamma\x1b[0m");
        assert_eq!(text.select_range(0..7), "\x1b[31malpha b\x1b[0m");

        let tagged = StyledText::parse("[row]\x1b[31malpha beta gamma\x1b[0m[/row]");
        assert_eq!(
            tagged.select_range(0..7),
            "[row]\x1b[31malpha b\x1b[0m[/row]"
        );
    }

    #[test]
    fn a_cut_that_leaves_nothing_open_adds_no_reset() {
        let text = StyledText::parse("\x1b[31mred\x1b[0m and plain");
        assert_eq!(text.select_range(0..5), "\x1b[31mred\x1b[0m a");

        let plain = StyledText::parse("alpha beta");
        assert_eq!(plain.select_range(0..5), "alpha");
    }

    #[test]
    fn a_cut_closes_the_less_common_sgr_groups_too() {
        for opener in [
            "\x1b[26m", "\x1b[51m", "\x1b[53m", "\x1b[60m", "\x1b[64m", "\x1b[73m", "\x1b[74m",
        ] {
            let source = format!("{opener}alpha beta");
            let text = StyledText::parse(&source);
            assert_eq!(
                text.select_range(0..5),
                format!("{opener}alpha\x1b[0m"),
                "{opener:?}"
            );
        }
    }

    #[test]
    fn a_cut_falling_on_a_tag_boundary_closes_the_ansi_after_the_tag() {
        let text = StyledText::parse("[row]\x1b[31mabc[/row]def");
        assert_eq!(text.select_range(0..3), "[row]\x1b[31mabc[/row]\x1b[0m");
    }

    #[test]
    fn a_range_covering_the_whole_text_leaves_unbalanced_source_alone() {
        let input = "[outer]hello[/outer]\x1b[31m";
        let text = StyledText::parse(input);
        assert_eq!(text.select_range(0..5), input);
    }

    #[test]
    fn a_suffix_range_carries_no_opener_and_mints_no_reset() {
        let text = StyledText::parse("\x1b[31malpha beta\x1b[0m");
        assert_eq!(text.select_range(6..10), "beta\x1b[0m");
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
