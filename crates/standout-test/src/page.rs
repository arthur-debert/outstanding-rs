//! Parses a rendered `--help` page back into structured [`Row`]s.
//!
//! Help text is free-form: a label, its description, and any wrapped
//! continuation lines that belong to it by indentation alone. [`rows`]
//! recovers that structure so [`clap_parity`](crate::clap_parity) and
//! [`invariants`](crate::invariants) can check facts about specific
//! arguments/subcommands instead of grep-ing the whole page.

use clap::{Arg, ArgAction, Command};
pub(crate) struct Row<'a> {
    pub(crate) label: &'a str,
    pub(crate) description: &'a str,
    pub(crate) continuations: Vec<&'a str>,
    pub(crate) line: &'a str,
    pub(crate) section: &'a str,
}
impl<'a> Row<'a> {
    pub(crate) fn block(&self) -> impl Iterator<Item = &&'a str> {
        std::iter::once(&self.description).chain(self.continuations.iter())
    }
    pub(crate) fn block_text(&self) -> String {
        normalize(&self.block().copied().collect::<Vec<_>>().join(" "))
    }
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
                if let Some(bullet) = line.trim_start().strip_prefix("- ") {
                    let bullet = bullet.trim_end();
                    let name = declared_name_at(bullet, &declared).unwrap_or_else(|| match bullet
                        .split_once(": ")
                    {
                        Some((name, _)) => name.trim(),
                        None => bullet,
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
fn find_label(line: &str, label: &str) -> Option<usize> {
    debug_assert!(
        !label.is_empty() && label.is_ascii(),
        "labels are non-empty ASCII by construction"
    );
    line.as_bytes()
        .windows(label.len())
        .position(|window| window.eq_ignore_ascii_case(label.as_bytes()))
}
pub(crate) fn value_clause(remainder: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
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
pub(crate) fn comma_values(clause: &str) -> Vec<String> {
    clause
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClapJoint {
    Spaces,
    CommaSpace,
}
pub(crate) fn list_values(clause: &str, joint: ClapJoint) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
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
                .unwrap_or(char::REPLACEMENT_CHARACTER)
        }
        Some(other) => other,
        None => '\\',
    }
}
pub(crate) fn rows(page: &str) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut section = "";
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
            let next_label = !known && looks_like_flag_label(line);
            if indent >= continuation_column && !next_label {
                rows[index].continuations.push(line.trim());
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
fn looks_like_flag_label(line: &str) -> bool {
    line.trim_start().strip_prefix('-').is_some_and(|tail| {
        tail.trim_start_matches('-')
            .starts_with(|c: char| !c.is_whitespace())
    })
}
pub(crate) fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
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
pub(crate) fn positional_row(label: &str) -> bool {
    if label.split_whitespace().any(|token| token.starts_with('-')) {
        return false;
    }
    if !label.contains(['<', '[']) {
        return !label.trim().is_empty();
    }
    undecorated(&split_brackets(label).1).is_empty()
}
fn undecorated(text: &str) -> String {
    text.replace(ELLIPSIS, " ").trim().to_string()
}
pub(crate) const ELLIPSIS: &str = "...";
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
pub(crate) fn contains_token(haystack: &str, token: &str) -> bool {
    haystack
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '<' | '>' | '='))
        .any(|word| word == token)
}
pub(crate) fn contains_flag_token(haystack: &str, spelling: &str, arg: &Arg) -> bool {
    contains_token(haystack, spelling)
        || (matches!(arg.get_action(), ArgAction::Count)
            && contains_token(haystack, &format!("{spelling}{ELLIPSIS}")))
}
pub(crate) fn visible_args(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments().filter(|arg| !arg.is_hide_set())
}
pub(crate) fn takes_values(arg: &Arg) -> bool {
    arg.get_num_args()
        .map(|range| range.takes_values())
        .unwrap_or_else(|| arg.get_action().takes_values())
}
pub(crate) fn declared_metavars(arg: &Arg) -> Option<Vec<String>> {
    arg.get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>())
        .filter(|names: &Vec<String>| !names.is_empty())
}
pub(crate) fn candidate_metavars(arg: &Arg) -> Vec<String> {
    declared_metavars(arg).unwrap_or_else(|| vec![arg.get_id().to_string()])
}
pub(crate) fn value_placeholders(label: &str) -> Vec<&str> {
    if label.contains(['<', '[']) {
        return split_brackets(label).0;
    }
    label
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|token| token.rsplit('=').next().unwrap_or(token))
        .filter(|token| !token.starts_with('-'))
        .map(metavar_text)
        .filter(|token| !token.is_empty())
        .collect()
}
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
    #[test]
    fn list_values_keeps_a_mid_word_quote_literal() {
        assert_eq!(
            list_values(" foo\"bar, json", ClapJoint::CommaSpace),
            ["foo\"bar", "json"]
        );
    }
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
    #[test]
    fn comma_values_keeps_a_values_whitespace() {
        assert_eq!(comma_values(" plain text, json"), ["plain text", "json"]);
        assert_eq!(comma_values(""), Vec::<String>::new());
    }
    #[test]
    fn value_clause_skips_a_bracket_inside_quotes() {
        assert_eq!(
            value_clause(" \"[notes.txt]\"] [aliases: -t]"),
            " \"[notes.txt]\""
        );
        assert_eq!(value_clause(" brief] [aliases: -t]"), " brief");
        assert_eq!(value_clause(" brief"), " brief");
    }
    #[test]
    fn value_clause_treats_a_mid_word_quote_as_literal() {
        assert_eq!(value_clause(" foo\"bar] [aliases: -t]"), " foo\"bar");
        assert_eq!(value_clause(" a,\"b] [aliases: -t]"), " a,\"b");
    }
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
    #[test]
    fn token_matching_does_not_read_through_an_ellipsis() {
        assert!(!contains_token("go...", "go"));
        assert!(!contains_token("try `go...` to continue", "go"));
        assert!(!contains_token("[aliases: go...]", "go"));
    }
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
