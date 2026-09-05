//! Reading text in ANSI units, and closing the state a cut leaves open.
//!
//! Escape sequences occupy no terminal columns, so every operation that
//! measures, escapes or cuts styled text splits the text into sequences and the
//! plain runs between them, and most also need the byte offset each unit starts
//! at. An operation that *cuts* needs one thing more: the sequence closing what
//! the discarded remainder would have closed.
//!
//! Both live here because separating them is what leaks colour. A walking
//! primitive on its own leaves each caller to remember the balancing rule, and a
//! caller that forgets it emits an opener with no reset, dyeing every later line
//! of terminal output.

use console::AnsiCodeIterator;

const ANSI_RESET: &str = "\u{1b}[0m";

/// One unit of a source string: a whole ANSI escape sequence, or a run of plain
/// text between sequences. `offset` is where `text` starts in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiUnit<'a> {
    pub text: &'a str,
    pub offset: usize,
    pub is_escape: bool,
}

/// Splits `source` into escape sequences and the plain runs between them.
pub fn ansi_units(source: &str) -> impl Iterator<Item = AnsiUnit<'_>> {
    let mut offset = 0;
    AnsiCodeIterator::new(source).map(move |(text, is_escape)| {
        let unit = AnsiUnit {
            text,
            offset,
            is_escape,
        };
        offset += text.len();
        unit
    })
}

/// Whether the escape sequences shown to it leave a style open, and the
/// sequence that closes it.
///
/// Only SGR sequences carry styling — `ESC [ … m` and the C1 `CSI … m`; anything
/// else passes without changing the answer. A sequence whose parameters are all
/// zero closes, and any other SGR sequence opens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnsiBalance {
    open: bool,
}

impl AnsiBalance {
    /// Records one emitted escape sequence.
    pub fn observe(&mut self, escape: &str) {
        let Some(parameters) = escape
            .strip_prefix("\u{1b}[")
            .or_else(|| escape.strip_prefix('\u{9b}'))
            .and_then(|rest| rest.strip_suffix('m'))
        else {
            return;
        };
        self.open = !parameters
            .split(';')
            .all(|parameter| parameter.bytes().all(|byte| byte == b'0'));
    }

    /// `ESC [ 0 m` while a style is open and `""` otherwise, so a caller ending
    /// a cut appends this unconditionally.
    pub fn closing(self) -> &'static str {
        if self.open {
            ANSI_RESET
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::strip_ansi_codes;

    const CORPUS: &[&str] = &[
        "",
        "plain text",
        "\u{1b}[31mred\u{1b}[0m",
        "before\u{1b}[1;32mgreen\u{1b}[0mafter",
        "\u{1b}[31m",
        "\u{1b}[0m\u{1b}[0m",
        "wide \u{4e2d}\u{6587} \u{1b}[34mblue\u{1b}[0m",
        "escaped \\[not a tag\\] \u{1b}[31mred",
        "\u{1b}[38;2;255;0;0mtruecolor\u{1b}[0m",
    ];

    #[test]
    fn units_reconstruct_the_source_at_the_offsets_they_report() {
        for source in CORPUS {
            let mut rebuilt = String::new();
            for unit in ansi_units(source) {
                assert_eq!(unit.offset, rebuilt.len(), "{source:?}");
                assert_eq!(
                    &source[unit.offset..unit.offset + unit.text.len()],
                    unit.text
                );
                rebuilt.push_str(unit.text);
            }
            assert_eq!(&rebuilt, source);
        }
    }

    // width.rs measures with console's `strip_ansi_codes` instead of walking;
    // this pins that the two draw the same line between text and escapes.
    #[test]
    fn dropping_the_escape_units_is_what_strip_ansi_codes_returns() {
        for source in CORPUS {
            let walked: String = ansi_units(source)
                .filter(|unit| !unit.is_escape)
                .map(|unit| unit.text)
                .collect();
            assert_eq!(walked, strip_ansi_codes(source), "{source:?}");
        }
    }

    #[test]
    fn a_zero_parameter_sequence_closes_and_any_other_opens() {
        let mut balance = AnsiBalance::default();
        assert_eq!(balance.closing(), "");

        for opener in [
            "\u{1b}[31m",
            "\u{1b}[1;32m",
            "\u{1b}[38;2;255;0;0m",
            "\u{1b}[1m",
            "\u{9b}31m",
        ] {
            balance = AnsiBalance::default();
            balance.observe(opener);
            assert_eq!(balance.closing(), ANSI_RESET, "{opener:?}");
        }

        for closer in [
            "\u{1b}[0m",
            "\u{1b}[m",
            "\u{1b}[00m",
            "\u{1b}[0;0m",
            "\u{9b}0m",
        ] {
            let mut balance = AnsiBalance::default();
            balance.observe("\u{1b}[31m");
            balance.observe(closer);
            assert_eq!(balance.closing(), "", "{closer:?}");
        }
    }

    #[test]
    fn a_non_sgr_sequence_leaves_the_answer_alone() {
        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[2J");
        assert_eq!(balance.closing(), "");
        balance.observe("\u{1b}[31m");
        balance.observe("\u{1b}[2J");
        assert_eq!(balance.closing(), ANSI_RESET);
    }
}
