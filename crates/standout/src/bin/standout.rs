use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use minijinja::{context, Environment};

#[derive(Parser)]
#[command(name = "standout", about = "Standout project tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the smallest runnable Standout workspace.
    NewProject,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::NewProject => {
            let answers = prompt_answers(&mut io::stdin().lock(), &mut io::stdout())?;
            let spec = ProjectSpec::from_answers(answers)?;
            write_review(&spec, &mut io::stdout())?;
            if !confirm(&mut io::stdin().lock(), &mut io::stdout())? {
                println!("Generation cancelled.");
                return Ok(());
            }
            publish_project(&spec)?;
            println!("Created {}", spec.destination.display());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WizardAnswers {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    input_name: String,
    result_shape: ResultShape,
    record_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultShape {
    Message,
    Record,
}

#[derive(Debug, Clone)]
struct ProjectSpec {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    input_name: String,
    result_shape: ResultShape,
    record_fields: Vec<String>,
    lib_crate: String,
    operation_name: String,
    view_name: String,
    destination: PathBuf,
    standout_version: String,
    local_patch_root: Option<PathBuf>,
}

impl ProjectSpec {
    fn from_answers(answers: WizardAnswers) -> Result<Self> {
        validate_crate_name(&answers.project_name, "project name")?;
        validate_crate_name(&answers.executable_name, "executable name")?;
        validate_ident(&answers.command_name.replace('-', "_"), "command name")?;
        validate_ident(&answers.input_name, "input name")?;
        validate_result_fields(answers.result_shape, &answers.record_fields)?;
        if answers.command_description.trim().is_empty() {
            bail!("command description cannot be empty");
        }

        let lib_crate = format!("{}lib", answers.project_name.replace('-', "_"));
        let command_ident = answers.command_name.replace('-', "_");
        let operation_name = format!("process_{command_ident}");
        let view_name = format!("{}View", pascal_case(&command_ident));
        Ok(Self {
            destination: PathBuf::from(&answers.project_name),
            project_name: answers.project_name,
            executable_name: answers.executable_name,
            command_name: answers.command_name,
            command_description: answers.command_description,
            input_name: answers.input_name,
            result_shape: answers.result_shape,
            record_fields: answers.record_fields,
            lib_crate,
            operation_name,
            view_name,
            standout_version: env!("CARGO_PKG_VERSION").to_string(),
            local_patch_root: None,
        })
    }

    fn tree(&self) -> Vec<String> {
        vec![
            "Cargo.toml".into(),
            format!("crates/{}/Cargo.toml", self.lib_crate),
            format!("crates/{}/src/lib.rs", self.lib_crate),
            format!("crates/{}/Cargo.toml", self.executable_name),
            format!("crates/{}/README.md", self.executable_name),
            format!("crates/{}/src/main.rs", self.executable_name),
            format!("crates/{}/src/cli.rs", self.executable_name),
            format!("crates/{}/src/handlers.rs", self.executable_name),
            format!(
                "crates/{}/src/templates/{}.jinja",
                self.executable_name, self.command_name
            ),
            format!(
                "crates/{}/src/styles/{}.css",
                self.executable_name, self.project_name
            ),
        ]
    }
}

impl ResultShape {
    fn as_str(self) -> &'static str {
        match self {
            ResultShape::Message => "message",
            ResultShape::Record => "record",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedFiles {
    files: BTreeMap<PathBuf, String>,
}

impl GeneratedFiles {
    fn render(spec: &ProjectSpec) -> Result<Self> {
        let mut env = Environment::new();
        for (name, source) in TEMPLATE_CATALOG {
            env.add_template(name, source)
                .with_context(|| format!("template {name} is malformed"))?;
        }

        let mut files = BTreeMap::new();
        for (path_template, template_name) in FILE_MAP {
            let path = render_inline(path_template, spec)?;
            let mut body = env
                .get_template(template_name)?
                .render(model(spec))
                .with_context(|| format!("template {template_name} is missing model data"))?;
            if !body.ends_with('\n') {
                body.push('\n');
            }
            files.insert(PathBuf::from(path), body);
        }
        Ok(Self { files })
    }
}

fn prompt_answers(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<WizardAnswers> {
    let project_name = prompt(input, output, "Project name")?;
    validate_crate_name(&project_name, "project name")?;

    let executable_name = prompt_default(input, output, "Executable name", &project_name)?;
    validate_crate_name(&executable_name, "executable name")?;

    let command_name = prompt(input, output, "Initial command name")?;
    validate_ident(&command_name.replace('-', "_"), "command name")?;

    let command_description = prompt(input, output, "One-sentence command description")?;
    if command_description.trim().is_empty() {
        bail!("command description cannot be empty");
    }

    let input_name = prompt(input, output, "Required string input name")?;
    validate_ident(&input_name, "input name")?;

    let result_shape = prompt_result_shape(input, output)?;
    let record_fields = match result_shape {
        ResultShape::Message => Vec::new(),
        ResultShape::Record => prompt_record_fields(input, output)?,
    };
    validate_result_fields(result_shape, &record_fields)?;

    Ok(WizardAnswers {
        project_name,
        executable_name,
        command_name,
        command_description,
        input_name,
        result_shape,
        record_fields,
    })
}

fn prompt(input: &mut dyn BufRead, output: &mut dyn Write, label: &str) -> Result<String> {
    write!(output, "{label}: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let value = line.trim().to_string();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value)
}

fn prompt_default(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
    default: &str,
) -> Result<String> {
    write!(output, "{label} [{default}]: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let value = line.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn confirm(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<bool> {
    write!(output, "Generate this project? Type 'yes' to continue: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line.trim() == "yes")
}

fn prompt_result_shape(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<ResultShape> {
    let value = prompt_default(input, output, "Result shape (message/record)", "record")?;
    match value.as_str() {
        "message" => Ok(ResultShape::Message),
        "record" => Ok(ResultShape::Record),
        _ => bail!("result shape must be message or record"),
    }
}

fn prompt_record_fields(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<Vec<String>> {
    let value = prompt_default(
        input,
        output,
        "Record fields (comma-separated)",
        "summary,count",
    )?;
    parse_record_fields(&value)
}

fn write_review(spec: &ProjectSpec, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "\nReview")?;
    writeln!(output, "Destination: {}", spec.destination.display())?;
    writeln!(output, "Generated tree:")?;
    for path in spec.tree() {
        writeln!(output, "  {path}")?;
    }
    writeln!(
        output,
        "Command syntax: {} {} <{}>",
        spec.executable_name, spec.command_name, spec.input_name
    )?;
    writeln!(
        output,
        "Input policy: {} is required and comes from the positional argument, then piped stdin.",
        spec.input_name
    )?;
    writeln!(
        output,
        "Core operation: {}::{}({}: String)",
        spec.lib_crate, spec.operation_name, spec.input_name
    )?;
    writeln!(
        output,
        "Output shape: {} {} renders human output and serializes as JSON.",
        spec.result_shape.as_str(),
        spec.view_name
    )?;
    writeln!(
        output,
        "Generated tests: core validation, typed handler mapping, TestHarness pipeline."
    )?;
    Ok(())
}

fn publish_project(spec: &ProjectSpec) -> Result<()> {
    if spec.destination.exists() {
        if !spec.destination.is_dir() {
            bail!(
                "destination {} already exists and is not a directory",
                spec.destination.display()
            );
        }
        if fs::read_dir(&spec.destination)?.next().is_some() {
            bail!(
                "destination {} already exists and is not empty",
                spec.destination.display()
            );
        }
    }

    let generated = GeneratedFiles::render(spec)?;
    let parent = spec.destination.parent().unwrap_or_else(|| Path::new("."));
    let name = spec
        .destination
        .file_name()
        .ok_or_else(|| anyhow!("destination must have a final path component"))?;
    let staging = parent.join(format!(
        ".{}.standout-new-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    if let Err(error) = write_generated_files(&staging, &generated) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if spec.destination.exists() {
        fs::remove_dir(&spec.destination).with_context(|| {
            format!(
                "destination {} must be empty before publish",
                spec.destination.display()
            )
        })?;
    }

    match fs::rename(&staging, &spec.destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error).with_context(|| {
                format!(
                    "failed to publish staged project to {}",
                    spec.destination.display()
                )
            })
        }
    }
}

fn write_generated_files(root: &Path, generated: &GeneratedFiles) -> Result<()> {
    for (path, body) in &generated.files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, body)?;
    }
    Ok(())
}

fn validate_crate_name(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        bail!("{label} may only contain letters, numbers, underscores, or hyphens");
    }
    Ok(())
}

fn validate_ident(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        bail!("{label} may only contain letters, numbers, or underscores");
    }
    if is_rust_keyword(value) {
        bail!("{label} cannot be a reserved Rust keyword");
    }
    Ok(())
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn parse_record_fields(value: &str) -> Result<Vec<String>> {
    let fields: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    validate_result_fields(ResultShape::Record, &fields)?;
    Ok(fields)
}

fn validate_result_fields(shape: ResultShape, fields: &[String]) -> Result<()> {
    match shape {
        ResultShape::Message => {
            if !fields.is_empty() {
                bail!("message results cannot declare record fields");
            }
        }
        ResultShape::Record => {
            if fields.is_empty() {
                bail!("record results must declare at least one field");
            }
            let mut seen = std::collections::BTreeSet::new();
            for field in fields {
                validate_ident(field, "record field")?;
                if !seen.insert(field) {
                    bail!("record field {field} is declared more than once");
                }
            }
        }
    }
    Ok(())
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    Environment::new()
        .template_from_str(template)?
        .render(model(spec))
        .with_context(|| format!("path template {template} is missing model data"))
}

fn model(spec: &ProjectSpec) -> minijinja::Value {
    context! {
        project_name => spec.project_name,
        executable_name => spec.executable_name,
        command_name => spec.command_name,
        command_ident => spec.command_name.replace('-', "_"),
        command_variant => pascal_case(&spec.command_name.replace('-', "_")),
        command_description => spec.command_description,
        input_name => spec.input_name,
        lib_crate => spec.lib_crate,
        lib_package => spec.lib_crate.replace('_', "-"),
        operation_name => spec.operation_name,
        view_name => spec.view_name,
        result_shape => spec.result_shape.as_str(),
        core_result_fields => core_result_fields(spec),
        core_result_init => core_result_init(spec),
        core_valid_assertions => core_valid_assertions(spec),
        cli_view_fields => cli_view_fields(spec),
        cli_view_from_fields => cli_view_from_fields(spec),
        handler_expected_fields => handler_expected_fields(spec),
        template_body => template_body(spec),
        human_assertions => human_assertions(spec),
        json_assertions => json_assertions(spec),
        standout_version => spec.standout_version,
        local_patch_root => spec.local_patch_root.as_ref().map(|path| path.display().to_string()),
    }
}

fn core_result_fields(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "    pub message: String,".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| format!("    pub {field}: String,"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_result_init(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "        message: format!(\"Processed {normalized}\"),".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        summary: format!(\"Processed {normalized}\"),".into(),
                "count" | "length" => {
                    format!("        {field}: normalized.chars().count().to_string(),")
                }
                other => format!("        {other}: normalized.clone(),"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_valid_assertions(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => {
            "        assert_eq!(result.message, \"Processed Standout\");".into()
        }
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        assert_eq!(result.summary, \"Processed Standout\");".into(),
                "count" | "length" => format!("        assert_eq!(result.{field}, \"8\");"),
                other => format!("        assert_eq!(result.{other}, \"Standout\");"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn cli_view_fields(spec: &ProjectSpec) -> String {
    core_result_fields(spec).replace("pub ", "pub(crate) ")
}

fn cli_view_from_fields(spec: &ProjectSpec) -> String {
    let fields: Vec<_> = match spec.result_shape {
        ResultShape::Message => vec!["message".to_string()],
        ResultShape::Record => spec.record_fields.clone(),
    };
    fields
        .into_iter()
        .map(|field| format!("            {field}: value.{field},"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn handler_expected_fields(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "                message: \"Processed hello\".into(),".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "                summary: \"Processed hello\".into(),".into(),
                "count" | "length" => format!("                {field}: \"5\".into(),"),
                other => format!("                {other}: \"hello\".into(),"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn template_body(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "[title]{{ message }}[/title]\n".into(),
        ResultShape::Record => {
            let mut lines = vec![format!(
                "[title]{{{{ {} }}}}[/title]",
                spec.record_fields[0]
            )];
            for field in &spec.record_fields {
                lines.push(format!("{}: {{{{ {} }}}}", pascal_case(field), field));
            }
            lines.join("\n") + "\n"
        }
    }
}

fn human_assertions(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "        result.assert_stdout_contains(\"Processed Ada\");".into(),
        ResultShape::Record => {
            let field = spec
                .record_fields
                .first()
                .expect("record fields are validated");
            let expected = match field.as_str() {
                "summary" => "Processed Ada",
                "count" | "length" => "3",
                _ => "Ada",
            };
            format!("        result.assert_stdout_contains(\"{expected}\");")
        }
    }
}

fn json_assertions(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => {
            "        assert_eq!(value[\"message\"], \"Processed Grace\");".into()
        }
        ResultShape::Record => {
            let field = spec
                .record_fields
                .first()
                .expect("record fields are validated");
            let expected = match field.as_str() {
                "summary" => "Processed Grace",
                "count" | "length" => "5",
                _ => "Grace",
            };
            format!("        assert_eq!(value[\"{field}\"], \"{expected}\");")
        }
    }
}

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
    ("crates/{{ executable_name }}/README.md", "readme"),
    ("crates/{{ executable_name }}/src/main.rs", "main"),
    ("crates/{{ executable_name }}/src/cli.rs", "cli"),
    ("crates/{{ executable_name }}/src/handlers.rs", "handlers"),
    (
        "crates/{{ executable_name }}/src/templates/{{ command_name }}.jinja",
        "template",
    ),
    (
        "crates/{{ executable_name }}/src/styles/{{ project_name }}.css",
        "style",
    ),
];

const TEMPLATE_CATALOG: &[(&str, &str)] = &[
    (
        "workspace",
        r#"[workspace]
resolver = "2"
members = [
    "crates/{{ lib_crate }}",
    "crates/{{ executable_name }}",
]

{%- if local_patch_root %}
[patch.crates-io]
standout = { path = "{{ local_patch_root }}/crates/standout" }
standout-test = { path = "{{ local_patch_root }}/crates/standout-test" }
standout-dispatch = { path = "{{ local_patch_root }}/crates/standout-dispatch" }
standout-input = { path = "{{ local_patch_root }}/crates/standout-input" }
{%- endif %}
"#,
    ),
    (
        "core_manifest",
        r#"[package]
name = "{{ lib_package }}"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"
"#,
    ),
    (
        "core_lib",
        r#"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct {{ view_name }} {
{{ core_result_fields }}
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("{{ input_name }} cannot be empty")]
    EmptyInput,
}

/// Runs the CLI-free core operation for the generated command.
///
/// The caller supplies explicit values. This crate deliberately has no Clap,
/// Standout, template, terminal, environment, or CLI-view dependencies.
pub fn {{ operation_name }}({{ input_name }}: impl Into<String>) -> Result<{{ view_name }}, CoreError> {
    let normalized = {{ input_name }}.into().trim().to_string();
    if normalized.is_empty() {
        return Err(CoreError::EmptyInput);
    }
    Ok({{ view_name }} {
{{ core_result_init }}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_returns_a_typed_result() {
        let result = {{ operation_name }}("  Standout  ").unwrap();

{{ core_valid_assertions }}
    }

    #[test]
    fn blank_input_is_rejected_by_the_core() {
        assert_eq!({{ operation_name }}("   "), Err(CoreError::EmptyInput));
    }
}
"#,
    ),
    (
        "cli_manifest",
        r#"[package]
name = "{{ executable_name }}"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
standout = "{{ standout_version }}"
standout-dispatch = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
standout-input = "{{ standout_version }}"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod handlers;

use anyhow::Result;
use standout::input::{ArgSource, InputChain, StdinSource};
use standout::{embed_styles, embed_templates};

fn main() -> Result<()> {
    let app = build_app()?;
    app.run(cli::command(), std::env::args());
    Ok(())
}

fn build_app() -> Result<standout::cli::App> {
    Ok(standout::cli::App::builder()
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("{{ project_name }}")
        .command_with("{{ command_name }}", handlers::{{ command_ident }}__handler, |config| {
            config.template("{{ command_name }}.jinja").input(
                "{{ input_name }}",
                InputChain::<String>::new()
                    .try_source(ArgSource::new("{{ input_name }}"))
                    .try_source(StdinSource::new())
                    .validate(|value| !value.trim().is_empty(), "{{ input_name }} cannot be empty"),
            )
        })?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serial_test::serial;
    use standout_test::TestHarness;

    #[test]
    #[serial]
    fn pipeline_renders_human_output_from_argument() {
        let app = build_app().unwrap();

        let result =
            TestHarness::new()
                .no_color()
                .run(&app, cli::command(), ["{{ executable_name }}", "{{ command_name }}", "Ada"]);

        result.assert_success();
{{ human_assertions }}
    }

    #[test]
    #[serial]
    fn pipeline_reads_piped_stdin_and_serializes_json() {
        let app = build_app().unwrap();

        let result = TestHarness::new().no_color().piped_stdin("Grace\n").run(
            &app,
            cli::command(),
            ["{{ executable_name }}", "{{ command_name }}", "--output", "json"],
        );

        result.assert_success();
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
{{ json_assertions }}
    }
}
"#,
    ),
    (
        "readme",
        r#"# {{ executable_name }}

This generated project is a Standout architecture starter, not a finished
application. The reusable `{{ lib_crate }}` crate owns the CLI-free operation
and validation. This binary crate owns Clap declarations, Standout wiring,
input policy, handlers, serializable view types, templates, styles, and process
execution.

Run the generated command:

```sh
cargo run -p {{ executable_name }} -- {{ command_name }} VALUE
cargo run -p {{ executable_name }} -- {{ command_name }} VALUE --output json
```

`{{ input_name }}` is resolved from the optional positional argument first, then
from piped stdin, and blank values are rejected before the core operation runs.

The generated `{{ result_shape }}` result is intentionally small. The handler
maps resolved shell input into `{{ lib_crate }}::{{ operation_name }}` and maps
the core result into the CLI-owned `{{ view_name }}`. Human output renders
through `src/templates/{{ command_name }}.jinja` and `src/styles/{{ project_name }}.css`;
structured output serializes the same view directly.

Verify the project with:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
```
"#,
    ),
    (
        "cli",
        r#"use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "{{ executable_name }}", about = "{{ command_description }}")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// {{ command_description }}
    {{ command_variant }} {
        #[arg(required = false)]
        {{ input_name }}: Option<String>,
    },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
"#,
    ),
    (
        "handlers",
        r#"#![allow(non_snake_case)]

use {{ lib_crate }} as core;
use serde::Serialize;
use standout::cli::{CommandContext, CommandContextInput, Output};
use standout::handler;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct {{ view_name }} {
{{ cli_view_fields }}
}

impl From<core::{{ view_name }}> for {{ view_name }} {
    fn from(value: core::{{ view_name }}) -> Self {
        Self {
{{ cli_view_from_fields }}
        }
    }
}

/// Adapts resolved shell input into the CLI-free core operation.
///
/// The handler owns the CLI view type and returns data for Standout to render
/// or serialize; it does not print, template, or read environment state.
#[handler]
pub(crate) fn {{ command_ident }}(#[ctx] ctx: &CommandContext) -> Result<Output<{{ view_name }}>, anyhow::Error> {
    let value: &String = ctx.input("{{ input_name }}")?;
    let result = core::{{ operation_name }}(value.clone())?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use standout_dispatch::Extensions;
    use standout_input::{InputSourceKind, Inputs, ResolvedInput};
    use std::rc::Rc;

    fn context_with_input(value: &str) -> CommandContext {
        let mut ctx = CommandContext::new(Vec::new(), Rc::new(Extensions::new()));
        let mut inputs = Inputs::new();
        inputs.insert(
            "{{ input_name }}",
            ResolvedInput {
                value: value.to_string(),
                source: InputSourceKind::Arg,
            },
        );
        ctx.extensions.insert(inputs);
        ctx
    }

    #[test]
    fn typed_handler_maps_input_to_core_and_view() {
        let ctx = context_with_input("  hello  ");

        let Output::Render(view) = {{ command_ident }}(&ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(
            view,
            {{ view_name }} {
{{ handler_expected_fields }}
            }
        );
    }
}
"#,
    ),
    ("template", r#"{{ template_body }}"#),
    (
        "style",
        r#".title {
  font-weight: bold;
  color: cyan;
}
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn sample_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "hello-tool".into(),
            executable_name: "hello-tool".into(),
            command_name: "greet".into(),
            command_description: "Greet one value".into(),
            input_name: "name".into(),
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();
        spec.destination = root.join("hello-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("standout crate lives under crates/standout in the repository")
            .to_path_buf()
    }

    #[test]
    fn answers_validate_before_rendering() {
        let mut input = io::Cursor::new("1bad\n");
        let mut output = Vec::new();

        let error = prompt_answers(&mut input, &mut output).unwrap_err();

        assert!(error.to_string().contains("project name must start"));
    }

    #[test]
    fn project_spec_is_private_validated_model() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            input_name: "document".into(),
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        assert_eq!(spec.lib_crate, "demolib");
        assert_eq!(spec.operation_name, "process_inspect");
        assert_eq!(spec.view_name, "InspectView");
        assert_eq!(spec.result_shape, ResultShape::Message);
    }

    #[test]
    fn record_result_fields_are_validated_before_rendering() {
        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            input_name: "document".into(),
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "summary".into()],
        })
        .unwrap_err();

        assert!(error.to_string().contains("declared more than once"));
    }

    #[test]
    fn record_result_fields_reject_reserved_rust_keywords() {
        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            input_name: "document".into(),
            result_shape: ResultShape::Record,
            record_fields: vec!["type".into()],
        })
        .unwrap_err();

        assert!(error.to_string().contains("reserved Rust keyword"));
    }

    #[test]
    fn generated_command_and_input_identifiers_reject_reserved_rust_keywords() {
        let command_error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "match".into(),
            command_description: "Inspect one value".into(),
            input_name: "document".into(),
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();
        let input_error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            input_name: "pub".into(),
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(command_error.to_string().contains("reserved Rust keyword"));
        assert!(input_error.to_string().contains("reserved Rust keyword"));
    }

    #[test]
    fn render_builds_files_in_memory_before_publish() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());

        let generated = GeneratedFiles::render(&spec).unwrap();

        assert!(generated.files.contains_key(Path::new("Cargo.toml")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello_toollib/src/lib.rs")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello-tool/README.md")));
        assert!(!spec.destination.exists());
    }

    #[test]
    fn render_omits_local_patch_paths_by_default() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            input_name: "document".into(),
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(!manifest.contains("[patch.crates-io]"));
        assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn publish_refuses_non_empty_destination_without_partial_staging() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::create_dir_all(&spec.destination).unwrap();
        fs::write(spec.destination.join("keep.txt"), "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not empty"));
        assert_eq!(
            fs::read_to_string(spec.destination.join("keep.txt")).unwrap(),
            "existing"
        );
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn publish_refuses_file_destination_with_clear_error() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::write(&spec.destination, "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not a directory"));
        assert_eq!(fs::read_to_string(&spec.destination).unwrap(), "existing");
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn declined_confirmation_is_a_non_mutating_cancel() {
        let mut input = io::Cursor::new("no\n");
        let mut output = Vec::new();

        assert!(!confirm(&mut input, &mut output).unwrap());
    }

    #[test]
    fn generated_project_formats_checks_tests_and_runs() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        publish_project(&spec).unwrap();

        run_cargo(&spec.destination, ["fmt", "--check"]);
        run_cargo(&spec.destination, ["check", "--workspace"]);
        run_cargo(&spec.destination, ["test", "--workspace"]);

        let human = Command::new("cargo")
            .current_dir(&spec.destination)
            .args(["run", "-q", "-p", "hello-tool", "--", "greet", "Ada"])
            .output()
            .unwrap();
        assert!(
            human.status.success(),
            "human run failed: {}",
            String::from_utf8_lossy(&human.stderr)
        );
        let stdout = String::from_utf8(human.stdout).unwrap();
        assert!(stdout.contains("Ada"));
        assert!(stdout.contains("Count: 3"));

        let json = Command::new("cargo")
            .current_dir(&spec.destination)
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "Ada",
                "--output",
                "json",
            ])
            .output()
            .unwrap();
        assert!(
            json.status.success(),
            "json run failed: {}",
            String::from_utf8_lossy(&json.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(value["summary"], "Processed Ada");
        assert_eq!(value["count"], "3");

        let invalid = Command::new("cargo")
            .current_dir(&spec.destination)
            .args(["run", "-q", "-p", "hello-tool", "--", "greet", "   "])
            .output()
            .unwrap();
        assert!(!invalid.status.success());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("name cannot be empty"));
    }

    fn run_cargo<const N: usize>(cwd: &Path, args: [&str; N]) {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
