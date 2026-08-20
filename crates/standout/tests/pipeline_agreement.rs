//! Dispatch, `render_inline`, and help-path rendering of the same request agree.
//!
//! Help stays human under structured `--output` (ADR-0029): glue maps those
//! modes to `Auto` on the help/topics request.

use clap::Command;
use serde_json::json;
use standout::cli::{render_help, App, HelpConfig, HelpResult, Output};
use standout::{
    render_request, AmbiguousWidth, ColorMode, ColorPolicy, IconMode, InputSources, OutputMode,
    RenderRequest, TargetProperties, TemplateRef, Theme,
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

fn greet_command() -> Command {
    Command::new("app").subcommand(Command::new("greet"))
}

#[test]
fn dispatch_and_render_inline_agree_with_render_request() {
    let template = "hello {{ name }}";
    let data = json!({"name": "Ada"});
    let theme = Theme::new();
    let app = App::builder()
        .theme(theme.clone())
        .command(
            "greet",
            {
                let data = data.clone();
                move |_m, _ctx| Ok(Output::Render(data.clone()))
            },
            template,
        )
        .unwrap()
        .build()
        .unwrap();

    let dispatched = app.run_with(
        greet_command(),
        ["app", "greet", "--output=text"],
        capable_target(),
        InputSources::from_process(),
    );
    let dispatch_out = dispatched
        .output()
        .expect("dispatch should render")
        .to_string();

    let inline = app
        .render_inline(template, &data, OutputMode::Text)
        .unwrap();

    let request = RenderRequest {
        data: data.clone(),
        template: TemplateRef::Inline(template.to_string()),
        theme: app.get_default_theme().cloned().unwrap_or(theme),
        format: OutputMode::Text,
        color_policy: ColorPolicy::Auto,
        target: capable_target(),
        engine: standout::default_template_engine(),
        registry: None,
        context_registry: None,
        csv_projection: None,
        extras: Default::default(),
        warnings: None,
    };
    let via_request = render_request(&request).unwrap();

    assert_eq!(
        dispatch_out, inline,
        "dispatch and render_inline must share the request pipeline"
    );
    assert_eq!(
        inline, via_request,
        "render_inline must match render_request of the same facts"
    );
}

#[test]
fn help_path_agrees_with_standalone_render_help_and_stays_human() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app").about("Demo");

    let via_app = match app.get_matches_from(cmd.clone(), ["app", "--help"]) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got {other:?}"),
    };

    let via_standalone = render_help(
        &cmd,
        Some(HelpConfig {
            output_mode: Some(OutputMode::Text),
            ..Default::default()
        }),
    )
    .unwrap();

    assert!(
        via_app.contains("USAGE") && via_app.contains("Demo"),
        "app help-path must render human help:\n{via_app}"
    );
    assert!(
        via_standalone.contains("USAGE") && via_standalone.contains("Demo"),
        "standalone render_help must render human help:\n{via_standalone}"
    );

    for mode in [
        OutputMode::Json,
        OutputMode::Yaml,
        OutputMode::Csv,
        OutputMode::Xml,
    ] {
        let structured = render_help(
            &cmd,
            Some(HelpConfig {
                output_mode: Some(mode),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(
            structured.contains("USAGE") && structured.contains("Demo"),
            "{mode:?} help must stay human, got:\n{structured}"
        );
        assert!(
            !structured.trim_start().starts_with('{'),
            "{mode:?} must not emit a JSON help document:\n{structured}"
        );
    }
}

#[test]
fn app_help_word_stays_human_under_structured_output() {
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("app")
        .about("Demo")
        .subcommand(Command::new("greet"));

    let human = match app.get_matches_from(cmd.clone(), ["app", "help"]) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got {other:?}"),
    };
    assert!(
        human.contains("USAGE") || human.contains("Demo") || human.contains("greet"),
        "help word must be human:\n{human}"
    );

    let structured = app.run_with(
        cmd,
        ["app", "help", "--output=json"],
        capable_target(),
        InputSources::from_process(),
    );
    let page = structured
        .output()
        .expect("help --output=json should render");
    assert!(
        page.contains("USAGE") || page.contains("Demo") || page.contains("greet"),
        "help --output=json must stay human, got:\n{page}"
    );
    assert!(
        !page.trim_start().starts_with('{'),
        "help --output=json must not emit a JSON document:\n{page}"
    );
}
