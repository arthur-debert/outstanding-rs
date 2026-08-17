//! The help surface pinned per matrix cell: (output mode × color × theme ×
//! entry point), one snapshot each.
//!
//! This is the inheritance later epics run against: when composition
//! contracts re-routes help through the unified pipeline, `cargo test` shows
//! exactly which cells changed, and the Spec's rule is that each delta is
//! justified or fixed. The snapshots are of [`stdout_plain`] — ANSI stripped —
//! so what they pin is layout and content per cell; the color axis still
//! matters because `Term`'s `TagTransform::Apply` is the transform that can
//! corrupt a page (#303's `[tag?]` markers are plain literals, visible after
//! stripping), and because `Auto` resolves to a different pipeline per color
//! state.
//!
//! The axes, and what is deliberately not one:
//!
//! - **Modes**: the four render modes (`Auto`, `Term`, `Text`, `TermDebug`).
//!   The structured modes are not help surfaces — `--help`/`-h` short-circuit
//!   through clap before an output mode is consulted (#295 records that as a
//!   design decision) — so cells there would pin the same page four more
//!   times. Stated rather than silently skipped.
//! - **Color**: both states, explicitly forced (ADR-0022's replacement for
//!   the TTY axis), so a cell renders identically on a TTY and in CI.
//! - **Theme**: the shared downstream fixture with its deliberately
//!   incomplete app theme, and the same fixture with no theme — the pair
//!   whose divergence was the whole themed-help defect cluster.
//! - **Entry point**: `-h`, `--help`, and the `help` word, chained onto the
//!   cell's snapshot case, since the entry point is the help suite's axis,
//!   not the matrix's.
//!
//! One `#[serial]` test walks all the cells — the combinator is plain data,
//! and each cell's harness restores the process globals it touches, which is
//! how the matrix composes with the `#[serial]` constraint that stands until
//! composition contracts removes the globals.
//!
//! [`stdout_plain`]: standout_test::TestResult::stdout_plain

use serial_test::serial;
use standout_fixtures::{downstream, Fixture};
use standout_render::OutputMode;
use standout_test::{assert_page_snapshot, matrix};

/// The three help entry points the fixture answers.
const ENTRY_POINTS: [&str; 3] = ["-h", "--help", "help"];

/// The render modes; the structured modes are excluded above, on purpose.
const MODES: [OutputMode; 4] = [
    OutputMode::Auto,
    OutputMode::Term,
    OutputMode::Text,
    OutputMode::TermDebug,
];

/// Builds the fixture a theme-axis value names.
fn fixture_for(theme_name: &str) -> Fixture {
    match theme_name {
        "downstream" => downstream().build(),
        "default" => downstream().without_theme().build(),
        other => panic!("no fixture for theme axis value {other:?}"),
    }
}

/// Every cell of the matrix, across every entry point, pinned by a snapshot
/// named after the cell.
///
/// The width is pinned so the committed pages do not depend on the terminal
/// of the machine that regenerates them.
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
