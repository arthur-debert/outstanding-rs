//! What `run_process` refuses to pretend it can do.
//!
//! The escape hatch shares one builder with the in-process runner, and most
//! of that builder describes *injection seams* — detectors and default
//! readers a same-process run installs behind the app's back. A child
//! process inherits none of them: it resolves width, color, stdin, and
//! prompts from its own environment. Carrying such a setting silently into a
//! `run_process` call would produce the worst kind of test — one that reads
//! as if it pinned the terminal and in fact asked the CI machine.
//!
//! So each is a loud panic, and these tests pin the message. The settings
//! that *do* cross the boundary — environment variables, working directory,
//! fixture files, argv — are exercised against a real binary in
//! `crates/todo-example/tdoo/tests/process_boundary.rs`, which has one to
//! run.
//!
//! No `#[serial]` here, deliberately: `run_process` mutates nothing
//! process-global in the test's own process, and this binary runs no
//! in-process `run()` whose env and cwd overrides a spawned child could
//! otherwise inherit.

use standout_input::{PromptResponse, ScriptedResponder};
use standout_render::AmbiguousWidth;
use standout_test::TestHarness;
use std::sync::Arc;

/// Never spawned: every test below panics before the fork.
const UNSPAWNED: &str = "standout-test-never-spawned";

#[test]
#[should_panic(expected = "terminal_width()/no_terminal_width()")]
fn a_forced_width_is_refused() {
    TestHarness::new()
        .terminal_width(80)
        .run_process(UNSPAWNED, ["--version"]);
}

#[test]
#[should_panic(expected = "ambiguous_width()")]
fn a_forced_ambiguous_width_policy_is_refused() {
    TestHarness::new()
        .ambiguous_width(AmbiguousWidth::Wide)
        .run_process(UNSPAWNED, ["--version"]);
}

#[test]
#[should_panic(expected = "with_color()/no_color()")]
fn a_forced_color_capability_is_refused() {
    TestHarness::new()
        .with_color()
        .run_process(UNSPAWNED, ["--version"]);
}

#[test]
#[should_panic(expected = "piped_stdin()/interactive_stdin()")]
fn simulated_stdin_is_refused() {
    TestHarness::new()
        .piped_stdin("note\n")
        .run_process(UNSPAWNED, ["--version"]);
}

#[test]
#[should_panic(expected = "clipboard()")]
fn a_mock_clipboard_is_refused() {
    TestHarness::new()
        .clipboard("copied")
        .run_process(UNSPAWNED, ["--version"]);
}

#[test]
#[should_panic(expected = "prompts()")]
fn scripted_prompts_are_refused() {
    TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::Bool(
            true,
        )])))
        .run_process(UNSPAWNED, ["--version"]);
}

/// Three settings, one run, one message: the fix is one edit rather than
/// three rounds of trial and error.
#[test]
fn every_refused_setting_is_named_in_one_message() {
    let panic = std::panic::catch_unwind(|| {
        TestHarness::new()
            .terminal_width(80)
            .no_color()
            .clipboard("copied")
            .run_process(UNSPAWNED, ["--version"]);
    })
    .expect_err("the run must panic");

    let message = panic
        .downcast_ref::<String>()
        .expect("panic payload should be a String");
    for expected in [
        "terminal_width()/no_terminal_width()",
        "with_color()/no_color()",
        "clipboard()",
    ] {
        assert!(
            message.contains(expected),
            "the message must name {expected}: {message}"
        );
    }
    assert!(
        message.contains("they are"),
        "a plural message reads as a plural: {message}"
    );
}
