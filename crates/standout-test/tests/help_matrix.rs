use serial_test::serial;
use standout_fixtures::{downstream, Fixture};
use standout_render::Representation;
mod common;
use common::{assert_page_snapshot, matrix};
const ENTRY_POINTS: [&str; 3] = ["-h", "--help", "help"];
const MODES: [Representation; 2] = [Representation::Human, Representation::TermDebug];
fn fixture_for(theme_name: &str) -> Fixture {
    match theme_name {
        "downstream" => downstream().build(),
        "default" => downstream().without_theme().build(),
        other => panic!("no fixture for theme axis value {other:?}"),
    }
}
#[test]
#[serial]
fn every_matrix_cell_pins_its_help_page() {
    for cell in matrix(
        &MODES,
        &[false, true],
        &[("default", ()), ("downstream", ())],
    ) {
        let fixture = fixture_for(&cell.theme_name);
        for entry in ENTRY_POINTS {
            let result = cell.harness().terminal_width(80).run(
                fixture.app(),
                fixture.command(),
                ["lookma", entry],
            );
            result.assert_success();
            assert_page_snapshot!(result, cell.snapshot_case("help").entry_point(entry));
        }
    }
}
