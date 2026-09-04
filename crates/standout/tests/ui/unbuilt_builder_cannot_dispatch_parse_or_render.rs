use clap::{ArgMatches, Command};
use standout::cli::App;
use standout::{InputSources, Representation, TargetProperties, TemplateRef};

fn matches() -> ArgMatches {
    Command::new("app")
        .try_get_matches_from(["app"])
        .expect("fixture arguments should parse")
}

fn main() {
    let _result = App::builder().dispatch(matches(), Representation::Human);
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
        Representation::Human,
        TargetProperties::detect(),
    );
}
