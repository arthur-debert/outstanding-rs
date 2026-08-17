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

use clap::{Arg, ArgAction, Command};

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

    /// The values the row states under `label` (`default:`), decoded under
    /// the grammar the stating line is written in.
    ///
    /// A fact like a default value is stated *under a label* on both pages —
    /// standout's `default: brief` and clap's `[default: brief]` — so reading
    /// the labelled clause is what separates "the row says this is the
    /// default" from "the word `brief` appears somewhere in the row". And the
    /// same bytes mean different values under the two grammars: clap quotes a
    /// value that is empty or carries whitespace, while standout comma-joins
    /// and never quotes, so a value's whitespace is part of its name. The
    /// bracket the label sits in is what says whose grammar the clause is,
    /// as in [`Row::possible_value_names`]; `joint` says how clap joins this
    /// label's list — a defaults clause is space-joined — which is the
    /// caller's knowledge because the label and its joint go together.
    pub(crate) fn labelled_values(&self, label: &str, joint: ClapJoint) -> Vec<String> {
        let mut values = Vec::new();
        for line in self.block() {
            if let Some(offset) = find_label(line, label) {
                let remainder = &line[offset + label.len()..];
                if line[..offset].trim_end().ends_with('[') {
                    values.extend(list_values(value_clause(remainder), joint));
                } else {
                    values.extend(comma_values(remainder));
                }
            }
        }
        values
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
    ///
    /// The bullet region is the one spelling that quotes nothing at all — clap
    /// prints a value's name raw and separates it from its description with
    /// `": "` — so `arg` is what says where a name ends: a declared name may
    /// itself contain `": "` (`PossibleValue::new("key: value")`), and reading
    /// the punctuation instead of the declaration would decode that bullet as
    /// `key` and reject clap's own page.
    pub(crate) fn possible_value_names(&self, arg: &Arg) -> Vec<String> {
        const LABEL: &str = "possible values:";
        let declared: Vec<String> = arg
            .get_possible_values()
            .iter()
            .map(|value| value.get_name().to_string())
            .collect();
        let mut names = Vec::new();
        let mut in_region = false;
        for line in self.block() {
            if let Some(offset) = find_label(line, LABEL) {
                in_region = true;
                let remainder = &line[offset + LABEL.len()..];
                if line[..offset].trim_end().ends_with('[') {
                    names.extend(list_values(value_clause(remainder), ClapJoint::CommaSpace));
                } else {
                    names.extend(comma_values(remainder));
                }
            } else if in_region {
                // A bullet under the heading states one value. Anything else —
                // a wrapped bullet description, a `[default: …]` clause — is
                // not a value name and is passed over.
                if let Some(bullet) = line.trim_start().strip_prefix("- ") {
                    let bullet = bullet.trim_end();
                    let name = declared_name_at(bullet, &declared).unwrap_or_else(|| {
                        // The page states a value the argument does not
                        // declare, so there is no declaration to read it
                        // against. Falling back to clap's separator keeps the
                        // failure legible — the caller reports the decoded
                        // names, and a truncated one still names the row —
                        // where dropping the bullet would report the value as
                        // simply absent.
                        match bullet.split_once(": ") {
                            Some((name, _)) => name.trim(),
                            None => bullet,
                        }
                    });
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names
    }
}

/// The declared possible value a long-help bullet states: the longest declared
/// name the bullet opens with, where what follows is either nothing or the
/// `": "` clap introduces a value's description with.
///
/// Longest wins because both readings can be declared at once — an argument
/// offering `key` and `key: value` renders the second as a bullet the first is
/// a prefix of, and clap's own page cannot tell them apart either.
fn declared_name_at<'a>(bullet: &str, declared: &'a [String]) -> Option<&'a str> {
    declared
        .iter()
        .filter(|name| {
            bullet
                .strip_prefix(name.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(": "))
        })
        .max_by_key(|name| name.len())
        .map(String::as_str)
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
///
/// Quotes only open where clap can have opened one. Clap quotes a value only
/// when it is empty or carries whitespace, and its separators always put a
/// space before a quoted value, so a `"` reads as clap's quoting only after
/// whitespace: a mid-word `"` is a literal character of an unquoted value
/// (`[default: foo"bar]`), and so is one directly after a comma (`a,"b` is
/// one value — a separator comma is always followed by a space). Reading
/// either as an opening quote would swallow the `]` that ends the clause.
pub(crate) fn value_clause(remainder: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    // Whether the next character sits where a value can start — the only
    // place a `"` reads as clap's quoting.
    let mut word_start = true;
    for (index, c) in remainder.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quotes = false;
            }
        } else if c == '"' && word_start {
            in_quotes = true;
        } else if c == ']' {
            return &remainder[..index];
        }
        word_start = !in_quotes && c.is_whitespace();
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

/// The joint one of *clap's* list clauses is written with, which decides what
/// a bare `,` means.
///
/// Clap joins a defaults list with a space (`[default: "plain text" b]`) and
/// a possible-values clause with `", "`. Under the space joint every comma is
/// a value's own character (`[default: a,b]` is one value). Under the
/// comma-space joint a `,` directly followed by whitespace is the separator
/// and any other comma belongs to the value — `a,b` is one value and `a,, b`
/// is `a,` then `b` — because clap would have quoted any value carrying the
/// whitespace half of the separator. The label a clause is stated under says
/// which joint clap wrote it with, so the caller passes it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClapJoint {
    /// A space-joined list: clap's defaults clause.
    Spaces,
    /// A `", "`-joined list: clap's possible-values clause.
    CommaSpace,
}

/// The values one of *clap's* list clauses states, with its quoting undone.
///
/// Whitespace always separates — clap wraps a value in Rust-debug quotes
/// (`format!("{:?}")`) when it is empty or carries whitespace
/// (`[default: "plain text"]`), so bare whitespace is never a value's own —
/// and `joint` says what a comma means. Quoting is undone exactly, escapes
/// included: `"line\nbreak"` decodes back to the newline it renders, not to
/// the letter `n`. Those are the *only* values clap quotes, so a `"` opens a
/// quoted value only at the start of a token; a mid-word `"` is a literal
/// character of an unquoted value (`foo"bar`), not a quote to enter.
/// Standout's lists never quote, so they must decode through
/// [`comma_values`] instead: this decoder would shear `plain text` apart at
/// its space.
pub(crate) fn list_values(clause: &str, joint: ClapJoint) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    // Whether `current` came from quotes: a quoted value may be empty, and
    // clap quotes an empty value for exactly that reason.
    let mut quoted = false;
    let mut in_quotes = false;
    let mut chars = clause.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '\\' {
                current.push(unescape(&mut chars));
            } else if c == '"' {
                in_quotes = false;
            } else {
                current.push(c);
            }
        } else if c == '"' && current.is_empty() && !quoted {
            in_quotes = true;
            quoted = true;
        } else if c.is_whitespace()
            || (c == ','
                && joint == ClapJoint::CommaSpace
                && chars.peek().is_some_and(|next| next.is_whitespace()))
        {
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

/// Reverses one character of the Rust-debug escaping clap quotes with: the
/// named escapes (`\n`, `\r`, `\t`, `\0`), the self-escapes (`\"`, `\\`),
/// and the `\u{…}` spelling Debug gives every other control or non-printable
/// character. Called with the iterator sitting just past the backslash.
fn unescape(chars: &mut std::iter::Peekable<std::str::Chars>) -> char {
    match chars.next() {
        Some('n') => '\n',
        Some('r') => '\r',
        Some('t') => '\t',
        Some('0') => '\0',
        Some('u') => {
            let mut hex = String::new();
            for c in chars.by_ref() {
                match c {
                    '{' => {}
                    '}' => break,
                    c => hex.push(c),
                }
            }
            u32::from_str_radix(&hex, 16)
                .ok()
                .and_then(char::from_u32)
                // Debug formatting cannot produce a malformed `\u{…}`; the
                // replacement character keeps a misread visible in the
                // failing equality rather than panicking the parser.
                .unwrap_or(char::REPLACEMENT_CHARACTER)
        }
        // `\"` and `\\` decode to the character behind the backslash.
        Some(other) => other,
        None => '\\',
    }
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
/// label offers its value name as a placeholder.
///
/// A positional is looked for among the positional rows alone: an option's
/// label carries placeholders too, and `--into <DEST>` is not the row for a
/// positional named `DEST`.
pub(crate) fn find_row<'a>(rows: &'a [Row<'a>], arg: &Arg) -> Option<&'a Row<'a>> {
    if arg.is_positional() {
        let mut names = candidate_metavars(arg);
        names.push(arg.get_id().to_string());
        return rows
            .iter()
            .filter(|row| positional_row(row.label))
            .find(|row| {
                value_placeholders(row.label)
                    .iter()
                    .any(|shown| names.iter().any(|name| shown.eq_ignore_ascii_case(name)))
            });
    }

    rows.iter().find(|row| {
        flag_spellings(arg)
            .iter()
            .any(|spelling| contains_flag_token(row.label, spelling, arg))
    })
}

/// Whether a row lists a positional: its label is value placeholders and the
/// decoration a formatter puts on them, and nothing else.
///
/// Two other labels carry placeholders and must not be mistaken for one. An
/// option's label spells its flags first (`-f, --file <PATH>`), so a flag
/// token rules the row out — otherwise `--into <DEST>` would answer for a
/// positional named `DEST` that has no row at all. A usage line is a row too
/// on a page whose usage sits under a heading (`notes [OPTIONS] [RANGE]`), and
/// it is ruled out by the bare *word* outside its brackets: a positional row
/// has none, and standout's own unbracketed label (`RANGE`) is the placeholder
/// itself.
///
/// The repetition ellipsis is decoration, not a word. Clap marks a repeating
/// positional `[FILE]...` or `<FILE>...` — the ellipsis states arity, the way
/// the brackets state requiredness — so it sits outside the bracketed group
/// and must not be read as text that makes the row something other than a
/// positional's.
pub(crate) fn positional_row(label: &str) -> bool {
    if label.split_whitespace().any(|token| token.starts_with('-')) {
        return false;
    }
    if !label.contains(['<', '[']) {
        return !label.trim().is_empty();
    }
    undecorated(&split_brackets(label).1).is_empty()
}

/// A label's text with the repetition ellipsis and the whitespace around it
/// taken off — what is left is the label's own words, if it has any.
fn undecorated(text: &str) -> String {
    text.replace(ELLIPSIS, " ").trim().to_string()
}

/// Clap's repetition marker, which it appends to a repeating argument's
/// spelling (`-v...`) or value placeholder (`[FILE]...`) to state arity.
pub(crate) const ELLIPSIS: &str = "...";

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
///
/// The comparison is exact: this matcher is shared by subcommand names,
/// aliases, and flag spellings, so it must not read anything extra off a
/// word — prose ending in `go...` does not state a name `go`. The one
/// formatter decoration a spelling can wear, clap's repetition ellipsis on a
/// counted flag, is accepted only by [`contains_flag_token`], where the
/// caller knows the argument that earns it.
pub(crate) fn contains_token(haystack: &str, token: &str) -> bool {
    haystack
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '<' | '>' | '='))
        .any(|word| word == token)
}

/// Whether `haystack` lists `spelling` as a flag of `arg`: the exact token,
/// or — only for a counted argument — the token wearing clap's repetition
/// ellipsis, which spells a counted flag `-v...` to state its arity, not its
/// name. Gating the ellipsis on `ArgAction::Count` keeps it from satisfying
/// anything else: a missing alias `go` must not be answered by prose that
/// happens to end in `go...`.
pub(crate) fn contains_flag_token(haystack: &str, spelling: &str, arg: &Arg) -> bool {
    contains_token(haystack, spelling)
        || (matches!(arg.get_action(), ArgAction::Count)
            && contains_token(haystack, &format!("{spelling}{ELLIPSIS}")))
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
///
/// The brackets, where the label has them, are the placeholder boundary rather
/// than decoration to strip: clap wraps every value name in `<…>` or `[…]`,
/// and a declared name is free to carry a space (`value_name("A B")` renders
/// `<A B>`), which a whitespace split would report as two placeholders and
/// neither of them the declared one. A label with no brackets is standout's
/// own, where the placeholder is a bare word in the name column — there a
/// space-carrying name is unstatable, the same page limit standout's unquoted
/// value lists have, and the tokens are all there is to read.
pub(crate) fn value_placeholders(label: &str) -> Vec<&str> {
    if label.contains(['<', '[']) {
        return split_brackets(label).0;
    }
    label
        .split(|c: char| c.is_whitespace() || c == ',')
        // `--file=<PATH>` carries the placeholder on the flag's own token.
        .map(|token| token.rsplit('=').next().unwrap_or(token))
        .filter(|token| !token.starts_with('-'))
        .map(metavar_text)
        .filter(|token| !token.is_empty())
        .collect()
}

/// Splits a bracketing label into its groups and everything between them.
///
/// A group is the text between a `<` or `[` and its matching closer, which is
/// the value name exactly as declared — a name is free to carry a space, and
/// the bracket is the only thing that says where it ends. What falls outside
/// is the rest of the label: flag spellings, their separators, a repetition
/// ellipsis. An unclosed bracket ends the walk, so a label the formatter
/// truncated yields the groups it did close.
fn split_brackets(label: &str) -> (Vec<&str>, String) {
    let mut groups = Vec::new();
    let mut outside = String::new();
    let mut rest = label;
    while let Some(open) = rest.find(['<', '[']) {
        let closer = if rest.as_bytes()[open] == b'<' {
            '>'
        } else {
            ']'
        };
        let Some(offset) = rest[open + 1..].find(closer) else {
            break;
        };
        outside.push_str(&rest[..open]);
        let close = open + 1 + offset;
        let name = &rest[open + 1..close];
        if !name.is_empty() {
            groups.push(name);
        }
        rest = &rest[close + 1..];
    }
    outside.push_str(rest);
    (groups, outside)
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
        .trim_end_matches(ELLIPSIS)
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
        assert_eq!(
            file.labelled_values("default:", ClapJoint::Spaces),
            ["notes.txt"]
        );
    }

    /// The labelled decode follows the line's grammar: clap's bracketed
    /// clause space-joins and quotes, standout's unbracketed one is a raw
    /// comma list whose values keep their whitespace — and their brackets.
    #[test]
    fn labelled_values_decodes_under_the_stating_lines_grammar() {
        const CLAP: &str = "\
Options:
  -f, --file <PATH>
          Notes file to read

          [default: \"plain text\" json]
";
        assert_eq!(
            rows(CLAP)[0].labelled_values("default:", ClapJoint::Spaces),
            ["plain text", "json"]
        );

        const STANDOUT: &str = "\
OPTIONS
  -f, --file <PATH>  Notes file to read
                     default: [a b]
";
        assert_eq!(
            rows(STANDOUT)[0].labelled_values("default:", ClapJoint::Spaces),
            ["[a b]"]
        );
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
        assert_eq!(
            rows[0].labelled_values("default:", ClapJoint::Spaces),
            ["notes.txt"]
        );
    }

    #[test]
    fn list_values_undoes_claps_quoting() {
        assert_eq!(
            list_values(" brief, full, none", ClapJoint::CommaSpace),
            ["brief", "full", "none"]
        );
        assert_eq!(
            list_values(" \"plain text\", json", ClapJoint::CommaSpace),
            ["plain text", "json"]
        );
        assert_eq!(list_values(" \"a b\" c", ClapJoint::Spaces), ["a b", "c"]);
        assert_eq!(list_values("\"\"", ClapJoint::Spaces), [""]);
        assert_eq!(
            list_values(" \"say \\\"hi\\\"\"", ClapJoint::Spaces),
            ["say \"hi\""]
        );
    }

    /// Clap quotes with Rust-debug formatting, so the escapes it writes are
    /// exactly Debug's: the named ones, the self-escapes, and `\u{…}` for
    /// every other control character. Each decodes back to the character it
    /// renders — `\n` is a newline, not the letter `n`.
    #[test]
    fn list_values_undoes_claps_debug_escapes() {
        assert_eq!(
            list_values(" \"line\\nbreak\" \"tab\\tstop\"", ClapJoint::Spaces),
            ["line\nbreak", "tab\tstop"]
        );
        assert_eq!(
            list_values(" \"back\\\\slash here\"", ClapJoint::Spaces),
            ["back\\slash here"]
        );
        assert_eq!(
            list_values(" \"esc \\u{1b}del\\u{7f}\"", ClapJoint::Spaces),
            ["esc \u{1b}del\u{7f}"]
        );
        assert_eq!(
            list_values(" \"cr\\r\\0end\"", ClapJoint::Spaces),
            ["cr\r\0end"]
        );
    }

    /// Clap quotes only a value that is empty or carries whitespace, so a
    /// mid-word `"` is a literal character of an unquoted value — not a quote
    /// to enter, which would swallow the separators after it.
    #[test]
    fn list_values_keeps_a_mid_word_quote_literal() {
        assert_eq!(
            list_values(" foo\"bar, json", ClapJoint::CommaSpace),
            ["foo\"bar", "json"]
        );
    }

    /// The joint decides what a bare comma means. A `", "`-joined clause
    /// separates only at comma-then-whitespace, so `a,b` is one value and a
    /// trailing comma is a value's own (`a,, b` is `a,` then `b`); a
    /// space-joined defaults clause never separates at a comma at all.
    #[test]
    fn list_values_reads_a_bare_comma_by_the_joint() {
        assert_eq!(
            list_values(" a,b, other", ClapJoint::CommaSpace),
            ["a,b", "other"]
        );
        assert_eq!(
            list_values(" a,, other", ClapJoint::CommaSpace),
            ["a,", "other"]
        );
        assert_eq!(
            list_values(" other, a,", ClapJoint::CommaSpace),
            ["other", "a,"]
        );
        assert_eq!(list_values(" a,b c,", ClapJoint::Spaces), ["a,b", "c,"]);
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

    /// A mid-word `"` belongs to an unquoted value — clap quotes only a value
    /// that is empty or carries whitespace — so it must not open quote mode
    /// and swallow the `]` that ends the clause. Neither must a `"` directly
    /// after a comma: clap's separators always put a space before a quoted
    /// value, so `a,"b` is an unquoted value's own characters.
    #[test]
    fn value_clause_treats_a_mid_word_quote_as_literal() {
        assert_eq!(value_clause(" foo\"bar] [aliases: -t]"), " foo\"bar");
        assert_eq!(value_clause(" a,\"b] [aliases: -t]"), " a,\"b");
    }

    /// An argument declaring `values` as its possible values — the metadata
    /// the bullet decoder reads a name's end against.
    fn valued(values: &[&str]) -> Arg {
        Arg::new("format")
            .long("format")
            .value_parser(clap::builder::PossibleValuesParser::new(
                values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>(),
            ))
    }

    /// Clap's long-help region: a heading, one bullet per value — printed raw
    /// and quoted nowhere, with `": "` before a description clap pads into its
    /// own column — and a spec clause after the bullets that states no value
    /// name at all. A name carrying the separator (`key: value`) ends where
    /// the declaration says it does, not where the punctuation first appears.
    #[test]
    fn possible_value_names_reads_the_bullet_region() {
        const CLAP_LONG: &str = "\
Options:
      --format <FORMAT>
          How to print the notes

          Possible values:
          - plain text
          - key:value
          - key: value
          - json:       One note per line

          [default: \"plain text\"]
";
        let rows = rows(CLAP_LONG);
        assert_eq!(rows.len(), 1, "one option row");
        assert_eq!(
            rows[0].possible_value_names(&valued(&[
                "plain text",
                "key:value",
                "key: value",
                "json"
            ])),
            ["plain text", "key:value", "key: value", "json"]
        );
    }

    /// A bullet the argument does not declare is still reported — decoded at
    /// clap's separator, which is all there is to go on — so the caller's
    /// failure names the row rather than calling the value absent.
    #[test]
    fn possible_value_names_reports_an_undeclared_bullet() {
        const CLAP_LONG: &str = "\
Options:
      --format <FORMAT>
          How to print the notes

          Possible values:
          - json: One note per line
";
        assert_eq!(
            rows(CLAP_LONG)[0].possible_value_names(&valued(&["yaml"])),
            ["json"]
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
            rows(INLINE)[0].possible_value_names(&valued(&["brief", "full", "none"])),
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
            rows(INLINE)[0].possible_value_names(&valued(&["plain text", "json"])),
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
            rows(CLAP_SHORT)[0].possible_value_names(&valued(&["plain text", "json"])),
            ["plain text", "json"]
        );
    }

    /// A bracket is where a placeholder ends, so a declared name carrying a
    /// space stays one placeholder. Where a label brackets nothing — standout
    /// spells its name column bare — the tokens are all there is to read.
    #[test]
    fn value_placeholders_read_a_bracketed_name_whole() {
        assert_eq!(value_placeholders("[A B]"), ["A B"]);
        assert_eq!(value_placeholders("-f, --file <P Q>"), ["P Q"]);
        assert_eq!(value_placeholders("[SRC] [DEST]"), ["SRC", "DEST"]);
        assert_eq!(value_placeholders("--file=<PATH>"), ["PATH"]);
        assert_eq!(value_placeholders("<RANGE>..."), ["RANGE"]);
        assert_eq!(value_placeholders("--file P"), ["P"]);
        assert_eq!(value_placeholders("--all"), Vec::<&str>::new());
    }

    /// A positional row is placeholders and their decoration and nothing else.
    /// A flag row spells its flags first, and a usage line filed under a
    /// heading — which is a row like any other to the parser — carries the
    /// program name outside its brackets.
    #[test]
    fn a_positional_row_is_told_from_a_flag_row_and_a_usage_line() {
        assert!(positional_row("[SRC] [DEST]"));
        assert!(positional_row("[A B]"));
        assert!(positional_row("RANGE"));
        assert!(!positional_row("-f, --file <PATH>"));
        assert!(!positional_row("--into <DEST>"));
        assert!(!positional_row("notes [OPTIONS] [RANGE]"));
        assert!(!positional_row(""));
    }

    /// Clap's repetition ellipsis states a repeating positional's arity, not
    /// that the row is something other than a positional's — and it sits
    /// outside the brackets, where a program name would.
    #[test]
    fn a_repeating_positional_row_keeps_its_ellipsis() {
        assert!(positional_row("[FILE]..."));
        assert!(positional_row("<FILE>..."));
        assert!(positional_row("<SRC>... <DEST>"));
        assert!(positional_row("FILE..."));
        assert_eq!(value_placeholders("[FILE]..."), ["FILE"]);
        assert_eq!(value_placeholders("FILE..."), ["FILE"]);
        assert!(
            !positional_row("notes [OPTIONS] <FILE>..."),
            "a usage line still carries the program name outside its brackets"
        );
        // The ellipsis is only decoration where clap puts it: outside the
        // group. A declared name is read whole, dots and all.
        assert!(positional_row("[A...B]"));
        assert_eq!(value_placeholders("[A...B]"), ["A...B"]);
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

    /// The shared matcher is exact: an ellipsis-wearing word states nothing
    /// shorter than itself, so prose, a subcommand name, or an alias spelled
    /// `go...` never answers for a missing `go`.
    #[test]
    fn token_matching_does_not_read_through_an_ellipsis() {
        assert!(!contains_token("go...", "go"));
        assert!(!contains_token("try `go...` to continue", "go"));
        assert!(!contains_token("[aliases: go...]", "go"));
    }

    /// Only a counted flag earns the ellipsis read: clap spells its row
    /// `-v...`, and the marker is about arity, not part of the spelling. Any
    /// other argument's match stays exact.
    #[test]
    fn only_a_counted_flag_matches_through_its_ellipsis() {
        let counted = Arg::new("verbose").short('v').action(ArgAction::Count);
        assert!(contains_flag_token("-v...", "-v", &counted));
        assert!(contains_flag_token("-v", "-v", &counted));
        assert!(
            !contains_flag_token("-v...", "-w", &counted),
            "the word behind the ellipsis still has to be the spelling"
        );

        let plain = Arg::new("verbose").short('v').action(ArgAction::SetTrue);
        assert!(contains_flag_token("-v", "-v", &plain));
        assert!(!contains_flag_token("-v...", "-v", &plain));
    }
}
