use std::collections::{HashMap, HashSet};

use super::Link;

/// Every markdown link on a page, in source order, skipping code.
pub fn links(markdown: &str) -> Vec<Link> {
    let lines: Vec<(usize, String)> = prose_lines(markdown)
        .into_iter()
        .map(|(index, raw)| (index, blank_code_spans(&raw)))
        .collect();
    let definitions = reference_definitions(&lines);

    // A definition line keeps its entry but no text: its `[label]` is not a usage of itself.
    let mut prose = String::new();
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for (index, line) in &lines {
        starts.push((prose.len(), *index));
        if !is_reference_definition(line) {
            prose.push_str(line);
        }
        prose.push('\n');
    }

    let mut found = Vec::new();
    scan_links(&prose, 0, prose.len(), &definitions, &starts, &mut found);
    found
}

/// Innermost destination first: a link's text (`[![alt](img.png)](page.md)`) is scanned before it.
fn scan_links(
    prose: &str,
    start: usize,
    end: usize,
    definitions: &HashMap<String, String>,
    starts: &[(usize, usize)],
    found: &mut Vec<Link>,
) {
    let line_of = |offset: usize| {
        starts
            .partition_point(|(line_start, _)| *line_start <= offset)
            .checked_sub(1)
            .map_or(1, |position| starts[position].1)
    };

    let mut cursor = start;
    while let Some(offset) = prose[cursor..end].find('[') {
        let open = cursor + offset;
        let Some(close) = matching_bracket(&prose[open + 1..end]).map(|at| open + 1 + at) else {
            // An unpaired `[` is prose, not the start of a link.
            cursor = open + 1;
            continue;
        };
        let text = &prose[open + 1..close];
        let after = &prose[close + 1..end];
        let (target, consumed) = if let Some(inline) = after.strip_prefix('(') {
            match matching_paren(inline) {
                Some(at) => (Some(inline[..at].trim().to_string()), close + 2 + at + 1),
                None => (None, close + 1),
            }
        } else if let Some(reference) = after.strip_prefix('[') {
            match matching_bracket(reference) {
                Some(at) => {
                    let label = &reference[..at];
                    let label = if label.trim().is_empty() { text } else { label };
                    (
                        definitions.get(&normalize_label(label)).cloned(),
                        close + 2 + at + 1,
                    )
                }
                None => (None, close + 1),
            }
        } else {
            (definitions.get(&normalize_label(text)).cloned(), close + 1)
        };
        if text.contains('[') {
            scan_links(prose, open + 1, close, definitions, starts, found);
        }
        if let Some(target) = target {
            if !target.is_empty() {
                found.push(Link {
                    target,
                    line: line_of(open),
                });
            }
        }
        cursor = consumed.max(open + 1);
    }
}

/// Every `[label]: destination` definition on the page, by normalized label.
fn reference_definitions(lines: &[(usize, String)]) -> HashMap<String, String> {
    let mut definitions = HashMap::new();
    for (_, line) in lines {
        let Some((label, destination)) = split_reference_definition(line) else {
            continue;
        };
        // CommonMark: the first definition of a label is the one that counts.
        definitions.entry(label).or_insert(destination);
    }
    definitions
}

fn is_reference_definition(line: &str) -> bool {
    split_reference_definition(line).is_some()
}

/// The destination stops at the first whitespace, which drops the optional title.
fn split_reference_definition(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    // More than three leading spaces would make it an indented code block.
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let rest = trimmed.strip_prefix('[')?;
    let close = matching_bracket(rest)?;
    let destination = rest[close + 1..].strip_prefix(':')?.trim();
    if destination.is_empty() {
        return None;
    }
    let destination = match destination.split_once(char::is_whitespace) {
        Some((destination, _title)) => destination,
        None => destination,
    };
    Some((normalize_label(&rest[..close]), destination.to_string()))
}

/// CommonMark's label matching: case-insensitive, whitespace-collapsed.
fn normalize_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Every heading anchor on a page, in mdbook's spelling.
pub fn heading_anchors(markdown: &str) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchors = Vec::new();
    for (_, line) in prose_lines(markdown) {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let text = trimmed.trim_start_matches('#');
        if !text.starts_with(' ') && !text.is_empty() {
            continue;
        }
        let anchor = normalize_id(&strip_inline_markup(
            text.trim().trim_end_matches('#').trim(),
        ));
        if anchor.is_empty() {
            continue;
        }
        let count = seen.entry(anchor.clone()).or_insert(0);
        anchors.push(if *count == 0 {
            anchor.clone()
        } else {
            format!("{}-{}", anchor, count)
        });
        *count += 1;
    }
    anchors
}

/// mdbook's heading-to-anchor rule.
pub fn normalize_id(content: &str) -> String {
    content
        .chars()
        .filter_map(|ch| {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn heading_anchors_set(markdown: &str) -> HashSet<String> {
    heading_anchors(markdown).into_iter().collect()
}

/// The offset of the `]` closing a `[` already consumed, counting nested brackets.
fn matching_bracket(rest: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(index),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn matching_paren(rest: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(index),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The page's lines with fenced blocks dropped, 1-indexed.
fn prose_lines(markdown: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut fence: Option<String> = None;
    for (index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        let opener = trimmed
            .starts_with("```")
            .then_some("```")
            .or_else(|| trimmed.starts_with("~~~").then_some("~~~"));
        match (&fence, opener) {
            (Some(open), Some(found)) if open == found => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, Some(found)) => {
                fence = Some(found.to_string());
                continue;
            }
            (None, None) => {}
        }
        lines.push((index + 1, line.to_string()));
    }
    lines
}

/// Backtick runs are counted, not toggled: ``[a](b.md)`` is one span; an unclosed run is literal.
fn blank_code_spans(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;

    let run_from = |start: usize| chars[start..].iter().take_while(|ch| **ch == '`').count();

    while at < chars.len() {
        if chars[at] == '\\' {
            out.push(' ');
            if at + 1 < chars.len() {
                out.push(' ');
            }
            at += 2;
            continue;
        }
        if chars[at] != '`' {
            out.push(chars[at]);
            at += 1;
            continue;
        }

        let opener = run_from(at);
        let mut scan = at + opener;
        let closer = loop {
            if scan >= chars.len() {
                break None;
            }
            if chars[scan] != '`' {
                scan += 1;
                continue;
            }
            let run = run_from(scan);
            if run == opener {
                break Some(scan);
            }
            scan += run;
        };

        let blanked = match closer {
            Some(close) => close + opener - at,
            None => opener,
        };
        out.extend(std::iter::repeat_n(' ', blanked));
        at += blanked;
    }

    out
}

fn strip_inline_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '`' | '*' => {}
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for skipped in chars.by_ref() {
                        if skipped == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_read_outside_code() {
        let markdown = "\
See [one](a.md) and [two](b.md#frag).
`[code](never.md)` stays out.

```rust
let s = \"[fenced](never.md)\";
```

[three](c.md)
";
        let found = links(markdown);
        let targets: Vec<&str> = found.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, ["a.md", "b.md#frag", "c.md"]);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[2].line, 8);
    }

    #[test]
    fn multi_backtick_code_spans_hide_the_links_inside_them() {
        let markdown = "\
A ``[double](never.md)`` span and a ```[triple](never.md)``` one.
A span holding a backtick, `` ` [inner](never.md) ``, stays out too.
An unmatched ` backtick leaves [real](a.md) readable.
An escaped \\`[escaped](b.md)` pair leaves the link readable.
";
        let targets: Vec<String> = links(markdown)
            .into_iter()
            .map(|link| link.target)
            .collect();
        assert_eq!(targets, ["a.md", "b.md"], "{markdown}");
    }

    #[test]
    fn a_link_whose_text_wraps_is_still_read() {
        let markdown = "\
See the [execution
outcomes](./execution-outcomes.md) page.
";
        let found = links(markdown);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].target, "./execution-outcomes.md");
        assert_eq!(found[0].line, 1, "reported where the link opened");
    }

    #[test]
    fn reference_links_are_read_in_all_three_forms() {
        let markdown = "\
A [full][setup] one, a [collapsed][] one, and a [shortcut] one.
An [undefined][nowhere] label is not a link, and neither is [tag].
`[setup]` in code stays out.

[setup]: a.md
[collapsed]: b.md#frag
[shortcut]: c.md \"A title\"
";
        let found = links(markdown);
        let targets: Vec<&str> = found.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, ["a.md", "b.md#frag", "c.md"]);
        assert!(
            found.iter().all(|link| link.line == 1),
            "a usage is reported where it is written, not where it is defined: {found:?}"
        );
    }

    #[test]
    fn an_image_wrapped_in_a_link_yields_both_destinations() {
        let markdown = "See [![diagram](missing.png)](page.md) above.\n";
        let found = links(markdown);
        let targets: Vec<&str> = found.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, ["missing.png", "page.md"], "{found:?}");
    }

    #[test]
    fn anchors_follow_mdbooks_rule() {
        let markdown = "\
# Term vs Text
## `App::run_with` and friends
### Term vs Text
#not-a-heading
";
        assert_eq!(
            heading_anchors(markdown),
            [
                "term-vs-text".to_string(),
                "apprun_with-and-friends".to_string(),
                "term-vs-text-1".to_string(),
            ]
        );
    }
}
