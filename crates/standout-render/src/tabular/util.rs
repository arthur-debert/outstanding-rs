//! Utility functions for ANSI-aware text measurement, truncation, and padding.
//!
//! All functions in this module correctly handle ANSI escape codes: they are
//! preserved in output but don't count toward display width calculations.

use crate::width::VisibleTruncateAt;
use crate::{AmbiguousWidth, WidthCalculator};
use console::AnsiCodeIterator;
use standout_bbparser::StyledText;
use std::ops::Range;

/// Returns the display width of a string, ignoring ANSI escape codes.
///
/// **Warning:** This only strips ANSI escape codes, not BBCode tags like
/// `[bold]...[/bold]`. For user-facing content that may contain BBCode markup,
/// use [`visible_width`] instead.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::display_width;
///
/// assert_eq!(display_width("hello"), 5);
/// assert_eq!(display_width("\x1b[31mred\x1b[0m"), 3);  // ANSI codes ignored
/// assert_eq!(display_width("日本"), 4);  // CJK characters are 2 columns each
/// ```
pub fn display_width(s: &str) -> usize {
    display_width_with_policy(s, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`display_width`].
pub fn display_width_with_policy(s: &str, policy: AmbiguousWidth) -> usize {
    WidthCalculator::new(policy).display_width(s)
}

/// Returns the visible display width of a string, parsing semantic style tags as
/// zero-width structure and ignoring ANSI escape codes.
///
/// Use this for any text that may contain markup. For strings known to be
/// tag-free (e.g., separator literals), [`display_width`] avoids the overhead.
///
/// ANSI controls use the same parser as [`display_width`], so control bytes
/// cannot be mistaken for semantic markup and all width paths agree.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::visible_width;
///
/// assert_eq!(visible_width("hello"), 5);
/// assert_eq!(visible_width("[bold]hello[/bold]"), 5);
/// assert_eq!(visible_width("\x1b[31m[red]hi[/red]\x1b[0m"), 2);
/// ```
pub fn visible_width(s: &str) -> usize {
    visible_width_with_policy(s, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`visible_width`].
pub fn visible_width_with_policy(s: &str, policy: AmbiguousWidth) -> usize {
    WidthCalculator::new(policy).visible_width(s)
}

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
/// use standout_render::tabular::truncate_end;
///
/// assert_eq!(truncate_end("Hello World", 8, "…"), "Hello W…");
/// assert_eq!(truncate_end("Short", 10, "…"), "Short");
/// ```
pub fn truncate_end(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_end_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`truncate_end`].
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
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let mut result = truncate_to_display_width(s, target_width, calculator);
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
/// use standout_render::tabular::truncate_start;
///
/// assert_eq!(truncate_start("Hello World", 8, "…"), "…o World");
/// assert_eq!(truncate_start("/path/to/file.rs", 12, "…"), "…/to/file.rs");
/// ```
pub fn truncate_start(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_start_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`truncate_start`].
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
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let target_width = max_width - ellipsis_width;
    let truncated = find_suffix_with_width(s, target_width, calculator);
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
/// use standout_render::tabular::truncate_middle;
///
/// assert_eq!(truncate_middle("Hello World", 8, "…"), "Hel…orld");
/// assert_eq!(truncate_middle("abcdefghij", 7, "..."), "ab...ij");
/// ```
pub fn truncate_middle(s: &str, max_width: usize, ellipsis: &str) -> String {
    truncate_middle_with_policy(s, max_width, ellipsis, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`truncate_middle`].
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
        // Not enough room even for ellipsis - truncate ellipsis itself
        return truncate_to_display_width(ellipsis, max_width, calculator);
    }
    if max_width == ellipsis_width {
        // Exactly enough room for ellipsis only
        return ellipsis.to_string();
    }

    let available = max_width - ellipsis_width;
    let right_width = available.div_ceil(2); // Bias toward end (more useful info usually)
    let left_width = available - right_width;

    let left = truncate_to_display_width(s, left_width, calculator);
    let right = find_suffix_with_width(s, right_width, calculator);

    format!("{}{}{}", left, ellipsis, right)
}

/// Pads a string on the left (right-aligns) to reach the target width.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// **Warning:** This does not strip BBCode tags—they will be counted toward
/// the display width. For tagged content, compute padding manually using
/// [`visible_width`] and `" ".repeat(padding)`.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::pad_left;
///
/// assert_eq!(pad_left("42", 5), "   42");
/// assert_eq!(pad_left("hello", 3), "hello");  // No truncation
/// ```
pub fn pad_left(s: &str, width: usize) -> String {
    pad_left_with_policy(s, width, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`pad_left`].
pub fn pad_left_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    format!("{}{}", " ".repeat(padding), s)
}

/// Pads a string on the right (left-aligns) to reach the target width.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// **Warning:** This does not strip BBCode tags—they will be counted toward
/// the display width. For tagged content, compute padding manually using
/// [`visible_width`] and `" ".repeat(padding)`.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::pad_right;
///
/// assert_eq!(pad_right("42", 5), "42   ");
/// assert_eq!(pad_right("hello", 3), "hello");  // No truncation
/// ```
pub fn pad_right(s: &str, width: usize) -> String {
    pad_right_with_policy(s, width, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`pad_right`].
pub fn pad_right_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    format!("{}{}", s, " ".repeat(padding))
}

/// Pads a string on both sides (centers) to reach the target width.
///
/// When the remaining space is odd, the extra space goes on the right.
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// **Warning:** This does not strip BBCode tags—they will be counted toward
/// the display width. For tagged content, compute padding manually using
/// [`visible_width`] and `" ".repeat(padding)`.
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::pad_center;
///
/// assert_eq!(pad_center("hi", 6), "  hi  ");
/// assert_eq!(pad_center("hi", 5), " hi  ");  // Extra space on right
/// ```
pub fn pad_center(s: &str, width: usize) -> String {
    pad_center_with_policy(s, width, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`pad_center`].
pub fn pad_center_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> String {
    let padding = width.saturating_sub(display_width_with_policy(s, policy));
    let left = padding / 2;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(padding - left))
}

/// Wraps text to fit within a maximum display width, breaking at word boundaries.
///
/// Returns a vector of lines, each fitting within the specified width.
/// Words longer than the width are force-broken to fit.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// # Arguments
///
/// * `s` - The string to wrap
/// * `width` - Maximum display width for each line
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::wrap;
///
/// let lines = wrap("hello world foo", 11);
/// assert_eq!(lines, vec!["hello world", "foo"]);
///
/// let lines = wrap("short", 20);
/// assert_eq!(lines, vec!["short"]);
///
/// // Long words are force-broken with ellipsis markers
/// let lines = wrap("supercalifragilistic", 10);
/// assert!(lines.len() >= 2);
/// for line in &lines {
///     assert!(standout_render::tabular::display_width(line) <= 10);
/// }
/// ```
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    wrap_with_policy(s, width, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`wrap`].
pub fn wrap_with_policy(s: &str, width: usize, policy: AmbiguousWidth) -> Vec<String> {
    wrap_indent_with_policy(s, width, 0, policy)
}

/// Wraps text with a continuation indent on subsequent lines.
///
/// The first line uses the full width. Subsequent lines are indented by the
/// specified amount, reducing their effective width.
///
/// ANSI escape codes are preserved and don't count toward width calculations.
///
/// # Arguments
///
/// * `s` - The string to wrap
/// * `width` - Maximum display width for each line
/// * `indent` - Number of spaces to indent continuation lines
///
/// # Example
///
/// ```rust
/// use standout_render::tabular::wrap_indent;
///
/// let lines = wrap_indent("hello world foo bar", 12, 2);
/// assert_eq!(lines, vec!["hello world", "  foo bar"]);
/// ```
pub fn wrap_indent(s: &str, width: usize, indent: usize) -> Vec<String> {
    wrap_indent_with_policy(s, width, indent, AmbiguousWidth::Narrow)
}

/// Policy-aware variant of [`wrap_indent`].
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

    // If the whole string fits, return it directly
    if calculator.display_width(s) <= width {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut is_first_line = true;

    // Split on whitespace, preserving the structure
    for word in s.split_whitespace() {
        let word_width = calculator.display_width(word);
        let effective_width = if is_first_line {
            width
        } else {
            width.saturating_sub(indent)
        };

        // Handle words longer than available width
        if word_width > effective_width {
            // Finish current line if it has content
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0;
                is_first_line = false;
            }

            // Force-break the long word
            let broken = break_long_word(word, effective_width, indent, is_first_line, calculator);
            let broken_len = broken.len();
            for (i, part) in broken.into_iter().enumerate() {
                if i == 0 && is_first_line {
                    lines.push(part);
                    is_first_line = false;
                } else if i < broken_len - 1 {
                    // Not the last part - push as complete line
                    lines.push(part);
                } else {
                    // Last part - becomes the start of the next line
                    current_line = part;
                    current_width = calculator.display_width(&current_line);
                }
            }
            continue;
        }

        // Check if word fits on current line
        let needed_width = if current_line.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width // +1 for space
        };

        if needed_width <= effective_width {
            // Word fits - add to current line
            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += 1;
            }
            current_line.push_str(word);
            current_width += word_width;
        } else {
            // Word doesn't fit - start new line
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            is_first_line = false;

            // Start new line with indent
            let indent_str: String = " ".repeat(indent);
            current_line = format!("{}{}", indent_str, word);
            current_width = indent + word_width;
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // Handle edge case where we produced no lines (shouldn't happen with non-empty input)
    if lines.is_empty() && !s.is_empty() {
        lines.push(truncate_to_display_width(s, width, calculator));
    }

    lines
}

/// Wraps semantic style text without turning it into plain text.
///
/// Every returned line is a balanced tagged fragment. Whitespace normalization
/// and long-word breaking match [`wrap_indent_with_policy`].
pub(crate) fn wrap_visible_indent_with_policy(
    s: &str,
    width: usize,
    indent: usize,
    policy: AmbiguousWidth,
) -> Vec<String> {
    if width == 0 {
        return vec![];
    }

    let styled = StyledText::parse(s);
    let mut characters = Vec::new();
    styled.visit_visible_chars(|character| characters.push(character));
    let Some(first) = characters
        .iter()
        .position(|character| !character.is_whitespace())
    else {
        return vec![];
    };
    let last = characters
        .iter()
        .rposition(|character| !character.is_whitespace())
        .unwrap()
        + 1;
    let calculator = WidthCalculator::new(policy);
    let total_width = characters[first..last]
        .iter()
        .map(|&character| calculator.char_width(character))
        .sum::<usize>();
    if total_width <= width {
        return vec![styled.select_range(first..last)];
    }

    let mut words = Vec::new();
    let mut cursor = first;
    while cursor < last {
        while cursor < last && characters[cursor].is_whitespace() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < last && !characters[cursor].is_whitespace() {
            cursor += 1;
        }
        if start < cursor {
            words.push(start..cursor);
        }
    }

    let mut lines = Vec::new();
    let mut current_words: Vec<Range<usize>> = Vec::new();
    let mut current_width = 0;

    for word in words {
        let word_width = range_width(&characters, &word, calculator);
        let effective_width = if lines.is_empty() {
            width
        } else {
            width.saturating_sub(indent)
        };

        if word_width > effective_width {
            if !current_words.is_empty() {
                push_selected_line(&mut lines, &styled, &current_words, indent);
                current_words.clear();
                current_width = 0;
            }

            let mut start = word.start;
            while start < word.end {
                let effective_width = if lines.is_empty() {
                    width
                } else {
                    width.saturating_sub(indent)
                };
                if effective_width == 0 {
                    break;
                }
                let remaining = start..word.end;
                if range_width(&characters, &remaining, calculator) <= effective_width {
                    current_words.push(remaining);
                    current_width =
                        range_width(&characters, current_words.last().unwrap(), calculator);
                    break;
                }

                let content_width = effective_width.saturating_sub(1);
                if content_width == 0 {
                    let mut line = " ".repeat(if lines.is_empty() { 0 } else { indent });
                    line.push('…');
                    lines.push(line);
                    break;
                }
                let count = prefix_count(&characters[start..word.end], content_width, calculator);
                if count == 0 {
                    break;
                }
                let mut line = " ".repeat(if lines.is_empty() { 0 } else { indent });
                line.push_str(&styled.select_range(start..start + count));
                line.push('…');
                lines.push(line);
                start += count;
            }
            continue;
        }

        let needed_width = if current_words.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };
        if needed_width <= effective_width {
            current_words.push(word);
            current_width = needed_width;
        } else {
            push_selected_line(&mut lines, &styled, &current_words, indent);
            current_words.clear();
            current_words.push(word);
            current_width = word_width;
        }
    }

    if !current_words.is_empty() {
        push_selected_line(&mut lines, &styled, &current_words, indent);
    }
    lines
}

fn push_selected_line(
    lines: &mut Vec<String>,
    styled: &StyledText<'_>,
    ranges: &[Range<usize>],
    indent: usize,
) {
    let mut line = " ".repeat(if lines.is_empty() { 0 } else { indent });
    line.push_str(&styled.select(ranges, " "));
    lines.push(line);
}

fn range_width(characters: &[char], range: &Range<usize>, calculator: WidthCalculator) -> usize {
    characters[range.clone()]
        .iter()
        .map(|&character| calculator.char_width(character))
        .sum()
}

fn prefix_count(characters: &[char], max_width: usize, calculator: WidthCalculator) -> usize {
    let mut width = 0;
    characters
        .iter()
        .take_while(|&&character| {
            let next = width + calculator.char_width(character);
            if next > max_width {
                false
            } else {
                width = next;
                true
            }
        })
        .count()
}

/// Break a word that's longer than the available width into multiple parts.
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
            // Can't fit anything - just return what we have
            break;
        }

        let remaining_width = calculator.display_width(remaining);
        if remaining_width <= effective_width {
            // Rest fits
            let prefix = if first_part {
                String::new()
            } else {
                " ".repeat(indent)
            };
            parts.push(format!("{}{}", prefix, remaining));
            break;
        }

        // Need to break - leave room for ellipsis to indicate continuation
        let break_width = effective_width.saturating_sub(1); // -1 for "…"
        if break_width == 0 {
            // Not enough room even for one char + ellipsis
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

// --- Internal helpers ---

/// Truncate string to fit display width, keeping characters from the start.
/// Preserves every ANSI control recognized by [`console::strip_ansi_codes`]
/// without charging it display width.
fn truncate_to_display_width(s: &str, max_width: usize, calculator: WidthCalculator) -> String {
    take_prefix_to_display_width(s, max_width, calculator).0
}

/// Returns the width-bounded source prefix and the number of source bytes it
/// consumed. Sharing the source offset keeps wrapping correct when the prefix
/// contains zero-width ANSI controls.
fn take_prefix_to_display_width(
    s: &str,
    max_width: usize,
    calculator: WidthCalculator,
) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }

    // Fast path: if string fits, return as-is
    if calculator.display_width(s) <= max_width {
        return (s.to_string(), s.len());
    }

    let mut result = String::new();
    let mut current_width = 0;
    'units: for (unit, is_ansi) in AnsiCodeIterator::new(s) {
        if is_ansi {
            result.push_str(unit);
            continue;
        }

        for character in unit.chars() {
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

/// Find the longest suffix of s that has display width <= max_width.
fn find_suffix_with_width(s: &str, max_width: usize, calculator: WidthCalculator) -> String {
    if max_width == 0 {
        return String::new();
    }

    let total_width = calculator.display_width(s);
    if total_width <= max_width {
        return s.to_string();
    }

    // Linear scan from the start to find where to cut.
    // We need to skip (total_width - max_width) display columns.
    let skip_width = total_width - max_width;

    let mut current_width = 0;
    let mut byte_offset = 0;
    let mut source_offset = 0;

    'units: for (unit, is_ansi) in AnsiCodeIterator::new(s) {
        if is_ansi {
            byte_offset = source_offset + unit.len();
            source_offset += unit.len();
            continue;
        }

        for (unit_offset, character) in unit.char_indices() {
            current_width += calculator.char_width(character);
            byte_offset = source_offset + unit_offset + character.len_utf8();

            if current_width >= skip_width {
                break 'units;
            }
        }
        source_offset += unit.len();
    }

    s[byte_offset..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::strip_ansi_codes;

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
    fn styled_wrap_uses_the_same_ansi_parser_as_measurement() {
        for (open, close) in [("\x1b(0", "\x1b(B"), ("\u{9b}31m", "\u{9b}0m")] {
            let input = format!("{open}[outer]abcdefghij[/outer]{close}");
            assert_eq!(visible_width(&input), 10);

            let lines = wrap_visible_indent_with_policy(&input, 5, 0, AmbiguousWidth::Narrow);
            assert!(lines.iter().all(|line| visible_width(line) <= 5));
            let plain = lines
                .iter()
                .map(|line| strip_ansi_codes(&standout_bbparser::strip_tags(line)).replace('…', ""))
                .collect::<String>();
            assert_eq!(plain, "abcdefghij");
        }
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

    // --- wrap tests ---

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
        // "supercalifragilistic" is 20 chars, width 10
        // With ellipsis breaks: "supercali…" (10), "fragilis…" (10), "tic" (3)
        let result = wrap("supercalifragilistic", 10);
        assert!(result.len() >= 2, "should produce multiple lines");
        for line in &result {
            assert!(display_width(line) <= 10, "line '{}' exceeds width", line);
        }
    }

    #[test]
    fn wrap_preserves_word_boundaries() {
        let result = wrap("hello world test", 10);
        // Should not break "hello" or "world" in the middle
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "world test");
    }

    #[test]
    fn wrap_multiple_spaces_normalized_when_wrapping() {
        // When wrapping occurs, multiple spaces between words get normalized
        // because we split_whitespace and rejoin with single spaces
        let result = wrap("hello    world    foo", 12);
        // "hello world" (11) fits, "foo" goes to next line
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    // --- wrap_indent tests ---

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
        // First line: no indent, up to 10 chars
        // Subsequent: 2-char indent, effective width 8
        assert!(!result[0].starts_with(' '));
        for line in result.iter().skip(1) {
            assert!(line.starts_with("  "), "continuation should be indented");
        }
    }

    #[test]
    fn wrap_indent_zero_indent() {
        // Same as regular wrap
        let result = wrap_indent("hello world foo", 11, 0);
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    #[test]
    fn wrap_cjk_characters() {
        // CJK characters are 2 columns each
        // "日本語" is 6 columns
        let result = wrap("日本語 テスト", 8);
        assert_eq!(result.len(), 2);
        for line in &result {
            assert!(display_width(line) <= 8);
        }
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

            // All original words should appear in the output
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
