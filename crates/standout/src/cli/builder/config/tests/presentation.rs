use super::*;

fn os_args(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(Into::into).collect()
}

#[test]
fn unparsed_output_mode_reads_equals_and_space_forms() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=json"])),
        Representation::Json
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output", "json"])),
        Representation::Json
    );
}

#[test]
fn unparsed_output_mode_stops_at_terminator_and_falls_back_on_bad_values() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--", "--output=csv"])),
        Representation::Human,
        "arguments after -- are not flags"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&[
            "app",
            "--output=csv",
            "--",
            "--output=json"
        ])),
        Representation::Csv,
        "a flag before -- still counts; one after it does not"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=nope"])),
        Representation::Human,
        "unknown value"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output"])),
        Representation::Human,
        "missing value"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output", "--output=csv"])),
        Representation::Csv,
        "standalone --output must not consume a following --output=csv"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output", "--output", "csv"])),
        Representation::Csv,
        "standalone --output must not consume a following --output value"
    );
}

#[test]
fn unparsed_output_mode_skips_argv0() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["--output=csv"])),
        Representation::Human,
        "argv[0] is the program name, even when it looks like a flag"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["--output=json", "--output=csv"])),
        Representation::Csv,
        "a flag-like program name must not count as an occurrence"
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["--", "--output=csv"])),
        Representation::Csv,
        "a -- program name must not terminate the scan"
    );
}

#[test]
fn unparsed_output_mode_is_auto_when_the_app_has_no_output_flag() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .no_output_flag()
        .build()
        .unwrap();
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=csv"])),
        Representation::Human
    );
}

#[test]
fn the_output_flag_default_spells_the_app_fallback() {
    let default_values = |fallback| {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .output_mode_fallback(fallback)
            .build()
            .unwrap();
        let augmented = app.augment_framework_surface(Command::new("app"));
        let defaults = augmented
            .get_arguments()
            .find(|arg| arg.get_id() == OUTPUT_MODE_ARG)
            .expect("the output flag is declared")
            .get_default_values()
            .to_vec();
        defaults
    };
    assert!(
        default_values(Representation::Human).is_empty(),
        "the human representation has no --output spelling to advertise"
    );
    assert_eq!(
        default_values(Representation::Csv),
        ["csv"],
        "the help page must advertise the encoding the app actually falls back to"
    );
}

#[test]
fn an_unusable_output_value_falls_back_to_the_app_fallback() {
    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_mode_fallback(Representation::Human)
        .build()
        .unwrap();
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=nope"])),
        Representation::Human
    );
    assert_eq!(
        app.extract_output_mode_from_unparsed(&os_args(&["app", "--output"])),
        Representation::Human
    );
}

#[test]
fn a_setup_validation_error_honours_the_app_fallback() {
    use crate::cli::handler::RunErrorKind;
    use crate::InputSources;
    use serde_json::json;

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_mode_fallback(Representation::Human)
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .build()
        .unwrap();
    let result = app.run_with(
        Command::new("app"),
        ["app", "list"],
        color_capable_stderr_target(),
        InputSources::from_process(),
    );
    assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
    assert_eq!(result.output_mode(), Representation::Human);
}

#[test]
fn clap_usage_error_carries_the_output_flag_and_the_startup_warnings() {
    use crate::cli::handler::RunErrorKind;
    use crate::InputSources;
    use serde_json::json;
    use standout_render::warnings::render_block_for_target;

    let mut app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .build()
        .unwrap();
    app.startup_warnings
        .push("stylesheet fell back".to_string());
    let target = color_capable_stderr_target();
    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = app.run_with(
        cmd,
        ["app", "--output=json", "not-a-command"],
        target,
        InputSources::from_process(),
    );
    assert!(
        result.is_error(),
        "unknown command should be a clap usage error, got {:?}",
        result.outcome()
    );
    assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
    assert_eq!(result.output_mode(), Representation::Json);
    assert!(
        result
            .warnings()
            .iter()
            .any(|warning| warning.contains("stylesheet fell back")),
        "expected startup warning on the clap-error result, got {:?}",
        result.warnings()
    );
    let theme = crate::Theme::default();
    let block = render_block_for_target(&theme, ColorPolicy::Never, target, result.warnings());
    assert!(
        !block.contains("\x1b["),
        "a never color policy must keep warnings plain, got {block:?}"
    );
    let styled = render_block_for_target(&theme, ColorPolicy::Always, target, result.warnings());
    assert!(
        styled.contains("\x1b["),
        "Auto on color-capable stderr should style warnings, got {styled:?}"
    );
}

#[test]
fn clap_help_and_version_honour_the_output_flag_from_the_unparsed_line() {
    use crate::InputSources;

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .version("1.0.0")
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(serde_json::json!({"n": 1})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .build()
        .unwrap();
    let target = color_capable_stderr_target();
    let cmd = Command::new("app").subcommand(Command::new("list"));
    let help = app.run_with(
        cmd.clone(),
        ["app", "--help", "--output=json"],
        target,
        InputSources::from_process(),
    );
    assert_eq!(
        help.output_mode(),
        Representation::Json,
        "--output after --help must still reach the run"
    );
    let version = app.run_with(
        cmd,
        ["app", "--output=json", "--version"],
        target,
        InputSources::from_process(),
    );
    assert_eq!(
        version.output_mode(),
        Representation::Json,
        "--output before --version must still reach the run"
    );
}

#[test]
fn unparsed_output_mode_skips_help_and_version_spellings_the_command_already_declares() {
    use crate::InputSources;
    use clap::{Arg, ArgAction};
    use serde_json::json;

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .build()
        .unwrap();
    let target = color_capable_stderr_target();
    let cases: [(&str, Command); 6] = [
        (
            "root --help",
            Command::new("app")
                .disable_help_flag(true)
                .arg(
                    Arg::new("manual_help")
                        .long("help")
                        .action(ArgAction::SetTrue),
                )
                .subcommand(Command::new("list")),
        ),
        (
            "root -h",
            Command::new("app")
                .disable_help_flag(true)
                .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue))
                .subcommand(Command::new("list")),
        ),
        (
            "root --version",
            Command::new("app")
                .disable_version_flag(true)
                .arg(
                    Arg::new("manual_version")
                        .long("version")
                        .action(ArgAction::SetTrue),
                )
                .subcommand(Command::new("list")),
        ),
        (
            "subcommand --help",
            Command::new("app").subcommand(
                Command::new("list").disable_help_flag(true).arg(
                    Arg::new("manual_help")
                        .long("help")
                        .action(ArgAction::SetTrue),
                ),
            ),
        ),
        (
            "subcommand -h",
            Command::new("app").subcommand(
                Command::new("list")
                    .disable_help_flag(true)
                    .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue)),
            ),
        ),
        (
            "subcommand --version",
            Command::new("app").subcommand(
                Command::new("list").disable_version_flag(true).arg(
                    Arg::new("manual_version")
                        .long("version")
                        .action(ArgAction::SetTrue),
                ),
            ),
        ),
    ];
    for (label, cmd) in cases {
        let result = app.run_with(
            cmd,
            ["app", "--output=json", "not-a-command"],
            target,
            InputSources::from_process(),
        );
        assert!(
            result.is_error(),
            "{label}: expected a clap usage error, got {:?}",
            result.outcome()
        );
        assert_eq!(
            result.output_mode(),
            Representation::Json,
            "{label}: custom help/version spellings must not drop --output=json"
        );
    }
}

#[test]
fn unparsed_output_mode_honours_text_output_on_a_sibling_when_another_branch_owns_the_spelling() {
    use crate::InputSources;
    use clap::{Arg, ArgAction};
    use serde_json::json;

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .command_with(
            "sibling",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let target = color_capable_stderr_target();

    let help_cmd = Command::new("app")
        .subcommand(
            Command::new("list").disable_help_flag(true).arg(
                Arg::new("manual_help")
                    .long("help")
                    .action(ArgAction::SetTrue),
            ),
        )
        .subcommand(Command::new("sibling"));
    let help = app.run_with(
        help_cmd,
        ["app", "sibling", "--help", "--output=json"],
        target,
        InputSources::from_process(),
    );
    assert_eq!(
        help.output_mode(),
        Representation::Json,
        "sibling --help --output=json must keep Json when list owns --help"
    );

    let short_cmd = Command::new("app")
        .subcommand(
            Command::new("list")
                .disable_help_flag(true)
                .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue)),
        )
        .subcommand(Command::new("sibling"));
    let short = app.run_with(
        short_cmd,
        ["app", "sibling", "-h", "--output=json"],
        target,
        InputSources::from_process(),
    );
    assert_eq!(
        short.output_mode(),
        Representation::Json,
        "sibling -h --output=json must keep Json when list owns -h"
    );

    let version_cmd = Command::new("app")
        .version("1.0.0")
        .propagate_version(true)
        .subcommand(
            Command::new("list").disable_version_flag(true).arg(
                Arg::new("manual_version")
                    .long("version")
                    .action(ArgAction::SetTrue),
            ),
        )
        .subcommand(Command::new("sibling"));
    let version = app.run_with(
        version_cmd,
        ["app", "sibling", "--version", "--output=json"],
        target,
        InputSources::from_process(),
    );
    assert_eq!(
        version.output_mode(),
        Representation::Json,
        "sibling --version --output=json must keep Json when list owns --version"
    );
}
