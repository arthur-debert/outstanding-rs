use crate::{AmbiguousWidth, WidthCalculator};
use standout_bbparser::StyledText;
use std::ops::Range;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::visible_width;
    use console::strip_ansi_codes;

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
}
