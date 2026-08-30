use clap::Command;
use serde_json::json;
use standout::cli::{
    App, DispatchResult, ExitStatus, ExternalFailure, HandlerResult, HookError, HookPhase, Hooks,
    Output, OutputKind, RunErrorKind, SuccessKind,
};

fn command() -> Command {
    Command::new("app")
        .version("1.2.3")
        .subcommand(Command::new("go"))
}

fn success_app() -> App {
    App::builder()
        .command(
            "go",
            |_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" }))),
            "{{ message }}",
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn clap_help_and_version_are_typed_successes() {
    let app = success_app();

    let help = app.run_to_string(command(), ["app", "--help"]);
    assert_eq!(help.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(help.success_kind(), Some(SuccessKind::ClapHelp));
    assert!(help.output().unwrap().contains("USAGE"));

    let version = app.run_to_string(command(), ["app", "--version"]);
    assert_eq!(version.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(version.success_kind(), Some(SuccessKind::ClapVersion));
    assert!(version.output().unwrap().contains("1.2.3"));
}

#[test]
fn clap_usage_error_is_status_two() {
    let result = success_app().run_to_string(command(), ["app", "--unknown"]);
    assert_eq!(result.exit_status(), Some(ExitStatus::USAGE_ERROR));
    assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
    assert!(result.error().unwrap().contains("unexpected argument"));
}

#[test]
fn command_silent_and_binary_success_are_status_zero() {
    let text = success_app().run_to_string(command(), ["app", "go"]);
    assert_eq!(text.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(text.output(), Some("ok"));

    let silent = App::builder()
        .command_with(
            "go",
            |_matches, _ctx| -> HandlerResult<()> { Ok(Output::Silent) },
            |config| config.silent(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);
    assert_eq!(silent.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(silent.output(), Some(""));

    let binary = App::builder()
        .command_with(
            "go",
            |_matches, _ctx| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "data.bin".into(),
                })
            },
            |config| config.binary(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);
    assert_eq!(binary.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(binary.binary(), Some((&[0, 1, 2][..], "data.bin")));
}

#[test]
fn handler_and_each_hook_phase_keep_their_origin() {
    let handler = App::builder()
        .command_with(
            "go",
            |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("handler failed"))
            },
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);
    assert_eq!(handler.exit_status(), Some(ExitStatus::FAILURE));
    assert_eq!(handler.error_kind(), Some(RunErrorKind::Handler));

    for (hooks, phase) in [
        (
            Hooks::new().pre_dispatch(|_, _| Err(HookError::pre_dispatch("no"))),
            HookPhase::PreDispatch,
        ),
        (
            Hooks::new().post_dispatch(|_, _, _| Err(HookError::post_dispatch("no"))),
            HookPhase::PostDispatch,
        ),
        (
            Hooks::new().post_output(|_, _, _| Err(HookError::post_output("no"))),
            HookPhase::PostOutput,
        ),
    ] {
        let result = App::builder()
            .command(
                "go",
                |_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" }))),
                "{{ message }}",
            )
            .unwrap()
            .hooks("go", hooks)
            .build()
            .unwrap()
            .run_to_string(command(), ["app", "go"]);
        assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
        assert_eq!(result.error_kind(), Some(RunErrorKind::Hook(phase)));
    }
}

#[test]
fn external_failure_metadata_crosses_handler_and_pre_dispatch_seams() {
    let handler = App::builder()
        .command_with(
            "go",
            |_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(anyhow::Error::new(
                    ExternalFailure::new(128, "fatal: handler external\n")
                        .unwrap()
                        .with_source(std::io::Error::other("git failed")),
                )
                .context("delegated Git invocation"))
            },
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);

    assert_eq!(handler.exit_status().unwrap().code(), 128);
    assert_eq!(handler.error_kind(), Some(RunErrorKind::External));
    assert_eq!(handler.error(), Some("fatal: handler external\n"));
    assert_eq!(handler.output(), None);
    let DispatchResult::Error(handler_error) = handler.outcome() else {
        panic!("expected external error");
    };
    assert_eq!(
        std::error::Error::source(handler_error)
            .unwrap()
            .to_string(),
        "git failed"
    );

    let pre_dispatch = App::builder()
        .command(
            "go",
            |_matches, _ctx| Ok(Output::Render(json!({ "message": "unreachable" }))),
            "{{ message }}",
        )
        .unwrap()
        .hooks(
            "go",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "fatal: pre-dispatch external\n").unwrap(),
                ))
            }),
        )
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);

    assert_eq!(pre_dispatch.exit_status().unwrap().code(), 128);
    assert_eq!(pre_dispatch.error_kind(), Some(RunErrorKind::External));
    assert_eq!(pre_dispatch.error(), Some("fatal: pre-dispatch external\n"));
    assert_eq!(pre_dispatch.output(), None);
}

#[test]
fn post_hooks_cannot_self_label_as_pre_dispatch_external() {
    for (hooks, phase) in [
        (
            Hooks::new().post_dispatch(|_, _, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "must stay ordinary").unwrap(),
                ))
            }),
            HookPhase::PostDispatch,
        ),
        (
            Hooks::new().post_output(|_, _, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "must stay ordinary").unwrap(),
                ))
            }),
            HookPhase::PostOutput,
        ),
    ] {
        let result = App::builder()
            .command(
                "go",
                |_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" }))),
                "{{ message }}",
            )
            .unwrap()
            .hooks("go", hooks)
            .build()
            .unwrap()
            .run_to_string(command(), ["app", "go"]);

        assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
        assert_eq!(result.error_kind(), Some(RunErrorKind::Hook(phase)));
    }
}

#[test]
fn render_and_output_file_write_failures_are_typed() {
    let render = App::builder()
        .command(
            "go",
            |_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" }))),
            "{{",
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "go"]);
    assert_eq!(render.error_kind(), Some(RunErrorKind::Render));
    assert_eq!(render.exit_status(), Some(ExitStatus::FAILURE));

    let tempdir = tempfile::tempdir().unwrap();
    let directory = tempdir.path().to_str().unwrap();
    let write =
        success_app().run_to_string(command(), ["app", "--output-file-path", directory, "go"]);
    assert_eq!(
        write.error_kind(),
        Some(RunErrorKind::FinalWrite(OutputKind::Text))
    );
    assert_eq!(write.exit_status(), Some(ExitStatus::FAILURE));

    let binary_app = App::builder()
        .command_with(
            "go",
            |_matches, _ctx| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "data.bin".into(),
                })
            },
            |config| config.binary(),
        )
        .unwrap()
        .build()
        .unwrap();
    let binary_write =
        binary_app.run_to_string(command(), ["app", "--output-file-path", directory, "go"]);
    assert_eq!(
        binary_write.error_kind(),
        Some(RunErrorKind::FinalWrite(OutputKind::Binary))
    );
    assert_eq!(binary_write.exit_status(), Some(ExitStatus::FAILURE));
}

#[test]
fn output_file_success_is_silent_and_no_match_has_no_status() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("output.txt");
    let path_string = path.to_string_lossy().into_owned();
    let result =
        success_app().run_to_string(command(), ["app", "--output-file-path", &path_string, "go"]);
    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(result.output(), Some(""));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "ok");

    let no_match = success_app().run_to_string(command(), ["app"]);
    assert!(matches!(no_match.outcome(), DispatchResult::NoMatch(_)));
    assert_eq!(no_match.exit_status(), None);
    assert_eq!(no_match.error_kind(), None);
}
