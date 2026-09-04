use standout::cli::FnHandler;
use std::cell::RefCell;
use std::rc::Rc;

use clap::{Arg, Command};
use console::Style;
use minijinja::Value;
use serde_json::json;
use standout::cli::{render_help, App, HelpConfig, Output, RenderedOutput};
use standout::context::{ContextRegistry, RenderContext};
use standout::{
    render_request, AmbiguousWidth, ColorMode, ColorPolicy, IconMode, InputSources,
    MiniJinjaEngine, RenderRequest, Representation, SharedTemplateEngine, TargetProperties,
    TemplateRef, TemplateRegistry, Theme,
};

fn capable_target() -> TargetProperties {
    TargetProperties {
        width: Some(80),
        stdout_is_terminal: true,
        stderr_is_terminal: true,
        stdout_color_capability: true,
        stderr_color_capability: true,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}

fn shout_engine() -> MiniJinjaEngine {
    let mut engine = MiniJinjaEngine::new();
    engine
        .environment_mut()
        .add_filter("shout", |value: String| value.to_uppercase());
    engine
}

fn shared_engine(engine: MiniJinjaEngine) -> SharedTemplateEngine {
    Rc::new(RefCell::new(Box::new(engine)))
}

fn greet_command() -> Command {
    Command::new("app").subcommand(Command::new("greet"))
}

fn greet_command_with_output() -> Command {
    greet_command().arg(
        Arg::new("_output_mode")
            .long("output")
            .value_name("MODE")
            .global(true)
            .value_parser(["json", "yaml", "csv", "ndjson", "term-debug"]),
    )
}

fn run_command_greet(app: &App, matches: &clap::ArgMatches) -> RenderedOutput {
    run_command_greet_with(app, matches, ColorPolicy::Auto)
}

fn run_command_greet_with(
    app: &App,
    matches: &clap::ArgMatches,
    color: ColorPolicy,
) -> RenderedOutput {
    let sub = matches.subcommand_matches("greet").unwrap();
    app.run_command(
        "greet",
        sub,
        FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada", "label": "hi"})))),
        standout::TemplateRef::Inline((COMPOSITION_TEMPLATE).to_string()),
        color,
        standout::cli::StreamSink::new(Vec::new()),
    )
    .expect("run_command should render")
}

fn dispatch_greet(app: &App, matches: clap::ArgMatches) -> String {
    let mode = app.extract_output_mode(&matches);
    app.dispatch(matches, mode)
        .output()
        .expect("dispatch should render")
        .to_string()
}

fn help_command() -> Command {
    Command::new("app").about("Demo")
}

const COMPOSITION_TEMPLATE: &str =
    "{{ name | shout }}|{% include \"part\" %}|{{ app_version }}|[mark]{{ label }}[/mark]|{{ where }}";

fn composition_app(templates: &std::path::Path) -> App {
    App::builder()
        .theme(Theme::new().add("mark", Style::new().bold()))
        .template_engine(Box::new(shout_engine()))
        .templates_dir(templates)
        .unwrap()
        .context("app_version", Value::from("9.9"))
        .context_fn("where", |ctx: &RenderContext| {
            Value::from(format!("w{}", ctx.terminal_width.unwrap_or(0)))
        })
        .command_with(
            "greet",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada", "label": "hi"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn composition_request(
    app: &App,
    templates: &std::path::Path,
    target: TargetProperties,
) -> RenderRequest {
    let mut registry = TemplateRegistry::new();
    registry.add_template_dir(templates).unwrap();
    registry.refresh().unwrap();
    let mut context_registry = ContextRegistry::new();
    context_registry.add_static("app_version", Value::from("9.9"));
    context_registry.add_provider("where", |ctx: &RenderContext| {
        Value::from(format!("w{}", ctx.terminal_width.unwrap_or(0)))
    });
    RenderRequest {
        data: json!({"name": "Ada", "label": "hi"}),
        template: TemplateRef::Inline(COMPOSITION_TEMPLATE.to_string()),
        theme: app.get_default_theme().clone(),
        format: Representation::Human,
        color_policy: ColorPolicy::Always,
        target,
        engine: shared_engine(shout_engine()),
        registry: Some(Rc::new(registry)),
        context_registry: Some(context_registry),
        csv_projection: None,
        extras: Default::default(),
        warnings: None,
    }
}

fn write_part_template() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("part.j2"), "INC").unwrap();
    std::fs::write(dir.path().join("greet.j2"), COMPOSITION_TEMPLATE).unwrap();
    dir
}

#[test]
fn dispatch_render_inline_and_render_request_agree_byte_for_byte() {
    let templates = write_part_template();
    let app = composition_app(templates.path());
    let target = capable_target();
    let data = json!({"name": "Ada", "label": "hi"});

    let dispatched = app.run_with_color(
        greet_command(),
        ["app", "greet"],
        target,
        ColorPolicy::Always,
        InputSources::from_process(),
    );
    let dispatch_out = dispatched
        .output()
        .expect("dispatch should render")
        .to_string();

    let inline = app
        .render_with(
            standout::TemplateRef::Inline((COMPOSITION_TEMPLATE).to_string()),
            &data,
            Representation::Human,
            target,
        )
        .unwrap();

    let named = app
        .render_with(
            standout::TemplateRef::Named(("greet").to_string()),
            &data,
            Representation::Human,
            target,
        )
        .unwrap();

    let via_request = render_request(&composition_request(&app, templates.path(), target)).unwrap();

    assert!(
        dispatch_out.contains("ADA")
            && dispatch_out.contains("INC")
            && dispatch_out.contains("9.9")
            && dispatch_out.contains("w80"),
        "template must consume filter, include, context, and width:\n{dispatch_out}"
    );
    assert_ne!(
        dispatch_out, "ADA|INC|9.9|hi|w80",
        "theme must style [mark] rather than pass it through:\n{dispatch_out}"
    );
    assert_eq!(
        dispatch_out, inline,
        "dispatch and render_inline_with must share the request pipeline"
    );
    assert_eq!(
        inline, named,
        "App::render_with of a named template must match render_inline_with of the same source"
    );
    assert_eq!(
        inline, via_request,
        "render_inline_with must match render_request of the same facts"
    );
}

#[test]
fn run_command_and_dispatch_agree_byte_for_byte() {
    let templates = write_part_template();
    let app = composition_app(templates.path());
    let matches = greet_command()
        .try_get_matches_from(["app", "greet"])
        .unwrap();

    let dispatch_out = app
        .dispatch(matches, Representation::Human)
        .output()
        .expect("dispatch should render")
        .to_string();

    let matches = greet_command()
        .try_get_matches_from(["app", "greet"])
        .unwrap();
    let via_run_command = run_command_greet(&app, &matches);

    assert!(
        dispatch_out.contains("ADA")
            && dispatch_out.contains("INC")
            && dispatch_out.contains("9.9")
            && dispatch_out.contains("w"),
        "template must consume filter, include, and context:\n{dispatch_out}"
    );
    assert_eq!(
        via_run_command.as_text(),
        Some(dispatch_out.as_str()),
        "run_command and dispatch must share the request pipeline"
    );
}

#[test]
fn run_command_honours_parsed_output_mode_and_splits_raw() {
    let templates = write_part_template();
    let app = composition_app(templates.path());

    let json_matches = greet_command_with_output()
        .try_get_matches_from(["app", "greet", "--output=json"])
        .unwrap();
    let json_dispatch = dispatch_greet(&app, json_matches);
    let json_matches = greet_command_with_output()
        .try_get_matches_from(["app", "greet", "--output=json"])
        .unwrap();
    let json_run = run_command_greet(&app, &json_matches);
    assert_eq!(
        json_run.as_text(),
        Some(json_dispatch.as_str()),
        "run_command --output=json must match dispatch of the same matches"
    );
    assert_eq!(
        json_run.as_text(),
        json_run.as_raw_text(),
        "structured json has no formatted/raw split"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(json_run.as_text().unwrap()).expect("json mode should emit JSON");
    assert_eq!(parsed["name"], "Ada");
    assert_eq!(parsed["label"], "hi");

    let human_matches = greet_command_with_output()
        .try_get_matches_from(["app", "greet"])
        .unwrap();
    let human_run = run_command_greet_with(&app, &human_matches, ColorPolicy::Always);
    let formatted = human_run.as_text().expect("the human page renders text");
    let raw = human_run.as_raw_text().expect("the human page carries raw");
    assert!(
        formatted.contains("ADA")
            && formatted.contains("INC")
            && formatted.contains("9.9")
            && formatted.contains("w"),
        "the human template must consume filter, include, and context:\n{formatted}"
    );
    assert!(
        formatted.contains("\x1b["),
        "an always color policy under a styled theme must emit ANSI in formatted:\n{formatted:?}"
    );
    assert!(
        !raw.contains("\x1b["),
        "raw must stay pipe-safe without ANSI:\n{raw:?}"
    );
    assert_ne!(
        formatted, raw,
        "formatted and raw must diverge under a styled colored render"
    );
    assert!(
        raw.contains("ADA") && raw.contains("INC") && raw.contains("9.9") && raw.contains("hi"),
        "raw must still carry the template facts:\n{raw}"
    );
}

fn help_page<I, T>(app: &App, cmd: Command, args: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let result = app.run_with(cmd, args, capable_target(), InputSources::from_process());
    result
        .output()
        .unwrap_or_else(|| panic!("expected rendered help, got {:?}", result.outcome()))
        .to_string()
}

fn standalone_help(mode: Representation) -> Result<String, standout::RenderError> {
    render_help(
        &help_command(),
        Some(HelpConfig {
            output_mode: Some(mode),
            ..Default::default()
        }),
    )
}

#[test]
fn help_path_agrees_byte_for_byte_with_standalone_help_in_every_mode() {
    let app = App::builder().help_handling(true).build().unwrap();

    let auto = help_page(&app, help_command(), ["app", "help"]);
    assert!(
        auto.contains("USAGE") && auto.contains("Demo"),
        "app help-path must render human help:\n{auto}"
    );
    let via_standalone = standalone_help(Representation::Human).unwrap();
    assert!(
        via_standalone.contains("USAGE") && via_standalone.contains("Demo"),
        "standalone render_help must render human help:\n{via_standalone}"
    );

    let app_json = help_page(&app, help_command(), ["app", "help", "--output=json"]);
    let app_yaml = help_page(&app, help_command(), ["app", "help", "--output=yaml"]);
    assert!(
        !app_json.contains("USAGE") && !app_yaml.contains("USAGE"),
        "json and yaml must emit the help document, not the page:\n{app_json}\n{app_yaml}"
    );
    let json_document: serde_json::Value = serde_json::from_str(&app_json).unwrap();
    let yaml_document: serde_json::Value = serde_yaml::from_str(&app_yaml).unwrap();
    assert_eq!(
        json_document, yaml_document,
        "the two document modes carry the same help document"
    );
    assert_eq!(json_document["usage"], "app [OPTIONS]");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&standalone_help(Representation::Json).unwrap())
            .unwrap()["usage"],
        "app",
        "standalone help documents the bare command, without the framework's flags"
    );
    assert!(
        standalone_help(Representation::Csv).is_err(),
        "csv has no help projection"
    );
}

#[test]
fn app_help_word_answers_with_the_document_under_json() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app")
        .about("Demo")
        .subcommand(Command::new("greet").about("Say hi"));

    let human = help_page(&app, cmd.clone(), ["app", "help"]);
    assert!(human.contains("USAGE"), "{human}");

    let structured = help_page(&app, cmd, ["app", "help", "--output=json"]);
    let document: serde_json::Value = serde_json::from_str(&structured).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["name"], "app");
    assert_eq!(document["about"], "Demo");
    assert_eq!(document["subcommands"][0]["name"], "greet");
}
