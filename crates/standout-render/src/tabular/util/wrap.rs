use super::truncate::{take_prefix_to_display_width, truncate_to_display_width};
use crate::{AmbiguousWidth, WidthCalculator};

pub fn wrap(s: &str, width: usize) -> Vec<String> {
    wrap_with_policy(s, width, AmbiguousWidth::Narrow)
}

pub fn wrap_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> Vec<String> {
    wrap_indent_with_policy(s, width, 0, policy)
}

pub fn wrap_indent(s: &str, width: usize, indent: usize) -> Vec<String> {
    wrap_indent_with_policy(s, width, indent, AmbiguousWidth::Narrow)
}

pub fn wrap_indent_with_policy(
    s: &str,
    width: usize,
    indent: usize,
    policy: AmbiguousWidth,
) -> Vec<String> {
    let calculator = WidthCalculator::new(policy);
    if width == 0 {
        return vec![];
    }

    let s = s.trim();
    if s.is_empty() {
        return vec![];
    }

    if calculator.display_width(s) <= width {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut is_first_line = true;

    for word in s.split_whitespace() {
        let word_width = calculator.display_width(word);
        let effective_width = if is_first_line {
            width
        } else {
            width.saturating_sub(indent)
        };

        if word_width > effective_width {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0;
                is_first_line = false;
            }

            let broken = break_long_word(word, effective_width, indent, is_first_line, calculator);
            let broken_len = broken.len();
            for (i, part) in broken.into_iter().enumerate() {
                if i == 0 && is_first_line {
                    lines.push(part);
                    is_first_line = false;
                } else if i < broken_len - 1 {
                    lines.push(part);
                } else {
                    current_line = part;
                    current_width = calculator.display_width(&current_line);
                }
            }
            continue;
        }

        let needed_width = if current_line.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width // +1 for space
        };

        if needed_width <= effective_width {
            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += 1;
            }
            current_line.push_str(word);
            current_width += word_width;
        } else {
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            is_first_line = false;

            let indent_str: String = " ".repeat(indent);
            current_line = format!("{}{}", indent_str, word);
            current_width = indent + word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() && !s.is_empty() {
        lines.push(truncate_to_display_width(s, width, calculator));
    }

    lines
}

fn break_long_word(
    word: &str,
    width: usize,
    indent: usize,
    is_first: bool,
    calculator: WidthCalculator,
) -> Vec<String> {
    let mut parts = Vec::new();
    let mut remaining = word;
    let mut first_part = is_first;

    while !remaining.is_empty() {
        let effective_width = if first_part {
            width
        } else {
            width.saturating_sub(indent)
        };

        if effective_width == 0 {
            break;
        }

        let remaining_width = calculator.display_width(remaining);
        if remaining_width <= effective_width {
            let prefix = if first_part {
                String::new()
            } else {
                " ".repeat(indent)
            };
            parts.push(format!("{}{}", prefix, remaining));
            break;
        }

        let break_width = effective_width.saturating_sub(1); // -1 for "…"
        if break_width == 0 {
            let prefix = if first_part {
                String::new()
            } else {
                " ".repeat(indent)
            };
            parts.push(format!("{}…", prefix));
            break;
        }

        let prefix = if first_part {
            String::new()
        } else {
            " ".repeat(indent)
        };
        let (truncated, consumed_bytes) =
            take_prefix_to_display_width(remaining, break_width, calculator);
        parts.push(format!("{}{}…", prefix, truncated));

        // Advance by source bytes, not output characters: ANSI controls are
        // preserved in the selected prefix but occupy no display columns.
        remaining = &remaining[consumed_bytes..];
        first_part = false;
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::display_width;

    #[test]
    fn wrapping_a_styled_value_reads_the_same_across_the_line_break() {
        let styled = "\u{1b}[31mabcdefghijklmno\u{1b}[0m";

        assert_eq!(
            wrap(styled, 6),
            ["\u{1b}[31mabcde…", "fghij…", "klmno\u{1b}[0m"]
        );
        assert_eq!(
            wrap_indent(styled, 6, 2),
            ["\u{1b}[31mabcde…", "  fgh…", "  ijk…", "  lmno\u{1b}[0m"]
        );
        assert_eq!(
            wrap("\u{1b}[31malpha beta gamma delta\u{1b}[0m", 11),
            ["\u{1b}[31malpha beta", "gamma delta\u{1b}[0m"]
        );
    }

    #[test]
    fn wrap_single_line_fits() {
        assert_eq!(wrap("hello world", 20), vec!["hello world"]);
        assert_eq!(wrap("short", 10), vec!["short"]);
    }

    #[test]
    fn wrap_basic_multiline() {
        assert_eq!(wrap("hello world foo", 11), vec!["hello world", "foo"]);
        assert_eq!(
            wrap("one two three four", 10),
            vec!["one two", "three four"]
        );
    }

    #[test]
    fn wrap_exact_fit() {
        assert_eq!(wrap("hello", 5), vec!["hello"]);
        assert_eq!(wrap("hello world", 11), vec!["hello world"]);
    }

    #[test]
    fn wrap_empty_string() {
        let result: Vec<String> = wrap("", 10);
        assert!(result.is_empty());
    }

    #[test]
    fn wrap_whitespace_only() {
        let result: Vec<String> = wrap("   ", 10);
        assert!(result.is_empty());
    }

    #[test]
    fn wrap_zero_width() {
        let result: Vec<String> = wrap("hello", 0);
        assert!(result.is_empty());
    }

    #[test]
    fn wrap_single_word_per_line() {
        assert_eq!(wrap("a b c d", 1), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn wrap_long_word_force_break() {
        let result = wrap("supercalifragilistic", 10);
        assert!(result.len() >= 2, "should produce multiple lines");
        for line in &result {
            assert!(display_width(line) <= 10, "line '{}' exceeds width", line);
        }
    }

    #[test]
    fn wrap_preserves_word_boundaries() {
        let result = wrap("hello world test", 10);
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "world test");
    }

    #[test]
    fn wrap_multiple_spaces_normalized_when_wrapping() {
        let result = wrap("hello    world    foo", 12);
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    #[test]
    fn wrap_indent_basic() {
        let result = wrap_indent("hello world foo bar", 12, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "hello world");
        assert!(result[1].starts_with("  ")); // 2-space indent
    }

    #[test]
    fn wrap_indent_no_wrap_needed() {
        assert_eq!(wrap_indent("short", 20, 4), vec!["short"]);
    }

    #[test]
    fn wrap_indent_multiple_lines() {
        let result = wrap_indent("one two three four five six", 10, 2);
        assert!(!result[0].starts_with(' '));
        for line in result.iter().skip(1) {
            assert!(line.starts_with("  "), "continuation should be indented");
        }
    }

    #[test]
    fn wrap_indent_zero_indent() {
        let result = wrap_indent("hello world foo", 11, 0);
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    #[test]
    fn wrap_cjk_characters() {
        let result = wrap("日本語 テスト", 8);
        assert_eq!(result.len(), 2);
        for line in &result {
            assert!(display_width(line) <= 8);
        }
    }
}
