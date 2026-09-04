use standout::cli::FnHandler;
use standout::EmbeddedTemplates;

const TEMPLATES: &[(&str, &str)] = &[("root", "{{ name }}")];
use clap::{Arg, ArgAction, Command};
use standout::cli::{App, CommandContextInput, HelpResult};
use standout_fixtures::downstream;

fn help_text(result: HelpResult) -> String {
    match result {
        HelpResult::Help(h) => h,
        other => panic!("expected rendered help, got: {other:?}"),
    }
}

#[test]
fn the_help_word_is_reachable_on_a_flat_command_with_required_args() {
    let fixture = downstream().flat().build();

    let output = help_text(fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "help"],
        &standout::InputSources::from_process(),
    ));
    assert!(output.contains("Diff a git range"), "output:\n{output}");
}

#[test]
fn a_flat_command_with_positionals_does_not_get_the_word_by_default() {
    let fixture = downstream().flat().without_help_word().build();

    match fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "help"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Matches(m) => {
            assert_eq!(
                m.get_one::<String>("range").map(String::as_str),
                Some("help")
            );
        }
        other => panic!("expected matches, got: {other:?}"),
    }
}

#[test]
fn the_word_is_not_advertised_without_the_opt_in() {
    let left_out = downstream().flat().without_help_word().build();
    let cmd = left_out.app().augment_command_with_help(left_out.command());
    assert!(cmd.find_subcommand("help").is_none());

    let opted_in = downstream().flat().build();
    let cmd = opted_in.app().augment_command_with_help(opted_in.command());
    assert!(cmd.find_subcommand("help").is_some());
}

#[test]
fn help_flags_still_render_themed_help_without_the_opt_in() {
    let fixture = downstream().flat().without_help_word().build();

    for flag in ["--help", "-h"] {
        let output = help_text(fixture.app().get_matches_from(
            fixture.command(),
            ["lookma", flag],
            &standout::InputSources::from_process(),
        ));
        assert!(
            output.contains("Diff a git range"),
            "{flag} output:\n{output}"
        );
    }
}

#[test]
fn a_flat_command_with_no_positionals_gets_the_word_automatically() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app").about("Flag-only app").arg(
        Arg::new("staged")
            .long("staged")
            .action(ArgAction::SetTrue)
            .required(true),
    );

    let output = help_text(app.get_matches_from(
        cmd,
        ["app", "help"],
        &standout::InputSources::from_process(),
    ));
    assert!(output.contains("Flag-only app"), "output:\n{output}");
}

#[test]
fn the_escape_delivers_the_literal_word_to_the_positional() {
    let fixture = downstream().flat().build();

    match fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "--", "help"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Matches(m) => {
            assert_eq!(
                m.get_one::<String>("range").map(String::as_str),
                Some("help")
            );
        }
        other => panic!("expected matches, got: {other:?}"),
    }
}

#[test]
fn the_help_word_parses_its_own_arguments() {
    let fixture = downstream().flat().build();

    let tagged = help_text(fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "help", "--output", "term-debug"],
        &standout::InputSources::from_process(),
    ));
    assert!(
        tagged.contains("[header]USAGE[/header]"),
        "output:\n{tagged}"
    );
}

#[test]
fn a_normal_invocation_is_untouched() {
    let fixture = downstream().flat().build();

    match fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "main..HEAD"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Matches(m) => {
            assert_eq!(
                m.get_one::<String>("range").map(String::as_str),
                Some("main..HEAD")
            );
        }
        other => panic!("expected matches, got: {other:?}"),
    }
}

#[test]
fn a_missing_required_argument_is_still_a_usage_error() {
    let fixture = downstream().flat().build();

    match fixture.app().get_matches_from(
        fixture.command(),
        ["lookma"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Error(e) => {
            assert_eq!(e.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        }
        other => panic!("expected a usage error, got: {other:?}"),
    }
}

#[derive(Debug, Clone, standout::Questionnaire)]
#[question(id = "flat.profile")]
struct RootAnswers {
    name: String,
}

#[test]
fn the_policy_reads_the_shape_the_framework_leaves_behind() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .help_handling(true)
        // This app runs the downstream command, which spells `--color` itself.
        .no_color_flag()
        .command_with(
            "",
            FnHandler::new(|_m, ctx| {
                let answers: &RootAnswers = ctx.questionnaire()?;
                Ok(standout::cli::Output::Render(
                    serde_json::json!({ "name": answers.name }),
                ))
            }),
            |cfg| cfg.template_name("root").questionnaire::<RootAnswers>(),
        )
        .unwrap()
        .build()
        .unwrap();
    let flat_root = downstream().flat().build();

    let augmented = app.augment_command_with_help(flat_root.command());
    assert!(
        augmented.find_subcommand("questions").is_some(),
        "the framework should have injected its questionnaire surface"
    );
    assert!(
        augmented.find_subcommand("help").is_some(),
        "the word belongs on a root that has subcommands, however it got them"
    );

    let output = help_text(app.get_matches_from(
        flat_root.command(),
        ["lookma", "help"],
        &standout::InputSources::from_process(),
    ));
    assert!(output.contains("Diff a git range"), "output:\n{output}");
}

#[test]
fn help_word_with_help_handling_off_is_a_setup_error() {
    match App::builder().help_handling(false).help_word(true).build() {
        Err(e) => assert!(
            e.to_string()
                .contains("help_word is set while help handling is off"),
            "error: {e}"
        ),
        Ok(_) => panic!("help_word without help interception must not build"),
    }
}
