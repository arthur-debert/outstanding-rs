//! Reachability and install policy for the `help` word on flat CLIs.
//!
//! The shape that surfaced the bug is the shared fixture's flat form
//! ([`standout_fixtures`]): one optional positional, one flag, and an
//! `ArgGroup` making "one of them" required. On a root like that, Clap
//! validates the requirements before routing, so an injected `help`
//! subcommand is unreachable until the declaration says otherwise — which is
//! what `subcommand_negates_reqs` does, and standout sets it exactly where it
//! installs the word.
//!
//! Two commands here are still built by hand, because their shape *is* the
//! test: a root with no positional at all, and an app whose questionnaire
//! makes the framework inject a subcommand of its own.

use clap::{Arg, ArgAction, Command};
use standout::cli::{App, CommandContextInput, HelpResult};
use standout_fixtures::downstream;

fn help_text(result: HelpResult) -> String {
    match result {
        HelpResult::Help(h) => h,
        HelpResult::PagedHelp(h) => h,
        other => panic!("expected rendered help, got: {other:?}"),
    }
}

/// The reported bug: this was `MissingRequiredArgument`, because the root's
/// `target` group was validated before Clap would route the word.
#[test]
fn the_help_word_is_reachable_on_a_flat_command_with_required_args() {
    let fixture = downstream().flat().build();

    let output = help_text(
        fixture
            .app()
            .get_matches_from(fixture.command(), ["lookma", "help"]),
    );
    assert!(output.contains("Diff a git range"), "output:\n{output}");
}

#[test]
fn a_flat_command_with_positionals_does_not_get_the_word_by_default() {
    let fixture = downstream().flat().without_help_word().build();

    // Not installed, so `help` is data: it reaches the positional and the
    // required group is satisfied by it.
    match fixture
        .app()
        .get_matches_from(fixture.command(), ["lookma", "help"])
    {
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
        let output = help_text(
            fixture
                .app()
                .get_matches_from(fixture.command(), ["lookma", flag]),
        );
        assert!(
            output.contains("Diff a git range"),
            "{flag} output:\n{output}"
        );
    }
}

/// A root with nothing for a bare word to be mistaken for: the word installs
/// itself, no opt-in asked. Built by hand because the fixture always carries
/// a positional.
#[test]
fn a_flat_command_with_no_positionals_gets_the_word_automatically() {
    let app = App::new().help_handling(true).build().unwrap();
    let cmd = Command::new("app").about("Flag-only app").arg(
        Arg::new("staged")
            .long("staged")
            .action(ArgAction::SetTrue)
            .required(true),
    );

    let output = help_text(app.get_matches_from(cmd, ["app", "help"]));
    assert!(output.contains("Flag-only app"), "output:\n{output}");
}

#[test]
fn the_escape_delivers_the_literal_word_to_the_positional() {
    let fixture = downstream().flat().build();

    match fixture
        .app()
        .get_matches_from(fixture.command(), ["lookma", "--", "help"])
    {
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

    // `--output` is a root global; the help arm still has to honour it.
    // `term-debug` leaves style tags visible, which is only true if the mode
    // reached the renderer. The fixture's app theme defines none of those
    // tags, so a resolved `[header]` is also the overlay working: an app theme
    // that replaced the help theme would leave the tag unresolved.
    let tagged = help_text(fixture.app().get_matches_from(
        fixture.command(),
        ["lookma", "help", "--output", "term-debug"],
    ));
    assert!(
        tagged.contains("[header]USAGE[/header]"),
        "output:\n{tagged}"
    );

    // `--page` routes the same rendering through the pager.
    let paged = fixture
        .app()
        .get_matches_from(fixture.command(), ["lookma", "help", "--page"]);
    assert!(
        matches!(paged, HelpResult::PagedHelp(_)),
        "expected paged help, got: {paged:?}"
    );
}

#[test]
fn a_normal_invocation_is_untouched() {
    let fixture = downstream().flat().build();

    match fixture
        .app()
        .get_matches_from(fixture.command(), ["lookma", "main..HEAD"])
    {
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

    match fixture
        .app()
        .get_matches_from(fixture.command(), ["lookma"])
    {
        HelpResult::Error(e) => {
            assert_eq!(e.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        }
        other => panic!("expected a usage error, got: {other:?}"),
    }
}

/// A questionnaire registered at the root path, which makes the framework
/// inject a `questions` subcommand onto the root itself.
#[derive(Debug, Clone, standout::Questionnaire)]
#[question(id = "flat.profile")]
struct RootAnswers {
    /// Project name.
    name: String,
}

#[test]
fn the_policy_reads_the_shape_the_framework_leaves_behind() {
    // `questions` is injected by the framework, so a root that declares no
    // subcommands of its own still has one by the time anybody meets it — and a
    // root with subcommands is a root where a bare word is already a command.
    // Deciding the install before augmentation answered for a shape that never
    // reaches the user.
    //
    // The app is bespoke because the questionnaire is the subject; the command
    // is the fixture's flat root, so the shape under test is the shared one.
    let app = App::new()
        .help_handling(true)
        .command_with(
            "",
            |_m, ctx| {
                let answers: &RootAnswers = ctx.questionnaire()?;
                Ok(standout::cli::Output::Render(answers.name.clone()))
            },
            |cfg| cfg.template("{{ . }}").questionnaire::<RootAnswers>(),
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

    // And it is reachable, without the opt-in the flat-with-positionals rule
    // would otherwise have required.
    let output = help_text(app.get_matches_from(flat_root.command(), ["lookma", "help"]));
    assert!(output.contains("Diff a git range"), "output:\n{output}");
}

#[test]
fn help_word_without_help_handling_is_a_setup_error() {
    match App::new().help_word(true).build() {
        Err(e) => assert!(
            e.to_string()
                .contains("help_word requires .help_handling(true)"),
            "error: {e}"
        ),
        Ok(_) => panic!("help_word without help_handling must not build"),
    }
}
