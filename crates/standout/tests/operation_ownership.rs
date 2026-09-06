use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const WALKER: &str = "AnsiCodeIterator";
const WALKER_OWNER: &str = "crates/standout-bbparser/src/ansi.rs";

const BALANCE: &str = "AnsiBalance";
const BALANCE_OWNER: &str = "crates/standout-bbparser/src/";

const CONTROL_ESCAPER: &str = "fn escape_control_characters";
const CONTROL_ESCAPER_COPIES: [&str; 2] = [
    "crates/standout-dispatch/src/escape.rs",
    "crates/standout-render/src/escape.rs",
];

const SETUP_CHECKS: [&str; 6] = [
    "malformed_registrations",
    "validate_questionnaire_surfaces",
    "unreachable_registrations",
    "config_override_flag_collision",
    "framework_flag_collision",
    "config_command_collision",
];

const PROPAGATION: &str = "with_globals_propagated";
const PROPAGATION_OWNER: &str = "crates/standout/src/cli/app.rs";

const SINGLE_DEFINITIONS: [(&str, &str); 5] = [
    ("pub fn ansi_units", "crates/standout-bbparser/src/ansi.rs"),
    ("pub fn closing_for", "crates/standout-bbparser/src/ansi.rs"),
    (
        "pub fn escape_style_tags",
        "crates/standout-render/src/util.rs",
    ),
    (
        "fn validate_questionnaire_surface(",
        "crates/standout/src/cli/questionnaire.rs",
    ),
    (
        "fn take_prefix_to_display_width",
        "crates/standout-render/src/tabular/util.rs",
    ),
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn walk_rs(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            walk_rs(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn crate_sources() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut files = Vec::new();
    walk_rs(&root.join("crates"), &mut files);
    files.sort();
    files
        .iter()
        .map(|file| {
            let relative = file
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (relative, fs::read_to_string(file).unwrap())
        })
        .filter(|(relative, _)| relative.contains("/src/"))
        .collect()
}

fn offenders_in(relative: &str, source: &str, needle: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(number, line)| format!("{relative}:{}: {}", number + 1, line.trim()))
        .collect()
}

fn mentions(needle: &str, allowed: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    for (relative, source) in crate_sources() {
        if allowed(&relative) {
            continue;
        }
        found.extend(offenders_in(&relative, &source, needle));
    }
    found
}

#[test]
fn only_the_bbparser_ansi_module_names_the_walker() {
    let offenders = mentions(WALKER, |relative| relative == WALKER_OWNER);

    assert!(
        offenders.is_empty(),
        "{WALKER} belongs to {WALKER_OWNER}; call `ansi_units` instead of walking again:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn only_the_bbparser_crate_drives_the_ansi_balance() {
    let offenders = mentions(BALANCE, |relative| relative.starts_with(BALANCE_OWNER));

    assert!(
        offenders.is_empty(),
        "{BALANCE} belongs to {BALANCE_OWNER}ansi.rs; a cutter outside the crate closes \
         what it cut with `closing_for`:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn each_setup_check_answers_to_one_caller() {
    let mut wrong = Vec::new();
    for check in SETUP_CHECKS {
        let calls = mentions(&format!(".{check}("), |_| false);
        if calls.len() != 1 {
            wrong.push(format!(
                "{check} has {} callers:\n{}",
                calls.len(),
                calls.join("\n")
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "a second list of setup checks drifts from the first; compose the one that exists:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_tree_the_setup_checks_read_is_built_once() {
    let builders = mentions(PROPAGATION, |relative| relative == PROPAGATION_OWNER);

    assert_eq!(
        builders.len(),
        1,
        "the propagated tree is built once outside {PROPAGATION_OWNER}, so verification and \
         the run path read the same one:\n{}",
        builders.join("\n")
    );
}

#[test]
fn the_control_character_escaper_has_two_copies_and_they_agree() {
    let definitions: BTreeSet<String> = crate_sources()
        .into_iter()
        .filter(|(_, source)| source.contains(CONTROL_ESCAPER))
        .map(|(relative, _)| relative)
        .collect();
    let expected: BTreeSet<String> = CONTROL_ESCAPER_COPIES
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        definitions, expected,
        "the escaper is duplicated because the two crates share no dependency; a third copy \
         needs one of them to become the owner"
    );

    let root = workspace_root();
    let [first, second] = CONTROL_ESCAPER_COPIES.map(|copy| fs::read_to_string(root.join(copy)));
    assert_eq!(
        first.unwrap(),
        second.unwrap(),
        "the two copies of the escaper have drifted: {} and {}",
        CONTROL_ESCAPER_COPIES[0],
        CONTROL_ESCAPER_COPIES[1]
    );
}

#[test]
fn each_named_operation_is_written_once() {
    let mut wrong = Vec::new();
    for (definition, owner) in SINGLE_DEFINITIONS {
        let sites = mentions(definition, |_| false);
        let expected_owner = sites.len() == 1 && sites[0].starts_with(&format!("{owner}:"));
        if !expected_owner {
            wrong.push(format!(
                "`{definition}` belongs to {owner}, found {} site(s):\n{}",
                sites.len(),
                sites.join("\n")
            ));
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_scan_reads_lines_a_cfg_test_item_would_have_hidden() {
    let source = "\
#[cfg(test)]
use console::AnsiCodeIterator;

fn production() {
    let _ = AnsiCodeIterator::new(\"\");
}

#[cfg(test)]
mod tests {
    fn t() {
        let _ = AnsiCodeIterator::new(\"\");
    }
}

// AnsiCodeIterator in a comment
fn after() {}
";
    assert_eq!(
        offenders_in("file.rs", source, WALKER),
        [
            "file.rs:2: use console::AnsiCodeIterator;",
            "file.rs:5: let _ = AnsiCodeIterator::new(\"\");",
            "file.rs:11: let _ = AnsiCodeIterator::new(\"\");",
            "file.rs:15: // AnsiCodeIterator in a comment",
        ]
    );
}
