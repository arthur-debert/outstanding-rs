use clap::{Arg, Command};
use serde_json::json;
use standout::cli::hooks::Hooks;
use standout::cli::FnHandler;
use standout::cli::{App, CommandContextInput, DispatchResult, Output};
use standout::input::{
    env::MockStdin, ArgSource, FlagSource, InputChain, InputSourceKind, InputSources,
    PromptResponse, ScriptedResponder, StdinSource, TextPromptSource,
};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};
use std::sync::{Arc, Mutex};

const TEMPLATES: &[(&str, &str)] = &[
    ("create", "{{ echo }}"),
    ("create-2", "{{ kind }}"),
    ("create-3", "{{ kind }}: {{ echo }}"),
    ("create-4", "{{ title }} | {{ body }}"),
    ("create-5", "body={{ body }} force={{ force }}"),
    ("create-6", "{{ error }}"),
];

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

fn run_create(app: &App, args: Vec<&str>, stdin: Option<MockStdin>) -> DispatchResult {
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                Ok(Output::Render(json!({ "echo": body })))
            }),
            |cfg| {
                cfg.input(
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
        DispatchResult::Handled(out) => assert_eq!(out, "hello"),
        other => panic!("expected Handled, got {:?}", other),
    }
}

#[test]
fn run_command_resolves_declared_input() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                Ok(Output::Render(json!({ "echo": body })))
            }),
            |cfg| {
                cfg.input(
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

    let cmd = body_command();
    let matches = cmd
        .try_get_matches_from(["test", "create", "--body", "hello"])
        .unwrap();
    let sub = matches.subcommand_matches("create").unwrap();
    let output = app
        .run_command(
            "create",
            sub,
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                Ok(Output::Render(json!({ "echo": body })))
            }),
            standout::TemplateRef::Inline(("{{ echo }}").to_string()),
            ColorPolicy::Auto,
            standout::cli::StreamSink::new(Vec::new()),
        )
        .expect("run_command should resolve the declared input");
    assert_eq!(output.as_text(), Some("hello"));
}

#[test]
fn default_kicks_in_when_no_source_provides_value() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                Ok(Output::Render(json!({ "echo": body })))
            }),
            |cfg| {
                cfg.input(
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
        DispatchResult::Handled(out) => assert_eq!(out, "FALLBACK"),
        other => panic!("expected Handled, got {:?}", other),
    }
}

#[test]
fn input_source_reports_arg_kind() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({ "kind": kind.to_string() })))
            }),
            |cfg| {
                cfg.template_name("create-2").input(
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
    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, InputSourceKind::Arg.to_string());
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

/// A value read through the wizard's `file` collector must not report itself as a typed argument.
#[test]
fn input_source_reports_file_kind() {
    struct FileSource {
        arg: &'static str,
    }

    impl standout::input::InputCollector<String> for FileSource {
        fn name(&self) -> &'static str {
            "file"
        }

        fn is_available(&self, matches: &clap::ArgMatches) -> bool {
            matches.get_one::<std::path::PathBuf>(self.arg).is_some()
        }

        fn collect(
            &self,
            matches: &clap::ArgMatches,
        ) -> Result<Option<String>, standout::input::InputError> {
            let Some(path) = matches.get_one::<std::path::PathBuf>(self.arg) else {
                return Ok(None);
            };
            std::fs::read_to_string(path).map(Some).map_err(|error| {
                standout::input::InputError::file(path.display().to_string(), error)
            })
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let document = dir.path().join("document.txt");
    std::fs::write(&document, "from a file").unwrap();

    let command = Command::new("test").subcommand(
        Command::new("create").arg(
            Arg::new("body_file")
                .long("body-file")
                .value_parser(clap::value_parser!(std::path::PathBuf)),
        ),
    );
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let kind = ctx.input_source("body").unwrap();
                let body: &String = ctx.input("body").expect("body should be resolved");
                Ok(Output::Render(
                    json!({ "kind": kind.to_string(), "echo": body }),
                ))
            }),
            |cfg| {
                cfg.template_name("create-3").input(
                    "body",
                    InputChain::<String>::new().try_source(FileSource { arg: "body_file" }),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = app
        .run_with(
            command,
            vec!["test", "create", "--body-file", document.to_str().unwrap()],
            capable_target(),
            InputSources::from_process(),
        )
        .into_outcome();
    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, format!("{}: from a file", InputSourceKind::File));
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn input_source_reports_default_kind_when_falling_back() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({ "kind": kind.to_string() })))
            }),
            |cfg| {
                cfg.template_name("create-2").input(
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
    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, InputSourceKind::Default.to_string());
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn stdin_fallback_when_arg_absent() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({
                    "echo": body,
                    "kind": kind.to_string(),
                })))
            }),
            |cfg| {
                cfg.template_name("create-3").input(
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

    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, "stdin: from stdin");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn arg_wins_over_stdin_when_both_available() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({
                    "echo": body,
                    "kind": kind.to_string(),
                })))
            }),
            |cfg| {
                cfg.template_name("create-3").input(
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

    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, "argument: from arg");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn command_config_input_consumes_scripted_responder() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                let kind = ctx.input_source("body").unwrap();
                Ok(Output::Render(json!({
                    "echo": body,
                    "kind": kind.to_string(),
                })))
            }),
            |cfg| {
                cfg.template_name("create-3").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .try_source(TextPromptSource::new("Body: ")),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let sources = InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
        PromptResponse::text("from prompt"),
    ])));
    let result = app
        .run_with(
            body_command(),
            vec!["test", "create"],
            capable_target(),
            sources,
        )
        .into_outcome();

    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, "prompt: from prompt");
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let title: &String = ctx.input("title").unwrap();
                Ok(Output::Render(json!({
                    "body": body,
                    "title": title,
                })))
            }),
            |cfg| {
                cfg.template_name("create-4")
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
        .run_with(
            cmd,
            vec![
                "test",
                "create",
                "--body",
                "the body",
                "--title",
                "the title",
            ],
            standout::TargetProperties::detect(),
            standout::InputSources::from_process(),
        )
        .into_outcome();
    if let DispatchResult::Handled(out) = result {
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let body: &String = ctx.input("body").unwrap();
                let force: &bool = ctx.input("force").unwrap();
                Ok(Output::Render(json!({
                    "body": body,
                    "force": force,
                })))
            }),
            |cfg| {
                cfg.template_name("create-5")
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
        .run_with(
            cmd,
            vec!["test", "create", "--body", "x", "--force"],
            standout::TargetProperties::detect(),
            standout::InputSources::from_process(),
        )
        .into_outcome();
    if let DispatchResult::Handled(out) = result {
        assert_eq!(out, "body=x force=true");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn validation_failure_aborts_before_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(
                |_m, _ctx| -> standout::cli::HandlerResult<serde_json::Value> {
                    panic!("handler must not run when pre-dispatch validation fails");
                },
            ),
            |cfg| {
                cfg.input(
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
        DispatchResult::Error(s) => s,
        other => panic!("expected Error, got {:?}", other),
    };
    assert_eq!(
        out.as_str(),
        "Error: hook error (pre-dispatch): input `body`: Validation failed: body must not be empty"
    );
}

#[test]
fn handler_asking_for_unregistered_input_gets_missing_input_error() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let err = ctx.input::<String>("nonexistent").unwrap_err();
                Ok(Output::Render(json!({ "error": err.to_string() })))
            }),
            |cfg| {
                cfg.template_name("create-6").input(
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
    if let DispatchResult::Handled(out) = result {
        assert!(out.contains("nonexistent"), "got: {out}");
        assert!(out.contains("no input"), "got: {out}");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn type_mismatch_lookup_returns_descriptive_error() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(|_m, ctx| {
                let err = ctx.input::<u32>("body").unwrap_err();
                Ok(Output::Render(json!({ "error": err.to_string() })))
            }),
            |cfg| {
                cfg.template_name("create-6").input(
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
    if let DispatchResult::Handled(out) = result {
        assert!(out.contains("body"), "got: {out}");
        assert!(out.contains("u32"), "got: {out}");
    } else {
        panic!("expected Handled, got {:?}", result);
    }
}

#[test]
fn an_input_chain_leaves_the_pre_dispatch_registration_free() {
    let seen = Arc::new(Mutex::new(None::<String>));
    let recorded = Arc::clone(&seen);
    let reported = Arc::clone(&seen);

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(move |_m, ctx| {
                let body: &String = ctx.input("body").expect("body should be resolved");
                let hook_saw = reported.lock().unwrap().clone();
                Ok(Output::Render(json!({
                    "kind": hook_saw.unwrap_or_else(|| "the hook did not run".to_string()),
                    "echo": body,
                })))
            }),
            |cfg| {
                cfg.template_name("create-3").input(
                    "body",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("body"))
                        .default("FALLBACK".to_string()),
                )
            },
        )
        .unwrap()
        .hooks(
            "create",
            Hooks::new().pre_dispatch(move |_m, ctx| {
                let body: &String = ctx
                    .input("body")
                    .expect("the chain resolves before the command's pre-dispatch hooks");
                *recorded.lock().unwrap() = Some(body.clone());
                Ok(())
            }),
        )
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create", "--body", "hello"], None);
    match result {
        DispatchResult::Handled(out) => assert_eq!(out, "hello: hello"),
        other => panic!("expected Handled, got {:?}", other),
    }
}

#[test]
fn two_chains_claiming_one_input_name_are_rejected() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "create",
            FnHandler::new(
                |_m, _ctx| -> standout::cli::HandlerResult<serde_json::Value> {
                    panic!("handler must not run when an input name collides");
                },
            ),
            |cfg| {
                cfg.input(
                    "body",
                    InputChain::<String>::new().default("first".to_string()),
                )
                .input(
                    "body",
                    InputChain::<String>::new().default("second".to_string()),
                )
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let result = run_create(&app, vec!["test", "create"], None);
    let out = match result {
        DispatchResult::Error(s) => s,
        other => panic!("expected Error, got {:?}", other),
    };
    assert!(
        out.contains("input `body` is already resolved"),
        "got: {out}"
    );
    assert!(
        out.contains("duplicate input names are not supported"),
        "got: {out}"
    );
}
