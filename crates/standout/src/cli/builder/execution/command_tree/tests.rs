use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::EmbeddedTemplates;
use clap::{Arg, ArgAction, Command};

#[test]
fn an_application_flag_colliding_with_a_framework_flag_is_a_setup_error() {
    use serde_json::json;

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(
        Command::new("list").arg(
            Arg::new("quiet")
                .long("no-pager")
                .action(ArgAction::SetTrue),
        ),
    );

    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(
        error.contains("pager_flag installs `--no-pager`")
            && error.contains("app list")
            && error.contains("no_pager_flag()"),
        "expected the seam named, got: {error}"
    );

    let result = app.run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );
    assert!(result.error().is_some_and(|error| error
        .to_string()
        .contains("pager_flag installs `--no-pager`")));
}

#[test]
fn a_colliding_subcommand_invocation_name_reports_the_subcommand() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app")
        .subcommand(Command::new("list").subcommand(Command::new("all").long_flag("no-pager")));

    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(
        error.contains("pager_flag installs `--no-pager`") && error.contains("`app list all`"),
        "expected the declaring subcommand named, got: {error}"
    );
}
