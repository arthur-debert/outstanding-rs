//! `preserve_order` (crates/standout/Cargo.toml) makes JSON, YAML, and XML
//! emit a handler's declared field order rather than sorting keys
//! alphabetically — see docs/topics/output-modes.md "Key ordering". The
//! order comes from serde's field-serialization order, so this asserts it
//! against a `#[derive(Serialize)]` struct dispatched through `App`, not
//! just a `json!` literal.

use clap::Command;
use serde::Serialize;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::EmbeddedTemplates;
use standout::{AmbiguousWidth, ColorMode, IconMode, InputSources, OutputMode, TargetProperties};

const TEMPLATES: &[(&str, &str)] = &[("info", "unused")];

// Declared out of alphabetical order (alphabetical would be
// machine_type, name, status, zone) so a sort would be visible.
#[derive(Serialize)]
struct Instance {
    name: &'static str,
    zone: &'static str,
    machine_type: &'static str,
    status: &'static str,
}

fn dispatch(mode: OutputMode) -> String {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "info",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Render(serde_json::to_value(Instance {
                    name: "web-1",
                    zone: "us-east1-b",
                    machine_type: "n2-standard-2",
                    status: "RUNNING",
                })?))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("info"));
    let flag = match mode {
        OutputMode::Json => "--output=json",
        OutputMode::Yaml => "--output=yaml",
        OutputMode::Xml => "--output=xml",
        _ => unreachable!("test only dispatches structured modes"),
    };
    let target = TargetProperties {
        width: None,
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    };
    let result = app.run_with(
        cmd,
        ["app", "info", flag],
        target,
        InputSources::from_process(),
    );
    result.output().unwrap().to_string()
}

fn assert_ascending(output: &str, needles: &[&str]) {
    let positions: Vec<usize> = needles
        .iter()
        .map(|n| {
            output
                .find(n)
                .unwrap_or_else(|| panic!("missing {n:?} in {output}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "expected {needles:?} in ascending order, got {output}"
    );
}

#[test]
fn json_struct_fields_keep_declaration_order() {
    let json = dispatch(OutputMode::Json);
    assert_ascending(
        &json,
        &["\"name\"", "\"zone\"", "\"machine_type\"", "\"status\""],
    );
}

#[test]
fn yaml_struct_fields_keep_declaration_order() {
    let yaml = dispatch(OutputMode::Yaml);
    assert_ascending(&yaml, &["name:", "zone:", "machine_type:", "status:"]);
}

#[test]
fn xml_struct_fields_keep_declaration_order() {
    let xml = dispatch(OutputMode::Xml);
    assert_ascending(&xml, &["<name>", "<zone>", "<machine_type>", "<status>"]);
}
