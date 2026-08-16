//! Snapshot keying for harness-driven page snapshots.
//!
//! A page snapshot is only worth its review cost if its name says which cell
//! of the test matrix produced it. [`SnapshotCase`] is that name: a subject
//! plus the axes that distinguish one rendered page from another (output
//! mode, TTY, theme, help entry point, …). [`SnapshotCase::key`] derives a
//! stable, filesystem-safe snapshot name from those axes, so a matrix that
//! grows a cell grows a snapshot file whose name already says what it is —
//! no hand-written labels to drift from the case they describe.
//!
//! ```
//! use standout_render::OutputMode;
//! use standout_test::SnapshotCase;
//!
//! let case = SnapshotCase::new("help")
//!     .output_mode(OutputMode::Term)
//!     .tty(true)
//!     .theme("default")
//!     .entry_point("--help");
//!
//! assert_eq!(case.key(), "help__mode-term__tty-on__theme-default__entry-help");
//! ```

use std::fmt;

use standout_render::OutputMode;

use crate::output_mode_flag;

/// The identity of one snapshot case: what was rendered, under which axes.
///
/// Axes are recorded in declaration order and appear in [`key`](Self::key) in
/// that order, so two cases that differ in one axis differ in one key segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCase {
    subject: String,
    axes: Vec<(String, String)>,
}

impl SnapshotCase {
    /// Starts a case for `subject` — what is being rendered, e.g. `"help"`.
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            axes: Vec::new(),
        }
    }

    /// Records an arbitrary axis as `name`/`value`.
    ///
    /// The named helpers below cover the axes the help matrix already has;
    /// this is the escape hatch for one a suite adds locally.
    pub fn axis(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.axes.push((name.into(), value.into()));
        self
    }

    /// Records the `mode` axis, spelled as the `--output` flag value.
    pub fn output_mode(self, mode: OutputMode) -> Self {
        self.axis("mode", output_mode_flag(mode))
    }

    /// Records the `tty` axis as `on` / `off`.
    pub fn tty(self, is_tty: bool) -> Self {
        self.axis("tty", if is_tty { "on" } else { "off" })
    }

    /// Records the `theme` axis under the theme's name.
    pub fn theme(self, name: impl Into<String>) -> Self {
        self.axis("theme", name)
    }

    /// Records the `entry` axis — which surface produced the page, e.g.
    /// `"--help"`, `"-h"`, or `"help"`.
    pub fn entry_point(self, entry: impl Into<String>) -> Self {
        self.axis("entry", entry)
    }

    /// Returns the snapshot name for this case.
    ///
    /// The subject leads; each axis follows as `__<name>-<value>`. Every
    /// segment is slugified (lowercased, runs of non-alphanumerics collapsed
    /// to a single `-`), so a case built from argv strings like `--help`
    /// still names a file a reviewer can read.
    pub fn key(&self) -> String {
        let mut key = slug(&self.subject);
        for (name, value) in &self.axes {
            key.push_str("__");
            key.push_str(&slug(name));
            key.push('-');
            key.push_str(&slug(value));
        }
        key
    }
}

impl fmt::Display for SnapshotCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

/// Lowercases `text` and collapses every run of non-alphanumeric characters
/// into a single `-`, with no leading or trailing separator.
///
/// A segment that slugs away to nothing becomes `none`, so a key never grows
/// an empty component that would make two different cases share a name.
fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Asserts the result's ANSI-stripped stdout against the snapshot named by a
/// [`SnapshotCase`].
///
/// The snapshot name is the case's [`key`](SnapshotCase::key) — derived, never
/// hand-written — so the snapshot file for a matrix cell is discoverable from
/// the cell alone. Stripping ANSI first keeps the committed snapshot readable:
/// the styling axis is asserted by the invariant `strip_ansi(term) == text`,
/// not by escape sequences in a `.snap` file.
///
/// Calling crates need `insta` as a dev-dependency; the macro expands to
/// `insta::assert_snapshot!` at the call site so snapshots land in the
/// calling crate's own `snapshots/` directory.
///
/// ```no_run
/// # use standout_test::{assert_page_snapshot, SnapshotCase, TestResult};
/// # fn example(result: TestResult) {
/// assert_page_snapshot!(result, SnapshotCase::new("help").entry_point("--help"));
/// # }
/// ```
#[macro_export]
macro_rules! assert_page_snapshot {
    ($result:expr, $case:expr $(,)?) => {{
        let case = $case;
        ::insta::assert_snapshot!(case.key(), $result.stdout_plain());
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_the_subject_when_no_axis_is_recorded() {
        assert_eq!(SnapshotCase::new("help").key(), "help");
    }

    #[test]
    fn key_appends_axes_in_declaration_order() {
        let case = SnapshotCase::new("help")
            .output_mode(OutputMode::Text)
            .tty(false)
            .theme("default");

        assert_eq!(case.key(), "help__mode-text__tty-off__theme-default");
    }

    #[test]
    fn key_slugifies_argv_shaped_values() {
        let case = SnapshotCase::new("Help Page")
            .entry_point("--help")
            .output_mode(OutputMode::TermDebug);

        assert_eq!(case.key(), "help-page__entry-help__mode-term-debug");
    }

    #[test]
    fn cases_differing_in_one_axis_differ_in_one_segment() {
        let dark = SnapshotCase::new("help").theme("dark").key();
        let light = SnapshotCase::new("help").theme("light").key();

        assert_ne!(dark, light);
        assert_eq!(dark.replace("dark", "light"), light);
    }

    #[test]
    fn an_axis_value_that_slugs_away_keeps_the_key_unambiguous() {
        assert_eq!(
            SnapshotCase::new("help").theme("").key(),
            "help__theme-none"
        );
    }

    #[test]
    fn display_renders_the_key() {
        let case = SnapshotCase::new("help").tty(true);
        assert_eq!(case.to_string(), case.key());
    }
}
