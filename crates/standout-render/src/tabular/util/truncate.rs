use crate::width::VisibleTruncateAt;
use crate::{AmbiguousWidth, WidthCalculator};
use standout_bbparser::ansi::{ansi_units, closing_for};

pub(crate) fn truncate_visible_end_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    WidthCalculator::new(policy).truncate_visible(s, max_width, ellipsis, VisibleTruncateAt::End)
}

pub(crate) fn truncate_visible_start_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    WidthCalculator::new(policy).truncate_visible(s, max_width, ellipsis, VisibleTruncateAt::Start)
}

pub(crate) fn truncate_visible_middle_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    WidthCalculator::new(policy).truncate_visible(s, max_width, ellipsis, VisibleTruncateAt::Middle)
}

pub fn truncate_end(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_end_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

pub fn truncate_end_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    let calculator = WidthCalculator::new(policy);
    let width = calculator.display_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = calculator.display_width(ellipsis);
    if max_width < ellipsis_width {
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let mut result = truncate_to_display_width(s, target_width, calculator);
    result.push_str(ellipsis);
    result
}

pub fn truncate_start(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_start_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

pub fn truncate_start_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    let calculator = WidthCalculator::new(policy);
    let width = calculator.display_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = calculator.display_width(ellipsis);
    if max_width < ellipsis_width {
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let truncated = find_suffix_with_width(s, target_width, calculator);
    format!("{}{}", ellipsis, truncated)
}

pub fn truncate_middle(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_middle_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

pub fn truncate_middle_with_policy(
    s: &str,
    max_width: usize,
    ellipsis: &str,
    policy: AmbiguousWidth,
) -> String {
    let calculator = WidthCalculator::new(policy);
    let width = calculator.display_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = calculator.display_width(ellipsis);
    if max_width < ellipsis_width {
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        return ellipsis.to_string();
    }

    let available = max_width - ellipsis_width;
    let right_width = available.div_ceil(2); // Bias toward end (more useful info usually)
    let left_width = available - right_width;

    let left = truncate_to_display_width(s, left_width, calculator);
    let right = find_suffix_with_width(s, right_width, calculator);

    format!("{}{}{}", left, ellipsis, right)
}

pub(super) fn truncate_to_display_width(
    s: &str,
    max_width: usize,
    calculator: WidthCalculator,
) -> String {
    let (mut prefix, consumed_bytes) = take_prefix_to_display_width(s, max_width, calculator);
    if consumed_bytes < s.len() {
        prefix.push_str(closing_for(&prefix));
    }
    prefix
}

pub(super) fn take_prefix_to_display_width(
    s: &str,
    max_width: usize,
    calculator: WidthCalculator,
) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }

    if calculator.display_width(s) <= max_width {
        return (s.to_string(), s.len());
    }

    let mut result = String::new();
    let mut current_width = 0;
    'units: for unit in ansi_units(s) {
        if unit.is_escape {
            result.push_str(unit.text);
            continue;
        }

        for character in unit.text.chars() {
            let char_width = calculator.char_width(character);
            if current_width + char_width > max_width {
                break 'units;
            }
            result.push(character);
            current_width += char_width;
        }
    }

    let consumed_bytes = result.len();
    (result, consumed_bytes)
}

fn find_suffix_with_width(s: &str, max_width: usize, calculator: WidthCalculator) -> String {
    if max_width == 0 {
        return String::new();
    }

    let total_width = calculator.display_width(s);
    if total_width <= max_width {
        return s.to_string();
    }

    let skip_width = total_width - max_width;

    let mut current_width = 0;
    let mut byte_offset = 0;

    'units: for unit in ansi_units(s) {
        if unit.is_escape {
            byte_offset = unit.offset + unit.text.len();
            continue;
        }

        for (unit_offset, character) in unit.text.char_indices() {
            current_width += calculator.char_width(character);
            byte_offset = unit.offset + unit_offset + character.len_utf8();

            if current_width >= skip_width {
                break 'units;
            }
        }
    }

    s[byte_offset..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::display_width;

    #[test]
    fn truncating_a_styled_value_closes_the_colour_it_cut() {
        let styled = "\u{1b}[31mabcdefghijklmno\u{1b}[0m";

        assert_eq!(truncate_end(styled, 6, "…"), "\u{1b}[31mabcde\u{1b}[0m…");
        assert_eq!(
            truncate_middle(styled, 8, "…"),
            "\u{1b}[31mabc\u{1b}[0m…lmno\u{1b}[0m"
        );
        assert_eq!(truncate_end(styled, 20, "…"), styled);
        assert_eq!(
            truncate_end("\u{1b}[31mred\u{1b}[0m ok", 5, "…"),
            "\u{1b}[31mred\u{1b}[0m …"
        );
        assert_eq!(truncate_end("abcdefgh", 5, "…"), "abcd…");
    }

    #[test]
    fn truncate_end_no_truncation() {
        assert_eq!(truncate_end("hello", 10, "…"), "hello");
        assert_eq!(truncate_end("hello", 5, "…"), "hello");
    }

    #[test]
    fn truncate_end_basic() {
        assert_eq!(truncate_end("hello world", 8, "…"), "hello w…");
        assert_eq!(truncate_end("hello world", 6, "…"), "hello…");
    }

    #[test]
    fn truncate_end_multi_char_ellipsis() {
        assert_eq!(truncate_end("hello world", 8, "..."), "hello...");
    }

    #[test]
    fn truncate_end_exact_fit() {
        assert_eq!(truncate_end("hello", 5, "…"), "hello");
    }

    #[test]
    fn truncate_end_tiny_width() {
        assert_eq!(truncate_end("hello", 1, "…"), "…");
        assert_eq!(truncate_end("hello", 0, "…"), "");
    }

    #[test]
    fn truncate_end_ansi() {
        let styled = "\x1b[31mhello world\x1b[0m";
        let result = truncate_end(styled, 8, "…");
        assert_eq!(display_width(&result), 8);
        assert!(result.contains("\x1b[31m")); // ANSI preserved
    }

    #[test]
    fn truncate_end_cjk() {
        assert_eq!(truncate_end("日本語テスト", 7, "…"), "日本語…"); // 3 chars (6 cols) + ellipsis
    }

    #[test]
    fn truncate_start_no_truncation() {
        assert_eq!(truncate_start("hello", 10, "…"), "hello");
    }

    #[test]
    fn truncate_start_basic() {
        assert_eq!(truncate_start("hello world", 8, "…"), "…o world");
    }

    #[test]
    fn truncate_start_path() {
        assert_eq!(truncate_start("/path/to/file.rs", 12, "…"), "…/to/file.rs");
    }

    #[test]
    fn truncate_start_tiny_width() {
        assert_eq!(truncate_start("hello", 1, "…"), "…");
        assert_eq!(truncate_start("hello", 0, "…"), "");
    }

    #[test]
    fn truncate_middle_no_truncation() {
        assert_eq!(truncate_middle("hello", 10, "…"), "hello");
    }

    #[test]
    fn truncate_middle_basic() {
        assert_eq!(truncate_middle("hello world", 8, "…"), "hel…orld");
    }

    #[test]
    fn truncate_middle_multi_char_ellipsis() {
        assert_eq!(truncate_middle("abcdefghij", 7, "..."), "ab...ij");
    }

    #[test]
    fn truncate_middle_tiny_width() {
        assert_eq!(truncate_middle("hello", 1, "…"), "…");
        assert_eq!(truncate_middle("hello", 0, "…"), "");
    }

    #[test]
    fn truncate_middle_even_split() {
        assert_eq!(truncate_middle("abcdefghij", 6, "…"), "ab…hij");
    }

    #[test]
    fn zero_width_target() {
        assert_eq!(truncate_end("hello", 0, "…"), "");
        assert_eq!(truncate_start("hello", 0, "…"), "");
        assert_eq!(truncate_middle("hello", 0, "…"), "");
    }
}
