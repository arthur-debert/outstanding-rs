//! The coverage-matrix combinator: (output mode × color × theme) cells.
//!
//! The themed-help defect cluster lived in the cells no test visited: every
//! help test ran in a tag-erasing mode, so the one transform that corrupts a
//! page (`Term`'s `TagTransform::Apply`) was the one no assertion ever saw.
//! [`matrix`] makes visiting *every* cell the cheap path: it yields the full
//! cross product of the axes it is given, and each [`MatrixCell`] knows how to
//! configure a [`TestHarness`] for itself and how to name a snapshot after
//! itself — so a suite that walks the cells cannot quietly skip one, and the
//! snapshot a cell produces is discoverable from the cell alone.
//!
//! # The axes
//!
//! - **Output mode** is the harness's [`output_mode`](TestHarness::output_mode)
//!   axis, exactly as a user would spell it with `--output`.
//! - **Color** is the axis the deleted TTY seam resolved into (see
//!   `docs/adr/0022-delete-the-in-process-tty-seam.md`): a cell with color on
//!   runs under [`with_color`](TestHarness::with_color), which opens both
//!   gates between a styled template and ANSI bytes; a cell with color off
//!   runs under [`no_color`](TestHarness::no_color), which pins both shut.
//!   Both are explicit so a cell renders the same on a developer's terminal
//!   and in CI — neither state is left to the machine the test happens to run
//!   on.
//! - **Theme** is whatever the caller's cells vary over — the combinator
//!   carries an arbitrary payload per theme name, because the thing a theme
//!   axis selects (a fixture, an `App`, a stylesheet) belongs to the suite,
//!   not to the harness.
//!
//! # Composing with `serial_test`
//!
//! The combinator is plain data and touches nothing global; the process-global
//! mutation happens per cell, inside each [`harness`](MatrixCell::harness)
//! run, and is restored when that run's [`TestResult`](crate::TestResult)
//! drops. Walking all the cells inside one `#[serial]` test is therefore the
//! intended shape — one serialized test, many cells — which is what keeps the
//! matrix compatible with the suite's `#[serial]` constraint until the process
//! globals are gone.
//!
//! ```no_run
//! use standout_render::OutputMode;
//! use standout_test::{assert_page_snapshot, matrix};
//! # fn example(app: &standout::cli::App, cmd: clap::Command) {
//! for cell in matrix(
//!     &[OutputMode::Term, OutputMode::Text],
//!     &[false, true],
//!     &[("default", ())],
//! ) {
//!     let result = cell.harness().terminal_width(80).run(app, cmd.clone(), ["app", "--help"]);
//!     assert_page_snapshot!(result, cell.snapshot_case("help").entry_point("--help"));
//! }
//! # }
//! ```

use standout_render::OutputMode;

use crate::{SnapshotCase, TestHarness};

/// One cell of the (output mode × color × theme) coverage matrix.
///
/// A cell is an identity first: the axis values are public so a suite can
/// branch on them (build the themed fixture for one theme name, the bare one
/// for another), and [`snapshot_case`](Self::snapshot_case) spells the same
/// identity as a snapshot name so the page a cell pins is traceable back to
/// it.
#[derive(Debug, Clone)]
pub struct MatrixCell<T> {
    /// The output mode this cell runs under.
    pub mode: OutputMode,
    /// Whether this cell forces color on ([`TestHarness::with_color`]) or
    /// off ([`TestHarness::no_color`]). Never inherited from the machine.
    pub color: bool,
    /// The name of this cell's theme axis value, as it appears in the
    /// snapshot key.
    pub theme_name: String,
    /// The caller's payload for this theme — a fixture selector, an `App`,
    /// whatever the suite varies per theme.
    pub theme: T,
}

impl<T> MatrixCell<T> {
    /// A harness configured for this cell: the cell's output mode, with color
    /// explicitly forced to the cell's state.
    ///
    /// Further settings (a pinned [`terminal_width`](TestHarness::terminal_width),
    /// fixtures, env) are the suite's to chain on — the cell owns only its own
    /// axes.
    pub fn harness(&self) -> TestHarness {
        let harness = TestHarness::new().output_mode(self.mode);
        if self.color {
            harness.with_color()
        } else {
            harness.no_color()
        }
    }

    /// The snapshot identity for `subject` rendered in this cell: the cell's
    /// mode, color, and theme axes, in that order.
    ///
    /// Axes a cell does not own — the help entry point, a shape — are chained
    /// on by the caller, after these.
    pub fn snapshot_case(&self, subject: impl Into<String>) -> SnapshotCase {
        SnapshotCase::new(subject)
            .output_mode(self.mode)
            .color(self.color)
            .theme(&self.theme_name)
    }
}

/// Yields the full cross product of the given axes, mode-major: for each
/// mode, each color state; for each color state, each theme.
///
/// The order is stable and documented because a matrix suite reads its
/// failures positionally — "cell 7" has to mean the same cell on every
/// machine. Axes with no values yield no cells; that is the caller declaring
/// an empty matrix, not an error.
pub fn matrix<T: Clone>(
    modes: &[OutputMode],
    colors: &[bool],
    themes: &[(&str, T)],
) -> Vec<MatrixCell<T>> {
    let mut cells = Vec::with_capacity(modes.len() * colors.len() * themes.len());
    for &mode in modes {
        for &color in colors {
            for (name, theme) in themes {
                cells.push(MatrixCell {
                    mode,
                    color,
                    theme_name: (*name).to_string(),
                    theme: theme.clone(),
                });
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [OutputMode; 2] = [OutputMode::Term, OutputMode::Text];

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
                "Term/false/default",
                "Term/false/downstream",
                "Term/true/default",
                "Term/true/downstream",
                "Text/false/default",
                "Text/false/downstream",
                "Text/true/default",
                "Text/true/downstream",
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
        let cells = matrix(&[OutputMode::Term], &[true], &[("downstream", ())]);

        assert_eq!(
            cells[0].snapshot_case("help").key(),
            "help__mode_term__color_on__theme_downstream"
        );
    }

    #[test]
    fn the_theme_payload_rides_along() {
        let cells = matrix(&[OutputMode::Text], &[false], &[("a", 41), ("b", 42)]);
        assert_eq!(cells[0].theme, 41);
        assert_eq!(cells[1].theme, 42);
    }

    #[test]
    fn an_empty_axis_yields_an_empty_matrix() {
        assert!(matrix::<()>(&[], &[false], &[]).is_empty());
        assert!(matrix(&MODES, &[], &[("default", ())]).is_empty());
    }
}
