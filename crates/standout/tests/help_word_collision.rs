use clap::{Arg, ArgAction, Command};
use standout::cli::{App, DispatchResult, HelpResult, Output};

fn app_with_its_own_help() -> Command {
    Command::new("app")
        .about("Test app")
        .disable_help_subcommand(true)
        .subcommand(Command::new("build").about("Build the project"))
        .subcommand(Command::new("help").about("The application's own help"))
}

fn error_text(result: HelpResult) -> String {
    match result {
        HelpResult::Error(e) => e.to_string(),
        other => panic!("expected a setup error, got: {other:?}"),
    }
}

#[test]
fn a_declared_help_subcommand_is_a_setup_error_not_a_panic() {
    let app = App::builder().help_handling(true).build().unwrap();

    let message = error_text(app.get_matches_from(app_with_its_own_help(), ["app", "build"]));
    assert!(
        message.contains("duplicate command: help"),
        "error: {message}"
    );
    assert!(
        message.contains("help handling is on by default"),
        "the message must name the setting that installs the word: {message}"
    );
    assert!(
        message.contains(".help_handling(false)"),
        "the message must name the way out: {message}"
    );
    assert!(
        message.contains("Rename"),
        "the message must tell the author what to do: {message}"
    );
}

#[test]
fn the_dispatch_path_reports_the_same_collision() {
    let app = App::builder().help_handling(true).build().unwrap();

    match app
        .run_to_string(app_with_its_own_help(), ["app", "build"])
        .into_outcome()
    {
        DispatchResult::Error(e) => assert!(
            e.to_string().contains("duplicate command: help"),
            "error: {e}"
        ),
        other => panic!("expected a setup error, got: {other:?}"),
    }
}

#[test]
fn an_aliased_help_subcommand_collides_too() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app")
        .disable_help_subcommand(true)
        .subcommand(Command::new("build"))
        .subcommand(Command::new("manual").alias("help"));

    let message = error_text(app.get_matches_from(cmd, ["app", "build"]));
    assert!(
        message.contains("duplicate command: help"),
        "error: {message}"
    );
}

#[test]
fn augmentation_hands_back_the_colliding_root_for_the_caller_to_refuse() {
    let app = App::builder().help_handling(true).build().unwrap();
    let augmented = app.augment_command_with_help(app_with_its_own_help());

    let claims = augmented
        .get_subcommands()
        .filter(|sub| sub.get_name() == "help")
        .count();
    assert_eq!(claims, 2, "the application's `help` and standout's own");
}

#[test]
fn a_registered_help_command_fails_at_build() {
    let result = App::builder()
        .help_handling(true)
        .command("help", |_m, _ctx| Ok(Output::Render("mine")), "mine")
        .unwrap()
        .build();

    match result {
        Err(e) => assert!(
            e.to_string().contains("duplicate command: help"),
            "error: {e}"
        ),
        Ok(_) => panic!("a registered `help` command must not build under help_handling(true)"),
    }
}

#[test]
fn a_command_registered_under_help_fails_at_build_too() {
    let result = App::builder()
        .help_handling(true)
        .command("help.topic", |_m, _ctx| Ok(Output::Render("mine")), "mine")
        .unwrap()
        .build();

    match result {
        Err(e) => {
            let message = e.to_string();
            assert!(
                message.contains("duplicate command: help"),
                "error: {message}"
            );
            assert!(
                message.contains("`help.topic`"),
                "the message must name the registration that collided: {message}"
            );
        }
        Ok(_) => panic!("a command under a root `help` must not build under help_handling(true)"),
    }
}

#[test]
fn a_help_group_fails_at_build_too() {
    let result = App::builder()
        .help_handling(true)
        .group("help", |g| {
            g.command("topic", |_m, _ctx| Ok(Output::Render("mine")))
        })
        .unwrap()
        .build();

    match result {
        Err(e) => assert!(
            e.to_string().contains("duplicate command: help"),
            "error: {e}"
        ),
        Ok(_) => panic!("a `help` group must not build under help_handling(true)"),
    }
}

#[test]
fn a_nested_help_command_is_not_the_root_word() {
    App::builder()
        .help_handling(true)
        .command("db.help", |_m, _ctx| Ok(Output::Render("mine")), "mine")
        .unwrap()
        .build()
        .unwrap();
}

#[test]
fn without_help_handling_the_application_keeps_the_name() {
    let app = App::builder()
        .help_handling(false)
        .command("help", |_m, _ctx| Ok(Output::Render("mine")), "mine")
        .unwrap()
        .build()
        .unwrap();

    match app
        .run_to_string(app_with_its_own_help(), ["app", "help"])
        .into_outcome()
    {
        DispatchResult::Handled(output) => assert_eq!(output.as_str(), "mine"),
        other => panic!("expected the application's own handler to run, got: {other:?}"),
    }
}

#[test]
fn a_flat_root_that_never_gets_the_word_is_unaffected() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app")
        .about("Flat app")
        .arg(Arg::new("range").help("A revision range"))
        .arg(Arg::new("staged").long("staged").action(ArgAction::SetTrue));

    match app.get_matches_from(cmd, ["app", "help"]) {
        HelpResult::Matches(m) => assert_eq!(
            m.get_one::<String>("range").map(String::as_str),
            Some("help")
        ),
        other => panic!("expected matches, got: {other:?}"),
    }
}
