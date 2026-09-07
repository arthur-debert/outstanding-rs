use clap::Command;
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{
    App, Artifact, CommandContextInput, ExitStatus, ExternalFailure, HandlerResult, HookError,
    Hooks, Output, OutputKind, RunErrorKind, SuccessKind,
};
use standout::tabular::{Column, Width};
use standout::views::list_view;
use standout::EmbeddedTemplates;
use standout::{
    ColorMode, ColorPolicy, CsvProjection, IconDefinition, IconMode, StructuredOutputProjection,
    Theme,
};
use standout_input::{ClipboardSource, EnvSource, InputChain, StdinSource};
use standout_render::{AmbiguousWidth, Representation};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[
    ("say", "[tone]{{ icons.mark }}[/tone]"),
    ("echo", "{{ msg }}"),
    ("external-pre", "{{ message }}"),
    ("whoami", "{{ user }}"),
    ("tok", "{{ tok }}"),
    ("read", "{{ val }}"),
    ("paste", "{{ val }}"),
    ("wizard", "{{ name }}/{{ proceed }}/{{ title }}"),
    ("wizard-2", "{{ body }}"),
    ("cat", "{{ text }}"),
    ("echo", "{{ msg }}"),
    ("echo-width", "{{ msg | display_width }}"),
    (
        "export",
        "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
    ),
];
fn build_echo_app(template_name: &'static str) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "echo",
            FnHandler::new(|m: &clap::ArgMatches, _ctx: &_| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            }),
            |cfg| cfg.template_name(template_name),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn echo_command() -> Command {
    Command::new("app")
        .subcommand(Command::new("echo").arg(clap::Arg::new("msg").required(false).index(1)))
}

#[path = "harness/environment.rs"]
mod environment;
#[path = "harness/input_sources.rs"]
mod input_sources;
#[path = "harness/outcomes.rs"]
mod outcomes;
#[path = "harness/rendering.rs"]
mod rendering;
#[path = "harness/target_properties.rs"]
mod target_properties;
