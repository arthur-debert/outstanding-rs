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

/// The reset code of each attribute group SGR can leave open, indexed by the
/// bit that records the group as open.
const GROUP_RESETS: [u16; 16] = [
    10, 22, 23, 24, 25, 27, 28, 29, 39, 49, 50, 54, 55, 59, 65, 75,
];

/// The group a parameter opens, as an index into `GROUP_RESETS`.
fn opened_group(code: u16) -> Option<usize> {
    let reset = match code {
        1 | 2 => 22,
        3 | 20 => 23,
        4 | 21 => 24,
        5 | 6 => 25,
        7 => 27,
        8 => 28,
        9 => 29,
        11..=19 => 10,
        26 => 50,
        30..=38 | 90..=97 => 39,
        40..=48 | 100..=107 => 49,
        51 | 52 => 54,
        53 => 55,
        58 => 59,
        60..=64 => 65,
        73 | 74 => 75,
        _ => return None,
    };
    GROUP_RESETS.iter().position(|group| *group == reset)
}

/// Whether the escape sequences shown to it leave a style open, and the
/// sequence that closes it.
///
/// Only SGR sequences carry styling — `ESC [ … m` and the C1 `CSI … m`; anything
/// else passes without changing the answer. Parameters apply in the order
/// written, each opening or closing one attribute group, so `0` and the
/// targeted resets (`22`, `23`, `24`, `39`, `49`, …) close what earlier
/// parameters opened and `ESC [ 31 ; 0 m` ends closed. The extended-colour
/// forms `38`/`48`/`58` swallow the parameters that spell out the colour, so
/// the zeroes in `ESC [ 38 ; 2 ; 255 ; 0 ; 0 m` are colour channels rather than
/// resets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnsiBalance {
    open_groups: u32,
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

        let mut parameters = parameters.split(';');
        while let Some(parameter) = parameters.next() {
            // A colon form spells the whole colour out inside one parameter, so
            // only the semicolon form swallows what follows it.
            let (head, is_compound) = match parameter.split_once(':') {
                Some((head, _)) => (head, true),
                None => (parameter, false),
            };
            let Ok(code) = head.trim().parse::<u16>() else {
                if head.trim().is_empty() {
                    self.open_groups = 0;
                }
                continue;
            };

            if !is_compound && matches!(code, 38 | 48 | 58) {
                let channels = match parameters
                    .clone()
                    .next()
                    .and_then(|space| space.parse::<u16>().ok())
                {
                    Some(2) => Some(3),
                    Some(5) => Some(1),
                    _ => None,
                };
                // A colour space nobody recognises swallows nothing, so a
                // parameter that follows still applies.
                if let Some(channels) = channels {
                    for _ in 0..=channels {
                        parameters.next();
                    }
                }
            }

            if code == 0 {
                self.open_groups = 0;
            } else if let Some(group) = GROUP_RESETS.iter().position(|reset| *reset == code) {
                self.open_groups &= !(1 << group);
            } else if let Some(group) = opened_group(code) {
                self.open_groups |= 1 << group;
            }
        }
    }

    /// `ESC [ 0 m` while a style is open and `""` otherwise, so a caller ending
    /// a cut appends this unconditionally.
    pub fn closing(self) -> &'static str {
        if self.open_groups != 0 {
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
    fn a_sequence_that_leaves_an_attribute_set_opens_and_a_full_reset_closes() {
        let mut balance = AnsiBalance::default();
        assert_eq!(balance.closing(), "");

        for opener in [
            "\u{1b}[31m",
            "\u{1b}[1;32m",
            "\u{1b}[38;2;255;0;0m",
            "\u{1b}[38;5;0m",
            "\u{1b}[38:2::255:0:0m",
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
    fn every_group_opens_on_its_parameter_and_closes_on_its_own_reset() {
        for (opener, closer) in [
            ("\u{1b}[31m", "\u{1b}[39m"),
            ("\u{1b}[38;2;255;0;0m", "\u{1b}[39m"),
            ("\u{1b}[41m", "\u{1b}[49m"),
            ("\u{1b}[1m", "\u{1b}[22m"),
            ("\u{1b}[2m", "\u{1b}[22m"),
            ("\u{1b}[3m", "\u{1b}[23m"),
            ("\u{1b}[4m", "\u{1b}[24m"),
            ("\u{1b}[5m", "\u{1b}[25m"),
            ("\u{1b}[7m", "\u{1b}[27m"),
            ("\u{1b}[8m", "\u{1b}[28m"),
            ("\u{1b}[9m", "\u{1b}[29m"),
            ("\u{1b}[11m", "\u{1b}[10m"),
            ("\u{1b}[26m", "\u{1b}[50m"),
            ("\u{1b}[51m", "\u{1b}[54m"),
            ("\u{1b}[53m", "\u{1b}[55m"),
            ("\u{1b}[58;5;9m", "\u{1b}[59m"),
            ("\u{1b}[60m", "\u{1b}[65m"),
            ("\u{1b}[61m", "\u{1b}[65m"),
            ("\u{1b}[62m", "\u{1b}[65m"),
            ("\u{1b}[63m", "\u{1b}[65m"),
            ("\u{1b}[64m", "\u{1b}[65m"),
            ("\u{1b}[73m", "\u{1b}[75m"),
            ("\u{1b}[74m", "\u{1b}[75m"),
        ] {
            let mut balance = AnsiBalance::default();
            balance.observe(opener);
            assert_eq!(balance.closing(), ANSI_RESET, "{opener:?}");
            balance.observe(closer);
            assert_eq!(balance.closing(), "", "{opener:?} then {closer:?}");

            // Another group's reset leaves this one open.
            let mut balance = AnsiBalance::default();
            balance.observe(opener);
            balance.observe(if closer == "\u{1b}[39m" {
                "\u{1b}[49m"
            } else {
                "\u{1b}[39m"
            });
            assert_eq!(balance.closing(), ANSI_RESET, "{opener:?}");
        }

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[1;31m");
        balance.observe("\u{1b}[39m");
        assert_eq!(balance.closing(), ANSI_RESET);
        balance.observe("\u{1b}[22m");
        assert_eq!(balance.closing(), "");
    }

    #[test]
    fn an_extended_colour_swallows_its_channels_and_nothing_else() {
        for sequence in ["\u{1b}[38;0m", "\u{1b}[48;0m", "\u{1b}[58;0m"] {
            let mut balance = AnsiBalance::default();
            balance.observe("\u{1b}[1m");
            balance.observe(sequence);
            assert_eq!(balance.closing(), "", "{sequence:?}");
        }

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[38;2;0;0;0;0m");
        assert_eq!(balance.closing(), "");

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[38;5;0;39m");
        assert_eq!(balance.closing(), "");

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[38;31m");
        balance.observe("\u{1b}[39m");
        assert_eq!(balance.closing(), "");
    }

    #[test]
    fn parameters_apply_in_the_order_they_are_written() {
        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[31;0m");
        assert_eq!(balance.closing(), "");

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[0;31m");
        assert_eq!(balance.closing(), ANSI_RESET);

        let mut balance = AnsiBalance::default();
        balance.observe("\u{1b}[39;31m");
        assert_eq!(balance.closing(), ANSI_RESET);
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
