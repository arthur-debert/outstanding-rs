mod truncate;
mod visible_wrap;
mod wrap;

pub use truncate::{
    truncate_end, truncate_end_with_policy, truncate_middle, truncate_middle_with_policy,
    truncate_start, truncate_start_with_policy,
};
pub(crate) use truncate::{
    truncate_visible_end_with_policy, truncate_visible_middle_with_policy,
    truncate_visible_start_with_policy,
};
pub(crate) use visible_wrap::wrap_visible_indent_with_policy;
pub use wrap::{wrap, wrap_indent, wrap_indent_with_policy, wrap_with_policy};

use crate::{AmbiguousWidth, WidthCalculator};

pub fn display_width(s: &str) -> usize {
    display_width_with_policy(s, AmbiguousWidth::Narrow)
}

pub fn display_width_with_policy(s: &str, policy: AmbiguousWidth) -> usize {
    WidthCalculator::new(policy).display_width(s)
}

pub fn visible_width(s: &str) -> usize {
    visible_width_with_policy(s, AmbiguousWidth::Narrow)
}

pub fn visible_width_with_policy(s: &str, policy: AmbiguousWidth) -> usize {
    WidthCalculator::new(policy).visible_width(s)
}

pub fn pad_left(s: &str, width: usize) -> String {
    pad_left_with_policy(s, width, AmbiguousWidth::Narrow)
}

pub fn pad_left_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    format!("{}{}", " ".repeat(padding), s)
}

pub fn pad_right(s: &str, width: usize) -> String {
    pad_right_with_policy(s, width, AmbiguousWidth::Narrow)
}

pub fn pad_right_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    format!("{}{}", s, " ".repeat(padding))
}

pub fn pad_center(s: &str, width: usize) -> String {
    pad_center_with_policy(s, width, AmbiguousWidth::Narrow)
}

pub fn pad_center_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    let left = padding / 2;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(padding - left))
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::strip_ansi_codes;

    #[test]
    fn display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width(" "), 1);
    }

    #[test]
    fn display_width_ansi() {
        assert_eq!(display_width("\x1b[31mred\x1b[0m"), 3);
        assert_eq!(display_width("\x1b[1;32mbold green\x1b[0m"), 10);
        assert_eq!(display_width("\x1b[38;5;196mcolor\x1b[0m"), 5);
    }

    #[test]
    fn display_width_unicode() {
        assert_eq!(display_width("日本語"), 6); // 3 chars, 2 columns each
        assert_eq!(display_width("café"), 4);
        assert_eq!(display_width("🎉"), 2); // Emoji typically 2 columns
    }

    #[test]
    fn policy_aware_helpers_share_ambiguous_width_measurement() {
        let text = "↦≈Δ";
        assert_eq!(display_width_with_policy(text, AmbiguousWidth::Narrow), 3);
        assert_eq!(display_width_with_policy(text, AmbiguousWidth::Wide), 5);
        assert_eq!(
            pad_right_with_policy(text, 7, AmbiguousWidth::Narrow),
            "↦≈Δ    "
        );
        assert_eq!(
            pad_right_with_policy(text, 7, AmbiguousWidth::Wide),
            "↦≈Δ  "
        );
        assert_eq!(
            truncate_end_with_policy(text, 4, "…", AmbiguousWidth::Wide),
            "↦…"
        );
        assert_eq!(wrap_with_policy("≈ Δ", 2, AmbiguousWidth::Wide), ["≈", "Δ"]);
    }

    #[test]
    fn ansi_parser_compatibility_spans_measure_truncate_and_wrap() {
        for (open, close) in [("\x1b(0", "\x1b(B"), ("\u{9b}31m", "\u{9b}0m")] {
            let input = format!("{open}abcdefghij{close}");
            assert_eq!(display_width(&input), 10);

            let end = truncate_end(&input, 5, "…");
            let start = truncate_start(&input, 5, "…");
            let middle = truncate_middle(&input, 5, "…");
            assert_eq!(strip_ansi_codes(&end), "abcd…");
            assert_eq!(strip_ansi_codes(&start), "…ghij");
            assert_eq!(strip_ansi_codes(&middle), "ab…ij");
            for result in [&end, &start, &middle] {
                assert_eq!(display_width(result), 5);
            }

            let lines = wrap(&input, 5);
            assert!(lines.iter().all(|line| display_width(line) <= 5));
            let plain = lines
                .iter()
                .map(|line| strip_ansi_codes(line).replace('…', ""))
                .collect::<String>();
            assert_eq!(plain, "abcdefghij");
        }
    }

    #[test]
    fn pad_left_basic() {
        assert_eq!(pad_left("42", 5), "   42");
        assert_eq!(pad_left("hello", 10), "     hello");
    }

    #[test]
    fn pad_left_no_padding_needed() {
        assert_eq!(pad_left("hello", 5), "hello");
        assert_eq!(pad_left("hello", 3), "hello"); // No truncation
    }

    #[test]
    fn pad_left_empty() {
        assert_eq!(pad_left("", 5), "     ");
    }

    #[test]
    fn pad_left_ansi() {
        let styled = "\x1b[31mhi\x1b[0m";
        let result = pad_left(styled, 5);
        assert!(result.ends_with("\x1b[0m"));
        assert_eq!(display_width(&result), 5);
    }

    #[test]
    fn pad_right_basic() {
        assert_eq!(pad_right("42", 5), "42   ");
        assert_eq!(pad_right("hello", 10), "hello     ");
    }

    #[test]
    fn pad_right_no_padding_needed() {
        assert_eq!(pad_right("hello", 5), "hello");
        assert_eq!(pad_right("hello", 3), "hello");
    }

    #[test]
    fn pad_right_empty() {
        assert_eq!(pad_right("", 5), "     ");
    }

    #[test]
    fn pad_center_basic() {
        assert_eq!(pad_center("hi", 6), "  hi  ");
    }

    #[test]
    fn pad_center_odd_space() {
        assert_eq!(pad_center("hi", 5), " hi  "); // Extra space on right
    }

    #[test]
    fn pad_center_no_padding() {
        assert_eq!(pad_center("hello", 5), "hello");
        assert_eq!(pad_center("hello", 3), "hello");
    }

    #[test]
    fn pad_center_empty() {
        assert_eq!(pad_center("", 4), "    ");
    }

    #[test]
    fn empty_string_operations() {
        assert_eq!(display_width(""), 0);
        assert_eq!(truncate_end("", 5, "…"), "");
        assert_eq!(truncate_start("", 5, "…"), "");
        assert_eq!(truncate_middle("", 5, "…"), "");
        assert_eq!(pad_left("", 0), "");
        assert_eq!(pad_right("", 0), "");
    }
}
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn truncate_end_respects_max_width(
            s in "[a-zA-Z0-9 ]{0,100}",
            max_width in 0usize..50,
        ) {
            let result = truncate_end(&s, max_width, "…");
            let result_width = display_width(&result);
            prop_assert!(
                result_width <= max_width,
                "truncate_end exceeded max_width: result '{}' has width {}, max was {}",
                result, result_width, max_width
            );
        }

        #[test]
        fn truncate_start_respects_max_width(
            s in "[a-zA-Z0-9 ]{0,100}",
            max_width in 0usize..50,
        ) {
            let result = truncate_start(&s, max_width, "…");
            let result_width = display_width(&result);
            prop_assert!(
                result_width <= max_width,
                "truncate_start exceeded max_width: result '{}' has width {}, max was {}",
                result, result_width, max_width
            );
        }

        #[test]
        fn truncate_middle_respects_max_width(
            s in "[a-zA-Z0-9 ]{0,100}",
            max_width in 0usize..50,
        ) {
            let result = truncate_middle(&s, max_width, "…");
            let result_width = display_width(&result);
            prop_assert!(
                result_width <= max_width,
                "truncate_middle exceeded max_width: result '{}' has width {}, max was {}",
                result, result_width, max_width
            );
        }

        #[test]
        fn truncate_preserves_short_strings(
            s in "[a-zA-Z0-9]{0,20}",
            extra_width in 0usize..30,
        ) {
            let width = display_width(&s);
            let max_width = width + extra_width;

            prop_assert_eq!(truncate_end(&s, max_width, "…"), s.clone());
            prop_assert_eq!(truncate_start(&s, max_width, "…"), s.clone());
            prop_assert_eq!(truncate_middle(&s, max_width, "…"), s);
        }

        #[test]
        fn pad_produces_exact_width_when_larger(
            s in "[a-zA-Z0-9]{0,20}",
            extra in 1usize..30,
        ) {
            let original_width = display_width(&s);
            let target_width = original_width + extra;

            prop_assert_eq!(display_width(&pad_left(&s, target_width)), target_width);
            prop_assert_eq!(display_width(&pad_right(&s, target_width)), target_width);
            prop_assert_eq!(display_width(&pad_center(&s, target_width)), target_width);
        }

        #[test]
        fn pad_preserves_content_when_smaller(
            s in "[a-zA-Z0-9]{1,30}",
        ) {
            let original_width = display_width(&s);
            let target_width = original_width.saturating_sub(5);

            prop_assert_eq!(pad_left(&s, target_width), s.clone());
            prop_assert_eq!(pad_right(&s, target_width), s.clone());
            prop_assert_eq!(pad_center(&s, target_width), s);
        }

        #[test]
        fn truncate_end_contains_ellipsis_when_truncated(
            s in "[a-zA-Z0-9]{10,50}",
            max_width in 3usize..9,
        ) {
            let result = truncate_end(&s, max_width, "…");
            if display_width(&s) > max_width {
                prop_assert!(
                    result.contains("…"),
                    "truncated string should contain ellipsis"
                );
            }
        }

        #[test]
        fn truncate_start_contains_ellipsis_when_truncated(
            s in "[a-zA-Z0-9]{10,50}",
            max_width in 3usize..9,
        ) {
            let result = truncate_start(&s, max_width, "…");
            if display_width(&s) > max_width {
                prop_assert!(
                    result.contains("…"),
                    "truncated string should contain ellipsis"
                );
            }
        }

        #[test]
        fn truncate_middle_contains_ellipsis_when_truncated(
            s in "[a-zA-Z0-9]{10,50}",
            max_width in 3usize..9,
        ) {
            let result = truncate_middle(&s, max_width, "…");
            if display_width(&s) > max_width {
                prop_assert!(
                    result.contains("…"),
                    "truncated string should contain ellipsis"
                );
            }
        }

        #[test]
        fn wrap_all_lines_respect_width(
            s in "[a-zA-Z]{1,10}( [a-zA-Z]{1,10}){0,10}",
            width in 5usize..30,
        ) {
            let lines = wrap(&s, width);
            for line in &lines {
                let line_width = display_width(line);
                prop_assert!(
                    line_width <= width,
                    "wrap produced line '{}' with width {}, max was {}",
                    line, line_width, width
                );
            }
        }

        #[test]
        fn wrap_preserves_all_words(
            words in prop::collection::vec("[a-zA-Z]{1,8}", 1..10),
            width in 10usize..40,
        ) {
            let input = words.join(" ");
            let lines = wrap(&input, width);
            let rejoined = lines.join(" ");

            for word in &words {
                prop_assert!(
                    rejoined.contains(word),
                    "word '{}' missing from wrapped output",
                    word
                );
            }
        }

        #[test]
        fn wrap_indent_continuation_lines_are_indented(
            s in "[a-zA-Z]{1,5}( [a-zA-Z]{1,5}){3,8}",
            width in 10usize..20,
            indent in 1usize..4,
        ) {
            let lines = wrap_indent(&s, width, indent);
            if lines.len() > 1 {
                let indent_str: String = " ".repeat(indent);
                for line in lines.iter().skip(1) {
                    prop_assert!(
                        line.starts_with(&indent_str),
                        "continuation line '{}' should start with {} spaces",
                        line, indent
                    );
                }
            }
        }

        #[test]
        fn wrap_nonempty_input_produces_nonempty_output(
            s in "[a-zA-Z]{1,20}",
            width in 1usize..30,
        ) {
            let lines = wrap(&s, width);
            prop_assert!(
                !lines.is_empty(),
                "non-empty input '{}' should produce non-empty output",
                s
            );
        }
    }
}
