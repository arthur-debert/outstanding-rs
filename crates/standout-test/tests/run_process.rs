use standout_input::{PromptResponse, ScriptedResponder};
use standout_render::AmbiguousWidth;
use standout_test::TestHarness;
use std::sync::Arc;
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
