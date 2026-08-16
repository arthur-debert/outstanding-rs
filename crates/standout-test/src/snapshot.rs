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
//! A key is also an identity, not just a label: cases that differ in any axis
//! value get different keys, so two cells can never end up sharing one
//! snapshot as their oracle. That holds because the key's punctuation is
//! disjoint from what a slugified segment can contain: `__` separates axes and
//! `_` separates an axis name from its value (a slug never contains `_`), and
//! `--` tags the digest carried by a value that had to be reshaped to be
//! readable in a filename (a slug never contains `--` either). So `--help`
//! keys apart from `help`, `group-1`/`test` from `group`/`1-test`, and a
//! reshaped value from one that merely spells its encoded form.
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
//! assert_eq!(
//!     case.key(),
//!     "help__mode_term__tty_on__theme_default__entry_help--7fb28c5c"
//! );
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
    /// The subject leads; each axis follows as `__<name>_<value>`. Every
    /// segment is slugified — lowercased, with runs of non-alphanumerics
    /// collapsed to a single `-` — so a case built from argv strings like
    /// `--help` still names a file a reviewer can read. A value that reshaping
    /// would make ambiguous (`--help` and `help` both read as `help`) carries
    /// a short digest of the original, so cases that differ always key apart.
    ///
    /// The name/value boundary is `_` rather than `-` precisely because `-` is
    /// what [`slug`] collapses runs into: with a hyphen there, `group-1`/`test`
    /// and `group`/`1-test` would name one file. `_` cannot occur inside a
    /// slug, so the boundary is unambiguous — and a single `_` between name and
    /// value can never spell the `__` that starts the next axis.
    pub fn key(&self) -> String {
        let mut key = slug(&self.subject);
        for (name, value) in &self.axes {
            key.push_str("__");
            key.push_str(&slug(name));
            key.push('_');
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

/// Separates a reshaped value's readable form from its [`digest`].
///
/// `--` is the one piece of punctuation [`squash`] cannot emit: it collapses
/// every run of non-alphanumerics to a *single* `-` and trims the ends. That
/// makes the two forms below structurally disjoint, which is what the fast
/// path relies on.
const DIGEST_TAG: &str = "--";

/// Renders `text` as one key segment: readable, and never ambiguous.
///
/// Squashing alone is lossy, and a key is what selects a snapshot file — two
/// cases that squash alike would silently share one oracle, the exact failure
/// this type exists to prevent. `--help` and `help` both squash to `help`; so
/// do `dark mode` and `dark-mode`; and an empty value squashes to nothing,
/// which reads like the literal `none`.
///
/// So a value that is *already* its own squashed form — lowercase
/// alphanumerics joined by single `-`, no leading or trailing separator — is
/// used verbatim, and anything else carries a [`digest`] of the original text
/// after its readable form and a [`DIGEST_TAG`] (`--help` → `help--7fb28c5c`,
/// `""` → `none--811c9dc5`). Every axis the named builders produce is already
/// canonical, so ordinary keys stay clean and only a hand-written value pays
/// for its own ambiguity.
///
/// The tag is what keeps the two forms from meeting in the middle. A verbatim
/// value can never contain `--`, so it can never spell an encoded one: the
/// literal `help--7fb28c5c` is not canonical (it squashes to `help-7fb28c5c`)
/// and so encodes to `help-7fb28c5c--<its own digest>` rather than colliding
/// with the encoding of `--help`. Distinct values therefore key apart unless
/// they share both a squashed form and a 32-bit digest.
fn slug(text: &str) -> String {
    let readable = squash(text);
    if !readable.is_empty() && readable == text {
        return readable;
    }

    let base = if readable.is_empty() {
        "none"
    } else {
        &readable
    };
    format!("{}{}{:08x}", base, DIGEST_TAG, digest(text))
}

/// Lowercases `text` and collapses every run of characters that cannot appear
/// in a key into a single `-`, with no leading or trailing separator.
fn squash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Returns the FNV-1a hash of `text`.
///
/// Hand-rolled rather than reached for from `std`: a snapshot name has to stay
/// identical across toolchains and releases, and `DefaultHasher`'s output is
/// explicitly not a stable value. A key that shifted under a new compiler would
/// orphan every committed snapshot at once.
fn digest(text: &str) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;

    let mut hash = OFFSET_BASIS;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
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
/// calling crate's own `snapshots/` directory. Re-exporting `insta` from here
/// would spare the caller that line, but only by making `insta` a regular
/// dependency of `standout-test` — a runtime dependency for every consumer, to
/// save one dev-dependency entry in the crates that actually snapshot. It
/// stays a dev-dependency.
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

        assert_eq!(case.key(), "help__mode_text__tty_off__theme_default");
    }

    #[test]
    fn key_slugifies_argv_shaped_values() {
        let case = SnapshotCase::new("Help Page")
            .entry_point("--help")
            .output_mode(OutputMode::TermDebug);

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

    /// The collision this keying exists to prevent: values that read alike
    /// once they are filename-shaped must still name different snapshots, or
    /// two matrix cells silently share one oracle.
    ///
    /// The last two pairs are the ones an encoding meets in the middle: a
    /// value that spells another value's *encoded* form has to key apart from
    /// it too, or the fast path for canonical values reopens the collision.
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

    /// The other boundary a key has to keep straight: where an axis name ends
    /// and its value begins. `-` is what [`slug`] collapses runs into, so a
    /// hyphen there would let `group-1`/`test` and `group`/`1-test` name one
    /// file — the same shared-oracle failure, one segment over.
    #[test]
    fn the_axis_name_value_boundary_is_unambiguous() {
        let split_in_the_name = SnapshotCase::new("help").axis("group-1", "test").key();
        let split_in_the_value = SnapshotCase::new("help").axis("group", "1-test").key();

        assert_eq!(split_in_the_name, "help__group-1_test");
        assert_eq!(split_in_the_value, "help__group_1-test");
        assert_ne!(split_in_the_name, split_in_the_value);
    }

    /// A slug's punctuation is disjoint from the key's: the separators cannot
    /// be forged from inside a segment, which is what makes the boundaries
    /// above hold for *any* value, not just the pairs enumerated here.
    #[test]
    fn a_slug_never_spells_the_keys_punctuation() {
        for text in ["--help", "dark mode", "", "a__b", "x--y", "Group_1"] {
            let slugged = slug(text);
            let readable = slugged
                .split_once(DIGEST_TAG)
                .map_or(slugged.as_str(), |(base, _)| base);

            assert!(!readable.contains('_'), "{:?} → {:?}", text, slugged);
            assert!(!readable.contains(DIGEST_TAG), "{:?} → {:?}", text, slugged);
        }
    }

    /// A value already in key shape is spent verbatim — the digest is the
    /// price of ambiguity, not a tax on every segment.
    #[test]
    fn a_canonical_value_keys_without_a_digest() {
        let case = SnapshotCase::new("help")
            .output_mode(OutputMode::Text)
            .tty(true)
            .theme("solarized-dark");

        assert_eq!(case.key(), "help__mode_text__tty_on__theme_solarized-dark");
    }

    #[test]
    fn an_axis_value_that_slugs_away_keeps_the_key_unambiguous() {
        assert_eq!(
            SnapshotCase::new("help").theme("").key(),
            "help__theme_none--811c9dc5"
        );
    }

    /// The digest is baked into committed snapshot filenames, so it has to be
    /// the same number on every toolchain, forever.
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
}
