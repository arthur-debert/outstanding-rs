use clap::Command;
use standout::cli::App;
use standout::{InputSources, OutputMode, TargetProperties, TemplateRef};

fn main() {
    let app = App::builder().build().expect("empty app should build");

    let _result = app.get_matches_from(Command::new("app"), ["app"], &InputSources::from_process());

    let matches = Command::new("app")
        .try_get_matches_from(["app"])
        .expect("fixture arguments should parse");
    let _result = app.dispatch(matches, OutputMode::Auto);
    let _result = app.run_with(
        Command::new("app"),
        ["app"],
        TargetProperties::detect(),
        InputSources::from_process(),
    );
    let _handled = app.run(Command::new("app"), ["app"]);

    let _rendered = app.render_with(
        TemplateRef::Named("missing".to_string()),
        &(),
        OutputMode::Text,
        TargetProperties::detect(),
    );
    let _rendered = app.render_with(
        TemplateRef::Inline("ok".to_string()),
        &(),
        OutputMode::Text,
        TargetProperties::detect(),
    );
}
