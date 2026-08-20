//! Authoritative terminal text-width calculation.
//!
//! Unicode assigns some characters an "ambiguous" East Asian width. Standout
//! does not infer a locale: applications choose whether those characters occupy
//! one terminal column ([`AmbiguousWidth::Narrow`], the compatibility default)
//! or two ([`AmbiguousWidth::Wide`]).

use console::strip_ansi_codes;
use serde::{Deserialize, Serialize};
use standout_bbparser::StyledText;
use std::sync::{
    atomic::{AtomicU8, AtomicUsize, Ordering},
    Arc,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// How East Asian Ambiguous characters are measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmbiguousWidth {
    /// Treat ambiguous characters as one terminal column.
    #[default]
    Narrow,
    /// Treat ambiguous characters as two terminal columns.
    Wide,
}

/// Centralized character and string width calculator.
///
/// Rendering, tabular formatting, and template filters use this same interface
/// so a selected ambiguous-width policy cannot drift between pipeline stages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidthCalculator {
    policy: AmbiguousWidth,
}

/// Which visible portion to retain when tagged text is truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisibleTruncateAt {
    End,
    Start,
    Middle,
}

#[derive(Debug)]
struct RenderWidthState {
    ambiguous_width: AtomicU8,
    terminal_width: AtomicUsize,
}

#[derive(Clone, Debug)]
pub(crate) struct RenderWidthSource(Arc<RenderWidthState>);

pub(crate) struct RenderWidthGuard<'a> {
    source: &'a RenderWidthSource,
    previous_ambiguous_width: AmbiguousWidth,
    previous_terminal_width: Option<usize>,
}

impl Drop for RenderWidthGuard<'_> {
    fn drop(&mut self) {
        self.source
            .store_ambiguous_width(self.previous_ambiguous_width);
        self.source
            .store_terminal_width(self.previous_terminal_width);
    }
}

impl RenderWidthSource {
    pub(crate) fn new(policy: AmbiguousWidth) -> Self {
        Self(Arc::new(RenderWidthState {
            ambiguous_width: AtomicU8::new(policy as u8),
            terminal_width: AtomicUsize::new(0),
        }))
    }

    pub(crate) fn ambiguous_width(&self) -> AmbiguousWidth {
        if self.0.ambiguous_width.load(Ordering::Relaxed) == AmbiguousWidth::Wide as u8 {
            AmbiguousWidth::Wide
        } else {
            AmbiguousWidth::Narrow
        }
    }

    pub(crate) fn terminal_width(&self) -> Option<usize> {
        match self.0.terminal_width.load(Ordering::Relaxed) {
            0 => None,
            width => Some(width),
        }
    }

    fn store_ambiguous_width(&self, policy: AmbiguousWidth) {
        self.0
            .ambiguous_width
            .store(policy as u8, Ordering::Relaxed);
    }

    fn store_terminal_width(&self, width: Option<usize>) {
        self.0
            .terminal_width
            .store(width.unwrap_or(0), Ordering::Relaxed);
    }

    pub(crate) fn scoped(
        &self,
        policy: AmbiguousWidth,
        terminal_width: Option<usize>,
    ) -> RenderWidthGuard<'_> {
        // Width is on the request; this stores it for MiniJinja filters
        // registered at engine construction. There is no mutex (ADR-0030).
        // MiniJinjaEngine is !Send/!Sync so concurrent renders cannot
        // interleave scoped() on a shared engine. Restoration is handled
        // by RenderWidthGuard, including after a template/filter panic.
        let previous_ambiguous_width = self.ambiguous_width();
        let previous_terminal_width = self.terminal_width();
        self.store_ambiguous_width(policy);
        self.store_terminal_width(terminal_width);
        RenderWidthGuard {
            source: self,
            previous_ambiguous_width,
            previous_terminal_width,
        }
    }
}

impl WidthCalculator {
    /// Creates a calculator for `policy`.
    pub const fn new(policy: AmbiguousWidth) -> Self {
        Self { policy }
    }

    /// Returns the selected policy.
    pub const fn policy(self) -> AmbiguousWidth {
        self.policy
    }

    /// Measures a single character in terminal columns.
    pub fn char_width(self, character: char) -> usize {
        let narrow = character.width().unwrap_or(0);
        match self.policy {
            AmbiguousWidth::Narrow => narrow,
            AmbiguousWidth::Wide
                if narrow == 1 && east_asian_width::is_ambiguous(character as u32) =>
            {
                2
            }
            AmbiguousWidth::Wide => character.width_cjk().unwrap_or(0),
        }
    }

    /// Measures plain text in terminal columns.
    pub fn text_width(self, text: &str) -> usize {
        match self.policy {
            AmbiguousWidth::Narrow => UnicodeWidthStr::width(text),
            AmbiguousWidth::Wide => {
                let base = UnicodeWidthStr::width_cjk(text);
                let missing_ambiguous = text
                    .chars()
                    .filter(|&c| {
                        c.width() == Some(1)
                            && c.width_cjk() == Some(1)
                            && east_asian_width::is_ambiguous(c as u32)
                    })
                    .count();
                base + missing_ambiguous
            }
        }
    }

    /// Measures text while ignoring ANSI escape sequences.
    pub fn display_width(self, text: &str) -> usize {
        self.text_width(&strip_ansi_codes(text))
    }

    /// Measures text while ignoring ANSI sequences and Standout style tags.
    ///
    /// Semantic tags are parsed as zero-width structure; this does not render a
    /// tag-free copy of the input.
    pub fn visible_width(self, text: &str) -> usize {
        let styled = StyledText::parse(text);
        let mut width = 0;
        styled.visit_visible_chars(|character| width += self.char_width(character));
        width
    }

    /// Truncates tagged text by visible terminal width while preserving balanced
    /// semantic style tags around every retained fragment.
    pub(crate) fn truncate_visible(
        self,
        text: &str,
        max_width: usize,
        marker: &str,
        at: VisibleTruncateAt,
    ) -> String {
        let styled = StyledText::parse(text);
        let characters = visible_characters(&styled);
        let width = characters
            .iter()
            .map(|&character| self.char_width(character))
            .sum::<usize>();
        if width <= max_width {
            return text.to_string();
        }

        let marker_text = StyledText::parse(marker);
        let marker_characters = visible_characters(&marker_text);
        let marker_width = marker_characters
            .iter()
            .map(|&character| self.char_width(character))
            .sum::<usize>();
        if max_width <= marker_width {
            let count = prefix_character_count(&marker_characters, max_width, self);
            return marker_text.select_range(0..count);
        }

        let available = max_width - marker_width;
        let total_characters = characters.len();
        match at {
            VisibleTruncateAt::End => {
                let count = prefix_character_count(&characters, available, self);
                format!("{}{}", styled.select_range(0..count), marker)
            }
            VisibleTruncateAt::Start => {
                let count = suffix_character_count(&characters, available, self);
                format!(
                    "{}{}",
                    marker,
                    styled.select_range(total_characters - count..total_characters)
                )
            }
            VisibleTruncateAt::Middle => {
                let right_width = available.div_ceil(2);
                let left_width = available - right_width;
                let left_count = prefix_character_count(&characters, left_width, self);
                let right_count = suffix_character_count(&characters, right_width, self);
                let left = styled.select_range(0..left_count);
                let right = styled.select_range(total_characters - right_count..total_characters);
                format!("{}{}{}", left, marker, right)
            }
        }
    }
}

fn visible_characters(styled: &StyledText<'_>) -> Vec<char> {
    let mut characters = Vec::new();
    styled.visit_visible_chars(|character| characters.push(character));
    characters
}

fn prefix_character_count(
    characters: &[char],
    max_width: usize,
    calculator: WidthCalculator,
) -> usize {
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

fn suffix_character_count(
    characters: &[char],
    max_width: usize,
    calculator: WidthCalculator,
) -> usize {
    let mut width = 0;
    characters
        .iter()
        .rev()
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
    use crate::tabular::{
        truncate_end_with_policy, truncate_middle_with_policy, truncate_start_with_policy,
    };
    use console::{strip_ansi_codes, Style};
    use proptest::prelude::*;
    use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};
    use std::collections::HashMap;

    #[test]
    fn ambiguous_characters_follow_the_selected_policy() {
        let narrow = WidthCalculator::new(AmbiguousWidth::Narrow);
        let wide = WidthCalculator::new(AmbiguousWidth::Wide);

        assert_eq!(narrow.char_width('↦'), 1);
        assert_eq!(wide.char_width('↦'), 1); // EAW Neutral, not Ambiguous
        for character in ['≈', 'Δ'] {
            assert_eq!(narrow.char_width(character), 1);
            assert_eq!(wide.char_width(character), 2);
        }
        assert_eq!(narrow.text_width("↦ ≈ Δ"), 5);
        assert_eq!(wide.text_width("↦ ≈ Δ"), 7);
    }

    #[test]
    fn visible_width_ignores_ansi_and_style_tags() {
        let wide = WidthCalculator::new(AmbiguousWidth::Wide);
        assert_eq!(wide.visible_width("\x1b[31m[status]↦≈Δ[/status]\x1b[0m"), 5);
    }

    #[test]
    fn truncation_preserves_nested_semantic_styles() {
        let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);
        let input = "[outer]ab[inner]cdef[/inner]ghij[/outer]";

        assert_eq!(
            calculator.truncate_visible(input, 6, "…", VisibleTruncateAt::End),
            "[outer]ab[inner]cde[/inner][/outer]…"
        );
        assert_eq!(
            calculator.truncate_visible(input, 6, "…", VisibleTruncateAt::Start),
            "…[outer][inner]f[/inner]ghij[/outer]"
        );
        assert_eq!(
            calculator.truncate_visible(input, 6, "…", VisibleTruncateAt::Middle),
            "[outer]ab[/outer]…[outer]hij[/outer]"
        );
    }

    fn tagged_text() -> impl Strategy<Value = String> {
        let leaf = "[a-zA-Z0-9 Δ≈日本]{0,12}".prop_map(|text| text);
        leaf.prop_recursive(4, 64, 4, |inner| {
            prop_oneof![
                (
                    prop::sample::select(vec!["outer", "inner", "match"]),
                    inner.clone()
                )
                    .prop_map(|(tag, content)| format!("[{tag}]{content}[/{tag}]")),
                prop::collection::vec(inner, 1..4).prop_map(|fragments| fragments.concat()),
            ]
        })
    }

    fn ansi_pair() -> impl Strategy<Value = (&'static str, &'static str)> {
        prop::sample::select(vec![
            ("\x1b[31m", "\x1b[0m"),
            ("\x1b(0", "\x1b(B"),
            ("\x1b)0", "\x1b)B"),
            ("\u{9b}31m", "\u{9b}0m"),
        ])
    }

    fn plain_render(input: &str) -> String {
        let styles = ["outer", "inner", "match"]
            .into_iter()
            .map(|tag| (tag.to_string(), Style::new()))
            .collect::<HashMap<_, _>>();
        BBParser::new(styles, TagTransform::Remove)
            .unknown_behavior(UnknownTagBehavior::Strip)
            .parse(input)
    }

    fn assert_balanced(input: &str) {
        let styles = ["outer", "inner", "match"]
            .into_iter()
            .map(|tag| (tag.to_string(), Style::new()))
            .collect::<HashMap<_, _>>();
        let parser = BBParser::new(styles, TagTransform::Keep);
        assert!(parser.validate(input).is_ok(), "unbalanced output: {input}");
    }

    proptest! {
        #[test]
        fn tagged_truncation_is_balanced_bounded_and_matches_plain_text(
            input in tagged_text(),
            max_width in 0usize..30,
        ) {
            let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);
            let plain = plain_render(&input);
            for at in [
                VisibleTruncateAt::End,
                VisibleTruncateAt::Start,
                VisibleTruncateAt::Middle,
            ] {
                let result = calculator.truncate_visible(&input, max_width, "…", at);
                let expected = match at {
                    VisibleTruncateAt::End => truncate_end_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                    VisibleTruncateAt::Start => truncate_start_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                    VisibleTruncateAt::Middle => truncate_middle_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                };

                prop_assert!(calculator.visible_width(&result) <= max_width);
                assert_balanced(&result);
                prop_assert_eq!(plain_render(&result), expected);
            }
        }

        #[test]
        fn ansi_compatible_tagged_truncation_matches_plain_text(
            tagged in tagged_text(),
            (open, close) in ansi_pair(),
            max_width in 0usize..30,
        ) {
            let input = format!("{open}{tagged}{close}");
            let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);
            let plain = plain_render(&tagged);
            for at in [
                VisibleTruncateAt::End,
                VisibleTruncateAt::Start,
                VisibleTruncateAt::Middle,
            ] {
                let result = calculator.truncate_visible(&input, max_width, "…", at);
                let expected = match at {
                    VisibleTruncateAt::End => truncate_end_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                    VisibleTruncateAt::Start => truncate_start_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                    VisibleTruncateAt::Middle => truncate_middle_with_policy(
                        &plain, max_width, "…", AmbiguousWidth::Narrow,
                    ),
                };

                prop_assert!(calculator.visible_width(&result) <= max_width);
                assert_balanced(&result);
                let plain_result = plain_render(&result);
                prop_assert_eq!(strip_ansi_codes(&plain_result), expected);
            }
        }

        #[test]
        fn unstyled_inputs_keep_existing_unicode_truncation_semantics(
            input in "[a-zA-Z0-9 Δ≈日本]{0,40}",
            max_width in 0usize..30,
        ) {
            let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);
            prop_assert_eq!(
                calculator.truncate_visible(&input, max_width, "…", VisibleTruncateAt::End),
                truncate_end_with_policy(&input, max_width, "…", AmbiguousWidth::Narrow),
            );
            prop_assert_eq!(
                calculator.truncate_visible(&input, max_width, "…", VisibleTruncateAt::Start),
                truncate_start_with_policy(&input, max_width, "…", AmbiguousWidth::Narrow),
            );
            prop_assert_eq!(
                calculator.truncate_visible(&input, max_width, "…", VisibleTruncateAt::Middle),
                truncate_middle_with_policy(&input, max_width, "…", AmbiguousWidth::Narrow),
            );
        }
    }
}
