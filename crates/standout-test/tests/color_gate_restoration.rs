//! Gate 2's restoration contract, in a binary where the harness gets the
//! process's *first* color read.
//!
//! `console::colors_enabled()` initializes its process-global lazily, on the
//! first read, from `CLICOLOR` / `CLICOLOR_FORCE`. A harness that captured it
//! after installing its own `.env()` overrides would capture a value it had
//! just caused, and would write that value back on drop — permanently, for
//! every later test in the binary. The bug is only observable when the first
//! color read in the process happens inside a harness run, which is why this
//! is the only test in its own test binary.

use serde_json::json;
use standout::cli::{App, Output};
use standout_test::TestHarness;

/// The harness must restore `console`'s color switch to the value the *real*
/// environment implies, not to the value its own temporary environment did.
#[test]
fn console_color_state_restores_to_the_pre_run_environment() {
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
        let app = App::builder()
            .command("say", |_m, _ctx| Ok(Output::Render(json!({}))), "hello")
            .unwrap()
            .build()
            .unwrap();
        let cmd = clap::Command::new("app").subcommand(clap::Command::new("say"));

        // The first color read in this process happens inside this run —
        // with `CLICOLOR_FORCE=1` installed by the harness itself.
        let _result = TestHarness::new()
            .env("CLICOLOR_FORCE", "1")
            .no_color()
            .run(&app, cmd, ["say"]);
    }

    assert_eq!(
        console::colors_enabled(),
        before,
        "the harness restored `console`'s color switch to a value its own temporary \
         CLICOLOR_FORCE produced, so every later test in this binary would inherit it"
    );
}
