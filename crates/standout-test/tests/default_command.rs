//! Integration tests for invocation-aware default-command resolution.
//!
//! The fixture app models the motivating policy: a naked invocation selects a
//! piped entry point (`add`, which reads stdin) when stdin is redirected, and
//! an interactive entry point (`list`) at a terminal. Everything else — explicit
//! commands, nested commands, help, version, invalid syntax — must be untouched
//! by that policy.
//!
//! All tests are `#[serial]` because the harness mutates process-global state
//! (the default stdin reader among them).

use clap::{Arg, ArgAction, Command};
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, ExitStatus, HelpResult, Output, RunErrorKind, SuccessKind};
use standout_input::env::MockStdin;
use standout_input::{reset_default_stdin_reader, set_default_stdin_reader};
use standout_test::TestHarness;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The clap surface: a root with a global flag, two leaf commands, one nested
/// group, and one command clap knows but standout has no handler for.
fn app_command() -> Command {
    Command::new("app")
        .version("1.2.3")
        .arg(
            Arg::new("loud")
                .long("loud")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .subcommand(Command::new("list").alias("ls"))
        .subcommand(Command::new("add"))
        .subcommand(Command::new("db").subcommand(Command::new("migrate")))
        .subcommand(Command::new("unhandled"))
}

/// Registers handlers for every command except `unhandled`, which exercises the
/// partial-adoption `NoMatch` path.
fn register(builder: App) -> App {
    builder
        .command(
            "list",
            |m, _ctx| {
                Ok(Output::Render(json!({
                    "cmd": "list",
                    "loud": m.get_flag("loud"),
                })))
            },
            "{{ cmd }} loud={{ loud }}",
        )
        .unwrap()
        .command(
            "add",
            |m, _ctx| {
                // The resolver never reads stdin; the handler still can.
                use standout_input::env::{DefaultStdin, StdinReader};
                let piped = DefaultStdin.read_to_string().unwrap_or_default();
                Ok(Output::Render(json!({
                    "cmd": "add",
                    "stdin": piped.trim(),
                    "loud": m.get_flag("loud"),
                })))
            },
            "{{ cmd }} stdin={{ stdin }} loud={{ loud }}",
        )
        .unwrap()
        .command(
            "db.migrate",
            |_m, _ctx| Ok(Output::Render(json!({ "cmd": "db.migrate" }))),
            "{{ cmd }}",
        )
        .unwrap()
}

/// The app under test: piped stdin means `add`, a terminal means `list`.
fn piped_aware_app() -> App {
    register(App::builder().default_command_with(|ctx| {
        Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    }))
    .build()
    .unwrap()
}

/// Like [`piped_aware_app`], but the resolver records how many times it ran so
/// tests can assert it stayed out of paths it must not touch.
fn counting_app(calls: Arc<AtomicUsize>) -> App {
    register(App::builder().default_command_with(move |ctx| {
        calls.fetch_add(1, Ordering::SeqCst);
        Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    }))
    .build()
    .unwrap()
}

// --- the invocation facts -------------------------------------------------

#[test]
#[serial]
fn terminal_stdin_selects_the_interactive_command() {
    let result =
        TestHarness::new()
            .interactive_stdin()
            .run(&piped_aware_app(), app_command(), ["app"]);

    result.assert_success();
    result.assert_stdout_eq("list loud=false");
}

#[test]
#[serial]
fn piped_stdin_with_data_selects_the_piped_command() {
    let result = TestHarness::new().piped_stdin("ship the docs\n").run(
        &piped_aware_app(),
        app_command(),
        ["app"],
    );

    result.assert_success();
    result.assert_stdout_eq("add stdin=ship the docs loud=false");
}

#[test]
#[serial]
fn piped_but_empty_stdin_still_selects_the_piped_command() {
    // The distinguishing fact is "stdin is not a terminal", which is knowable
    // without reading. Emptiness is the handler's business, not the resolver's.
    let result = TestHarness::new()
        .piped_stdin("")
        .run(&piped_aware_app(), app_command(), ["app"]);

    result.assert_success();
    result.assert_stdout_eq("add stdin= loud=false");
}

#[test]
#[serial]
fn globals_survive_the_resolved_default() {
    let result = TestHarness::new().interactive_stdin().run(
        &piped_aware_app(),
        app_command(),
        ["app", "--loud"],
    );

    result.assert_success();
    result.assert_stdout_eq("list loud=true");
}

#[test]
#[serial]
fn resolver_reads_app_state() {
    // App state is a fact the resolver may consult alongside the root matches
    // and the stdin terminal fact.
    struct Fallback(&'static str);

    let app = register(
        App::builder()
            .app_state(Fallback("add"))
            .default_command_with(|ctx| ctx.app_state::<Fallback>().map(|f| f.0.to_string())),
    )
    .build()
    .unwrap();

    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, app_command(), ["app"]);
    result.assert_stdout_eq("add stdin= loud=false");
}

// --- what resolution must never touch -------------------------------------

#[test]
#[serial]
fn a_naked_invocation_runs_the_resolver_once() {
    // The positive control for every `calls == 0` assertion below: the counting
    // fixture does increment when resolution is supposed to happen.
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().interactive_stdin().run(
        &counting_app(calls.clone()),
        app_command(),
        ["app"],
    );

    result.assert_stdout_eq("list loud=false");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
#[serial]
fn an_explicit_command_takes_precedence() {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("would have meant add").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "list"],
    );

    result.assert_success();
    result.assert_stdout_eq("list loud=false");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resolver must not run");
}

#[test]
#[serial]
fn a_nested_command_takes_precedence() {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("data").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "db", "migrate"],
    );

    result.assert_success();
    result.assert_stdout_eq("db.migrate");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resolver must not run");
}

#[test]
#[serial]
fn help_is_unchanged() {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("data").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "--help"],
    );

    assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
    result.assert_stdout_contains("Usage:");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resolver must not run");
}

#[test]
#[serial]
fn version_is_unchanged() {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("data").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "--version"],
    );

    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    result.assert_stdout_contains("1.2.3");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resolver must not run");
}

#[test]
#[serial]
fn invalid_syntax_stays_a_clap_usage_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("data").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "--nonexistent"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::ClapUsage);
    // A refused line is offered to the default command, so the resolver does
    // answer here: Clap's probe finds no command in `app --nonexistent`, the
    // default is substituted, and the amended line is parsed. What that must
    // never do is turn a usage error into something else — the diagnostic is
    // still Clap's, from the parse of the line that was actually run.
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// --- interaction with the static default ----------------------------------

#[test]
#[serial]
fn a_static_default_still_applies_on_its_own() {
    let app = register(App::builder().default_command("list"))
        .build()
        .unwrap();

    let result = TestHarness::new()
        .piped_stdin("ignored — no resolver configured")
        .run(&app, app_command(), ["app"]);

    result.assert_success();
    result.assert_stdout_eq("list loud=false");
}

#[test]
#[serial]
fn a_declining_resolver_falls_back_to_the_static_default() {
    let app = register(
        App::builder()
            .default_command("list")
            .default_command_with(|ctx| ctx.stdin_is_piped().then(|| "add".to_string())),
    )
    .build()
    .unwrap();

    let piped = TestHarness::new()
        .piped_stdin("payload")
        .run(&app, app_command(), ["app"]);
    piped.assert_stdout_eq("add stdin=payload loud=false");
    drop(piped);

    // Resolver declines at a terminal, so the static default takes over.
    let terminal = TestHarness::new()
        .interactive_stdin()
        .run(&app, app_command(), ["app"]);
    terminal.assert_stdout_eq("list loud=false");
}

#[test]
#[serial]
fn no_default_configured_leaves_a_naked_invocation_alone() {
    let app = register(App::builder()).build().unwrap();

    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, app_command(), ["app"]);

    result.assert_no_match();
}

// --- partial adoption -----------------------------------------------------

#[test]
#[serial]
fn resolving_to_a_command_standout_does_not_handle_reports_no_match() {
    // `unhandled` is a real clap command with no standout handler: resolution
    // succeeds and dispatch hands back cleanly for the app to handle.
    let app = register(App::builder().default_command_with(|_ctx| Some("unhandled".to_string())))
        .build()
        .unwrap();

    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, app_command(), ["app"]);

    result.assert_no_match();
}

#[test]
#[serial]
fn resolving_to_an_unknown_command_is_a_typed_error_not_a_panic() {
    let app = register(App::builder().default_command_with(|_ctx| Some("nope".to_string())))
        .build()
        .unwrap();

    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, app_command(), ["app"]);

    result.assert_error();
    result.assert_error_kind(RunErrorKind::DefaultCommand);
    // The diagnostic blames the resolver, not the user's command line.
    result.assert_error_contains("default command resolver returned `nope`");
    result.assert_exit_status(ExitStatus::FAILURE);
}

#[test]
#[serial]
fn get_matches_from_reports_an_unknown_command_as_a_clap_error() {
    let app = register(App::builder().default_command_with(|_ctx| Some("nope".to_string())))
        .build()
        .unwrap();

    with_stdin(MockStdin::terminal(), || {
        match app.get_matches_from(app_command(), ["app"]) {
            HelpResult::Error(e) => assert!(
                e.to_string()
                    .contains("default command resolver returned `nope`"),
                "{e}"
            ),
            other => panic!("expected a clap error, got {other:?}"),
        }
    });
}

// --- the configured parsing path ------------------------------------------

/// Runs `body` with the process-global stdin reader mocked, then restores it.
///
/// The `TestHarness` owns this seam for `run()`; `get_matches_from` is a
/// parse-only path with no harness entry point, so these tests drive the same
/// override directly.
struct StdinGuard;

impl StdinGuard {
    fn install(reader: MockStdin) -> Self {
        set_default_stdin_reader(Arc::new(reader));
        Self
    }
}

impl Drop for StdinGuard {
    fn drop(&mut self) {
        reset_default_stdin_reader();
    }
}

fn with_stdin<R>(reader: MockStdin, body: impl FnOnce() -> R) -> R {
    let _guard = StdinGuard::install(reader);
    body()
}

#[test]
#[serial]
fn get_matches_from_resolves_the_same_default_as_dispatch() {
    // Consumers that parse first and build dispatch state afterwards must see
    // the command a naked `run()` would have selected.
    let app = piped_aware_app();

    with_stdin(MockStdin::terminal(), || {
        match app.get_matches_from(app_command(), ["app"]) {
            HelpResult::Matches(m) => assert_eq!(m.subcommand_name(), Some("list")),
            other => panic!("expected matches, got {other:?}"),
        }
    });

    with_stdin(MockStdin::piped("payload"), || {
        match app.get_matches_from(app_command(), ["app"]) {
            HelpResult::Matches(m) => assert_eq!(m.subcommand_name(), Some("add")),
            other => panic!("expected matches, got {other:?}"),
        }
    });

    // Piped-but-empty is a pipe here too — same answer, no read.
    with_stdin(MockStdin::piped_empty(), || {
        match app.get_matches_from(app_command(), ["app"]) {
            HelpResult::Matches(m) => assert_eq!(m.subcommand_name(), Some("add")),
            other => panic!("expected matches, got {other:?}"),
        }
    });
}

#[test]
#[serial]
fn get_matches_from_leaves_invalid_syntax_a_clap_error() {
    let app = piped_aware_app();

    with_stdin(MockStdin::piped("data"), || {
        match app.get_matches_from(app_command(), ["app", "--nonexistent"]) {
            HelpResult::Error(_) => {}
            other => panic!("expected a clap error, got {other:?}"),
        }
    });
}

#[test]
#[serial]
fn get_matches_from_applies_a_static_default() {
    let app = register(App::builder().default_command("list"))
        .build()
        .unwrap();

    match app.get_matches_from(app_command(), ["app", "--loud"]) {
        HelpResult::Matches(m) => {
            assert_eq!(m.subcommand_name(), Some("list"));
            assert!(m.get_flag("loud"));
        }
        other => panic!("expected matches, got {other:?}"),
    }
}

#[test]
#[serial]
fn get_matches_from_leaves_explicit_and_nested_commands_alone() {
    let app = piped_aware_app();

    match app.get_matches_from(app_command(), ["app", "db", "migrate"]) {
        HelpResult::Matches(m) => {
            let (name, sub) = m.subcommand().expect("db");
            assert_eq!(name, "db");
            assert_eq!(sub.subcommand_name(), Some("migrate"));
        }
        other => panic!("expected matches, got {other:?}"),
    }
}

// --- what Clap classifies, resolution does not second-guess ----------------

#[test]
#[serial]
fn a_global_flag_before_the_command_does_not_make_the_line_naked() {
    // Clap reads the option and still selects `list`, so the line is not naked
    // and resolution never runs.
    let calls = Arc::new(AtomicUsize::new(0));
    let result = TestHarness::new().piped_stdin("would have meant add").run(
        &counting_app(calls.clone()),
        app_command(),
        ["app", "--loud", "list"],
    );

    result.assert_success();
    result.assert_stdout_eq("list loud=true");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resolver must not run");
}

#[test]
#[serial]
fn a_required_subcommand_no_longer_blocks_the_default() {
    // Clap refuses the naked line, the default is substituted, and the amended
    // line parses — so a root that requires a subcommand, which is what
    // `#[command(subcommand)] command: Commands` produces, accepts one.
    let app = register(App::builder().default_command("list"))
        .build()
        .unwrap();

    let result = TestHarness::new().interactive_stdin().run(
        &app,
        app_command().subcommand_required(true),
        ["app"],
    );

    result.assert_success();
    result.assert_stdout_eq("list loud=false");
}

#[test]
#[serial]
fn both_paths_select_the_same_command_for_a_flagged_line() {
    let app = piped_aware_app();

    let dispatched =
        TestHarness::new()
            .piped_stdin("data")
            .run(&app, app_command(), ["app", "--loud", "list"]);
    dispatched.assert_stdout_eq("list loud=true");
    drop(dispatched);

    with_stdin(MockStdin::piped("data"), || {
        match app.get_matches_from(app_command(), ["app", "--loud", "list"]) {
            HelpResult::Matches(m) => assert_eq!(m.subcommand_name(), Some("list")),
            other => panic!("expected matches, got {other:?}"),
        }
    });
}

// --- the `help` word on a flat command -------------------------------------

/// A flat root whose required group makes the injected `help` word unreachable
/// unreachable until the declaration says otherwise: `app help` used to be
/// `MissingRequiredArgument`.
fn flat_required_command() -> Command {
    Command::new("app")
        .about("Flat app")
        .arg(Arg::new("range"))
        .arg(Arg::new("staged").long("staged").action(ArgAction::SetTrue))
        .group(
            clap::ArgGroup::new("target")
                .args(["range", "staged"])
                .required(true),
        )
}

#[test]
#[serial]
fn the_help_word_is_reachable_on_a_root_that_requires_arguments() {
    let app = App::builder()
        .help_handling(true)
        .help_word(true)
        .build()
        .unwrap();

    match app.get_matches_from(flat_required_command(), ["app", "help"]) {
        HelpResult::Help(h) => assert!(h.contains("Flat app"), "output:\n{h}"),
        other => panic!("expected rendered help, got {other:?}"),
    }
}

#[test]
#[serial]
fn a_flat_command_keeps_the_word_as_data_without_the_opt_in() {
    let app = App::builder().help_handling(true).build().unwrap();

    match app.get_matches_from(flat_required_command(), ["app", "help"]) {
        HelpResult::Matches(m) => assert_eq!(
            m.get_one::<String>("range").map(String::as_str),
            Some("help")
        ),
        other => panic!("expected matches, got {other:?}"),
    }
}

// --- arguments reach Clap verbatim -----------------------------------------

/// A non-UTF8 argument is an ordinary argument on Unix — a path, most often —
/// and standout must not stand between it and Clap. The parse seam therefore
/// carries `OsString`s end to end, including across the re-parse that
/// substitutes a default command.
#[cfg(unix)]
mod non_utf8 {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    /// `fo\x80o` — a byte sequence no UTF-8 decoder accepts.
    fn wild_path() -> OsString {
        OsString::from_vec(vec![b'f', b'o', 0x80, b'o'])
    }

    fn path_command() -> Command {
        Command::new("app").subcommand(
            Command::new("list").arg(Arg::new("path").value_parser(clap::value_parser!(PathBuf))),
        )
    }

    /// Reports whether the handler received the argument byte for byte.
    fn path_app(default: bool) -> App {
        let builder = if default {
            App::builder().default_command("list")
        } else {
            App::builder()
        };
        builder
            .command(
                "list",
                |m, _ctx| {
                    let seen = m
                        .get_one::<PathBuf>("path")
                        .map(|p| p.as_os_str().to_owned());
                    Ok(Output::Render(json!({
                        "verbatim": seen.as_deref() == Some(wild_path().as_os_str()),
                    })))
                },
                "verbatim={{ verbatim }}",
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    #[serial]
    fn a_non_utf8_argument_reaches_the_handler_unmangled() {
        let result = TestHarness::new().interactive_stdin().run(
            &path_app(false),
            path_command(),
            [OsString::from("app"), OsString::from("list"), wild_path()],
        );

        result.assert_success();
        result.assert_stdout_eq("verbatim=true");
    }

    #[test]
    #[serial]
    fn substituting_a_default_command_does_not_mangle_the_rest() {
        // The line is naked, so it is re-parsed with `list` inserted — the
        // amended argument list must still be the user's bytes.
        let result = TestHarness::new().interactive_stdin().run(
            &path_app(true),
            path_command(),
            [OsString::from("app"), wild_path()],
        );

        result.assert_success();
        result.assert_stdout_eq("verbatim=true");
    }

    #[test]
    #[serial]
    fn get_matches_from_hands_back_the_argument_it_was_given() {
        let app = path_app(false);

        match app.get_matches_from(
            path_command(),
            [OsString::from("app"), OsString::from("list"), wild_path()],
        ) {
            HelpResult::Matches(m) => {
                let sub = m.subcommand_matches("list").expect("list");
                assert_eq!(
                    sub.get_one::<PathBuf>("path").map(|p| p.as_os_str()),
                    Some(wild_path().as_os_str())
                );
            }
            other => panic!("expected matches, got {other:?}"),
        }
    }
}
