//! Integration tests for the declarative input API on `CommandConfig::input`.
//!
//! These tests exercise the full path from `App::builder().command_with(...)
//! .input(name, chain)` through pre-dispatch resolution into the handler's
//! `ctx.input::<T>(name)` lookup.

use clap::{Arg, Command};
use serde_json::json;
use standout::cli::{App, CommandContextInput, DispatchResult as RunResult, Output};
use standout::input::{
    env::MockStdin, ArgSource, FlagSource, InputChain, InputSourceKind, InputSources, StdinSource,
};
use standout::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};

fn body_command() -> Command {
    Command::new("test")
        .subcommand(Command::new("create").arg(Arg::new("body").long("body").short('b')))
}

fn capable_target() -> TargetProperties {
    TargetProperties {
        width: Some(80),
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}

fn run_create(app: &App, args: Vec<&str>, stdin: Option<MockStdin>) -> RunResult {
    let mut sources = InputSources::from_process();
    if let Some(stdin) = stdin {
        sources = sources.with_stdin(stdin);
    }
    app.run_with(body_command(), args, capable_target(), sources)
        .into_outcome()
}

#[test]
fn arg_value_reaches_handler_via_ctx_input() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                Ok(Output::Render(json!({ "echo": body })))
            },
            |cfg| {
                cfg.template("{{ echo }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create", "--body", "hello"], None);
    match result {
        RunResult::Handled(out) => assert_eq!(out, "hello"),
        other => panic!("expected Handled, got {:?}", other),
    }
}

#[test]
fn default_kicks_in_when_no_source_provides_value() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                Ok(Output::Render(json!({ "echo": body })))
            },
            |cfg| {
                cfg.template("{{ echo }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create"], None);
    match result {
        RunResult::Handled(out) => assert_eq!(out, "FALLBACK"),
        other => panic!("expected Handled, got {:?}", other),
    }
}

#[test]
fn input_source_reports_arg_kind() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({ "kind": kind.to_string() })))
            },
            |cfg| {
                cfg.template("{{ kind }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create", "--body", "x"], None);
    if let RunResult::Handled(out) = result {
        assert_eq!(out, InputSourceKind::Arg.to_string());
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn input_source_reports_default_kind_when_falling_back() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({ "kind": kind.to_string() })))
            },
            |cfg| {
                cfg.template("{{ kind }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create"], None);
    if let RunResult::Handled(out) = result {
        assert_eq!(out, InputSourceKind::Default.to_string());
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn stdin_fallback_when_arg_absent() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({
                    "echo": body,
                    "kind": kind.to_string(),
                })))
            },
            |cfg| {
                cfg.template("{{ kind }}: {{ echo }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .try_source(StdinSource::new())
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(
        &app,
        vec!["test", "create"],
        Some(MockStdin::piped("from stdin\n")),
    );

    if let RunResult::Handled(out) = result {
        // StdinSource trims trailing newlines.
        assert_eq!(out, "stdin: from stdin");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn arg_wins_over_stdin_when_both_available() {
    // With arg present, stdin source must not be reached. The MockStdin
    // terminal mode avoids accidentally reading real stdin if precedence is
    // wrong.

    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({
                    "echo": body,
                    "kind": kind.to_string(),
                })))
            },
            |cfg| {
                cfg.template("{{ kind }}: {{ echo }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .try_source(StdinSource::new())
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(
        &app,
        vec!["test", "create", "--body", "from arg"],
        Some(MockStdin::terminal()),
    );

    if let RunResult::Handled(out) = result {
        assert_eq!(out, "argument: from arg");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn multiple_named_inputs_of_same_type_do_not_collide() {
    let cmd = Command::new("test").subcommand(
        Command::new("create")
            .arg(Arg::new("body").long("body"))
            .arg(Arg::new("title").long("title")),
    );

    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let title: &String = ctx.input("title").unwrap();
                Ok(Output::Render(json!({
                    "body": body,
                    "title": title,
                })))
            },
            |cfg| {
                cfg.template("{{ title }} | {{ body }}")
                    .input(
                        "body",
                        InputChain::<String>::new()
                            .try_source(ArgSource::new("body"))
                            .default("nobody".to_string()),
                    )
                    .input(
                        "title",
                        InputChain::<String>::new()
                            .try_source(ArgSource::new("title"))
                            .default("untitled".to_string()),
                    )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app
        .run_to_string(
            cmd,
            vec![
                "test",
                "create",
                "--body",
                "the body",
                "--title",
                "the title",
            ],
        )
        .into_outcome();
    if let RunResult::Handled(out) = result {
        assert_eq!(out, "the title | the body");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn mixed_types_string_and_bool_coexist() {
    let cmd = Command::new("test").subcommand(
        Command::new("create")
            .arg(Arg::new("body").long("body"))
            .arg(
                Arg::new("force")
                    .long("force")
                    .action(clap::ArgAction::SetTrue),
            ),
    );

    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let force: &bool = ctx.input("force").unwrap();
                Ok(Output::Render(json!({
                    "body": body,
                    "force": force,
                })))
            },
            |cfg| {
                cfg.template("body={{ body }} force={{ force }}")
                    .input(
                        "body",
                        InputChain::<String>::new()
                            .try_source(ArgSource::new("body"))
                            .default("default".to_string()),
                    )
                    .input(
                        "force",
                        InputChain::<bool>::new()
                            .try_source(FlagSource::new("force"))
                            .default(false),
                    )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app
        .run_to_string(cmd, vec!["test", "create", "--body", "x", "--force"])
        .into_outcome();
    if let RunResult::Handled(out) = result {
        assert_eq!(out, "body=x force=true");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn validation_failure_aborts_before_handler() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, _ctx| -> standout::cli::HandlerResult<serde_json::Value> {
                panic!("handler must not run when pre-dispatch validation fails");
            },
            |cfg| {
                cfg.template("{{ echo }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .validate(|s| !s.trim().is_empty(), "body must not be empty"),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create", "--body", "   "], None);
    let out = match result {
        RunResult::Error(s) => s,
        other => panic!("expected Error, got {:?}", other),
    };
    assert!(out.starts_with("Hook error:"), "unexpected output: {out}");
    assert!(out.contains("body"), "error should name the input: {out}");
    assert!(
        out.contains("must not be empty"),
        "error should surface validator message: {out}"
    );
}

#[test]
fn handler_asking_for_unregistered_input_gets_missing_input_error() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                // Ask for a name we never registered.
                let err = ctx.input::<String>("nonexistent").unwrap_err();
                Ok(Output::Render(json!({ "error": err.to_string() })))
            },
            |cfg| {
                cfg.template("{{ error }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("x".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create"], None);
    if let RunResult::Handled(out) = result {
        assert!(out.contains("nonexistent"), "got: {out}");
        assert!(out.contains("no input"), "got: {out}");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn type_mismatch_lookup_returns_descriptive_error() {
    let app = App::builder()
        .command_with(
            "create",
            |_m, ctx| {
                // Stored as String, asked as u32.
                let err = ctx.input::<u32>("body").unwrap_err();
                Ok(Output::Render(json!({ "error": err.to_string() })))
            },
            |cfg| {
                cfg.template("{{ error }}").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("x".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create"], None);
    if let RunResult::Handled(out) = result {
        assert!(out.contains("body"), "got: {out}");
        assert!(out.contains("u32"), "got: {out}");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}
