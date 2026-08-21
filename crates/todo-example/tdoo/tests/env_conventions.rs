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
//! conventions are genuinely observable. No `#[serial]` here: this binary
//! runs no in-process `run()`, whose env and cwd overrides a spawned child
//! would otherwise be free to inherit.
//!
//! # Two boundaries, two kinds of pin
//!
//! The **piped** tests (`run_process`) assert the outcome a redirecting user
//! gets. On a pipe, `colors_supported` is `false` before `NO_COLOR` or
//! `TERM` are ever consulted, so the suppression runs and the plain `Auto`
//! baseline share one observable — a plain page. Those tests state the piped
//! *outcome contract*; on their own they could not catch a `console` upgrade
//! dropping a convention, because on a pipe the TTY check alone already
//! produces the page they assert.
//!
//! The **pty** tests (`run_pty`) are the pins that can. There the child's
//! stdout is a real pseudo-terminal, where a color-capable `TERM` makes the
//! `Auto` baseline emit ANSI — proven by its own test — so on that boundary
//! `NO_COLOR` and `TERM=dumb` are the only thing standing between a run and
//! color. A `console` upgrade that dropped either convention turns that
//! test's plain page back into ANSI: red, not silently green. Making the
//! conventions *deliberate* standout behavior instead of an inherited
//! accident remains parity-program work
//! (`docs/spec/parity-terminal-citizenship.md`); pinning them no longer
//! waits on it.
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
/// convention's own suppression is pinned on the pty below — but the outcome
/// a `NO_COLOR` user is promised when piping is pinned here.
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
// The suppression conventions on a real terminal, via a pty
// ---------------------------------------------------------------------------

/// The color-capable baseline: `Auto` on a pty with a color-capable `TERM`
/// emits ANSI. This is the proof the two pins below lean on — on this
/// boundary color is otherwise *on*, so each convention is the only thing
/// standing between its run and ANSI, and deleting it goes red below
/// instead of staying green.
#[cfg(unix)]
#[test]
fn auto_on_a_pty_renders_with_ansi() {
    let result = conventions(&[("TERM", "xterm-256color")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

/// `NO_COLOR` set on the color-capable pty: plain. This is the convention's
/// actual pin — the one run where `NO_COLOR` itself, not the TTY check,
/// suppresses the color the baseline proves would otherwise appear.
#[cfg(unix)]
#[test]
fn no_color_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "xterm-256color"), ("NO_COLOR", "1")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

/// `TERM=dumb` on the pty: plain, same standing as `NO_COLOR` above — the
/// terminal is real, so only the `TERM` value stands between this run and
/// the baseline's ANSI.
#[cfg(unix)]
#[test]
fn term_dumb_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "dumb")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

// ---------------------------------------------------------------------------
// Explicit output flags override the environment (flag > env)
// ---------------------------------------------------------------------------

/// `--output=term` through a pipe emits ANSI: the request applies
/// `force_styling` for Term (ADR-0030), independent of `console`'s
/// process-global switch. This is the documented contract that #329 pinned
/// as a contradiction; the delta is that contract becoming true.
#[test]
fn explicit_term_through_a_pipe_emits_ansi() {
    let result = conventions(&[])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

/// `NO_COLOR=1` with `--output=term` still emits ANSI. An explicit output
/// flag overrides the environment so a test or script can ask for a known
/// rendering (Spec `flag > env > config > detection` for terminal settings).
/// The override is deliberate: without it there is no way to pin Term
/// colour from a piped process. `--color` (parity program,
/// `docs/spec/parity-terminal-citizenship.md`) will take this half so
/// `--output` can mean format alone.
#[test]
fn explicit_term_overrides_no_color() {
    let result = conventions(&[("NO_COLOR", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

/// `CLICOLOR_FORCE=1` with `--output=text` stays plain. Same flag > env
/// rule as `explicit_term_overrides_no_color`, in the other direction:
/// naming a format that does not colour beats the force convention.
/// `--color` is the eventual owner of this decision.
#[test]
fn explicit_text_overrides_clicolor_force() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Text)
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
