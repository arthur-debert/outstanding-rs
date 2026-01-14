//! Utility functions for ANSI-aware text measurement, truncation, and padding.
//!
//! All functions in this module correctly handle ANSI escape codes: they are
//! preserved in output but don't count toward display width calculations.

use console::{measure_text_width, pad_str, Alignment};

/// Returns the display width of a string, ignoring ANSI escape codes.
///
/// This is a convenience wrapper around `console::measure_text_width` that
/// correctly handles:
/// - ANSI escape sequences (colors, styles)
/// - Unicode characters including CJK wide characters
/// - Zero-width characters and combining marks
///
/// # Example
///
/// ```rust
/// use outstanding::table::display_width;
///
/// assert_eq!(display_width("hello"), 5);
/// assert_eq!(display_width("\x1b[31mred\x1b[0m"), 3);  // ANSI codes ignored
/// assert_eq!(display_width("日本"), 4);  // CJK characters are 2 columns each
/// ```
pub fn display_width(s: &str) -> usize {
    measure_text_width(s)
}

/// Truncates a string from the end to fit within a maximum display width.
///
/// If the string already fits, it is returned unchanged. Otherwise, characters
/// are removed from the end and the ellipsis is appended.
///
/// ANSI escape codes are preserved but don't count toward display width.
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width in terminal columns
/// * `ellipsis` - String to append when truncation occurs (e.g., "…" or "...")
///
/// # Example
///
/// ```rust
/// use outstanding::table::truncate_end;
///
/// assert_eq!(truncate_end("Hello World", 8, "…"), "Hello W…");
/// assert_eq!(truncate_end("Short", 10, "…"), "Short");
/// ```
pub fn truncate_end(s: &str, max_width: usize, ellipsis: &str) -> String {
    let width = measure_text_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = measure_text_width(ellipsis);
    if max_width < ellipsis_width {
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let mut result = truncate_to_display_width(s, target_width);
    result.push_str(ellipsis);
    result
}

/// Truncates a string from the start to fit within a maximum display width.
///
/// Characters are removed from the beginning, and the ellipsis is prepended.
/// Useful for paths where the filename at the end is more important than
/// the directory prefix.
///
/// ANSI escape codes are preserved but don't count toward display width.
///
/// # Example
///
/// ```rust
/// use outstanding::table::truncate_start;
///
/// assert_eq!(truncate_start("Hello World", 8, "…"), "…o World");
/// assert_eq!(truncate_start("/path/to/file.rs", 12, "…"), "…/to/file.rs");
/// ```
pub fn truncate_start(s: &str, max_width: usize, ellipsis: &str) -> String {
    let width = measure_text_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = measure_text_width(ellipsis);
    if max_width < ellipsis_width {
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let truncated = find_suffix_with_width(s, target_width);
    format!("{}{}", ellipsis, truncated)
}

/// Truncates a string from the middle to fit within a maximum display width.
///
/// Characters are removed from the middle, preserving both start and end.
/// The ellipsis is placed in the middle. Useful for identifiers or filenames
/// where both prefix and suffix are meaningful.
///
/// ANSI escape codes are preserved but don't count toward display width.
///
/// # Example
///
/// ```rust
/// use outstanding::table::truncate_middle;
///
/// assert_eq!(truncate_middle("Hello World", 8, "…"), "Hel…orld");
/// assert_eq!(truncate_middle("abcdefghij", 7, "..."), "ab...ij");
/// ```
pub fn truncate_middle(s: &str, max_width: usize, ellipsis: &str) -> String {
    let width = measure_text_width(s);
    if width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = measure_text_width(ellipsis);
    if max_width < ellipsis_width {
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let available = max_width - ellipsis_width;
    let right_width = available.div_ceil(2); // Bias toward end (more useful info usually)
    let left_width = available - right_width;

    let left = truncate_to_display_width(s, left_width);
    let right = find_suffix_with_width(s, right_width);

    format!("{}{}{}", left, ellipsis, right)
}

/// Pads a string on the left (right-aligns) to reach the target width.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// # Example
///
/// ```rust
/// use outstanding::table::pad_left;
///
/// assert_eq!(pad_left("42", 5), "   42");
/// assert_eq!(pad_left("hello", 3), "hello");  // No truncation
/// ```
pub fn pad_left(s: &str, width: usize) -> String {
    pad_str(s, width, Alignment::Right, None).into_owned()
}

/// Pads a string on the right (left-aligns) to reach the target width.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// # Example
///
/// ```rust
/// use outstanding::table::pad_right;
///
/// assert_eq!(pad_right("42", 5), "42   ");
/// assert_eq!(pad_right("hello", 3), "hello");  // No truncation
/// ```
pub fn pad_right(s: &str, width: usize) -> String {
    pad_str(s, width, Alignment::Left, None).into_owned()
}

/// Pads a string on both sides (centers) to reach the target width.
///
/// When the remaining space is odd, the extra space goes on the right.
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// # Example
///
/// ```rust
/// use outstanding::table::pad_center;
///
/// assert_eq!(pad_center("hi", 6), "  hi  ");
/// assert_eq!(pad_center("hi", 5), " hi  ");  // Extra space on right
/// ```
pub fn pad_center(s: &str, width: usize) -> String {
    pad_str(s, width, Alignment::Center, None).into_owned()
}

// --- Internal helpers ---

/// Truncate string to fit display width, keeping characters from the start.
/// Handles ANSI escape codes properly.
fn truncate_to_display_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    // Fast path: if string fits, return as-is
    if measure_text_width(s) <= max_width {
        return s.to_string();
    }

    // We need to walk through the string carefully, tracking both
    // printable width and ANSI escape sequences
    let mut result = String::new();
    let mut current_width = 0;
    let chars = s.chars().peekable();
    let mut in_escape = false;

    for c in chars {
        if c == '\x1b' {
            // Start of ANSI escape sequence - include it all
            result.push(c);
            in_escape = true;
            continue;
        }

        if in_escape {
            result.push(c);
            // ANSI CSI sequences end with a letter (@ through ~)
            if c.is_ascii_alphabetic() || c == '~' {
                in_escape = false;
            }
            continue;
        }

        // Regular character - check width
        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if current_width + char_width > max_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }

    result
}

/// Find the longest suffix of s that has display width <= max_width.
fn find_suffix_with_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let total_width = measure_text_width(s);
    if total_width <= max_width {
        return s.to_string();
    }

    // Linear scan from the start to find where to cut.
    // We need to skip (total_width - max_width) display columns.
    let skip_width = total_width - max_width;

    let mut current_width = 0;
    let mut byte_offset = 0;
    let mut in_escape = false;

    for (i, c) in s.char_indices() {
        if c == '\x1b' {
            in_escape = true;
            byte_offset = i + c.len_utf8();
            continue;
        }

        if in_escape {
            byte_offset = i + c.len_utf8();
            if c.is_ascii_alphabetic() || c == '~' {
                in_escape = false;
            }
            continue;
        }

        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        current_width += char_width;
        byte_offset = i + c.len_utf8();

        if current_width >= skip_width {
            break;
        }
    }

    s[byte_offset..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- display_width tests ---

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

    // --- truncate_end tests ---

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

    // --- truncate_start tests ---

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

    // --- truncate_middle tests ---

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
        // 10 chars, max 6, ellipsis 1 = 5 available, split 2/3 (bias toward end)
        assert_eq!(truncate_middle("abcdefghij", 6, "…"), "ab…hij");
    }

    // --- pad_left tests ---

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

    // --- pad_right tests ---

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

    // --- pad_center tests ---

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

    // --- Edge cases ---

    #[test]
    fn empty_string_operations() {
        assert_eq!(display_width(""), 0);
        assert_eq!(truncate_end("", 5, "…"), "");
        assert_eq!(truncate_start("", 5, "…"), "");
        assert_eq!(truncate_middle("", 5, "…"), "");
        assert_eq!(pad_left("", 0), "");
        assert_eq!(pad_right("", 0), "");
    }

    #[test]
    fn zero_width_target() {
        assert_eq!(truncate_end("hello", 0, "…"), "");
        assert_eq!(truncate_start("hello", 0, "…"), "");
        assert_eq!(truncate_middle("hello", 0, "…"), "");
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

            // If string fits, it should be unchanged
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

            // When target is smaller, string should be unchanged
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
    }
}
