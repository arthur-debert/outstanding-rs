//! A rendered help page, read as rows.
//!
//! Two oracles read the same page: the invariant library
//! ([`crate::invariants`]), which states properties of the rows that *are*
//! rendered, and the clap-parity differential ([`crate::clap_parity`]), which
//! asks whether a row exists at all for each fact clap knows. They have to
//! agree about what a row *is* — where its label ends, which lines belong to
//! it, what counts as its value placeholder — or the same page reads two ways
//! and one of them is wrong.
//!
//! So the parsing lives here once, deliberately format-tolerant: it takes
//! standout's page (`  --summary <STYLE>  How much…`) and clap's own
//! (`      --summary <STYLE>` with the description indented beneath it) on the
//! same terms, because the differential's grounding test runs the same
//! expectations against clap's formatter to prove they are clap's facts and
//! not standout's habits.

use clap::{Arg, Command};

/// One rendered row: its label, its description, the continuation lines filed
/// under it, and the section it sits in.
pub(crate) struct Row<'a> {
    /// The row's name column — flag spellings and value placeholders, or a
    /// positional's metavar.
    pub(crate) label: &'a str,
    /// The first line of the row's description, which may be empty when a
    /// formatter puts the description on the following line.
    pub(crate) description: &'a str,
    /// Lines indented past the label: a wrapped description, a default, a
    /// possible-values list.
    pub(crate) continuations: Vec<&'a str>,
    /// The whole first line, for failure messages.
    pub(crate) line: &'a str,
    /// The nearest heading above the row — `OPTIONS`, `Arguments:` — or `""`
    /// for rows before any heading.
    pub(crate) section: &'a str,
}

impl<'a> Row<'a> {
    /// The row's description and every continuation line under it.
    pub(crate) fn block(&self) -> impl Iterator<Item = &&'a str> {
        std::iter::once(&self.description).chain(self.continuations.iter())
    }

    /// The row's whole block as one whitespace-normalized line.
    ///
    /// Normalizing is what lets a wrapped description be matched against the
    /// text it was wrapped from: a formatter is free to break a help string
    /// across lines, and the fact is that the words are on the page.
    pub(crate) fn block_text(&self) -> String {
        normalize(&self.block().copied().collect::<Vec<_>>().join(" "))
    }

    /// The block's lines that carry `label` (`default`, `possible values`),
    /// with everything up to and including the label dropped.
    ///
    /// A fact like a default value is stated *under a label* on both pages —
    /// `default: brief` and `[default: brief]` — so matching the value inside
    /// the labelled remainder is what separates "the row says this is the
    /// default" from "the word `brief` appears somewhere in the row".
    pub(crate) fn labelled(&self, label: &str) -> Vec<String> {
        self.block()
            .filter_map(|line| {
                let offset = find_label(line, label)?;
                Some(line[offset + label.len()..].to_string())
            })
            .collect()
    }

    /// The value names the row's possible-values list states, however the
    /// page spells the list.
    ///
    /// Three spellings exist across the two formatters this parser reads:
    /// standout's labelled line (`possible values: plain text, json` —
    /// comma-joined and never quoted, so a value's whitespace is part of its
    /// name), clap's bracketed clause (`[possible values: "plain text",
    /// json]`, a value quoted when it carries whitespace), and clap's
    /// long-help region — a `Possible values:` heading with a
    /// `- value: description` bullet per value, which clap switches to when
    /// any value has help text. The differential asks whether a value is
    /// *stated*, so every spelling decodes to the same list of names — which
    /// takes two decoders, because the same bytes mean different values under
    /// the two grammars: `plain text, json` is two values in standout's
    /// spelling and three in clap's, where whitespace separates and a
    /// space-carrying value would have arrived quoted. The bracket the label
    /// sits in is what says whose grammar the clause is.
    pub(crate) fn possible_value_names(&self) -> Vec<String> {
        const LABEL: &str = "possible values:";
        let mut names = Vec::new();
        let mut in_region = false;
        for line in self.block() {
            if let Some(offset) = find_label(line, LABEL) {
                in_region = true;
                let remainder = &line[offset + LABEL.len()..];
                if line[..offset].trim_end().ends_with('[') {
                    names.extend(list_values(value_clause(remainder)));
                } else {
                    names.extend(comma_values(remainder));
                }
            } else if in_region {
                // A bullet under the heading states one value, named up to
                // the `: ` that introduces its description — clap always puts
                // the space there, so a bare colon inside a name (`foo:bar`)
                // is the name's own. Anything else — a wrapped bullet
                // description, a `[default: …]` clause — is not a value name
                // and is passed over.
                if let Some(bullet) = line.trim_start().strip_prefix("- ") {
                    let name = match bullet.split_once(": ") {
                        Some((name, _)) => name,
                        None => bullet,
                    }
                    .trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names
    }
}

/// The byte offset of `label` in `line`, matched ASCII-case-insensitively.
///
/// The match runs over bytes rather than over a lowercased copy because
/// lowercasing can change a character's byte length ('İ' lowers to two
/// characters), so an offset found in the copy can point past — or into the
/// middle of — a character of the original. A label is ASCII, and a byte only
/// matches an ASCII byte case-insensitively, so an offset found this way
/// always sits on a character boundary of `line`.
fn find_label(line: &str, label: &str) -> Option<usize> {
    debug_assert!(
        !label.is_empty() && label.is_ascii(),
        "labels are non-empty ASCII by construction"
    );
    line.as_bytes()
        .windows(label.len())
        .position(|window| window.eq_ignore_ascii_case(label.as_bytes()))
}

/// The part of a labelled remainder the label owns: everything up to clap's
/// closing bracket (`[default: brief] [aliases: -t]` must not read the alias
/// as a second default), or the whole remainder on a page that does not
/// bracket its clauses (`default: brief`).
///
/// The bracket that closes the clause is the first one *outside* quotes:
/// clap quotes a value that needs it, so `[default: "[notes.txt]"]` is a
/// one-value clause whose brackets belong to the value, not to the clause.
pub(crate) fn value_clause(remainder: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, c) in remainder.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quotes = false;
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ']' {
            return &remainder[..index];
        }
    }
    remainder
}

/// The values standout's labelled list states: comma-joined and never quoted,
/// so the commas are the only separators and a value's whitespace is part of
/// its name — `plain text, json` is two values. Clap's clauses must not
/// decode here: under clap's grammar bare whitespace separates too, and only
/// [`list_values`] knows its quoting.
pub(crate) fn comma_values(clause: &str) -> Vec<String> {
    clause
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// The values one of *clap's* list clauses states, with its quoting undone.
///
/// Clap separates values with `, ` in a possible-values clause and a bare
/// space in a defaults list, so both separators split here — which is safe
/// precisely because clap wraps a value in Rust-debug quotes when it is empty
/// or carries either (`[default: "plain text"]`). Standout's lists never
/// quote, so they must decode through [`comma_values`] instead: this decoder
/// would shear `plain text` apart at its space.
pub(crate) fn list_values(clause: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    // Whether `current` came from quotes: a quoted value may be empty, and
    // clap quotes an empty value for exactly that reason.
    let mut quoted = false;
    let mut in_quotes = false;
    let mut escaped = false;
    for c in clause.chars() {
        if in_quotes {
            if escaped {
                current.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quotes = false;
            } else {
                current.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
            quoted = true;
        } else if c == ',' || c.is_whitespace() {
            if !current.is_empty() || quoted {
                values.push(std::mem::take(&mut current));
                quoted = false;
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() || quoted {
        values.push(current);
    }
    values
}

/// Parses every two-column row on the page, with its continuation lines.
///
/// A line at column zero is a section heading; an indented line is either a
/// new row or a continuation of the row above it. The two are told apart by
/// the column the open row's *description* starts at: a continuation sits in
/// the description column (that is what makes it read as more of the same
/// row), while a new label starts left of it.
///
/// Indent alone cannot make that call. Clap's own short help indents a flag
/// with a short by two and one without by six, so `--summary` is indented
/// further than the `-p, --pattern` row above it while plainly being its own
/// row. A row whose label sits alone on its line — clap's long help — has no
/// description column yet, and there a deeper indent is its description —
/// unless the line itself spells a flag, which is the next row: a help-less
/// `-f` must not swallow the `--long-only` clap indents past it.
pub(crate) fn rows(page: &str) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut section = "";
    // The row a continuation belongs to: its index, the column a continuation
    // of it must reach, and whether that column is known or still a guess. A
    // heading closes it, so a section's first line is never filed under the
    // previous section's last row.
    let mut open: Option<(usize, usize, bool)> = None;

    for line in page.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if indent == 0 {
            section = line.trim();
            open = None;
            continue;
        }

        if let Some((index, continuation_column, known)) = open {
            // While the description column is still a guess, a deeper line
            // that spells a flag is the next row, not this row's description:
            // clap indents a short-only flag by two and a long-only flag by
            // six, so "deeper than the label" would swallow the long flag
            // whenever the short one above it has no help text.
            let next_label = !known && looks_like_flag_label(line);
            if indent >= continuation_column && !next_label {
                rows[index].continuations.push(line.trim());
                // A label-only row learns its description column from the
                // first line that continues it. Until then any deeper indent
                // would do, which would swallow the next label — clap's long
                // help indents a short-less flag by six and a flag with a
                // short by two, so "deeper than the label" is not the same
                // question as "in the description column".
                if !known {
                    open = Some((index, indent, true));
                }
                continue;
            }
        }

        let rest = &line[indent..];
        let (label, description, continuation_column, known) = match rest.find("  ") {
            Some(gap) => {
                let tail = &rest[gap..];
                let description = tail.trim();
                if description.is_empty() {
                    (rest.trim_end(), "", indent + 1, false)
                } else {
                    let leading = tail.len() - tail.trim_start().len();
                    (
                        rest[..gap].trim_end(),
                        description,
                        indent + gap + leading,
                        true,
                    )
                }
            }
            None => (rest.trim_end(), "", indent + 1, false),
        };
        rows.push(Row {
            label,
            description,
            continuations: Vec::new(),
            line,
            section,
        });
        open = Some((rows.len() - 1, continuation_column, known));
    }

    rows
}

/// Whether a line reads as a new flag row's label rather than as text: a dash
/// followed directly by more of a spelling (`-f`, `--long-only`). A prose dash
/// and clap's `- value: description` bullet put a space after the dash, so
/// neither is mistaken for a flag. Description text that *opens* with a
/// literal flag spelling is — that trade is what keeps the parser free of the
/// command it is parsing, and it only arises while a row's description column
/// is still unknown.
fn looks_like_flag_label(line: &str) -> bool {
    line.trim_start().strip_prefix('-').is_some_and(|tail| {
        tail.trim_start_matches('-')
            .starts_with(|c: char| !c.is_whitespace())
    })
}

/// Collapses every run of whitespace to a single space and trims the ends.
pub(crate) fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds the rendered row for `arg`: the one whose label carries its flag
/// spelling, or — for a positional, which is listed under its metavar — whose
/// label opens with its value name.
pub(crate) fn find_row<'a>(rows: &'a [Row<'a>], arg: &Arg) -> Option<&'a Row<'a>> {
    if arg.is_positional() {
        let mut names = candidate_metavars(arg);
        names.push(arg.get_id().to_string());
        return rows.iter().find(|row| {
            row.label
                .split_whitespace()
                .next()
                .map(metavar_text)
                .is_some_and(|first| names.iter().any(|name| first.eq_ignore_ascii_case(name)))
        });
    }

    rows.iter().find(|row| {
        flag_spellings(arg)
            .iter()
            .any(|spelling| contains_token(row.label, spelling))
    })
}

/// Every spelling a flag row lists the argument under: `-c`, `--color`, or its
/// id when it has neither.
pub(crate) fn flag_spellings(arg: &Arg) -> Vec<String> {
    let mut spellings: Vec<String> = Vec::new();
    if let Some(short) = arg.get_short() {
        spellings.push(format!("-{}", short));
    }
    if let Some(long) = arg.get_long() {
        spellings.push(format!("--{}", long));
    }
    if spellings.is_empty() {
        spellings.push(arg.get_id().to_string());
    }
    spellings
}

/// Whether `haystack` contains `token` delimited by whitespace, a comma, or
/// the punctuation a formatter wraps a value in — so `--all` does not match
/// the row for `--all-files`, and `[aliases: -t, --thr]` yields `--thr`.
pub(crate) fn contains_token(haystack: &str, token: &str) -> bool {
    haystack
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '<' | '>' | '='))
        .any(|word| word == token)
}

/// The arguments a help page is expected to render.
pub(crate) fn visible_args(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments().filter(|arg| !arg.is_hide_set())
}

/// Whether the argument consumes command-line values.
///
/// This asks clap, not standout: a bool presence flag still carries a bool
/// value parser, but it accepts no value in argv, and it is the parse contract
/// help must follow. `get_num_args` is only populated on a built command, so
/// the action is the fallback for a command passed in unbuilt.
pub(crate) fn takes_values(arg: &Arg) -> bool {
    arg.get_num_args()
        .map(|range| range.takes_values())
        .unwrap_or_else(|| arg.get_action().takes_values())
}

/// The value names the argument declared, or `None` when it declared none and
/// clap would fall back to its id.
pub(crate) fn declared_metavars(arg: &Arg) -> Option<Vec<String>> {
    arg.get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>())
        .filter(|names: &Vec<String>| !names.is_empty())
}

/// Every spelling a row might list this argument's value under — the declared
/// names, else its id. Used to *find* a row, where the match is deliberately
/// loose; whether the row spells the name correctly is the assertion's job.
pub(crate) fn candidate_metavars(arg: &Arg) -> Vec<String> {
    declared_metavars(arg).unwrap_or_else(|| vec![arg.get_id().to_string()])
}

/// The value placeholders a row's label offers, with brackets and a repetition
/// ellipsis trimmed off.
///
/// A label is the argument's flag spellings followed by its value placeholders
/// (`-f, --file <PATH>`), or — for a positional — the placeholder alone. Only
/// the placeholders say the argument takes a value, so the flag spellings are
/// dropped: were they kept, a row reduced to `--output` would "show" the
/// metavar of an `output` argument by spelling its own flag.
pub(crate) fn value_placeholders(label: &str) -> Vec<&str> {
    label
        .split(|c: char| c.is_whitespace() || c == ',')
        // `--file=<PATH>` carries the placeholder on the flag's own token.
        .map(|token| token.rsplit('=').next().unwrap_or(token))
        .filter(|token| !token.starts_with('-'))
        .map(metavar_text)
        .filter(|token| !token.is_empty())
        .collect()
}

/// A placeholder's name, without the punctuation a formatter wraps it in.
///
/// Clap brackets a value name to show whether it is required (`<RANGE>`,
/// `[RANGE]`) and marks a repeating one with an ellipsis (`<RANGE>...`);
/// standout's own help leaves the brackets off and tags the metavar instead.
/// The invariant is about the name, so every spelling of the punctuation
/// reduces to it — otherwise a clap-rendered page silently matches no row and
/// the assertion passes by asserting nothing.
pub(crate) fn metavar_text(token: &str) -> &str {
    token
        .trim_end_matches("...")
        .trim_matches(|c| matches!(c, '<' | '>' | '[' | ']'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "\
Keep short notes

USAGE
  notes [OPTIONS] <RANGE>

ARGUMENTS
  RANGE            A range of notes

OPTIONS
  -f, --file <PATH>  Notes file to read
                     default: notes.txt
  --all              Include archived notes
";

    /// Clap's own long help, whose labels and descriptions sit on separate
    /// lines at different indents — the second shape the parser has to read.
    const CLAP_PAGE: &str = "\
Keep short notes

Options:
  -f, --file <PATH>
          Notes file to read

          [default: notes.txt]
";

    #[test]
    fn a_row_carries_its_continuation_lines() {
        let rows = rows(PAGE);
        let file = rows
            .iter()
            .find(|row| row.label.starts_with("-f"))
            .expect("the --file row");

        assert_eq!(file.label, "-f, --file <PATH>");
        assert_eq!(file.description, "Notes file to read");
        assert_eq!(file.continuations, ["default: notes.txt"]);
        assert_eq!(file.section, "OPTIONS");
    }

    #[test]
    fn a_row_knows_the_section_it_sits_in() {
        let rows = rows(PAGE);
        let range = rows
            .iter()
            .find(|row| row.label == "RANGE")
            .expect("the RANGE row");

        assert_eq!(range.section, "ARGUMENTS");
    }

    /// Clap indents an option's description past its label, so the two are one
    /// row; a two-space rule would file the label under the section and lose
    /// the description entirely.
    #[test]
    fn a_clap_shaped_row_keeps_its_indented_description() {
        let rows = rows(CLAP_PAGE);
        assert_eq!(rows.len(), 1, "clap's page has one option row");

        let file = &rows[0];
        assert_eq!(file.label, "-f, --file <PATH>");
        assert_eq!(
            file.block_text(),
            "Notes file to read [default: notes.txt]",
            "the block reads as one line whichever way it was wrapped"
        );
        assert_eq!(file.labelled("default"), [": notes.txt]"]);
    }

    /// Clap indents a short-only flag by two and a long-only flag by six;
    /// when the short flag has no help text, the deeper long flag must open
    /// its own row rather than reading as the first continuation of the row
    /// above it.
    #[test]
    fn a_help_less_row_does_not_swallow_the_next_label() {
        const PAGE: &str = "\
Options:
  -f

      --long-only
          Spelled out in full
";
        let rows = rows(PAGE);
        assert_eq!(rows.len(), 2, "two flags, two rows");
        assert_eq!(rows[0].label, "-f");
        assert_eq!(rows[1].label, "--long-only");
        assert_eq!(rows[1].block_text(), "Spelled out in full");
    }

    /// The label match must not slice the line at an offset found in a
    /// lowercased copy: lowercasing 'İ' grows it by a byte, and the shifted
    /// offset returns the wrong remainder — or splits a character. The text
    /// is application metadata, so the parser has no say in what it contains.
    #[test]
    fn labelled_survives_text_whose_lowercase_shifts_byte_offsets() {
        const PAGE: &str = "\
OPTIONS
  --file <PATH>  İİİİ file İİ Default: notes.txt
";
        let rows = rows(PAGE);
        assert_eq!(rows[0].labelled("default:"), [" notes.txt"]);
    }

    #[test]
    fn list_values_undoes_claps_quoting() {
        assert_eq!(list_values(" brief, full, none"), ["brief", "full", "none"]);
        assert_eq!(list_values(" \"plain text\", json"), ["plain text", "json"]);
        assert_eq!(list_values(" \"a b\" c"), ["a b", "c"]);
        assert_eq!(list_values("\"\""), [""]);
        assert_eq!(list_values(" \"say \\\"hi\\\"\""), ["say \"hi\""]);
    }

    /// Standout never quotes, so its commas are the only separators and a
    /// value keeps its whitespace.
    #[test]
    fn comma_values_keeps_a_values_whitespace() {
        assert_eq!(comma_values(" plain text, json"), ["plain text", "json"]);
        assert_eq!(comma_values(""), Vec::<String>::new());
    }

    /// A quoted value owns its brackets: the clause ends at the first `]`
    /// *outside* quotes, not at the first `]` clap happened to print.
    #[test]
    fn value_clause_skips_a_bracket_inside_quotes() {
        assert_eq!(
            value_clause(" \"[notes.txt]\"] [aliases: -t]"),
            " \"[notes.txt]\""
        );
        assert_eq!(value_clause(" brief] [aliases: -t]"), " brief");
        assert_eq!(value_clause(" brief"), " brief");
    }

    /// Clap's long-help region: a heading, one bullet per value — named up to
    /// the `: ` that introduces its description, quoted nowhere, a bare colon
    /// belonging to the name — and a spec clause after the bullets that
    /// states no value name at all.
    #[test]
    fn possible_value_names_reads_the_bullet_region() {
        const CLAP_LONG: &str = "\
Options:
      --format <FORMAT>
          How to print the notes

          Possible values:
          - plain text
          - key:value
          - json: One note per line

          [default: \"plain text\"]
";
        let rows = rows(CLAP_LONG);
        assert_eq!(rows.len(), 1, "one option row");
        assert_eq!(
            rows[0].possible_value_names(),
            ["plain text", "key:value", "json"]
        );
    }

    #[test]
    fn possible_value_names_reads_the_inline_list() {
        const INLINE: &str = "\
OPTIONS
  --summary <STYLE>  How much detail
                     possible values: brief, full, none
";
        assert_eq!(
            rows(INLINE)[0].possible_value_names(),
            ["brief", "full", "none"]
        );
    }

    /// Standout's inline list is comma-joined and unquoted: a value carrying
    /// whitespace is one value, where clap's grammar would read three.
    #[test]
    fn possible_value_names_reads_standouts_unquoted_list() {
        const INLINE: &str = "\
OPTIONS
  --format <FORMAT>  How to print the notes
                     possible values: plain text, json
";
        assert_eq!(
            rows(INLINE)[0].possible_value_names(),
            ["plain text", "json"]
        );
    }

    /// Clap's bracketed clause keeps clap's grammar: quotes undone, bare
    /// whitespace separating.
    #[test]
    fn possible_value_names_reads_claps_bracketed_clause() {
        const CLAP_SHORT: &str = "\
Options:
  --format <FORMAT>  How to print the notes [possible values: \"plain text\", json]
";
        assert_eq!(
            rows(CLAP_SHORT)[0].possible_value_names(),
            ["plain text", "json"]
        );
    }

    #[test]
    fn token_matching_does_not_confuse_a_prefix_for_a_flag() {
        assert!(contains_token("--all", "--all"));
        assert!(contains_token("-a, --all <N>", "--all"));
        assert!(!contains_token("--all-files", "--all"));
        assert!(
            contains_token("[aliases: -t, --thr]", "--thr"),
            "clap's alias list is punctuation around tokens"
        );
        assert!(
            !contains_token("--threshold <RATIO>", "-t"),
            "a short alias must not be found inside a long flag's spelling"
        );
    }
}
