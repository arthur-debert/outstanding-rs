use clap::{ArgMatches, Command};
use standout::cli::App;
use standout::{InputSources, OutputMode, TargetProperties, TemplateRef};

fn matches() -> ArgMatches {
    Command::new("app")
        .try_get_matches_from(["app"])
        .expect("fixture arguments should parse")
}

fn main() {
    let _result = App::builder().dispatch(matches(), OutputMode::Auto);
    let _result = App::builder().run_with(
        Command::new("app"),
        ["app"],
        TargetProperties::detect(),
        InputSources::from_process(),
    );
    let _matches =
        App::builder().get_matches_from(Command::new("app"), ["app"], &InputSources::from_process());
    let _rendered = App::builder().render_with(
        TemplateRef::Inline("ok".to_string()),
        &(),
        OutputMode::Text,
        TargetProperties::detect(),
    );
}
