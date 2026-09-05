mod common;
use common::snapshot::{digest, slug, squash, DIGEST_TAG};
use common::{matrix, SnapshotCase};
use standout_render::Representation;
#[test]
fn key_is_the_subject_when_no_axis_is_recorded() {
    assert_eq!(SnapshotCase::new("help").key(), "help");
}
#[test]
fn key_appends_axes_in_declaration_order() {
    let case = SnapshotCase::new("help")
        .output_mode(Representation::Human)
        .tty(false)
        .theme("default");
    assert_eq!(case.key(), "help__mode_human__tty_off__theme_default");
}
#[test]
fn key_slugifies_argv_shaped_values() {
    let case = SnapshotCase::new("Help Page")
        .entry_point("--help")
        .output_mode(Representation::TermDebug);
    assert_eq!(
        case.key(),
        "help-page--f5f24815__entry_help--7fb28c5c__mode_term-debug"
    );
}
#[test]
fn cases_differing_in_one_axis_differ_in_one_segment() {
    let dark = SnapshotCase::new("help").theme("dark").key();
    let light = SnapshotCase::new("help").theme("light").key();
    assert_ne!(dark, light);
    assert_eq!(dark.replace("dark", "light"), light);
}
#[test]
fn values_that_squash_alike_still_key_apart() {
    let pairs = [
        ("--help", "help"),
        ("dark mode", "dark-mode"),
        ("", "none"),
        ("-h", "h"),
        ("--help", "help--7fb28c5c"),
        ("--help", "help-7fb28c5c"),
    ];
    for (left, right) in pairs {
        let left_key = SnapshotCase::new("help").entry_point(left).key();
        let right_key = SnapshotCase::new("help").entry_point(right).key();
        assert_ne!(
            left_key, right_key,
            "{:?} and {:?} must not share a snapshot name",
            left, right
        );
    }
}
#[test]
fn the_axis_name_value_boundary_is_unambiguous() {
    let split_in_the_name = SnapshotCase::new("help").axis("group-1", "test").key();
    let split_in_the_value = SnapshotCase::new("help").axis("group", "1-test").key();
    assert_eq!(split_in_the_name, "help__group-1_test");
    assert_eq!(split_in_the_value, "help__group_1-test");
    assert_ne!(split_in_the_name, split_in_the_value);
}
#[test]
fn a_slug_spells_the_keys_punctuation_only_in_its_reserved_tag() {
    for text in ["--help", "dark mode", "", "a__b", "x--y", "Group_1", "help"] {
        let slugged = slug(text);
        let squashed = squash(text);
        let expected_readable = if squashed.is_empty() {
            "none"
        } else {
            &squashed
        };
        let readable = match slugged.split_once(DIGEST_TAG) {
            Some((readable, tag)) => {
                assert_eq!(
                    tag,
                    format!("{:08x}", digest(text)),
                    "{text:?} → {slugged:?}"
                );
                readable
            }
            None => slugged.as_str(),
        };
        assert_eq!(readable, expected_readable, "{text:?} → {slugged:?}");
        assert!(!slugged.contains('_'), "{text:?} → {slugged:?}");
        assert!(!readable.contains(DIGEST_TAG), "{text:?} → {slugged:?}");
    }
}
#[test]
fn a_canonical_value_keys_without_a_digest() {
    let case = SnapshotCase::new("help")
        .output_mode(Representation::Human)
        .tty(true)
        .theme("solarized-dark");
    assert_eq!(case.key(), "help__mode_human__tty_on__theme_solarized-dark");
}
#[test]
fn an_axis_value_that_slugs_away_keeps_the_key_unambiguous() {
    assert_eq!(
        SnapshotCase::new("help").theme("").key(),
        "help__theme_none--811c9dc5"
    );
}
#[test]
fn the_digest_is_a_fixed_value_not_a_toolchain_detail() {
    assert_eq!(digest(""), 0x811c_9dc5);
    assert_eq!(format!("{:08x}", digest("--help")), "7fb28c5c");
}
#[test]
fn display_renders_the_key() {
    let case = SnapshotCase::new("help").tty(true);
    assert_eq!(case.to_string(), case.key());
}
const MODES: [Representation; 2] = [Representation::Human, Representation::Json];
#[test]
fn the_matrix_is_the_full_cross_product_in_mode_major_order() {
    let cells = matrix(&MODES, &[false, true], &[("default", 0), ("downstream", 1)]);
    assert_eq!(cells.len(), 8);
    let spelled: Vec<String> = cells
        .iter()
        .map(|c| format!("{:?}/{}/{}", c.mode, c.color, c.theme_name))
        .collect();
    assert_eq!(
        spelled,
        [
            "Human/false/default",
            "Human/false/downstream",
            "Human/true/default",
            "Human/true/downstream",
            "Json/false/default",
            "Json/false/downstream",
            "Json/true/default",
            "Json/true/downstream",
        ]
    );
}
#[test]
fn every_cell_names_a_distinct_snapshot() {
    let cells = matrix(
        &MODES,
        &[false, true],
        &[("default", ()), ("downstream", ())],
    );
    let mut keys: Vec<String> = cells
        .iter()
        .map(|c| c.snapshot_case("help").key())
        .collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 8, "two cells share a snapshot name");
}
#[test]
fn a_cell_spells_its_axes_into_the_snapshot_key() {
    let cells = matrix(&[Representation::Json], &[true], &[("downstream", ())]);
    assert_eq!(
        cells[0].snapshot_case("help").key(),
        "help__mode_json__color_on__theme_downstream"
    );
}
#[test]
fn the_theme_payload_rides_along() {
    let cells = matrix(&[Representation::Human], &[false], &[("a", 41), ("b", 42)]);
    assert_eq!(cells[0].theme, 41);
    assert_eq!(cells[1].theme, 42);
}
#[test]
fn an_empty_axis_yields_an_empty_matrix() {
    assert!(matrix::<()>(&[], &[false], &[]).is_empty());
    assert!(matrix(&MODES, &[], &[("default", ())]).is_empty());
}
