use clap::{ArgMatches, Command};
use standout::cli::App;
use standout::OutputMode;

fn matches() -> ArgMatches {
    Command::new("app")
        .try_get_matches_from(["app"])
        .expect("fixture arguments should parse")
}

fn main() {
    let _result = App::builder().dispatch(matches(), OutputMode::Auto);
    let _result = App::builder().dispatch_from(Command::new("app"), ["app"]);
    let _matches = App::builder().parse(Command::new("app"));
    let _matches = App::builder().parse_from(Command::new("app"), ["app"]);
    let _matches = App::builder().get_matches(Command::new("app"));
    let _rendered = App::builder().render_inline("ok", &(), OutputMode::Text);
}
