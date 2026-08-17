//! Gate 2's restoration contract for a run that sets **no** color knob, in a
//! binary where the harness gets the process's *first* color read.
//!
//! Sibling of `color_gate_restoration.rs`, which covers the same contract for
//! a run that does call `.no_color()`. The knobless case is the harder one:
//! nothing the run overrides is `console`'s switch, so there is nothing to
//! restore on drop — the only defense is that the harness spends `console`'s
//! one lazy initialization on the *real* environment before installing this
//! run's `.env()`. If it did not, an app that styles its own output would
//! initialize `console` from `CLICOLOR_FORCE=1` that only ever existed for
//! the duration of one run, and every later test in the binary would inherit
//! it with no record anywhere that it had been changed.
//!
//! The bug is only observable when the first color read in the process
//! happens inside a harness run, which is why this is the only test in its
//! own test binary.

use serde_json::json;
use standout::cli::{App, Output};
use standout_test::TestHarness;

/// A run that sets no color knob must still leave `console`'s color switch at
/// what the real environment implies, even when app code inside the run is
/// what triggers `console`'s lazy initialization.
#[test]
fn console_color_state_survives_a_run_with_no_color_knob() {
    // `CLICOLOR_FORCE` unset in the ambient environment is what makes the
    // harness's temporary one distinguishable; a developer exporting either
    // variable has already decided the answer, so there is nothing to prove.
    if std::env::var_os("CLICOLOR_FORCE").is_some() || std::env::var_os("CLICOLOR").is_some() {
        return;
    }

    // `Term::features()` asks the file descriptor and the environment
    // directly on every call — it does not touch, or initialize, the
    // process-global this test is about. So this is the pre-run truth,
    // readable without spending the one lazy initialization.
    let before = console::Term::stdout().features().colors_supported();

    {
        // The handler styles a string the way an application colors its own
        // output. Rendering a `console::StyledObject` reads
        // `console::colors_enabled()` — so this run's *app code*, not the
        // harness, is what would trigger the lazy initialization.
        let app = App::builder()
            .command(
                "say",
                |_m, _ctx| {
                    let _ = console::Style::new().red().apply_to("hello").to_string();
                    Ok(Output::Render(json!({})))
                },
                "hello",
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = clap::Command::new("app").subcommand(clap::Command::new("say"));

        // No `.with_color()` / `.no_color()`: the harness writes nothing to
        // `console`, so nothing is recorded for `Drop` to put back.
        let result = TestHarness::new()
            .env("CLICOLOR_FORCE", "1")
            .run(&app, cmd, ["app", "say"]);

        // The handler is what reads `console` here, so a run that never
        // dispatched it would assert nothing at all.
        assert_eq!(result.stdout_plain().trim_end(), "hello");
    }

    assert_eq!(
        console::colors_enabled(),
        before,
        "a run with no color knob let its own temporary CLICOLOR_FORCE initialize \
         `console`'s color switch, and nothing restores it — every later test in this \
         binary would inherit it"
    );
}
