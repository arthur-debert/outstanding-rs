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
    inputs: Vec<CommandInput>,
}

#[derive(Debug, Clone)]
struct ProjectSpec {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    lib_crate: String,
    operation_name: String,
    view_name: String,
    destination: PathBuf,
    standout_version: String,
    local_patch_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandInput {
    name: String,
    value_type: InputValueType,
    cardinality: InputCardinality,
    sources: Vec<InputSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputValueType {
    String,
    Bool,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputCardinality {
    Required,
    Optional,
    Repeated,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSource {
    Argument,
    File,
    Stdin,
}

impl ProjectSpec {
    fn from_answers(answers: WizardAnswers) -> Result<Self> {
        validate_crate_name(&answers.project_name, "project name")?;
        validate_crate_name(&answers.executable_name, "executable name")?;
        validate_ident(&answers.command_name.replace('-', "_"), "command name")?;
        if answers.inputs.is_empty() {
            bail!("at least one command input is required");
        }
        for input in &answers.inputs {
            input.validate()?;
        }
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
            inputs: answers.inputs,
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

impl CommandInput {
    fn required_string(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
        }
    }

    fn validate(&self) -> Result<()> {
        validate_ident(&self.name, "input name")?;
        if self.sources.is_empty() {
            bail!("{} must allow at least one input source", self.name);
        }
        if self.cardinality == InputCardinality::Boolean {
            if self.value_type != InputValueType::Bool {
                bail!("{} uses boolean cardinality but is not bool", self.name);
            }
            if self.sources != [InputSource::Argument] {
                bail!("{} boolean flags only support argument source", self.name);
            }
        }
        if self.value_type == InputValueType::Bool && self.cardinality != InputCardinality::Boolean
        {
            bail!("{} bool inputs must use boolean cardinality", self.name);
        }
        if self.value_type == InputValueType::Path
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} path inputs only support argument source", self.name);
        }
        if self.cardinality == InputCardinality::Repeated
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} repeated inputs only support argument source", self.name);
        }
        Ok(())
    }

    fn policy_sentence(&self) -> String {
        let sources = self
            .sources
            .iter()
            .map(|source| match source {
                InputSource::Argument if self.cardinality == InputCardinality::Boolean => {
                    format!("--{}", self.name.replace('_', "-"))
                }
                InputSource::Argument => format!("--{}", self.name.replace('_', "-")),
                InputSource::File => format!("--{}-file", self.name.replace('_', "-")),
                InputSource::Stdin => "piped stdin".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", then ");
        format!("{} comes from {sources}", self.name)
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
    let command_input = CommandInput::required_string(input_name);
    command_input.validate()?;

    Ok(WizardAnswers {
        project_name,
        executable_name,
        command_name,
        command_description,
        inputs: vec![command_input],
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

fn write_review(spec: &ProjectSpec, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "\nReview")?;
    writeln!(output, "Destination: {}", spec.destination.display())?;
    writeln!(output, "Generated tree:")?;
    for path in spec.tree() {
        writeln!(output, "  {path}")?;
    }
    writeln!(
        output,
        "Command syntax: {} {} {}",
        spec.executable_name,
        spec.command_name,
        spec.inputs
            .iter()
            .map(command_syntax_fragment)
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    writeln!(output, "Input policy:")?;
    for input in &spec.inputs {
        writeln!(output, "  - {}.", input.policy_sentence())?;
    }
    writeln!(
        output,
        "Core operation: {}::{}({})",
        spec.lib_crate,
        spec.operation_name,
        spec.inputs
            .iter()
            .map(core_signature_fragment)
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(
        output,
        "Output shape: {} renders human output and serializes as JSON.",
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

fn command_syntax_fragment(input: &CommandInput) -> String {
    match input.cardinality {
        InputCardinality::Boolean => format!("[--{}]", input.name.replace('_', "-")),
        InputCardinality::Required => {
            format!("--{} <{}>", input.name.replace('_', "-"), input.name)
        }
        InputCardinality::Optional => {
            format!("[--{} <{}>]", input.name.replace('_', "-"), input.name)
        }
        InputCardinality::Repeated => {
            format!("[--{} <{}>]...", input.name.replace('_', "-"), input.name)
        }
    }
}

fn core_signature_fragment(input: &CommandInput) -> String {
    format!("{}: {}", input.name, input.rust_type())
}

impl CommandInput {
    fn rust_type(&self) -> &'static str {
        match (self.value_type, self.cardinality) {
            (InputValueType::String, InputCardinality::Required) => "String",
            (InputValueType::String, InputCardinality::Optional) => "Option<String>",
            (InputValueType::String, InputCardinality::Repeated) => "Vec<String>",
            (InputValueType::Bool, InputCardinality::Boolean) => "bool",
            (InputValueType::Path, InputCardinality::Required) => "std::path::PathBuf",
            (InputValueType::Path, InputCardinality::Optional) => "Option<std::path::PathBuf>",
            (InputValueType::Path, InputCardinality::Repeated) => "Vec<std::path::PathBuf>",
            _ => "String",
        }
    }

    fn cli_arg(&self) -> String {
        let long = self.name.replace('_', "-");
        let mut args = match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                format!("#[arg(long = \"{long}\", action = clap::ArgAction::SetTrue)]\n        {}: bool,", self.name)
            }
            (InputValueType::Path, InputCardinality::Required) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: std::path::PathBuf,", self.name)
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Option<std::path::PathBuf>,", self.name)
            }
            (InputValueType::Path, InputCardinality::Repeated) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Vec<std::path::PathBuf>,", self.name)
            }
            (_, InputCardinality::Required) => {
                if self.sources == [InputSource::Argument] {
                    format!("#[arg(long = \"{long}\")]\n        {}: String,", self.name)
                } else {
                    format!(
                        "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                        self.name
                    )
                }
            }
            (_, InputCardinality::Optional) => {
                format!(
                    "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                    self.name
                )
            }
            (_, InputCardinality::Repeated) => {
                format!(
                    "#[arg(long = \"{long}\")]\n        {}: Vec<String>,",
                    self.name
                )
            }
            _ => unreachable!("validated input combinations are renderable"),
        };
        if self.value_type == InputValueType::String && self.sources.contains(&InputSource::File) {
            args.push_str(&format!(
                "\n        #[arg(long = \"{long}-file\", value_name = \"PATH\")]\n        {0}_file: Option<std::path::PathBuf>,",
                self.name
            ));
        }
        args
    }

    fn core_call_arg(&self) -> String {
        self.name.clone()
    }

    fn core_view_field(&self) -> String {
        format!("pub {}: {},", self.name, self.rust_type())
    }

    fn core_view_init(&self) -> String {
        format!("{0},", self.name)
    }

    fn core_validation(&self) -> Option<String> {
        if self.value_type == InputValueType::String
            && self.cardinality == InputCardinality::Required
        {
            Some(format!(
                "if {}.trim().is_empty() {{\n        return Err(CoreError::EmptyInput);\n    }}",
                self.name
            ))
        } else {
            None
        }
    }

    fn handler_view_field(&self) -> String {
        self.core_view_field().replace("pub ", "pub(crate) ")
    }

    fn handler_view_from(&self) -> String {
        format!("{0}: value.{0},", self.name)
    }

    fn resolve_statement(&self) -> String {
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                format!("let {} = matches.get_flag(\"{}\");", self.name, self.name)
            }
            (InputValueType::Path, InputCardinality::Required) => format!(
                "let {} = matches.get_one::<std::path::PathBuf>(\"{}\").cloned().ok_or_else(|| anyhow::anyhow!(\"{} is required\"))?;",
                self.name, self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Optional) => format!(
                "let {} = matches.get_one::<std::path::PathBuf>(\"{}\").cloned();",
                self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Repeated) => format!(
                "let {} = matches.get_many::<std::path::PathBuf>(\"{}\").map(|values| values.cloned().collect()).unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Repeated) => format!(
                "let {} = matches.get_many::<String>(\"{}\").map(|values| values.cloned().collect()).unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Optional) => {
                self.string_resolver("None")
            }
            (InputValueType::String, InputCardinality::Required) => {
                self.string_resolver(&format!(
                    "return Err(anyhow::anyhow!(\"{} is required\"))",
                    self.name
                ))
            }
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn string_resolver(&self, missing: &str) -> String {
        let mut lines = vec![format!("let mut {} = None;", self.name)];
        for source in &self.sources {
            match source {
                InputSource::Argument => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = matches.get_one::<String>(\"{0}\").cloned();\n    }}",
                    self.name
                )),
                InputSource::File => lines.push(format!(
                    "if {0}.is_none() {{\n        if let Some(path) = matches.get_one::<std::path::PathBuf>(\"{0}_file\") {{\n            {0} =\n                Some(std::fs::read_to_string(path).map_err(|error| {{\n                    anyhow::anyhow!(\"failed to read {{}}: {{error}}\", path.display())\n                }})?);\n        }}\n    }}",
                    self.name
                )),
                InputSource::Stdin => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = standout_input::read_if_piped()?;\n    }}",
                    self.name
                )),
            }
        }
        lines.push(format!(
            "let {} = match {} {{\n        Some(value) => value,\n        None => {missing},\n    }};",
            self.name, self.name
        ));
        lines.join("\n    ")
    }
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    Environment::new()
        .template_from_str(template)?
        .render(model(spec))
        .with_context(|| format!("path template {template} is missing model data"))
}

fn model(spec: &ProjectSpec) -> minijinja::Value {
    let primary = &spec.inputs[0];
    context! {
        project_name => spec.project_name,
        executable_name => spec.executable_name,
        command_name => spec.command_name,
        command_ident => spec.command_name.replace('-', "_"),
        command_variant => pascal_case(&spec.command_name.replace('-', "_")),
        command_description => spec.command_description,
        input_name => primary.name,
        inputs => spec.inputs.iter().map(|input| {
            context! {
                name => input.name,
                cli_arg => input.cli_arg(),
                core_call_arg => input.core_call_arg(),
                core_view_field => input.core_view_field(),
                core_view_init => input.core_view_init(),
                handler_view_field => input.handler_view_field(),
                handler_view_from => input.handler_view_from(),
                rust_type => input.rust_type(),
                policy => input.policy_sentence(),
            }
        }).collect::<Vec<_>>(),
        core_params => spec.inputs.iter().map(core_signature_fragment).collect::<Vec<_>>().join(", "),
        core_call_args => spec.inputs.iter().map(CommandInput::core_call_arg).collect::<Vec<_>>().join(", "),
        core_validations => spec.inputs.iter().filter_map(CommandInput::core_validation).collect::<Vec<_>>().join("\n    "),
        cli_args => spec.inputs.iter().map(CommandInput::cli_arg).collect::<Vec<_>>().join("\n        "),
        core_view_fields => spec.inputs.iter().map(CommandInput::core_view_field).collect::<Vec<_>>().join("\n    "),
        core_view_inits => spec.inputs.iter().map(CommandInput::core_view_init).collect::<Vec<_>>().join("\n        "),
        handler_view_fields => spec.inputs.iter().map(CommandInput::handler_view_field).collect::<Vec<_>>().join("\n    "),
        handler_view_from => spec.inputs.iter().map(CommandInput::handler_view_from).collect::<Vec<_>>().join("\n            "),
        resolve_inputs => spec.inputs.iter().map(CommandInput::resolve_statement).collect::<Vec<_>>().join("\n    "),
        lib_crate => spec.lib_crate,
        lib_package => spec.lib_crate.replace('_', "-"),
        operation_name => spec.operation_name,
        view_name => spec.view_name,
        standout_version => spec.standout_version,
        local_patch_root => spec.local_patch_root.as_ref().map(|path| path.display().to_string()),
    }
}

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
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
    {{ core_view_fields }}
    pub summary: String,
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
pub fn {{ operation_name }}({{ core_params }}) -> Result<{{ view_name }}, CoreError> {
    let summary = format!("{:?}", (&{{ core_call_args }}));
    {{ core_validations }}
    Ok({{ view_name }} { {{ core_view_inits }} summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_returns_a_typed_result() {
        let result = {{ operation_name }}("Standout".to_string()).unwrap();

        assert_eq!(result.{{ input_name }}, "Standout");
        assert!(result.summary.contains("Standout"));
    }

    #[test]
    fn blank_input_is_rejected_by_the_core() {
        assert_eq!({{ operation_name }}("".to_string()), Err(CoreError::EmptyInput));
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
standout-input = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod handlers;

use anyhow::Result;
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
            config.template("{{ command_name }}.jinja")
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

        let result = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            ["{{ executable_name }}", "{{ command_name }}", "--{{ input_name }}", "Ada"],
        );

        result.assert_success();
        result.assert_stdout_contains("Ada");
        result.assert_stdout_contains("Summary:");
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
        assert_eq!(value["{{ input_name }}"], "Grace");
    }
}
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
        {{ cli_args }}
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

use clap::ArgMatches;
use {{ lib_crate }} as core;
use serde::Serialize;
use standout::cli::{CommandContext, Output};
use standout::handler;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct {{ view_name }} {
    {{ handler_view_fields }}
    pub(crate) summary: String,
}

impl From<core::{{ view_name }}> for {{ view_name }} {
    fn from(value: core::{{ view_name }}) -> Self {
        Self {
            {{ handler_view_from }}
            summary: value.summary,
        }
    }
}

/// Adapts typed shell input into the CLI-free core operation.
///
/// The handler owns CLI-only source resolution, including file-content reads,
/// then returns data for Standout to render or serialize.
#[handler]
pub(crate) fn {{ command_ident }}(
    #[matches] matches: &ArgMatches,
    #[ctx] _ctx: &CommandContext,
) -> Result<Output<{{ view_name }}>, anyhow::Error> {
    {{ resolve_inputs }}
    let result = core::{{ operation_name }}({{ core_call_args }})?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_handler_maps_input_to_core_and_view() {
        let matches = crate::cli::command()
            .try_get_matches_from(["{{ executable_name }}", "{{ command_name }}", "--{{ input_name }}", "hello"])
            .unwrap();
        let (_, matches) = matches.subcommand().unwrap();
        let ctx = CommandContext::default();

        let Output::Render(view) = {{ command_ident }}(matches, &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(view.{{ input_name }}, "hello");
        assert!(view.summary.contains("hello"));
    }
}
"#,
    ),
    (
        "template",
        r#"[title]{{ command_name }}[/title]
{{ input_name }}: {{ "{{ " }}{{ input_name }}{{ " }}" }}
Summary: {{ "{{ summary }}" }}
"#,
    ),
    (
        "style",
        r#"title:
  bold: true
  fg: cyan
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
            inputs: vec![CommandInput::required_string("name")],
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
            inputs: vec![CommandInput::required_string("document")],
        })
        .unwrap();

        assert_eq!(spec.lib_crate, "demolib");
        assert_eq!(spec.operation_name, "process_inspect");
        assert_eq!(spec.view_name, "InspectView");
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
        assert!(!spec.destination.exists());
    }

    #[test]
    fn render_omits_local_patch_paths_by_default() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput::required_string("document")],
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(!manifest.contains("[patch.crates-io]"));
        assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn validates_supported_typed_cardinality_source_combinations() {
        let rich_inputs = vec![
            CommandInput {
                name: "document".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
            },
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
            CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ];

        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: rich_inputs,
        })
        .unwrap();

        assert_eq!(
            spec.inputs[0].policy_sentence(),
            "document comes from --document, then --document-file, then piped stdin"
        );
    }

    #[test]
    fn rejects_unsupported_input_combinations_before_rendering() {
        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "enabled".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            }],
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("bool inputs must use boolean cardinality"));

        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument, InputSource::File],
            }],
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("path inputs only support argument source"));
    }

    #[test]
    fn path_input_rendering_does_not_emit_string_validation() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument],
            }],
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let core = generated
            .files
            .get(Path::new("crates/demolib/src/lib.rs"))
            .unwrap();

        assert!(core.contains("config: std::path::PathBuf"));
        assert!(!core.contains("config.trim()"));
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
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "Ada",
            ])
            .output()
            .unwrap();
        assert!(
            human.status.success(),
            "human run failed: {}",
            String::from_utf8_lossy(&human.stderr)
        );
        let stdout = String::from_utf8(human.stdout).unwrap();
        assert!(stdout.contains("Ada"));
        assert!(stdout.contains("Summary:"));

        let json = Command::new("cargo")
            .current_dir(&spec.destination)
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
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
        assert_eq!(value["name"], "Ada");

        let input_file = spec.destination.join("input.txt");
        fs::write(&input_file, "File Ada").unwrap();
        let file_json = Command::new("cargo")
            .current_dir(&spec.destination)
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ])
            .output()
            .unwrap();
        assert!(
            file_json.status.success(),
            "file json run failed: {}",
            String::from_utf8_lossy(&file_json.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&file_json.stdout).unwrap();
        assert_eq!(value["name"], "File Ada");

        let precedence_json = Command::new("cargo")
            .current_dir(&spec.destination)
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "Arg Ada",
                "--name-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ])
            .output()
            .unwrap();
        assert!(
            precedence_json.status.success(),
            "precedence json run failed: {}",
            String::from_utf8_lossy(&precedence_json.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&precedence_json.stdout).unwrap();
        assert_eq!(value["name"], "Arg Ada");

        let invalid = Command::new("cargo")
            .current_dir(&spec.destination)
            .args([
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "   ",
            ])
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
