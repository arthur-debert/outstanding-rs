//! Dispatch, `render_inline_with`, and `render_request` of the same facts agree.
//!
//! Help stays human under structured `--output` (ADR-0029): glue maps those
//! modes to `Auto` on the help/topics request. Structured help pages of the
//! same invocation facts are compared byte-for-byte.

use std::cell::RefCell;
use std::rc::Rc;

use clap::Command;
use console::Style;
use minijinja::Value;
use serde_json::json;
use standout::cli::{render_help, App, HelpConfig, Output};
use standout::context::{ContextRegistry, RenderContext};
use standout::{
    render_request, AmbiguousWidth, ColorMode, ColorPolicy, IconMode, InputSources,
    MiniJinjaEngine, OutputMode, RenderRequest, SharedTemplateEngine, TargetProperties,
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
        .command(
            "greet",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada", "label": "hi"}))),
            COMPOSITION_TEMPLATE,
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
        theme: app
            .get_default_theme()
            .cloned()
            .expect("build merged a theme"),
        format: OutputMode::Term,
        color_policy: ColorPolicy::Auto,
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
    dir
}

#[test]
fn dispatch_render_inline_and_render_request_agree_byte_for_byte() {
    let templates = write_part_template();
    let app = composition_app(templates.path());
    let target = capable_target();
    let data = json!({"name": "Ada", "label": "hi"});

    let dispatched = app.run_with(
        greet_command(),
        ["app", "greet", "--output=term"],
        target,
        InputSources::from_process(),
    );
    let dispatch_out = dispatched
        .output()
        .expect("dispatch should render")
        .to_string();

    let inline = app
        .render_inline_with(COMPOSITION_TEMPLATE, &data, OutputMode::Term, target)
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
        inline, via_request,
        "render_inline_with must match render_request of the same facts"
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

#[test]
fn help_path_agrees_byte_for_byte_and_stays_human_under_structured_output() {
    let app = App::builder().help_handling(true).build().unwrap();

    let auto = help_page(&app, help_command(), ["app", "help"]);
    assert!(
        auto.contains("USAGE") && auto.contains("Demo"),
        "app help-path must render human help:\n{auto}"
    );

    for mode in ["json", "yaml", "csv", "xml"] {
        let structured = help_page(
            &app,
            help_command(),
            vec!["app".into(), "help".into(), format!("--output={mode}")],
        );
        assert_eq!(
            auto, structured,
            "help --output={mode} must equal Auto help (ADR-0029 maps structured modes to Auto)"
        );
        assert!(
            !structured.trim_start().starts_with('{'),
            "{mode} must not emit a JSON help document:\n{structured}"
        );
    }

    let via_standalone = render_help(
        &help_command(),
        Some(HelpConfig {
            output_mode: Some(OutputMode::Text),
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(
        via_standalone.contains("USAGE") && via_standalone.contains("Demo"),
        "standalone render_help must render human help:\n{via_standalone}"
    );

    let mut standalone_pages = Vec::new();
    for mode in [
        OutputMode::Json,
        OutputMode::Yaml,
        OutputMode::Csv,
        OutputMode::Xml,
    ] {
        let structured = render_help(
            &help_command(),
            Some(HelpConfig {
                output_mode: Some(mode),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(
            !structured.trim_start().starts_with('{'),
            "{mode:?} must not emit a JSON help document:\n{structured}"
        );
        standalone_pages.push((mode, structured));
    }
    let first = &standalone_pages[0].1;
    for (mode, page) in &standalone_pages[1..] {
        assert_eq!(
            first, page,
            "standalone {mode:?} help must equal {:?} help",
            standalone_pages[0].0
        );
    }
}

#[test]
fn app_help_word_stays_human_under_structured_output() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app")
        .about("Demo")
        .subcommand(Command::new("greet"));

    let human = help_page(&app, cmd.clone(), ["app", "help"]);
    let structured = help_page(&app, cmd, ["app", "help", "--output=json"]);
    assert_eq!(
        human, structured,
        "help word under --output=json must equal Auto help"
    );
    assert!(
        !structured.trim_start().starts_with('{'),
        "help --output=json must not emit a JSON document:\n{structured}"
    );
}
