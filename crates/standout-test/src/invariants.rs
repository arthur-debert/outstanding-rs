//! Universal and negative invariants, as reusable assertions.
//!
//! The suite's existing help assertions are **existential over hand-picked
//! strings** — `assert_stdout_contains("default: auto")`. Every defect that
//! escaped to a downstream app was the other two shapes: a **universal**
//! ("every value-taking option shows a metavar") or a **negative** ("no
//! presence flag shows a possible-values row"). Neither can be expressed by
//! listing strings that should be there, which is why the bogus
//! `possible values: true, false` row of #301 rendered inside the test suite's
//! own output on every run with no oracle to reject it.
//!
//! These assertions are that missing vocabulary. Each states a property of a
//! *whole page* and names the offending element — the argument, the tag, the
//! row — when the property does not hold.
//!
//! # The two forms
//!
//! Every invariant *about the text of a page* comes in two functions:
//!
//! - `assert_*(result, …)` takes a [`TestResult`] and is what tests call.
//! - `assert_*_in_page(page, …)` takes the rendered page (and any oracle it
//!   needs) directly — `assert_styling_preserves_layout_in_pages` takes the
//!   two pages it compares.
//!
//! The page form exists because an oracle that cannot fail is worse than none:
//! it lets the assertion library's own tests feed it a page that violates the
//! invariant, without having to find or build a framework defect first. It is
//! also what a caller with a page from somewhere else — a process run, a
//! snapshot file — reaches for.
//!
//! [`assert_every_tag_resolved`] is the one exception, and necessarily so: it
//! reads the run's structured render diagnostics, which a rendered page does
//! not carry — that is the whole reason it can name a tag in a mode where the
//! page shows no evidence at all. Its page-shaped counterpart is
//! [`assert_no_unresolved_tag_markers_in_page`], which proves the weaker thing
//! a page *can* state.
//!
//! # The two forms of "no unresolved tag"
//!
//! [`assert_every_tag_resolved`] and [`assert_no_unresolved_tag_markers`] look
//! like the same check and are not. The first reads the structured record of
//! what the style-tag pass resolved ([`standout::diagnostics`]) and proves *the
//! theme is incomplete*, in every output mode, naming the tag. The second
//! searches the page for the `[tag?]` marker and proves *the corruption
//! reached the page a user sees*, which only happens under
//! [`TagTransform::Apply`](standout_render::TagTransform). #303 deserves both:
//! a page can be structurally corrupt in `Text` mode while looking perfect,
//! and the marker is what the user actually filed the bug about.
//!
//! # What these do not do
//!
//! They do not check that a row exists for every argument — that is the
//! clap-parity differential's job. These state properties of the rows that
//! *are* rendered; an argument with no row at all is simply not their subject.

use clap::{Arg, Command};

use crate::TestResult;

// ---------------------------------------------------------------------------
// Tag resolution
// ---------------------------------------------------------------------------

/// Panics unless every tag the run's renders emitted was defined in the
/// resolved theme.
///
/// This is the structural form: it reads what the style-tag pass recorded, so
/// it holds in `Text` and `TermDebug` as strongly as in `Term`, needs no ANSI
/// and no TTY, and names the tag rather than a substring of the page.
///
/// "The run's renders" includes a run the handler itself drove — an app
/// invoking another app — whatever it then did with that run's output, because
/// the capture layer cannot see whether the output was embedded or discarded
/// and the safe total policy is the one with no false pass. The failure marks
/// those offenders as coming from a nested run, and
/// [`TagResolution::nesting_depth`](standout_render::TagResolution::nesting_depth)
/// is what to filter on to state the narrower property.
#[track_caller]
pub fn assert_every_tag_resolved(result: &TestResult) {
    let offenders: Vec<String> = result
        .tag_resolutions()
        .iter()
        .filter(|pass| !pass.is_clean())
        .flat_map(|pass| {
            let origin = match pass.nesting_depth() {
                0 => String::new(),
                depth => format!(", rendered by a nested run {depth} level(s) in"),
            };
            pass.unresolved().iter().map(move |error| {
                format!(
                    "  - {} (transform: {:?}, unknown-tag policy: {:?}{})",
                    error,
                    pass.transform(),
                    pass.unknown_behavior(),
                    origin
                )
            })
        })
        .collect();

    if offenders.is_empty() {
        return;
    }

    let defined: Vec<&str> = result
        .tag_resolutions()
        .iter()
        .find(|pass| !pass.is_clean())
        .map(|pass| pass.defined_tags().iter().map(String::as_str).collect())
        .unwrap_or_default();

    panic!(
        "the run emitted {} tag(s) the resolved theme does not define:\n{}\n\
         the resolved theme defines: {}",
        offenders.len(),
        offenders.join("\n"),
        if defined.is_empty() {
            "nothing".to_string()
        } else {
            defined.join(", ")
        }
    );
}

/// Panics if any `[tag?]` marker reached the rendered page.
///
/// The marker is what an unresolved tag leaves behind under
/// [`TagTransform::Apply`](standout_render::TagTransform) with the
/// [`Passthrough`](standout_render::UnknownTagBehavior) policy the framework
/// renders with — template markup leaking into user-facing output. Checking it
/// needs no real ANSI: `Term` mode applies the transform whether or not color
/// bytes are produced.
#[track_caller]
pub fn assert_no_unresolved_tag_markers(result: &TestResult) {
    assert_no_unresolved_tag_markers_in_page(&result.stdout_plain());
}

/// [`assert_no_unresolved_tag_markers`] against a page directly.
#[track_caller]
pub fn assert_no_unresolved_tag_markers_in_page(page: &str) {
    let markers = unresolved_tag_markers(page);
    if markers.is_empty() {
        return;
    }

    panic!(
        "{} unresolved tag marker(s) reached the page: {}\n--- page ---\n{}\n------------",
        markers.len(),
        markers
            .iter()
            .map(|tag| format!("[{}?]", tag))
            .collect::<Vec<_>>()
            .join(", "),
        page
    );
}

/// The distinct tag names marked unresolved in `page`, in first-seen order.
fn unresolved_tag_markers(page: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = page[cursor..].find("?]") {
        let marker_end = cursor + offset;
        if let Some(open) = page[..marker_end].rfind('[') {
            let inner = &page[open + 1..marker_end];
            let name = inner.strip_prefix('/').unwrap_or(inner);
            if is_tag_name(name) && !found.iter().any(|seen| seen == name) {
                found.push(name.to_string());
            }
        }
        cursor = marker_end + 2;
    }

    found
}

fn is_tag_name(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

// ---------------------------------------------------------------------------
// Styling may not change layout or content
// ---------------------------------------------------------------------------

/// Panics unless the styled page, with its escapes stripped, is exactly the
/// unstyled page.
///
/// Styling is allowed to add color and nothing else: any difference in layout
/// or content between the two modes is a rendering defect in one of them. This
/// is the second direction #303 was visible from — a page whose tags resolve in
/// `Text` and corrupt in `Term` differs here even before anyone looks for a
/// marker.
#[track_caller]
pub fn assert_styling_preserves_layout(styled: &TestResult, plain: &TestResult) {
    assert_styling_preserves_layout_in_pages(&styled.stdout_plain(), plain.stdout());
}

/// [`assert_styling_preserves_layout`] against two pages directly. `styled` is
/// expected to have already had its escapes stripped.
#[track_caller]
pub fn assert_styling_preserves_layout_in_pages(styled_stripped: &str, plain: &str) {
    if styled_stripped == plain {
        return;
    }

    let detail = match first_difference(styled_stripped, plain) {
        Some((line_number, styled_line, plain_line)) => format!(
            "first difference at line {}:\n  styled (stripped): {:?}\n  plain            : {:?}",
            line_number, styled_line, plain_line
        ),
        None => "every rendered line matches; the pages differ only in trailing \
                 line-ending bytes"
            .to_string(),
    };

    panic!(
        "styling changed the page beyond color; {}\n\
         --- styled (stripped) ---\n{}\n--- plain ---\n{}\n-------------",
        detail, styled_stripped, plain
    );
}

/// The 1-based line number of the first differing line, with both sides.
///
/// `None` when the two pages have the same lines and differ only in bytes
/// [`str::lines`] does not yield — a trailing newline, a `\r` before one. The
/// caller says so rather than reporting a difference at a line that does not
/// exist.
fn first_difference(left: &str, right: &str) -> Option<(usize, String, String)> {
    let mut left_lines = left.lines();
    let mut right_lines = right.lines();
    let mut line_number = 0;

    loop {
        line_number += 1;
        match (left_lines.next(), right_lines.next()) {
            (None, None) => return None,
            (l, r) if l != r => {
                return Some((
                    line_number,
                    l.unwrap_or("<end of page>").to_string(),
                    r.unwrap_or("<end of page>").to_string(),
                ))
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Argument rows, against clap's own metadata
// ---------------------------------------------------------------------------

/// Panics if any argument that takes no value shows a possible-values row.
///
/// Clap's own formatter suppresses possible values for `SetTrue`, `SetFalse`,
/// and `Count`, because the underlying bool parser's `true`/`false` are not
/// things the user may type: `--staged true` is not valid syntax. A row that
/// lists them invites exactly that mistake (#301).
///
/// `cmd` is the command whose help was rendered — clap's metadata is the
/// oracle for which arguments take a value.
#[track_caller]
pub fn assert_no_possible_values_for_valueless_args(result: &TestResult, cmd: &Command) {
    assert_no_possible_values_for_valueless_args_in_page(&result.stdout_plain(), cmd);
}

/// [`assert_no_possible_values_for_valueless_args`] against a page directly.
#[track_caller]
pub fn assert_no_possible_values_for_valueless_args_in_page(page: &str, cmd: &Command) {
    let rows = rows(page);

    for arg in visible_args(cmd) {
        if takes_values(arg) {
            continue;
        }
        let Some(row) = find_row(&rows, arg) else {
            continue;
        };
        if let Some(offending) = row
            .block()
            .find(|line| line.to_lowercase().contains("possible values"))
        {
            panic!(
                "`{}` takes no value, so its row must not list possible values \
                 (clap's own formatter suppresses them):\n  row  : {:?}\n  found: {:?}",
                arg.get_id(),
                row.line,
                offending
            );
        }
    }
}

/// Panics unless every value-taking argument's row shows a metavar.
///
/// An option that takes a value must say so where the user reads it — in the
/// row's label, not by luck in its prose. Before #302 was fixed,
/// `--threshold <RATIO>` rendered as `--threshold`, indistinguishable from a
/// presence flag.
///
/// The expected metavar is clap's: a declared value name must appear exactly
/// as declared — an app writes `value_name = "RATIO"` precisely so the user
/// reads `RATIO` — while an argument that declared none is matched against its
/// id case-insensitively, since clap's fallback uppercases the id and
/// standout's does not.
///
/// It is matched against the row's *value placeholders* alone, never against
/// the whole label: `--output` spells its own id, so a row that lost its
/// metavar entirely would satisfy a substring search of the label while telling
/// the user nothing about the value it takes.
#[track_caller]
pub fn assert_metavar_for_valued_args(result: &TestResult, cmd: &Command) {
    assert_metavar_for_valued_args_in_page(&result.stdout_plain(), cmd);
}

/// [`assert_metavar_for_valued_args`] against a page directly.
#[track_caller]
pub fn assert_metavar_for_valued_args_in_page(page: &str, cmd: &Command) {
    let rows = rows(page);

    for arg in visible_args(cmd) {
        if !takes_values(arg) {
            continue;
        }
        let Some(row) = find_row(&rows, arg) else {
            continue;
        };

        let placeholders = value_placeholders(row.label);
        let missing: Vec<String> = match declared_metavars(arg) {
            Some(declared) => declared
                .into_iter()
                .filter(|name| !placeholders.contains(&name.as_str()))
                .collect(),
            None => {
                let id = arg.get_id().to_string();
                if placeholders
                    .iter()
                    .any(|shown| shown.eq_ignore_ascii_case(&id))
                {
                    Vec::new()
                } else {
                    vec![id]
                }
            }
        };

        if !missing.is_empty() {
            panic!(
                "`{}` takes a value, so its row must show the value name(s) {:?}; \
                 the row shows {:?}:\n  row  : {:?}\n  label: {:?}",
                arg.get_id(),
                missing,
                placeholders,
                row.line,
                row.label
            );
        }
    }
}

/// The arguments a help page is expected to render.
fn visible_args(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments().filter(|arg| !arg.is_hide_set())
}

/// Whether the argument consumes command-line values.
///
/// This asks clap, not standout: a bool presence flag still carries a bool
/// value parser, but it accepts no value in argv, and it is the parse contract
/// help must follow. `get_num_args` is only populated on a built command, so
/// the action is the fallback for a command passed in unbuilt.
fn takes_values(arg: &Arg) -> bool {
    arg.get_num_args()
        .map(|range| range.takes_values())
        .unwrap_or_else(|| arg.get_action().takes_values())
}

/// The value names the argument declared, or `None` when it declared none and
/// clap would fall back to its id.
fn declared_metavars(arg: &Arg) -> Option<Vec<String>> {
    arg.get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>())
        .filter(|names: &Vec<String>| !names.is_empty())
}

/// Every spelling a row might list this argument's value under — the declared
/// names, else its id. Used to *find* a row, where the match is deliberately
/// loose; whether the row spells the name correctly is the assertion's job.
fn candidate_metavars(arg: &Arg) -> Vec<String> {
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
fn value_placeholders(label: &str) -> Vec<&str> {
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
fn metavar_text(token: &str) -> &str {
    token
        .trim_end_matches("...")
        .trim_matches(|c| matches!(c, '<' | '>' | '[' | ']'))
}

/// Finds the rendered row for `arg`: the one whose label carries its flag
/// spelling, or — for a positional, which is listed under its metavar — whose
/// label opens with its value name.
fn find_row<'a>(rows: &'a [Row<'a>], arg: &Arg) -> Option<&'a Row<'a>> {
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

    let mut spellings: Vec<String> = Vec::new();
    if let Some(long) = arg.get_long() {
        spellings.push(format!("--{}", long));
    }
    if let Some(short) = arg.get_short() {
        spellings.push(format!("-{}", short));
    }
    if spellings.is_empty() {
        spellings.push(arg.get_id().to_string());
    }

    rows.iter().find(|row| {
        spellings
            .iter()
            .any(|spelling| contains_token(row.label, spelling))
    })
}

/// Whether `haystack` contains `token` delimited by whitespace or a comma —
/// so `--all` does not match the row for `--all-files`.
fn contains_token(haystack: &str, token: &str) -> bool {
    haystack
        .split(|c: char| c.is_whitespace() || c == ',')
        .any(|word| word == token)
}

// ---------------------------------------------------------------------------
// Whole-page column alignment
// ---------------------------------------------------------------------------

/// Panics unless every row in a section starts its description at the same
/// column.
///
/// The suite's existing alignment check names two rows by hand and compares
/// them; this parses the whole page, so a third row that drifts out of the
/// column fails without anyone having remembered to list it.
///
/// A section's rows are the lines indented two spaces with a label and a
/// description separated by a gap; continuation lines (a default, a
/// possible-values list) sit under that same column and are checked with them.
/// A section holding no such lines — usage, an examples block — has no column
/// to state anything about and is skipped.
#[track_caller]
pub fn assert_descriptions_aligned(result: &TestResult) {
    assert_descriptions_aligned_in_page(&result.stdout_plain());
}

/// [`assert_descriptions_aligned`] against a page directly.
#[track_caller]
pub fn assert_descriptions_aligned_in_page(page: &str) {
    for section in sections(page) {
        let columns: Vec<(usize, usize, &str)> = section
            .lines
            .iter()
            .filter_map(|(number, line)| {
                description_column(line).map(|column| (*number, column, *line))
            })
            .collect();

        let Some((first_number, first_column, first_line)) = columns.first().copied() else {
            continue;
        };

        for (number, column, line) in columns.iter().copied() {
            if column != first_column {
                panic!(
                    "section {:?}: descriptions start at different columns; \
                     line {} starts at column {} but line {} starts at column {}\n  \
                     {:?}\n  {:?}",
                    section.title, first_number, first_column, number, column, first_line, line
                );
            }
        }
    }
}

/// A page section: a line at column zero, and the indented lines under it.
struct Section<'a> {
    title: &'a str,
    lines: Vec<(usize, &'a str)>,
}

/// Splits a page into sections. Lines before the first heading (the about
/// paragraph) form a leading section titled by their own text.
fn sections(page: &str) -> Vec<Section<'_>> {
    let mut sections: Vec<Section<'_>> = Vec::new();

    for (index, line) in page.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            if let Some(section) = sections.last_mut() {
                section.lines.push((index + 1, line));
            }
        } else {
            sections.push(Section {
                title: line.trim(),
                lines: Vec::new(),
            });
        }
    }

    sections
}

/// The column a line's description starts at, or `None` when the line is not a
/// two-column row.
///
/// A row is `  <label><gap><description>`, where the gap is two or more spaces;
/// a continuation line is indentation followed by the description alone. A
/// label with no description after it (an empty about, a bare usage line)
/// states nothing about the column and returns `None`.
///
/// The column is counted in characters, not bytes, so a label carrying
/// non-ASCII text reports the column a reader sees.
fn description_column(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    if indent < 2 {
        return None;
    }
    if indent > 2 {
        return Some(line[..indent].chars().count());
    }

    let rest = &line[indent..];
    let gap = rest.find("  ")?;
    let after_gap = rest[gap..].trim_start_matches(' ');
    if after_gap.is_empty() {
        return None;
    }
    Some(line[..line.len() - after_gap.len()].chars().count())
}

/// One rendered row: its label, its description, and the continuation lines
/// filed under it.
struct Row<'a> {
    label: &'a str,
    description: &'a str,
    continuations: Vec<&'a str>,
    line: &'a str,
}

impl<'a> Row<'a> {
    /// The row's description and every continuation line under it.
    fn block(&self) -> impl Iterator<Item = &&'a str> {
        std::iter::once(&self.description).chain(self.continuations.iter())
    }
}

/// Parses every two-column row on the page, with its continuation lines.
fn rows(page: &str) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();

    for line in page.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if indent > 2 {
            if let Some(row) = rows.last_mut() {
                row.continuations.push(line.trim());
            }
            continue;
        }
        if indent < 2 {
            continue;
        }

        let rest = &line[indent..];
        let (label, description) = match rest.find("  ") {
            Some(gap) => (rest[..gap].trim_end(), rest[gap..].trim()),
            None => (rest.trim_end(), ""),
        };
        rows.push(Row {
            label,
            description,
            continuations: Vec::new(),
            line,
        });
    }

    rows
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
    }

    #[test]
    fn a_bare_line_has_no_description_column() {
        assert_eq!(description_column("  notes [OPTIONS] <RANGE>"), None);
        assert_eq!(description_column("Keep short notes"), None);
        assert_eq!(description_column("  RANGE            A range"), Some(19));
        assert_eq!(description_column("      default: x"), Some(6));
    }

    #[test]
    fn markers_are_named_once_each() {
        let page = "[header?]USAGE[/header?]\n  [item?]list[/item?]  [desc?]List[/desc?]";
        assert_eq!(
            unresolved_tag_markers(page),
            ["header".to_string(), "item".to_string(), "desc".to_string()]
        );
    }

    #[test]
    fn a_clean_page_names_no_markers() {
        assert!(unresolved_tag_markers(PAGE).is_empty());
    }

    #[test]
    fn token_matching_does_not_confuse_a_prefix_for_a_flag() {
        assert!(contains_token("--all", "--all"));
        assert!(contains_token("-a, --all <N>", "--all"));
        assert!(!contains_token("--all-files", "--all"));
    }
}
