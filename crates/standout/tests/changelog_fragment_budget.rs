use std::fs;
use std::path::PathBuf;

const MAX_WORDS_PER_BULLET: usize = 80;
const MAX_WORDS_PER_FRAGMENT: usize = 200;

const GUIDANCE: &str = "A CHANGELOG fragment says what changed and what a reader must do to keep \
working, then stops. Reasoning about why the fix was hard, the internal call paths it crossed, and \
anything the code already states belong in the issue or the PR body, not here. Cut until only the \
reader's action is left; do not add a qualifying clause to answer a review comment.";

fn changelog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("CHANGELOG")
}

fn fragments() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(changelog_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("unreleased-") && name.ends_with(".md") {
            found.push((name, fs::read_to_string(&path).unwrap()));
        }
    }
    found.sort();
    found
}

fn bullets(source: &str) -> Vec<String> {
    let mut collected: Vec<String> = Vec::new();
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("- ") {
            collected.push(rest.to_string());
        } else if let Some(current) = collected.last_mut() {
            current.push('\n');
            current.push_str(line);
        }
    }
    collected
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn structure_problem(source: &str) -> Option<String> {
    let mut seen_bullet = false;
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with("- ") {
            seen_bullet = true;
        } else if !(seen_bullet && line.starts_with(char::is_whitespace)) {
            return Some(format!(
                "line {} is {line:?}, not a `- ` bullet or an indented continuation",
                index + 1
            ));
        }
    }
    (!seen_bullet).then(|| "is empty".to_string())
}

#[test]
fn every_unreleased_fragment_is_a_bullet_list() {
    let mut offenders = Vec::new();
    for (name, source) in fragments() {
        if let Some(problem) = structure_problem(&source) {
            offenders.push(format!("CHANGELOG/{name}: {problem}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "a fragment is a list of `- ` bullets and nothing else:\n{}\n\n{GUIDANCE}",
        offenders.join("\n")
    );
}

#[test]
fn every_unreleased_bullet_is_within_budget() {
    let mut offenders = Vec::new();
    for (name, source) in fragments() {
        for (index, bullet) in bullets(&source).iter().enumerate() {
            let count = words(bullet);
            if count > MAX_WORDS_PER_BULLET {
                offenders.push(format!(
                    "CHANGELOG/{name}: bullet {} is {count} words, over the {MAX_WORDS_PER_BULLET}-word budget",
                    index + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{}\n\n{GUIDANCE}",
        offenders.join("\n")
    );
}

#[test]
fn every_unreleased_fragment_is_within_budget() {
    let mut offenders = Vec::new();
    for (name, source) in fragments() {
        let bullets = bullets(&source);
        let count: usize = bullets.iter().map(|bullet| words(bullet)).sum();
        if count > MAX_WORDS_PER_FRAGMENT {
            offenders.push(format!(
                "CHANGELOG/{name}: {count} words across {} bullets, over the {MAX_WORDS_PER_FRAGMENT}-word budget",
                bullets.len()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "{}\n\n{GUIDANCE}",
        offenders.join("\n")
    );
}

#[test]
fn content_outside_a_bullet_is_not_a_bullet_list() {
    assert_eq!(structure_problem("- one\n  two\n\n- three\n"), None);
    assert!(structure_problem("- one\n\n# a heading\n").is_some());
    assert!(structure_problem("- one\ntop-level prose\n").is_some());
    assert!(structure_problem("preamble\n\n- one\n").is_some());
    assert!(structure_problem("  orphan continuation\n").is_some());
    assert!(structure_problem("\n\n").is_some());
}

#[test]
fn a_wrapped_bullet_counts_as_one() {
    let source = "\
- one two
  three four
- five

  six
";
    assert_eq!(bullets(source), ["one two\n  three four", "five\n\n  six"]);
    assert_eq!(bullets(source).iter().map(|b| words(b)).sum::<usize>(), 6);
}
