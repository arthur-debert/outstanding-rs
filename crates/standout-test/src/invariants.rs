use crate::page::{
    declared_metavars, find_row, rows, takes_values, value_placeholders, visible_args,
};
use crate::TestResult;
use clap::Command;
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
#[track_caller]
pub fn assert_no_unresolved_tag_markers(result: &TestResult) {
    assert_no_unresolved_tag_markers_in_page(&result.stdout_plain());
}
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
#[track_caller]
pub fn assert_styling_preserves_layout(styled: &TestResult, plain: &TestResult) {
    assert_styling_preserves_layout_in_pages(&styled.stdout_plain(), plain.stdout());
}
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
#[track_caller]
pub fn assert_no_possible_values_for_valueless_args(result: &TestResult, cmd: &Command) {
    assert_no_possible_values_for_valueless_args_in_page(&result.stdout_plain(), cmd);
}
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
#[track_caller]
pub fn assert_metavar_for_valued_args(result: &TestResult, cmd: &Command) {
    assert_metavar_for_valued_args_in_page(&result.stdout_plain(), cmd);
}
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
#[track_caller]
pub fn assert_descriptions_aligned(result: &TestResult) {
    assert_descriptions_aligned_in_page(&result.stdout_plain());
}
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
struct Section<'a> {
    title: &'a str,
    lines: Vec<(usize, &'a str)>,
}
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
}
