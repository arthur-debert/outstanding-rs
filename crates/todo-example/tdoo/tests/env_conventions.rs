//! Environment color conventions, pinned at the process boundary.
//!
//! Standout honors `NO_COLOR` and `TERM=dumb` **by accident of dependency
//! choice**: `OutputMode::Auto` resolves through `console`'s
//! `Term::stdout().features().colors_supported()`, which happens to read
//! both — and which is *not* the API that reads `CLICOLOR`/`CLICOLOR_FORCE`.
//! Those reach a render through a different door: `console`'s process-global
//! color switch, consulted inside `Style::apply_to` (gate 2 of #329), whose
//! lazy default is `(colors_supported && CLICOLOR != "0") || CLICOLOR_FORCE
//! != "0"`. Nothing in standout implements, tests, or documents any of this;
//! these tests pin it so a `console` upgrade cannot silently change color
//! semantics (test-net Spec, `docs/spec/robustness-test-net.md`).
//!
//! They live at the process boundary on purpose. ADR-0022
//! (`docs/adr/0022-delete-the-in-process-tty-seam.md`) records why an
//! in-process `.env("NO_COLOR", …)` test would measure the harness rather
//! than the framework: every in-process run latches `console`'s globals
//! before applying its env map. A child process performs its own lazy
//! initialization from a real environment, so `run_process` is where these
//! conventions are genuinely observable — and a `run_process` test needs no
//! `#[serial]`.
//!
//! # What a pipe can and cannot show
//!
//! The child's stdout is a real pipe, so `colors_supported` is `false` before
//! `NO_COLOR` or `TERM` are ever consulted. Two consequences, stated so
//! nobody mistakes the coverage:
//!
//! - The suppression pins (`NO_COLOR`, `TERM=dumb`, and plain `Auto`) all
//!   assert the same observable — a plain page — and on a pipe the TTY check
//!   alone would produce it. What they pin is the *outcome contract* a piped
//!   user gets, and the force-path pins below are what would go red if a
//!   `console` upgrade rerouted the initialization.
//! - The TTY-side path — `NO_COLOR` suppressing color on a real terminal —
//!   needs a PTY, which `run_process` deliberately does not fake. That gap is
//!   known and owned: the parity program's terminal-citizenship Spec
//!   (`docs/spec/parity-terminal-citizenship.md`) is where the conventions
//!   become deliberate, implemented behavior instead of an inherited accident.
//!
//! # The known `CLICOLOR_FORCE` gap
//!
//! `CLICOLOR_FORCE=1` is the clicolors convention for "color no matter what,
//! piped included". Standout's `Auto` path ignores it (`colors_supported`
//! never reads it) — so a piped `Auto` run stays plain even under force. Yet
//! the same variable *does* reach an explicit `--output=term` run, because
//! gate 2's lazy default honors it. Honoring the force-path convention in
//! `Auto` — and deciding who owns the styling gate at all (#329) — is
//! parity-program work (`docs/spec/parity-terminal-citizenship.md`), not an
//! oversight here. The tests pin both sides of the accident as they stand.

use standout::OutputMode;
use standout_test::TestHarness;

/// The escape byte every ANSI sequence starts with.
const ESC: char = '\u{1b}';

/// The store `tdoo` loads before parsing; one styled row for `list`.
const STORE: &str = r#"{"todos":[{"id":1,"title":"buy milk","done":false}],"next_id":1}"#;

/// Every color-convention variable a CI machine might leak into the child.
const CONVENTION_VARS: [&str; 4] = ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "TERM"];

/// A harness with the store wired, `vars` set, and every *other* convention
/// variable scrubbed — so the child's environment holds exactly the
/// conventions the test names, whatever the CI machine exports.
///
/// Set-or-scrub is decided per key here because the harness applies its
/// remove list after its set map: a key in both is removed, so a test cannot
/// scrub a variable and then set it.
fn conventions(vars: &[(&str, &str)]) -> TestHarness {
    let mut harness = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json");
    for key in CONVENTION_VARS {
        harness = match vars.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => harness.env(key, *value),
            None => harness.env_remove(key),
        };
    }
    harness
}

fn assert_plain(result: &standout_test::ProcessResult) {
    result.assert_success();
    assert!(
        !result.stdout().contains(ESC),
        "expected a plain page, got ANSI:\n{:?}",
        result.stdout()
    );
    result.assert_stdout_contains("buy milk");
}

fn assert_ansi(result: &standout_test::ProcessResult) {
    result.assert_success();
    assert!(
        result.stdout().contains(ESC),
        "expected ANSI escapes, got a plain page:\n{:?}",
        result.stdout()
    );
}

// ---------------------------------------------------------------------------
// The suppression conventions, as a piped user meets them
// ---------------------------------------------------------------------------

/// The baseline: `Auto` through a pipe is plain. Every suppression convention
/// below asserts the same outcome; this is the page they suppress *to*.
#[test]
fn auto_through_a_pipe_renders_plain() {
    let result = conventions(&[])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

/// `NO_COLOR` set: plain. On a pipe the TTY check already decides this — the
/// convention's TTY-side effect is the PTY gap the module doc records — but
/// the outcome a `NO_COLOR` user is promised is pinned here.
#[test]
fn no_color_keeps_a_piped_run_plain() {
    let result = conventions(&[("NO_COLOR", "1")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

/// `TERM=dumb`: plain, same standing as `NO_COLOR` above.
#[test]
fn term_dumb_keeps_a_piped_run_plain() {
    let result = conventions(&[("TERM", "dumb")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

// ---------------------------------------------------------------------------
// Explicit Term mode: standout opens its gate, console's stays authoritative
// ---------------------------------------------------------------------------

/// `--output=term` through a pipe emits **no** ANSI: standout opens gate 1,
/// `console`'s process-global switch (gate 2) initializes to off on a
/// non-TTY, and standout never sets it. This contradicts `Term`'s documented
/// "always use ANSI" contract — #329 tracks that; the pin is what makes the
/// eventual fix a visible, deliberate diff instead of a silent change.
#[test]
fn explicit_term_through_a_pipe_emits_no_ansi() {
    let result = conventions(&[])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

/// `CLICOLOR_FORCE=1` *does* reach an explicit `--output=term` run — through
/// `console`'s gate-2 default, not through anything standout does. This is
/// the accidental half of the force-path: the one spelling that yields ANSI
/// on a pipe today.
#[test]
fn clicolor_force_reaches_term_mode_through_consoles_gate() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

/// `NO_COLOR` does not reach the force path: with `CLICOLOR_FORCE=1`, gate
/// 2's default is true before `NO_COLOR` is consulted, so the run still
/// emits ANSI. The suppression conventions arrive only through
/// `colors_supported` — a `console` upgrade that taught its force path to
/// respect `NO_COLOR` would flip this test, which is the point of pinning it.
#[test]
fn no_color_does_not_reach_the_force_path() {
    let result = conventions(&[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

// ---------------------------------------------------------------------------
// The known gap: the force path never reaches Auto
// ---------------------------------------------------------------------------

/// `CLICOLOR_FORCE=1` under `Auto` stays plain: `Auto` resolves through
/// `colors_supported`, which never reads it. Per the clicolors convention
/// this run *should* color; making the force path deliberate is owned by the
/// parity program (`docs/spec/parity-terminal-citizenship.md`), and this pin
/// is the executable record that the gap is known, not overlooked.
#[test]
fn the_force_path_does_not_reach_auto_mode() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}
