//! Authoritative terminal text-width calculation.
//!
//! Unicode assigns some characters an "ambiguous" East Asian width. Standout
//! does not infer a locale: applications choose whether those characters occupy
//! one terminal column ([`AmbiguousWidth::Narrow`], the compatibility default)
//! or two ([`AmbiguousWidth::Wide`]).

use console::strip_ansi_codes;
use serde::{Deserialize, Serialize};
use standout_bbparser::strip_tags;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex, MutexGuard,
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

#[derive(Debug)]
struct WidthPolicyState {
    current: AtomicU8,
    render_lock: Mutex<()>,
}

#[derive(Clone, Debug)]
pub(crate) struct WidthPolicySource(Arc<WidthPolicyState>);

pub(crate) struct WidthPolicyGuard<'a> {
    source: &'a WidthPolicySource,
    previous: AmbiguousWidth,
    _render_lock: MutexGuard<'a, ()>,
}

impl Drop for WidthPolicyGuard<'_> {
    fn drop(&mut self) {
        self.source.store(self.previous);
    }
}

impl WidthPolicySource {
    pub(crate) fn new(policy: AmbiguousWidth) -> Self {
        Self(Arc::new(WidthPolicyState {
            current: AtomicU8::new(policy as u8),
            render_lock: Mutex::new(()),
        }))
    }

    pub(crate) fn get(&self) -> AmbiguousWidth {
        if self.0.current.load(Ordering::Relaxed) == AmbiguousWidth::Wide as u8 {
            AmbiguousWidth::Wide
        } else {
            AmbiguousWidth::Narrow
        }
    }

    fn store(&self, policy: AmbiguousWidth) {
        self.0.current.store(policy as u8, Ordering::Relaxed);
    }

    pub(crate) fn scoped(&self, policy: AmbiguousWidth) -> WidthPolicyGuard<'_> {
        // A render that unwinds poisons the standard mutex. Recover its guard:
        // policy restoration is handled by WidthPolicyGuard, so the protected
        // state remains valid after a template/filter panic.
        let render_lock = self
            .0
            .render_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = self.get();
        self.store(policy);
        WidthPolicyGuard {
            source: self,
            previous,
            _render_lock: render_lock,
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
    pub fn visible_width(self, text: &str) -> usize {
        let no_ansi = strip_ansi_codes(text);
        self.text_width(&strip_tags(&no_ansi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn visible_width_strips_ansi_and_style_tags() {
        let wide = WidthCalculator::new(AmbiguousWidth::Wide);
        assert_eq!(wide.visible_width("\x1b[31m[status]↦≈Δ[/status]\x1b[0m"), 5);
    }
}
