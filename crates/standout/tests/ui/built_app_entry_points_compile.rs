use clap::Command;
use standout::cli::App;
use standout::OutputMode;

fn main() {
    let app = App::builder().build().expect("empty app should build");

    let _matches = app.parse(Command::new("app"));
    let _matches = app.parse_with(Command::new("app"));
    let _matches = app.parse_from(Command::new("app"), ["app"]);
    let _result = app.get_matches(Command::new("app"));
    let _result = app.get_matches_from(Command::new("app"), ["app"]);

    let matches = Command::new("app")
        .try_get_matches_from(["app"])
        .expect("fixture arguments should parse");
    let _result = app.dispatch(matches, OutputMode::Auto);
    let _result = app.dispatch_from(Command::new("app"), ["app"]);
    let _handled = app.run(Command::new("app"), ["app"]);
    let _result = app.run_to_string(Command::new("app"), ["app"]);

    let _rendered = app.render("missing", &(), OutputMode::Text);
    let _rendered = app.render_inline("ok", &(), OutputMode::Text);
}
