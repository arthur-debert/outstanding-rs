use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use minijinja::context;
use serde_json::json;
use standout::cli::{
    App, CommandContext, CommandContextInput, FnHandler, HandlerResult, Output as HandlerOutput,
};
use standout::embed_templates;
use standout_input::questionnaire::{AnswerValue, EarlierAnswers, FormError};
use standout_render::template::new_environment;

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
    build_app()?.run(Cli::command(), std::env::args());
    Ok(())
}

fn build_app() -> Result<App> {
    Ok(App::builder()
        .no_output_flag()
        .no_output_file_flag()
        .templates(embed_templates!("src/bin/templates"))
        .command_with("new-project", FnHandler::new(run_new_project), |config| {
            config.questionnaire_with_form_and_review::<NewProjectAnswers, _, _>(
                new_project_form_rules,
                write_new_project_review,
            )
        })?
        .build()?)
}

fn run_new_project(
    _matches: &clap::ArgMatches,
    ctx: &CommandContext,
) -> HandlerResult<serde_json::Value> {
    let answers: &NewProjectAnswers = ctx.questionnaire()?;
    let spec = ProjectSpec::from_answers(answers.clone())?;
    let mut transcript = Vec::new();
    publish_project(&spec)?;
    writeln!(transcript, "Created {}", spec.destination.display())?;
    Ok(HandlerOutput::Render(json!({
        "transcript": String::from_utf8(transcript)
            .expect("wizard transcript is generated from UTF-8 literals and paths"),
    })))
}

fn write_new_project_review(
    answers: &NewProjectAnswers,
    output: &mut dyn Write,
) -> anyhow::Result<()> {
    let spec = ProjectSpec::from_answers(answers.clone())?;
    write_review(&spec, output)
}

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "standout.new-project")]
struct NewProjectAnswers {
    /// Project identity.
    #[question(id = "project")]
    project: ProjectAnswers,

    /// Initial command.
    #[question(id = "command")]
    command: CommandAnswers,

    /// Result shape.
    #[question(id = "result")]
    result: ResultAnswers,
}

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "standout.new-project.project")]
struct ProjectAnswers {
    /// What is the project name? It is also the destination directory.
    #[question(id = "name", validate = validate_project_name, revision = "crate-name.v1")]
    name: String,

    /// What is the executable name? Leave blank to reuse the project name.
    #[question(
        id = "executable",
        default_with = executable_default,
        validate = validate_executable_name,
        revision = "crate-name.v2"
    )]
    executable_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "standout.new-project.command")]
struct CommandAnswers {
    /// What is the command name?
    #[question(id = "name", validate = validate_command_answer, revision = "command-name.v1")]
    name: String,

    /// Describe the command in a sentence or two.
    #[question(id = "description", prose)]
    description: String,

    /// Describe a command input.
    #[question(id = "inputs", min = 1)]
    inputs: Vec<CommandInputAnswers>,
}

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "standout.new-project.input")]
struct CommandInputAnswers {
    /// What is its name?
    #[question(id = "name", validate = validate_input_name, revision = "input-name.v1")]
    name: String,

    /// What type of value is it?
    #[question(id = "value_type", choice, default = "string")]
    value_type: InputValueType,

    /// How many values does it take?
    #[question(
        id = "cardinality",
        choice,
        default_with = cardinality_default,
        revision = "input-cardinality-default.v1"
    )]
    cardinality: InputCardinality,

    /// Where can its value come from, in precedence order (comma-separated: argument, file, stdin)?
    #[question(
        id = "sources",
        default_with = sources_default,
        validate = validate_sources_answer,
        revision = "input-sources.v2"
    )]
    sources: String,
}

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "standout.new-project.result")]
struct ResultAnswers {
    /// Should the result be a message or a record?
    #[question(id = "shape", choice, default = "record")]
    shape: ResultShape,

    /// Which fields should the record carry (comma-separated)?
    #[question(
        id = "fields",
        default = "summary,count",
        active_when(field = "shape", is = "record"),
        validate = validate_record_fields_answer,
        revision = "record-fields.v1"
    )]
    fields: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandInput {
    name: String,
    value_type: InputValueType,
    cardinality: InputCardinality,
    sources: Vec<InputSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSpec {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
    lib_crate: String,
    operation_name: String,
    view_name: String,
    destination: PathBuf,
    standout_version: String,
    local_patch_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, standout::QuestionnaireChoices)]
enum InputValueType {
    #[question(rename = "string")]
    String,
    #[question(rename = "bool")]
    Bool,
    #[question(rename = "path")]
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, standout::QuestionnaireChoices)]
enum InputCardinality {
    #[question(rename = "required")]
    Required,
    #[question(rename = "optional")]
    Optional,
    #[question(rename = "repeated")]
    Repeated,
    #[question(rename = "boolean")]
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSource {
    Argument,
    File,
    Stdin,
}

#[cfg(test)]
impl InputSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Argument => "argument",
            Self::File => "file",
            Self::Stdin => "stdin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, standout::QuestionnaireChoices)]
enum ResultShape {
    #[question(rename = "message")]
    Message,
    #[question(rename = "record")]
    Record,
}

impl ResultShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Record => "record",
        }
    }
}

impl ProjectSpec {
    fn from_answers(answers: impl Into<NewProjectAnswers>) -> Result<Self> {
        let answers = answers.into();
        validate_crate_name(&answers.project.name, "project name")?;
        validate_crate_name(&answers.project.executable_name, "executable name")?;
        validate_ident(&answers.command.name.replace('-', "_"), "command name")?;
        if answers.command.inputs.is_empty() {
            bail!("at least one command input is required");
        }
        let inputs = command_inputs_from_answers(&answers.command.inputs)?;
        for input in &inputs {
            input.validate()?;
        }
        validate_generated_flags(&inputs)?;
        let record_fields = record_fields_from_answers(&answers.result)?;
        validate_result_fields(answers.result.shape, &record_fields)?;
        if answers.command.description.trim().is_empty() {
            bail!("command description cannot be empty");
        }

        let lib_crate = format!("{}lib", answers.project.name.replace('-', "_"));
        let command_ident = answers.command.name.replace('-', "_");
        let operation_name = format!("process_{command_ident}");
        let view_name = format!("{}View", pascal_case(&command_ident));
        Ok(Self {
            destination: PathBuf::from(&answers.project.name),
            project_name: answers.project.name,
            executable_name: answers.project.executable_name,
            command_name: answers.command.name,
            command_description: answers.command.description,
            inputs,
            result_shape: answers.result.shape,
            record_fields,
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

impl CommandInput {
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
        if self.cardinality == InputCardinality::Optional
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} optional inputs only support argument source", self.name);
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
        let mut env = new_environment();
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

fn executable_default(answers: &EarlierAnswers<'_>) -> String {
    answers
        .get_text("project.name")
        .unwrap_or_default()
        .to_string()
}

fn cardinality_default(answers: &EarlierAnswers<'_>) -> String {
    if answers.get_text("command.inputs.value_type") == Some("bool") {
        "boolean"
    } else {
        "required"
    }
    .to_string()
}

fn sources_default(answers: &EarlierAnswers<'_>) -> String {
    let value_type = answers.get_text("command.inputs.value_type");
    let cardinality = answers.get_text("command.inputs.cardinality");
    if value_type == Some("string") && matches!(cardinality, Some("required") | Some("optional")) {
        "argument,file,stdin"
    } else {
        "argument"
    }
    .to_string()
}

fn validate_project_name(value: &AnswerValue) -> Result<(), String> {
    validate_crate_name(value.as_text().unwrap_or_default(), "project name")
        .map_err(|error| error.to_string())
}

fn validate_executable_name(value: &AnswerValue) -> Result<(), String> {
    validate_crate_name(value.as_text().unwrap_or_default(), "executable name")
        .map_err(|error| error.to_string())
}

fn validate_command_answer(value: &AnswerValue) -> Result<(), String> {
    validate_ident(
        &value.as_text().unwrap_or_default().replace('-', "_"),
        "command name",
    )
    .map_err(|error| error.to_string())
}

fn validate_input_name(value: &AnswerValue) -> Result<(), String> {
    validate_ident(value.as_text().unwrap_or_default(), "input name")
        .map_err(|error| error.to_string())
}

fn validate_sources_answer(value: &AnswerValue) -> Result<(), String> {
    parse_input_sources(value.as_text().unwrap_or_default())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn validate_record_fields_answer(value: &AnswerValue) -> Result<(), String> {
    parse_record_fields(value.as_text().unwrap_or_default())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn new_project_form_rules(answers: &NewProjectAnswers) -> Vec<FormError> {
    let mut errors = Vec::new();
    let inputs = match command_inputs_from_answers(&answers.command.inputs) {
        Ok(inputs) => inputs,
        Err(error) => {
            errors.push(FormError::new(
                ["command.inputs.sources"],
                error.to_string(),
            ));
            return errors;
        }
    };
    for (index, input) in inputs.iter().enumerate() {
        if let Err(error) = input.validate() {
            errors.push(FormError::new(
                [format!("command.inputs[{index}]")],
                error.to_string(),
            ));
        }
    }
    if let Err(error) = validate_generated_flags(&inputs) {
        errors.push(FormError::new(["command.inputs.name"], error.to_string()));
    }
    if let Err(error) = record_fields_from_answers(&answers.result) {
        errors.push(FormError::new(["result.fields"], error.to_string()));
    }
    errors
}

fn command_inputs_from_answers(inputs: &[CommandInputAnswers]) -> Result<Vec<CommandInput>> {
    inputs
        .iter()
        .map(|input| {
            Ok(CommandInput {
                name: input.name.clone(),
                value_type: input.value_type,
                cardinality: input.cardinality,
                sources: parse_input_sources(&input.sources)?,
            })
        })
        .collect()
}

fn record_fields_from_answers(result: &ResultAnswers) -> Result<Vec<String>> {
    match result.shape {
        ResultShape::Message => Ok(Vec::new()),
        ResultShape::Record => parse_record_fields(result.fields.as_deref().unwrap_or_default()),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestProjectAnswers {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
}

#[cfg(test)]
impl From<TestProjectAnswers> for NewProjectAnswers {
    fn from(answers: TestProjectAnswers) -> Self {
        Self {
            project: ProjectAnswers {
                name: answers.project_name,
                executable_name: answers.executable_name,
            },
            command: CommandAnswers {
                name: answers.command_name,
                description: answers.command_description,
                inputs: answers
                    .inputs
                    .into_iter()
                    .map(CommandInputAnswers::from)
                    .collect(),
            },
            result: ResultAnswers {
                shape: answers.result_shape,
                fields: match answers.record_fields.is_empty() {
                    true => None,
                    false => Some(answers.record_fields.join(",")),
                },
            },
        }
    }
}

#[cfg(test)]
impl From<CommandInput> for CommandInputAnswers {
    fn from(input: CommandInput) -> Self {
        Self {
            name: input.name,
            value_type: input.value_type,
            cardinality: input.cardinality,
            sources: input
                .sources
                .into_iter()
                .map(InputSource::as_str)
                .collect::<Vec<_>>()
                .join(","),
        }
    }
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

fn parse_input_sources(value: &str) -> Result<Vec<InputSource>> {
    let mut sources = Vec::new();
    for source in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let parsed = match source {
            "argument" | "arg" => InputSource::Argument,
            "file" => InputSource::File,
            "stdin" | "piped stdin" => InputSource::Stdin,
            _ => bail!("input source must be argument, file, or stdin"),
        };
        if sources.contains(&parsed) {
            bail!("input source {source} is declared more than once");
        }
        sources.push(parsed);
    }
    if sources.is_empty() {
        bail!("at least one input source is required");
    }
    Ok(sources)
}

fn validate_result_fields(shape: ResultShape, fields: &[String]) -> Result<()> {
    match shape {
        ResultShape::Message if !fields.is_empty() => {
            bail!("message results cannot declare record fields");
        }
        ResultShape::Message => {}
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

fn validate_generated_flags(inputs: &[CommandInput]) -> Result<()> {
    const RESERVED_FLAGS: &[&str] = &["help", "output", "output-file-path"];

    let mut flags = BTreeMap::new();
    for input in inputs {
        let logical_flag = input.name.replace('_', "-");
        if RESERVED_FLAGS.contains(&logical_flag.as_str()) {
            bail!(
                "input {} generates reserved framework/Clap flag --{logical_flag}",
                input.name
            );
        }
        if let Some(owner) = flags.insert(logical_flag.clone(), input.name.as_str()) {
            bail!(
                "input {} generates --{logical_flag}, which conflicts with input {owner}",
                input.name
            );
        }
        if input.sources.contains(&InputSource::File) {
            let file_flag = format!("{logical_flag}-file");
            if RESERVED_FLAGS.contains(&file_flag.as_str()) {
                bail!(
                    "input {} generates reserved framework/Clap flag --{file_flag}",
                    input.name
                );
            }
            if let Some(owner) = flags.insert(file_flag.clone(), input.name.as_str()) {
                bail!(
                    "input {} generates --{file_flag}, which conflicts with input {owner}",
                    input.name
                );
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

fn command_syntax_fragment(input: &CommandInput) -> String {
    let long = input.name.replace('_', "-");
    let sources = input
        .sources
        .iter()
        .map(|source| match source {
            InputSource::Argument => match input.cardinality {
                InputCardinality::Boolean => format!("--{long}"),
                _ => format!("--{long} <{}>", input.name),
            },
            InputSource::File => format!("--{long}-file <PATH>"),
            InputSource::Stdin => "<piped stdin>".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" | ");
    match input.cardinality {
        InputCardinality::Required => {
            if input.sources.len() == 1 {
                sources
            } else {
                format!("({sources})")
            }
        }
        InputCardinality::Repeated => format!("[{sources}]..."),
        _ => format!("[{sources}]"),
    }
}

fn core_signature_fragment(input: &CommandInput) -> String {
    format!("{}: {}", input.name, input.rust_type())
}

fn core_fn_signature(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!("({})", core_signature_fragment(&spec.inputs[0]))
    } else {
        let params = spec
            .inputs
            .iter()
            .map(|input| format!("    {},", core_signature_fragment(input)))
            .collect::<Vec<_>>()
            .join("\n");
        format!("(\n{params}\n)")
    }
}

fn core_primary_value(spec: &ProjectSpec) -> String {
    let primary = &spec.inputs[0];
    match (primary.value_type, primary.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            format!("{}.trim().to_string()", primary.name)
        }
        _ => format!("format!(\"{{:?}}\", &{})", primary.name),
    }
}

fn core_unused_inputs(spec: &ProjectSpec) -> String {
    let names = spec
        .inputs
        .iter()
        .skip(1)
        .map(|input| format!("&{}", input.name))
        .collect::<Vec<_>>();
    if names.is_empty() {
        String::new()
    } else if names.len() == 1 {
        format!("let _ = {};", names[0])
    } else {
        format!("let _ = ({});", names.join(", "))
    }
}

fn result_fields(spec: &ProjectSpec, visibility: &str) -> String {
    match spec.result_shape {
        ResultShape::Message => format!("    {visibility}message: String,"),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| format!("    {visibility}{field}: String,"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn result_init(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "        message: format!(\"Processed {primary}\"),".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        summary: format!(\"Processed {primary}\"),".into(),
                "count" | "length" => {
                    format!("        {field}: primary.chars().count().to_string(),")
                }
                other => format!("        {other}: primary.clone(),"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn result_field_names(spec: &ProjectSpec) -> Vec<String> {
    match spec.result_shape {
        ResultShape::Message => vec!["message".into()],
        ResultShape::Record => spec.record_fields.clone(),
    }
}

fn view_from_fields(spec: &ProjectSpec) -> String {
    result_field_names(spec)
        .into_iter()
        .map(|field| format!("            {field}: value.{field},"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn core_valid_assertions(spec: &ProjectSpec) -> String {
    let primary = match (spec.inputs[0].value_type, spec.inputs[0].cardinality) {
        (InputValueType::String, InputCardinality::Required) => "Standout".to_string(),
        (InputValueType::String, InputCardinality::Optional) => "Some(\"optional\")".to_string(),
        (InputValueType::String, InputCardinality::Repeated) => "[\"alpha\", \"beta\"]".to_string(),
        (InputValueType::Bool, InputCardinality::Boolean) => "true".to_string(),
        (InputValueType::Path, InputCardinality::Required) => "\"config.toml\"".to_string(),
        (InputValueType::Path, InputCardinality::Optional) => "Some(\"config.toml\")".to_string(),
        (InputValueType::Path, InputCardinality::Repeated) => "[\"one.toml\"]".to_string(),
        _ => unreachable!("validated input combinations are renderable"),
    };
    match spec.result_shape {
        ResultShape::Message => {
            let expected = format!("Processed {primary}");
            format!("        assert_eq!(result.message, {});", quote(&expected))
        }
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => format!(
                    "        assert_eq!(result.summary, {});",
                    quote(&format!("Processed {primary}"))
                ),
                "count" | "length" => format!(
                    "        assert_eq!(result.{field}, {});",
                    quote(&primary.chars().count().to_string())
                ),
                other => format!("        assert_eq!(result.{other}, {});", quote(&primary)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_test_call(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        return format!(
            "{}({})",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        );
    }
    let args = spec
        .inputs
        .iter()
        .map(|input| format!("            {},", core_test_arg(input, false)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}(\n{args}\n        )", spec.operation_name)
}

fn core_sample_result(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!(
            "        let result = {}({}).unwrap();",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        )
    } else {
        format!(
            "        let result = {}\n        .unwrap();",
            core_test_call(spec)
        )
    }
}

fn core_invalid_test(spec: &ProjectSpec) -> String {
    let Some(index) = spec.inputs.iter().position(|input| {
        input.value_type == InputValueType::String
            && input.cardinality == InputCardinality::Required
    }) else {
        let call = core_test_call(spec);
        let result = if spec.inputs.len() == 1 {
            format!("        let result = {call}.unwrap();")
        } else {
            format!("        let result = {call}\n        .unwrap();")
        };
        return format!(
            r#"    #[test]
    fn configured_inputs_are_accepted_by_the_core() {{
{result}

        assert_eq!(result, result.clone());
    }}"#
        );
    };

    if spec.inputs.len() == 1 {
        return format!(
            r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}({}),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
            spec.operation_name,
            core_test_arg(&spec.inputs[0], true),
            quote(&spec.inputs[index].name)
        );
    }

    let args = spec
        .inputs
        .iter()
        .enumerate()
        .map(|(input_index, input)| {
            format!(
                "                {},",
                core_test_arg(input, input_index == index)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}(
{args}
            ),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
        spec.operation_name,
        quote(&spec.inputs[index].name)
    )
}

fn core_test_arg(input: &CommandInput, blank: bool) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            if blank {
                "\"\".to_string()".into()
            } else {
                "\"Standout\".to_string()".into()
            }
        }
        (InputValueType::String, InputCardinality::Optional) => {
            if blank {
                "Some(\"\".to_string())".into()
            } else {
                "Some(\"optional\".to_string())".into()
            }
        }
        (InputValueType::String, InputCardinality::Repeated) => {
            "vec![\"alpha\".to_string(), \"beta\".to_string()]".into()
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => {
            "std::path::PathBuf::from(\"config.toml\")".into()
        }
        (InputValueType::Path, InputCardinality::Optional) => {
            "Some(std::path::PathBuf::from(\"config.toml\"))".into()
        }
        (InputValueType::Path, InputCardinality::Repeated) => {
            "vec![std::path::PathBuf::from(\"one.toml\")]".into()
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}

fn handler_expected_fields(spec: &ProjectSpec) -> String {
    let expected = expected_first_field(spec, "hello");
    result_field_names(spec)
        .into_iter()
        .map(|field| match field.as_str() {
            "message" | "summary" => {
                format!("                {field}: {}.into(),", quote(&expected.1))
            }
            "count" | "length" => {
                let value = expected_primary_text(&spec.inputs[0], "hello")
                    .chars()
                    .count()
                    .to_string();
                format!("                {field}: {}.into(),", quote(&value))
            }
            _ => {
                let value = expected_primary_text(&spec.inputs[0], "hello");
                format!("                {field}: {}.into(),", quote(&value))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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

fn expected_first_field(spec: &ProjectSpec, input: &str) -> (String, String) {
    let field = result_field_names(spec).remove(0);
    let input = expected_primary_text(&spec.inputs[0], input);
    let value = match field.as_str() {
        "message" | "summary" => format!("Processed {input}"),
        "count" | "length" => input.chars().count().to_string(),
        _ => input,
    };
    (field, value)
}

fn expected_primary_text(input: &CommandInput, value: &str) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => value.to_string(),
        (InputValueType::String, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::String, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra\"]")
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => format!("\"{value}\""),
        (InputValueType::Path, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::Path, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra.toml\"]")
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}

/// The human page is what a bare invocation renders, so it names no `--output`.
fn sample_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![quote(&spec.executable_name), quote(&spec.command_name)];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_json_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("json"),
    ];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_command_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        args.extend(input.sample_args_for_source(
            input.sources[0],
            if index == 0 { primary_value } else { "sample" },
        ));
    }
    args
}

/// The chained `TestHarness` setup calls the samples need, in order, for the
/// caller to join.
fn harness_setup_calls(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut calls = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        let value = if index == 0 { primary_value } else { "sample" };
        match input.sources[0] {
            InputSource::File => calls.push(format!(
                ".fixture({}, {})",
                quote(&input.sample_file_name()),
                quote(value)
            )),
            InputSource::Stdin => {
                calls.push(format!(".piped_stdin({})", quote(&format!("{value}\n"))))
            }
            InputSource::Argument => {}
        }
    }
    calls
}

fn harness_expression(calls: &[String]) -> String {
    match calls.len() {
        0 | 1 => format!("TestHarness::new(){}", calls.join("")),
        _ => format!(
            "TestHarness::new()\n            {}",
            calls.join("\n            ")
        ),
    }
}

/// The harness binding and the `run` call, as two statements: a one-call chain
/// is what rustfmt formats predictably.
fn generated_harness_run(spec: &ProjectSpec, primary_value: &str, args: &[String]) -> String {
    let harness = harness_expression(&harness_setup_calls(spec, primary_value));
    let inline_args = format!("&app, cli::command(), [{}]", args.join(", "));
    let run = if inline_args.len() <= FN_CALL_WIDTH {
        format!("        let result = harness.run({inline_args});")
    } else {
        format!(
            "        let result = harness.run(\n            &app,\n            cli::command(),\n            {},\n        );",
            rust_array(args, 16, 63)
        )
    };
    format!("        let harness = {harness};\n{run}")
}

fn generated_json_pipeline_test(spec: &ProjectSpec) -> String {
    let primary_value = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Grace",
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let args = sample_json_cli_args(spec, primary_value);
    let run = generated_harness_run(spec, primary_value, &args);
    format!(
        r#"    #[test]
    #[serial]
    fn pipeline_serializes_json_for_configured_inputs() {{
        let (_user_dir, app) = test_app();

{run}

        result.assert_success();
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});
    }}"#,
        expected = quote(&expected)
    )
}

fn generated_config_test(spec: &ProjectSpec) -> String {
    let primary_value = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Grace",
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let executable = quote(&spec.executable_name);
    let file = quote(&format!("{}.toml", spec.executable_name));
    let mut calls = harness_setup_calls(spec, primary_value);
    calls.push(format!(".fixture({file}, CONFIG_FILE)"));
    let harness = harness_expression(&calls);
    let mut args = vec![executable.clone(), quote(&spec.command_name)];
    args.extend(sample_command_args(spec, primary_value));
    let inline_args = format!("&app, cli::command(), [{}]", args.join(", "));
    let run = if inline_args.len() <= FN_CALL_WIDTH {
        format!("        let result = harness.run({inline_args});")
    } else {
        format!(
            "        let result = harness.run(\n            &app,\n            cli::command(),\n            {},\n        );",
            rust_array(&args, 16, 63)
        )
    };
    format!(
        r#"    #[test]
    #[serial]
    fn term_output_in_the_config_file_selects_json() {{
        let (_user_dir, app) = test_app();
        let harness = {harness};
{run}

        result.assert_success();
        assert_eq!(result.output_mode(), Representation::Json);
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});

        let shown = TestHarness::new()
            .fixture({file}, CONFIG_FILE)
            .run(
                &app,
                cli::command(),
                [{executable}, "config", "get", "term.output"],
            );

        shown.assert_success();
        shown.assert_stdout_contains("term.output = json");
    }}"#,
        expected = quote(&expected)
    )
}

fn readme_input_policy(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .map(|input| format!("- {}.", input.policy_sentence()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn readme_validation_note(spec: &ProjectSpec) -> String {
    let inputs = spec
        .inputs
        .iter()
        .filter(|input| {
            input.value_type == InputValueType::String
                && input.cardinality == InputCardinality::Required
        })
        .map(|input| format!("`{}`", input.name))
        .collect::<Vec<_>>();
    match inputs.as_slice() {
        [] => String::new(),
        [input] => {
            format!("Blank values for the required string input {input} are rejected by the core operation.")
        }
        _ => format!(
            "Blank values for the required string inputs {} are rejected by the core operation.",
            inputs.join(", ")
        ),
    }
}

fn has_chain_inputs(spec: &ProjectSpec) -> bool {
    spec.inputs.iter().any(CommandInput::is_chain)
}

fn has_file_source(spec: &ProjectSpec) -> bool {
    spec.inputs
        .iter()
        .any(|input| input.sources.contains(&InputSource::File))
}

fn handler_imports(spec: &ProjectSpec) -> String {
    let mut imports = vec![
        format!("use {} as core;", spec.lib_crate),
        "use serde::Serialize;".to_string(),
        "use standout::handler;".to_string(),
    ];
    if has_file_source(spec) {
        imports.push("use clap::ArgMatches;".to_string());
    }
    if has_chain_inputs(spec) {
        imports.push(
            "use standout::cli::{CommandConfig, CommandContext, CommandContextInput, Output};"
                .to_string(),
        );
        let mut chain = vec!["InputChain"];
        if spec
            .inputs
            .iter()
            .any(|input| input.sources.contains(&InputSource::Argument))
        {
            chain.push("ArgSource");
        }
        if spec
            .inputs
            .iter()
            .any(|input| input.sources.contains(&InputSource::Stdin))
        {
            chain.push("StdinSource");
        }
        chain.sort_unstable();
        imports.push(match chain.as_slice() {
            [single] => format!("use standout::input::{single};"),
            several => format!("use standout::input::{{{}}};", several.join(", ")),
        });
    } else {
        imports.push("use standout::cli::Output;".to_string());
    }
    imports.sort();
    imports.join("\n")
}

fn handler_signature(spec: &ProjectSpec) -> String {
    use unicode_width::UnicodeWidthStr;

    let mut params = spec
        .inputs
        .iter()
        .filter(|input| !input.is_chain())
        .map(CommandInput::handler_param)
        .collect::<Vec<_>>();
    if has_chain_inputs(spec) {
        params.push("#[ctx] ctx: &CommandContext".to_string());
    }
    let command_ident = spec.command_name.replace('-', "_");
    let return_type = format!("-> Result<Output<{}>, anyhow::Error> {{", spec.view_name);
    let one_line = format!(
        "pub(crate) fn {command_ident}({}) {return_type}",
        params.join(", ")
    );
    if one_line.width() <= MAX_WIDTH {
        return one_line;
    }
    let lines = params
        .iter()
        .map(|param| format!("    {param},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("pub(crate) fn {command_ident}(\n{lines}\n) {return_type}")
}

fn handler_input_reads(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .filter(|input| input.is_chain())
        .map(|input| {
            format!(
                "    let {}: &String = ctx.input({})?;",
                input.name,
                quote(&input.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Empty when every value arrives as a typed parameter.
fn command_inputs_fn(spec: &ProjectSpec) -> String {
    let chained = spec
        .inputs
        .iter()
        .filter(|input| input.is_chain())
        .collect::<Vec<_>>();
    let body = match chained.as_slice() {
        [] => return String::new(),
        [input] => format!(
            "    config.input(\n        {},\n        {},\n    )",
            quote(&input.name),
            input.chain_expr(8)
        ),
        inputs => {
            let entries = inputs
                .iter()
                .map(|input| {
                    format!(
                        "        .input(\n            {},\n            {},\n        )",
                        quote(&input.name),
                        input.chain_expr(12)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("    config\n{entries}")
        }
    };
    format!(
        "/// Where the command's values come from, tried in the order listed.\npub(crate) fn {}_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H> {{\n{body}\n}}",
        spec.command_name.replace('-', "_")
    )
}

/// The app's own `InputCollector`; it reports the `file` source kind and names the path on failure.
fn file_source_item(spec: &ProjectSpec) -> String {
    if !has_file_source(spec) {
        return String::new();
    }
    r#"struct FileSource {
    arg: &'static str,
}

impl FileSource {
    fn new(arg: &'static str) -> Self {
        Self { arg }
    }
}

impl standout::input::InputCollector<String> for FileSource {
    fn name(&self) -> &'static str {
        "file"
    }

    fn is_available(&self, matches: &ArgMatches) -> bool {
        matches.get_one::<std::path::PathBuf>(self.arg).is_some()
    }

    fn collect(&self, matches: &ArgMatches) -> Result<Option<String>, standout::input::InputError> {
        let Some(path) = matches.get_one::<std::path::PathBuf>(self.arg) else {
            return Ok(None);
        };
        std::fs::read_to_string(path)
            .map(Some)
            .map_err(|error| standout::input::InputError::file(path.display().to_string(), error))
    }

    fn bind_sources(
        &self,
        _sources: &standout::input::InputSources,
    ) -> Option<Box<dyn standout::input::InputCollector<String>>> {
        None
    }
}"#
    .to_string()
}

fn handler_sample_value(index: usize) -> &'static str {
    if index == 0 {
        "hello"
    } else {
        "sample"
    }
}

fn handler_call(spec: &ProjectSpec) -> String {
    use unicode_width::UnicodeWidthStr;

    let mut args = spec
        .inputs
        .iter()
        .enumerate()
        .filter(|(_, input)| !input.is_chain())
        .map(|(index, input)| input.handler_test_value(handler_sample_value(index)))
        .collect::<Vec<_>>();
    if has_chain_inputs(spec) {
        args.push("&ctx".to_string());
    }
    let command_ident = spec.command_name.replace('-', "_");
    let inline = args.join(", ");
    let one_line = format!("        let Output::Render(view) = {command_ident}({inline}).unwrap()");
    if inline.width() <= FN_CALL_WIDTH && one_line.width() <= MAX_WIDTH {
        if one_line.width() + " else {".width() <= MAX_WIDTH {
            return format!("{one_line} else {{");
        }
        return format!("{one_line}\n        else {{");
    }
    let lines = args
        .iter()
        .map(|arg| format!("            {arg},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "        let Output::Render(view) = {command_ident}(\n{lines}\n        )\n        .unwrap() else {{"
    )
}

fn source_kind_variant(source: InputSource) -> &'static str {
    match source {
        InputSource::Argument => "Arg",
        InputSource::File => "File",
        InputSource::Stdin => "Stdin",
    }
}

/// Stands in for pre-dispatch resolution: each value carries its first source's kind.
fn handler_test_inputs(spec: &ProjectSpec) -> String {
    if !has_chain_inputs(spec) {
        return String::new();
    }
    let mut lines = vec![
        "        let mut ctx = CommandContext::default();".to_string(),
        "        let mut inputs = standout_input::Inputs::new();".to_string(),
    ];
    for (index, input) in spec.inputs.iter().enumerate() {
        if !input.is_chain() {
            continue;
        }
        lines.push(format!(
            "        inputs.insert(\n            {},\n            standout_input::ResolvedInput {{\n                value: {}.to_string(),\n                source: standout_input::InputSourceKind::{},\n            }},\n        );",
            quote(&input.name),
            quote(handler_sample_value(index)),
            source_kind_variant(input.sources[0])
        ));
    }
    lines.push("        ctx.extensions.insert(inputs);".to_string());
    lines.join("\n")
}

fn readme_examples(spec: &ProjectSpec) -> String {
    let command = format!(
        "cargo run -p {} -- {}",
        spec.executable_name, spec.command_name
    );
    let args = spec
        .inputs
        .iter()
        .enumerate()
        .flat_map(|(index, input)| input.readme_args(if index == 0 { "VALUE" } else { "SAMPLE" }))
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = Vec::new();
    for input in &spec.inputs {
        if input.sources[0] == InputSource::File {
            lines.push(format!("printf '%s' VALUE > {}", input.sample_file_name()));
        }
    }
    let invocation = format!("{command} {args}").trim().to_string();
    if spec
        .inputs
        .iter()
        .any(|input| input.sources[0] == InputSource::Stdin)
    {
        lines.push(format!("printf '%s\\n' VALUE | {invocation}"));
        lines.push(format!("printf '%s\\n' VALUE | {invocation} --output json"));
    } else {
        lines.push(invocation.clone());
        lines.push(format!("{invocation} --output json"));
    }
    lines.join("\n")
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

// rustfmt's defaults: generated code is laid out as rustfmt would, so a fresh project is clean.
const ATTR_FN_LIKE_WIDTH: usize = 70;
const MAX_WIDTH: usize = 100;
const FN_CALL_WIDTH: usize = 60;

fn attribute(name: &str, arguments: &[String], indent: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let inline = arguments.join(", ");
    if inline.width() <= ATTR_FN_LIKE_WIDTH.saturating_sub(indent) {
        return format!("#[{name}({inline})]");
    }
    let pad = " ".repeat(indent);
    let lines = arguments
        .iter()
        .map(|argument| format!("{pad}    {argument}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("#[{name}(\n{lines}\n{pad})]")
}

fn dispatch_attribute(spec: &ProjectSpec) -> String {
    let mut arguments = vec!["pure".to_string(), "default".to_string()];
    // The derive registers the kebab-case name; only an underscore spelling needs `name`.
    if spec.command_name.contains('_') {
        arguments.push(format!("name = {}", quote(&spec.command_name)));
    }
    if has_chain_inputs(spec) {
        arguments.push(format!(
            "inputs = crate::handlers::{}_inputs",
            spec.command_name.replace('-', "_")
        ));
    }
    attribute("dispatch", &arguments, 4)
}

fn cli_command_attribute(spec: &ProjectSpec) -> String {
    let name = quote(&spec.executable_name);
    let about = quote(&spec.command_description);
    let arguments = [format!("name = {name}"), format!("about = {about}")];
    attribute("command", &arguments, 0)
}

fn toml_basic_string_content(path: &Path) -> String {
    let mut escaped = String::new();
    for character in path.to_string_lossy().chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\u{000C}' => escaped.push_str("\\f"),
            '\r' => escaped.push_str("\\r"),
            character if character <= '\u{001F}' || character == '\u{007F}' => {
                escaped.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn rust_array(items: &[String], indent: usize, max_inline_len: usize) -> String {
    let inline = format!("[{}]", items.join(", "));
    if inline.len() <= max_inline_len {
        return inline;
    }
    let spaces = " ".repeat(indent);
    let mut output = String::from("[\n");
    for item in items {
        output.push_str(&spaces);
        output.push_str(item);
        output.push_str(",\n");
    }
    output.push_str(&" ".repeat(indent.saturating_sub(4)));
    output.push(']');
    output
}

impl CommandInput {
    /// Reaches the command through an `InputChain`, not a typed `#[handler]` parameter.
    fn is_chain(&self) -> bool {
        self.sources != [InputSource::Argument]
    }

    fn chain_expr(&self, indent: usize) -> String {
        use unicode_width::UnicodeWidthStr;

        let sources = self
            .sources
            .iter()
            .map(|source| match source {
                InputSource::Argument => {
                    format!(".try_source(ArgSource::new({}))", quote(&self.name))
                }
                InputSource::File => format!(
                    ".try_source(FileSource::new({}))",
                    quote(&format!("{}_file", self.name))
                ),
                InputSource::Stdin => ".try_source(StdinSource::new())".to_string(),
            })
            .collect::<Vec<_>>();
        let inline = format!("InputChain::<String>::new(){}", sources.concat());
        // A trailing comma follows the chain on its line.
        if indent + inline.width() < MAX_WIDTH {
            return inline;
        }
        let pad = " ".repeat(indent + 4);
        let lines = sources
            .iter()
            .map(|source| format!("{pad}{source}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("InputChain::<String>::new()\n{lines}")
    }

    /// An underscored name is spelled out: clap ids after the field, `#[handler]` hyphenates.
    fn handler_param(&self) -> String {
        let attribute = if self.cardinality == InputCardinality::Boolean {
            "flag"
        } else {
            "arg"
        };
        let attribute = if self.name.contains('_') {
            format!("#[{attribute}(name = {})]", quote(&self.name))
        } else {
            format!("#[{attribute}]")
        };
        format!("{attribute} {}: {}", self.name, self.rust_type())
    }

    fn handler_test_value(&self, value: &str) -> String {
        match (self.value_type, self.cardinality) {
            (InputValueType::String, InputCardinality::Required) => {
                format!("{}.to_string()", quote(value))
            }
            (InputValueType::String, InputCardinality::Optional) => {
                format!("Some({}.to_string())", quote(value))
            }
            (InputValueType::String, InputCardinality::Repeated) => {
                format!("vec![{}.to_string(), \"extra\".to_string()]", quote(value))
            }
            (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
            (InputValueType::Path, InputCardinality::Required) => {
                format!("std::path::PathBuf::from({})", quote(value))
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                format!("Some(std::path::PathBuf::from({}))", quote(value))
            }
            (InputValueType::Path, InputCardinality::Repeated) => format!(
                "vec![std::path::PathBuf::from({}), std::path::PathBuf::from(\"extra.toml\")]",
                quote(value)
            ),
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

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
        let mut args = Vec::new();
        let argument = match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                Some(format!("#[arg(long = \"{long}\", action = clap::ArgAction::SetTrue)]\n        {}: bool,", self.name))
            }
            (InputValueType::Path, InputCardinality::Required) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: std::path::PathBuf,", self.name))
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Option<std::path::PathBuf>,", self.name))
            }
            (InputValueType::Path, InputCardinality::Repeated) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Vec<std::path::PathBuf>,", self.name))
            }
            (_, InputCardinality::Required) => {
                if !self.sources.contains(&InputSource::Argument) {
                    None
                } else if self.sources == [InputSource::Argument] {
                    Some(format!("#[arg(long = \"{long}\")]\n        {}: String,", self.name))
                } else {
                    Some(format!(
                        "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                        self.name
                    ))
                }
            }
            (_, InputCardinality::Optional) => {
                self.sources.contains(&InputSource::Argument).then(|| format!(
                    "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                    self.name
                ))
            }
            (_, InputCardinality::Repeated) => {
                Some(format!(
                    "#[arg(long = \"{long}\")]\n        {}: Vec<String>,",
                    self.name
                ))
            }
            _ => unreachable!("validated input combinations are renderable"),
        };
        if let Some(argument) = argument {
            args.push(argument);
        }
        if self.value_type == InputValueType::String && self.sources.contains(&InputSource::File) {
            args.push(format!(
                "#[arg(long = \"{long}-file\", value_name = \"PATH\")]\n        {0}_file: Option<std::path::PathBuf>,",
                self.name
            ));
        }
        args.join("\n        ")
    }

    fn sample_args_for_source(&self, source: InputSource, primary_value: &str) -> Vec<String> {
        let long = format!("--{}", self.name.replace('_', "-"));
        match source {
            InputSource::File => {
                return vec![
                    quote(&format!("{long}-file")),
                    quote(&self.sample_file_name()),
                ];
            }
            InputSource::Stdin => return Vec::new(),
            InputSource::Argument => {}
        }
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => vec![quote(&long)],
            (InputValueType::String, InputCardinality::Required | InputCardinality::Optional) => {
                if self.sources.contains(&InputSource::Argument) {
                    vec![quote(&long), quote(primary_value)]
                } else {
                    Vec::new()
                }
            }
            (InputValueType::String, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra"),
            ],
            (InputValueType::Path, InputCardinality::Required | InputCardinality::Optional) => {
                vec![quote(&long), quote(primary_value)]
            }
            (InputValueType::Path, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra.toml"),
            ],
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn sample_file_name(&self) -> String {
        format!("{}-input.txt", self.name.replace('_', "-"))
    }

    fn readme_args(&self, value: &str) -> Vec<String> {
        let long = format!("--{}", self.name.replace('_', "-"));
        match self.sources[0] {
            InputSource::File => vec![format!("{long}-file"), self.sample_file_name()],
            InputSource::Stdin => Vec::new(),
            InputSource::Argument => match (self.value_type, self.cardinality) {
                (InputValueType::Bool, InputCardinality::Boolean) => vec![long],
                (_, InputCardinality::Repeated) => {
                    vec![long.clone(), value.into(), long, "EXTRA".into()]
                }
                _ => vec![long, value.into()],
            },
        }
    }

    fn core_call_arg(&self) -> String {
        if self.is_chain() {
            format!("{}.clone()", self.name)
        } else {
            self.name.clone()
        }
    }

    fn core_validation(&self) -> Option<String> {
        if self.value_type == InputValueType::String
            && self.cardinality == InputCardinality::Required
        {
            Some(format!(
                "if {}.trim().is_empty() {{\n        return Err(CoreError::EmptyInput {{ field: {} }});\n    }}",
                self.name,
                quote(&self.name)
            ))
        } else {
            None
        }
    }
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    new_environment()
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
        cli_command_attribute => cli_command_attribute(spec),
        input_name => primary.name,
        inputs => spec.inputs.iter().map(|input| {
            context! {
                name => input.name,
                cli_arg => input.cli_arg(),
                core_call_arg => input.core_call_arg(),
                rust_type => input.rust_type(),
                policy => input.policy_sentence(),
            }
        }).collect::<Vec<_>>(),
        core_params => spec.inputs.iter().map(core_signature_fragment).collect::<Vec<_>>().join(", "),
        core_fn_signature => core_fn_signature(spec),
        core_call_args => spec.inputs.iter().map(CommandInput::core_call_arg).collect::<Vec<_>>().join(", "),
        core_validations => spec.inputs.iter().filter_map(CommandInput::core_validation).collect::<Vec<_>>().join("\n    "),
        core_unused_inputs => core_unused_inputs(spec),
        cli_args => spec.inputs.iter().map(CommandInput::cli_arg).collect::<Vec<_>>().join("\n        "),
        dispatch_attribute => dispatch_attribute(spec),
        lib_crate => spec.lib_crate,
        lib_package => spec.lib_crate.replace('_', "-"),
        operation_name => spec.operation_name,
        view_name => spec.view_name,
        result_shape => spec.result_shape.as_str(),
        core_primary_value => core_primary_value(spec),
        core_result_fields => result_fields(spec, "pub "),
        cli_view_fields => result_fields(spec, "pub(crate) "),
        result_init => result_init(spec),
        view_from_fields => view_from_fields(spec),
        core_valid_assertions => core_valid_assertions(spec),
        core_sample_result => core_sample_result(spec),
        core_invalid_test => core_invalid_test(spec),
        handler_expected_fields => handler_expected_fields(spec),
        handler_imports => handler_imports(spec),
        handler_signature => handler_signature(spec),
        handler_input_reads => handler_input_reads(spec),
        handler_call => handler_call(spec),
        handler_test_inputs => handler_test_inputs(spec),
        command_inputs_fn => command_inputs_fn(spec),
        file_source_item => file_source_item(spec),
        pipeline_human_run => generated_harness_run(spec, "Ada", &sample_cli_args(spec, "Ada")),
        pipeline_json_test => generated_json_pipeline_test(spec),
        template_body => template_body(spec),
        human_expected => quote(&expected_first_field(spec, "Ada").1),
        readme_input_policy => readme_input_policy(spec),
        readme_validation_note => readme_validation_note(spec),
        readme_examples => readme_examples(spec),
        command_syntax => spec.inputs.iter().map(command_syntax_fragment).collect::<Vec<_>>().join(" "),
        standout_version => spec.standout_version,
        clapfig_version => CLAPFIG_VERSION,
        config_test => generated_config_test(spec),
        local_patch_root => spec.local_patch_root.as_deref().map(toml_basic_string_content),
    }
}

const CLAPFIG_VERSION: &str = "0.26";

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
    ("crates/{{ executable_name }}/README.md", "readme"),
    ("crates/{{ executable_name }}/src/main.rs", "main"),
    ("crates/{{ executable_name }}/src/cli.rs", "cli"),
    ("crates/{{ executable_name }}/src/config.rs", "config"),
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
resolver = "3"
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
    #[error("{field} cannot be empty")]
    EmptyInput { field: &'static str },
}

/// Runs the CLI-free core operation for the generated command.
///
/// The caller supplies explicit values. This crate deliberately has no Clap,
/// Standout, template, terminal, environment, or CLI-view dependencies.
pub fn {{ operation_name }}{{ core_fn_signature }} -> Result<{{ view_name }}, CoreError> {
{%- if core_validations %}
    {{ core_validations }}
{%- endif %}
{%- if core_unused_inputs %}
    {{ core_unused_inputs }}
{%- endif %}
    let primary = {{ core_primary_value }};
    Ok({{ view_name }} {
{{ result_init }}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_returns_a_typed_result() {
{{ core_sample_result }}

{{ core_valid_assertions }}
    }

{{ core_invalid_test }}
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
clapfig = "{{ clapfig_version }}"
serde = { version = "1", features = ["derive"] }
standout = "{{ standout_version }}"
standout-dispatch = "{{ standout_version }}"
standout-input = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
tempfile = "3"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod config;
mod handlers;

use anyhow::Result;
use standout::{embed_styles, embed_templates};

fn main() -> Result<()> {
    let app = build_app(clapfig::SearchPath::Platform)?;
    app.run(cli::command(), std::env::args());
    Ok(())
}

fn build_app(user_scope: clapfig::SearchPath) -> Result<standout::cli::App> {
    Ok(standout::cli::App::builder()
        .name(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("{{ project_name }}")
        .config(config::builder(user_scope))
        .term_settings(|config: &config::Config| &config.term)
        .commands(cli::Commands::dispatch_config())?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serial_test::serial;
    use standout::Representation;
    use standout_test::TestHarness;
    use tempfile::TempDir;

    const CONFIG_FILE: &str = "[term]\noutput = \"json\"\n";

    fn test_app() -> (TempDir, standout::cli::App) {
        let user_dir = TempDir::new().unwrap();
        let app = build_app(clapfig::SearchPath::Path(user_dir.path().to_path_buf())).unwrap();
        (user_dir, app)
    }

    #[test]
    #[serial]
    fn pipeline_renders_human_output_from_argument() {
        let (_user_dir, app) = test_app();

{{ pipeline_human_run }}

        result.assert_success();
        result.assert_stdout_contains({{ human_expected }});
    }

{{ pipeline_json_test }}

{{ config_test }}
}
"#,
    ),
    (
        "cli",
        r#"use clap::{CommandFactory, Parser, Subcommand};
use standout::cli::Dispatch;

#[derive(Parser)]
{{ cli_command_attribute }}
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = crate::handlers)]
pub(crate) enum Commands {
    /// {{ command_description }}
    #[command(name = "{{ command_name }}")]
    {{ dispatch_attribute }}
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
        "config",
        r#"use serde::{Deserialize, Serialize};
use standout::TermSettings;

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
pub(crate) struct Config {
    pub(crate) term: TermSettings,
}

pub(crate) fn builder(user_scope: clapfig::SearchPath) -> clapfig::TypedBuilder<Config> {
    clapfig::Clapfig::typed::<Config>()
        .app_name("{{ executable_name }}")
        .add_search_path(user_scope.clone())
        .add_search_path(clapfig::SearchPath::Cwd)
        .persist_scope("local", clapfig::SearchPath::Cwd)
        .persist_scope("global", user_scope)
}
"#,
    ),
    (
        "handlers",
        r#"{{ handler_imports }}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct {{ view_name }} {
{{ cli_view_fields }}
}

impl From<core::{{ view_name }}> for {{ view_name }} {
    fn from(value: core::{{ view_name }}) -> Self {
        Self {
{{ view_from_fields }}
        }
    }
}
{%- if file_source_item %}

{{ file_source_item }}
{%- endif %}
{%- if command_inputs_fn %}

{{ command_inputs_fn }}
{%- endif %}

/// Adapts typed shell input into the CLI-free core operation.
///
/// Values that can come from more than one place are resolved before dispatch
/// by the command's input chains; the rest arrive as typed parameters. The
/// handler returns data for Standout to render or serialize.
#[handler]
{{ handler_signature }}
{%- if handler_input_reads %}
{{ handler_input_reads }}
{%- endif %}
    let result = core::{{ operation_name }}({{ core_call_args }})?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_handler_maps_input_to_core_and_view() {
{%- if handler_test_inputs %}
{{ handler_test_inputs }}

{%- endif %}
{{ handler_call }}
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
{{ readme_examples }}
```

Command syntax:

```text
{{ executable_name }} {{ command_name }} {{ command_syntax }}
```

Input policy:

{{ readme_input_policy }}

{%- if readme_validation_note %}

{{ readme_validation_note }}
{%- endif %}

The generated `{{ result_shape }}` result is intentionally small. The handler
maps resolved shell input into `{{ lib_crate }}::{{ operation_name }}` and maps
the core result into the CLI-owned `{{ view_name }}`.

Verify the project with:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
```
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use standout_input::{
        questionnaire::QuestionnaireInput, InputSources, PromptResponse, ScriptedResponder,
    };
    use std::process::Command;
    use std::sync::Arc;
    use tempfile::TempDir;

    struct PromptResponderGuard {
        sources: InputSources,
    }

    impl PromptResponderGuard {
        fn install(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
            Self {
                sources: InputSources::from_process()
                    .with_responder(Arc::new(ScriptedResponder::new(responses))),
            }
        }

        fn sources(&self) -> &InputSources {
            &self.sources
        }
    }

    fn required_string(name: impl Into<String>) -> CommandInput {
        CommandInput {
            name: name.into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
        }
    }

    fn sample_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "hello-tool".into(),
            executable_name: "hello-tool".into(),
            command_name: "greet".into(),
            command_description: "Greet one value".into(),
            inputs: vec![required_string("name")],
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
    fn input_source_aliases_cannot_declare_the_same_source_twice() {
        let duplicate = parse_input_sources("argument,arg").unwrap_err();

        assert!(duplicate.to_string().contains("declared more than once"));
    }

    #[test]
    fn project_spec_is_private_validated_model() {
        let spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
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
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello-tool/README.md")));
        assert!(!spec.destination.exists());
    }

    #[test]
    fn generated_manifests_only_depend_on_publishable_workspace_crates() {
        let dir = TempDir::new().unwrap();
        let mut spec = sample_spec(dir.path());
        spec.local_patch_root = None;
        let generated = GeneratedFiles::render(&spec).unwrap();
        write_generated_files(&spec.destination, &generated).unwrap();

        let emitted = generated_family_crates_io_deps(&spec.destination.join("Cargo.toml"));
        assert!(
            !emitted.is_empty(),
            "the generated project is expected to depend on the standout family"
        );

        let publishable = workspace_crates_io_publishable();
        let stranded: Vec<&String> = emitted
            .iter()
            .filter(|name| publishable.get(*name) != Some(&true))
            .collect();
        assert!(
            stranded.is_empty(),
            "the wizard emits crates.io dependencies on workspace crates that are not \
             published to crates.io: {stranded:?}. A generated project cannot resolve \
             them at any version this workspace pins. Publish those crates or stop \
             generating a dependency on them.\nemitted: {emitted:?}\npublishable: {publishable:?}"
        );

        assert!(emitted.contains("clapfig"), "{emitted:?}");
        assert_eq!(
            crates_io_requirement(
                &spec.destination.join("Cargo.toml"),
                "hello-tool",
                "clapfig"
            ),
            crates_io_requirement(&workspace_root().join("Cargo.toml"), "standout", "clapfig"),
            "the generated project must pin clapfig at the requirement standout itself uses"
        );
    }

    #[test]
    fn result_fields_and_generated_identifiers_are_validated() {
        let duplicate = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "summary".into()],
        })
        .unwrap_err();
        let keyword = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "match".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(duplicate.to_string().contains("declared more than once"));
        assert!(keyword.to_string().contains("reserved Rust keyword"));
    }

    #[test]
    fn generated_flags_cannot_collide_across_inputs() {
        let answers_with = |inputs| TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs,
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        };

        for reserved in ["output", "output_file_path", "help"] {
            let error = ProjectSpec::from_answers(answers_with(vec![required_string(reserved)]))
                .unwrap_err();
            assert!(error.to_string().contains("reserved framework/Clap flag"));
            assert!(error
                .to_string()
                .contains(&format!("--{}", reserved.replace('_', "-"))));
        }

        let derived_collision = ProjectSpec::from_answers(answers_with(vec![
            required_string("document"),
            CommandInput {
                name: "document_file".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ]))
        .unwrap_err();
        assert!(derived_collision.to_string().contains("--document-file"));
        assert!(derived_collision.to_string().contains("conflicts"));
    }

    #[test]
    fn render_omits_local_patch_paths_by_default() {
        let spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(!manifest.contains("[patch.crates-io]"));
        assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn local_patch_paths_are_escaped_as_toml_basic_string_content() {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.local_patch_root = Some(PathBuf::from(r#"C:\Users\Ada "Q"\standout"#));

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(manifest
            .contains(r#"standout = { path = "C:\\Users\\Ada \"Q\"\\standout/crates/standout" }"#));
        assert!(!manifest.contains(r#"path = "C:\Users"#));
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

        let spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: rich_inputs,
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();

        assert_eq!(
            spec.inputs[0].policy_sentence(),
            "document comes from --document, then --document-file, then piped stdin"
        );
    }

    #[test]
    fn generated_sources_commands_and_validation_preserve_the_validated_model() {
        let file_only = CommandInput {
            name: "document".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::File],
        };
        assert_eq!(
            command_syntax_fragment(&file_only),
            "--document-file <PATH>"
        );
        assert_eq!(
            command_syntax_fragment(&CommandInput {
                name: "tag".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Repeated,
                sources: vec![InputSource::Argument],
            }),
            "[--tag <tag>]..."
        );

        let spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "send_email".into(),
            command_description: "Send one message".into(),
            inputs: vec![
                file_only,
                CommandInput {
                    name: "subject".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Required,
                    sources: vec![InputSource::Stdin],
                },
            ],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let cli = generated
            .files
            .get(Path::new("crates/demo/src/cli.rs"))
            .unwrap();
        let handlers = generated
            .files
            .get(Path::new("crates/demo/src/handlers.rs"))
            .unwrap();
        let core = generated
            .files
            .get(Path::new("crates/demolib/src/lib.rs"))
            .unwrap();
        let readme = generated
            .files
            .get(Path::new("crates/demo/README.md"))
            .unwrap();

        assert!(cli.contains("#[command(name = \"send_email\")]"));
        assert!(cli.contains("long = \"document-file\""));
        assert!(!cli.contains("long = \"document\""));
        assert!(!cli.contains("long = \"subject\""));
        assert!(handlers.contains("    fn typed_handler_maps_input_to_core_and_view()"));
        assert!(core.contains("CoreError::EmptyInput { field: \"document\" }"));
        assert!(core.contains("CoreError::EmptyInput { field: \"subject\" }"));
        assert!(readme.contains("demo send_email --document-file <PATH> <piped stdin>"));
    }

    #[test]
    fn rejects_unsupported_input_combinations_before_rendering() {
        let error = ProjectSpec::from_answers(TestProjectAnswers {
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
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("bool inputs must use boolean cardinality"));

        let error = ProjectSpec::from_answers(TestProjectAnswers {
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
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("path inputs only support argument source"));
    }

    #[test]
    fn path_input_rendering_does_not_emit_string_validation() {
        let spec = ProjectSpec::from_answers(TestProjectAnswers {
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
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
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

    fn spec_with_description(description: &str) -> ProjectSpec {
        ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: description.into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap()
    }

    #[test]
    fn command_attribute_splits_exactly_at_the_rustfmt_width_boundary() {
        let at_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 25));
        assert_eq!(
            cli_command_attribute(&at_limit),
            format!(
                "#[command(name = \"demo\", about = \"{}\")]",
                "a".repeat(ATTR_FN_LIKE_WIDTH - 25)
            )
        );

        let over_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 24));
        assert_eq!(
            cli_command_attribute(&over_limit),
            format!(
                "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
                "a".repeat(ATTR_FN_LIKE_WIDTH - 24)
            )
        );
    }

    #[test]
    fn command_attribute_measures_display_width_not_char_count() {
        let inline = spec_with_description(&"検".repeat(22));
        assert_eq!(
            cli_command_attribute(&inline),
            format!(
                "#[command(name = \"demo\", about = \"{}\")]",
                "検".repeat(22)
            )
        );

        let split = spec_with_description(&"検".repeat(23));
        assert_eq!(
            cli_command_attribute(&split),
            format!(
                "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
                "検".repeat(23)
            )
        );
    }

    #[test]
    fn long_description_generated_project_is_rustfmt_clean() {
        let dir = TempDir::new().unwrap();
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "provisioning-tool".into(),
            executable_name: "provisioning-tool".into(),
            command_name: "provision".into(),
            command_description: "Provisions pinned env either container or bare metal".into(),
            inputs: vec![required_string("target")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = dir.path().join("provisioning-tool");
        spec.local_patch_root = Some(workspace_root());

        publish_project(&spec).unwrap();

        let cli = fs::read_to_string(spec.destination.join("crates/provisioning-tool/src/cli.rs"))
            .unwrap();
        assert!(cli.contains(
            "#[command(\n    name = \"provisioning-tool\",\n    \
             about = \"Provisions pinned env either container or bare metal\"\n)]"
        ));
        run_cargo(&spec.destination, ["fmt", "--check"]);
    }

    fn rich_questionnaire_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "inspect-tool".into(),
            executable_name: "inspect-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect document input".into(),
            inputs: vec![
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
                    name: "tag".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Repeated,
                    sources: vec![InputSource::Argument],
                },
                CommandInput {
                    name: "config".into(),
                    value_type: InputValueType::Path,
                    cardinality: InputCardinality::Optional,
                    sources: vec![InputSource::Argument],
                },
            ],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into(), "echo".into()],
        })
        .unwrap();
        spec.destination = root.join("inspect-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn file_only_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "file-tool".into(),
            executable_name: "file-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect file contents".into(),
            inputs: vec![CommandInput {
                name: "document".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::File],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join("file-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn single_input_spec(root: &Path, project_name: &str, input: CommandInput) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: project_name.into(),
            executable_name: project_name.into(),
            command_name: "inspect".into(),
            command_description: "Inspect configured input".into(),
            inputs: vec![input],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join(project_name);
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn path_first_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "config-tool".into(),
            executable_name: "config-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect a config path".into(),
            inputs: vec![
                CommandInput {
                    name: "config".into(),
                    value_type: InputValueType::Path,
                    cardinality: InputCardinality::Required,
                    sources: vec![InputSource::Argument],
                },
                CommandInput {
                    name: "note".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Optional,
                    sources: vec![InputSource::Argument],
                },
            ],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join("config-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn generated_source(generated: &GeneratedFiles, path: &str) -> String {
        generated
            .files
            .get(Path::new(path))
            .unwrap_or_else(|| panic!("{path} is generated"))
            .clone()
    }

    // The negative assertions alone would pass on output that dropped the concern entirely.
    #[test]
    fn generated_project_uses_the_blessed_idioms() {
        let dir = TempDir::new().unwrap();
        let generated = GeneratedFiles::render(&rich_questionnaire_spec(dir.path())).unwrap();
        let cli = generated_source(&generated, "crates/inspect-tool/src/cli.rs");
        let main = generated_source(&generated, "crates/inspect-tool/src/main.rs");
        let handlers = generated_source(&generated, "crates/inspect-tool/src/handlers.rs");

        assert!(cli.contains("#[derive(Subcommand, Dispatch)]"));
        assert!(cli.contains("#[dispatch(handlers = crate::handlers)]"));
        assert!(
            cli.contains("#[dispatch(pure, default, inputs = crate::handlers::inspect_inputs)]")
        );
        assert!(main.contains(".commands(cli::Commands::dispatch_config())?"));
        assert!(main.contains("build_app(clapfig::SearchPath::Platform)?"));
        assert!(main.contains("fn build_app(user_scope: clapfig::SearchPath)"));
        assert!(main.contains(".config(config::builder(user_scope))"));
        assert!(
            main.contains("build_app(clapfig::SearchPath::Path(user_dir.path().to_path_buf()))")
        );
        assert!(main.contains(".term_settings(|config: &config::Config| &config.term)"));
        assert!(main.contains("fn term_output_in_the_config_file_selects_json()"));
        assert!(main.contains(".fixture(\"inspect-tool.toml\", CONFIG_FILE)"));
        assert!(main.contains("[\"inspect-tool\", \"config\", \"get\", \"term.output\"]"));

        let config = generated_source(&generated, "crates/inspect-tool/src/config.rs");
        assert!(config.contains("#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]"));
        assert!(config.contains("pub(crate) term: TermSettings,"));
        assert!(config.contains(".app_name(\"inspect-tool\")"));
        assert!(config.contains("fn builder(user_scope: clapfig::SearchPath)"));
        assert!(config.contains(".add_search_path(user_scope.clone())"));
        assert!(config.contains(".add_search_path(clapfig::SearchPath::Cwd)"));
        assert!(config.contains(".persist_scope(\"global\", user_scope)"));

        assert!(handlers.contains("#[handler]"));
        assert!(handlers.contains("#[flag] verbose: bool"));
        assert!(handlers.contains("#[arg] config: Option<std::path::PathBuf>"));

        assert!(cli.contains("#[derive(Parser)]"));
        assert!(cli.contains("Cli::command()"));

        assert!(main.contains(".templates(embed_templates!(\"src/templates\"))"));

        assert!(main.contains(".styles(embed_styles!(\"src/styles\"))"));
        assert!(main.contains(".default_theme(\"inspect-tool\")"));

        assert!(cli.contains("inputs = crate::handlers::inspect_inputs"));
        assert!(handlers.contains(
            "pub(crate) fn inspect_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H>"
        ));
        assert!(handlers.contains("InputChain::<String>::new()"));
        assert!(handlers.contains(".try_source(ArgSource::new(\"document\"))"));
        assert!(handlers.contains(".try_source(StdinSource::new())"));
        assert!(handlers.contains("ctx.input(\"document\")?"));

        assert!(main.contains("app.run(cli::command(), std::env::args());"));
        assert!(main.contains(".version(env!(\"CARGO_PKG_VERSION\"))"));

        assert!(!main.contains("command_with"));
        assert!(!main.contains("FnHandler"));
        assert!(!main.contains("AppBuilder::default"));
        assert!(!main.contains("template_name"));
        assert!(!main.contains("help_handling"));
        assert!(!main.contains("Theme::"));
        assert!(!handlers.contains("#[matches]"));
        assert!(!handlers.contains("matches.get_one::<String>"));
        assert!(!cli.contains("#[derive(Subcommand)]"));
    }

    #[test]
    fn file_source_reports_file_provenance_and_names_an_unreadable_path() {
        let dir = TempDir::new().unwrap();
        let generated = GeneratedFiles::render(&file_only_spec(dir.path())).unwrap();
        let handlers = generated_source(&generated, "crates/file-tool/src/handlers.rs");

        assert!(handlers.contains("    fn name(&self) -> &'static str {\n        \"file\"\n    }"));
        assert!(handlers
            .contains("standout::input::InputError::file(path.display().to_string(), error)"));
        assert!(!handlers.contains("InputError::parse(self.arg"));
        assert!(handlers.contains("source: standout_input::InputSourceKind::File,"));
    }

    #[test]
    fn argument_only_input_stays_a_typed_handler_parameter() {
        let dir = TempDir::new().unwrap();
        let spec = single_input_spec(
            dir.path(),
            "bool-tool",
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
        );
        let generated = GeneratedFiles::render(&spec).unwrap();
        let cli = generated_source(&generated, "crates/bool-tool/src/cli.rs");
        let handlers = generated_source(&generated, "crates/bool-tool/src/handlers.rs");

        assert!(handlers.contains("pub(crate) fn inspect(#[flag] verbose: bool)"));
        assert!(!handlers.contains("InputChain"));
        assert!(!handlers.contains("CommandContext"));
        assert!(!cli.contains("inputs = "));
    }

    #[test]
    fn an_underscored_input_name_spells_out_the_argument_id() {
        let dir = TempDir::new().unwrap();
        let spec = single_input_spec(
            dir.path(),
            "note-tool",
            CommandInput {
                name: "note_text".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument],
            },
        );
        let generated = GeneratedFiles::render(&spec).unwrap();
        let handlers = generated_source(&generated, "crates/note-tool/src/handlers.rs");

        assert!(handlers.contains("#[arg(name = \"note_text\")] note_text: String"));
    }

    /// An optional value with a second source has no blessed spelling, so the wizard refuses it.
    #[test]
    fn an_optional_input_cannot_take_a_second_source() {
        let error = ProjectSpec::from_answers(TestProjectAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "note".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument, InputSource::Stdin],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("optional inputs only support argument source"));
    }

    #[test]
    fn generated_project_matrix_formats_checks_tests_and_runs() {
        let dir = TempDir::new().unwrap();
        let mut message = sample_spec(dir.path());
        message.result_shape = ResultShape::Message;
        message.record_fields.clear();
        let rich = rich_questionnaire_spec(dir.path());
        let path_first = path_first_spec(dir.path());
        let file_only = file_only_spec(dir.path());
        let bool_first = single_input_spec(
            dir.path(),
            "bool-tool",
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
        );
        let optional_first = single_input_spec(
            dir.path(),
            "optional-tool",
            CommandInput {
                name: "note".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        );
        let repeated_first = single_input_spec(
            dir.path(),
            "repeated-tool",
            CommandInput {
                name: "tag".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Repeated,
                sources: vec![InputSource::Argument],
            },
        );

        for spec in [
            &message,
            &rich,
            &path_first,
            &file_only,
            &bool_first,
            &optional_first,
            &repeated_first,
        ] {
            publish_project(spec).unwrap();
            run_cargo(&spec.destination, ["fmt", "--check"]);
            run_cargo(&spec.destination, ["check", "--workspace"]);
            run_cargo(&spec.destination, ["test", "--workspace"]);
        }

        let file_readme =
            fs::read_to_string(file_only.destination.join("crates/file-tool/README.md")).unwrap();
        assert!(file_readme.contains("--document-file document-input.txt"));
        assert!(!file_readme.contains("--document VALUE"));
        assert!(file_readme
            .contains("Blank values for the required string input `document` are rejected"));
        let bool_readme =
            fs::read_to_string(bool_first.destination.join("crates/bool-tool/README.md")).unwrap();
        assert!(bool_readme.contains("inspect --verbose"));
        assert!(!bool_readme.contains("Blank values"));
        let path_readme =
            fs::read_to_string(path_first.destination.join("crates/config-tool/README.md"))
                .unwrap();
        assert!(!path_readme.contains("Blank values"));
        let optional_readme = fs::read_to_string(
            optional_first
                .destination
                .join("crates/optional-tool/README.md"),
        )
        .unwrap();
        assert!(!optional_readme.contains("Blank values"));
        let repeated_readme = fs::read_to_string(
            repeated_first
                .destination
                .join("crates/repeated-tool/README.md"),
        )
        .unwrap();
        assert!(!repeated_readme.contains("Blank values"));

        let file_input = file_only.destination.join("document.txt");
        fs::write(&file_input, "File only").unwrap();
        let file_only_run = run_binary(
            &file_only.destination,
            [
                "run",
                "-q",
                "-p",
                "file-tool",
                "--",
                "inspect",
                "--document-file",
                file_input.to_str().unwrap(),
            ],
        );
        assert!(String::from_utf8(file_only_run.stdout)
            .unwrap()
            .contains("Processed File only"));

        let missing_file_run = Command::new("cargo")
            .current_dir(&file_only.destination)
            .args([
                "run",
                "-q",
                "-p",
                "file-tool",
                "--",
                "inspect",
                "--document-file",
                "absent-document.txt",
            ])
            .output()
            .unwrap();
        assert!(!missing_file_run.status.success());
        let missing_file_stderr = String::from_utf8(missing_file_run.stderr).unwrap();
        assert!(
            missing_file_stderr.contains("absent-document.txt"),
            "the unreadable path belongs in the diagnostic\nstderr:\n{missing_file_stderr}"
        );

        let message_human = run_binary(
            &message.destination,
            [
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "Ada",
            ],
        );
        let stdout = String::from_utf8(message_human.stdout).unwrap();
        assert!(stdout.contains("Processed Ada"));

        let help = run_binary(
            &message.destination,
            ["run", "-q", "-p", "hello-tool", "--", "--help"],
        );
        let help = String::from_utf8(help.stdout).unwrap();
        assert!(help.contains("USAGE"), "unexpected help page:\n{help}");
        assert!(!help.contains("Usage:"), "unexpected help page:\n{help}");

        let bare = run_binary(
            &message.destination,
            ["run", "-q", "-p", "hello-tool", "--", "--name", "Ada"],
        );
        assert!(String::from_utf8(bare.stdout)
            .unwrap()
            .contains("Processed Ada"));

        let human = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--verbose",
                "--tag",
                "alpha",
                "--tag",
                "beta",
                "--config",
                "settings.toml",
            ],
        );
        let stdout = String::from_utf8(human.stdout).unwrap();
        assert!(stdout.contains("Ada"));
        assert!(stdout.contains("Summary:"));
        assert!(stdout.contains("Echo: Ada"));

        let json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--output",
                "json",
            ],
        );
        let value = json_value(&json);
        assert_eq!(value["summary"], "Processed Ada");
        assert_eq!(value["count"], "3");
        assert_eq!(value["echo"], "Ada");

        let input_file = rich.destination.join("input.txt");
        fs::write(&input_file, "File Ada").unwrap();
        let file_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&file_json);
        assert_eq!(value["summary"], "Processed File Ada");
        assert_eq!(value["count"], "8");

        let stdin_json = run_binary_with_stdin(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--output",
                "json",
            ],
            "Pipe Ada\n",
        );
        let value = json_value(&stdin_json);
        assert_eq!(value["summary"], "Processed Pipe Ada");
        assert_eq!(value["count"], "8");

        let precedence_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Arg Ada",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&precedence_json);
        assert_eq!(value["summary"], "Processed Arg Ada");
        assert_eq!(value["count"], "7");

        let invalid = Command::new("cargo")
            .current_dir(&rich.destination)
            .args([
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "   ",
            ])
            .output()
            .unwrap();
        assert!(!invalid.status.success());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("document cannot be empty"));

        let missing = Command::new("cargo")
            .current_dir(&rich.destination)
            .args(["run", "-q", "-p", "inspect-tool", "--", "inspect"])
            .output()
            .unwrap();
        assert!(!missing.status.success());
        let missing = String::from_utf8_lossy(&missing.stderr);
        assert!(
            missing.contains("input `document`"),
            "unexpected stderr: {missing}"
        );
        assert!(
            missing.contains("No input provided"),
            "unexpected stderr: {missing}"
        );
    }

    fn cargo_metadata(manifest: &Path) -> serde_json::Value {
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(manifest)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo metadata failed for {}\nstderr:\n{}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

    fn generated_family_crates_io_deps(manifest: &Path) -> std::collections::BTreeSet<String> {
        let metadata = cargo_metadata(manifest);
        let mut names = std::collections::BTreeSet::new();
        for package in metadata["packages"].as_array().unwrap() {
            for dependency in package["dependencies"].as_array().unwrap() {
                let name = dependency["name"].as_str().unwrap();
                let from_crates_io = dependency["source"].as_str() == Some(CRATES_IO_SOURCE);
                if from_crates_io && (name.starts_with("standout") || name == "clapfig") {
                    names.insert(name.to_string());
                }
            }
        }
        names
    }

    fn workspace_crates_io_publishable() -> std::collections::BTreeMap<String, bool> {
        let metadata = cargo_metadata(&workspace_root().join("Cargo.toml"));
        let mut publishable = std::collections::BTreeMap::new();
        for package in metadata["packages"].as_array().unwrap() {
            let registries = &package["publish"];
            let allowed = match registries.as_array() {
                None => true,
                Some(registries) => registries
                    .iter()
                    .any(|registry| registry.as_str() == Some("crates-io")),
            };
            publishable.insert(package["name"].as_str().unwrap().to_string(), allowed);
            for dependency in package["dependencies"].as_array().unwrap() {
                if dependency["source"].as_str() == Some(CRATES_IO_SOURCE) {
                    publishable
                        .entry(dependency["name"].as_str().unwrap().to_string())
                        .or_insert(true);
                }
            }
        }
        publishable
    }

    fn crates_io_requirement(manifest: &Path, package: &str, dependency: &str) -> String {
        let metadata = cargo_metadata(manifest);
        metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|candidate| candidate["name"].as_str() == Some(package))
            .and_then(|candidate| {
                candidate["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["name"].as_str() == Some(dependency))
            })
            .map(|d| d["req"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("{package} depends on {dependency}"))
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

    fn run_binary<const N: usize>(cwd: &Path, args: [&str; N]) -> std::process::Output {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "binary run failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn run_binary_with_stdin<const N: usize>(
        cwd: &Path,
        args: [&str; N],
        stdin: &str,
    ) -> std::process::Output {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "binary run with stdin failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn json_value(output: &std::process::Output) -> serde_json::Value {
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn fill_nth(sheet: &str, id: &str, value: &str, nth: usize) -> String {
        let tag = format!("<id:{id}>");
        let lines: Vec<&str> = sheet.lines().collect();
        let mut out: Vec<String> = Vec::new();
        let mut seen = 0;
        let mut done = false;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            out.push(line.to_string());
            i += 1;
            if line.trim_end().ends_with(&tag) {
                if seen == nth {
                    if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                        i += 1;
                    }
                    out.push(value.to_string());
                    done = true;
                }
                seen += 1;
            }
        }
        assert!(done, "answer sheet has no occurrence {nth} of {tag}");
        out.join("\n") + "\n"
    }

    fn fill(sheet: &str, id: &str, value: &str) -> String {
        fill_nth(sheet, id, value, 0)
    }

    fn duplicate_inputs_block(sheet: &str) -> String {
        let lines: Vec<&str> = sheet.lines().collect();
        let start = lines
            .iter()
            .rposition(|line| line.trim_end().ends_with("<id:command.inputs>"))
            .expect("sheet renders the repeatable inputs group tag line");
        let sources = lines
            .iter()
            .rposition(|line| line.trim_end().ends_with("<id:command.inputs.sources>"))
            .expect("sheet renders the sources question");
        let end = sources + 1;
        let mut copied: Vec<&str> = lines[..=end].to_vec();
        copied.push("");
        copied.extend(&lines[start..=end]);
        copied.extend(&lines[end + 1..]);
        copied.join("\n") + "\n"
    }

    fn questionnaire() -> standout_input::questionnaire::Questionnaire {
        NewProjectAnswers::questionnaire().unwrap()
    }

    fn hand_built_questionnaire() -> standout_input::questionnaire::Questionnaire {
        use standout_input::questionnaire::{
            DynamicDefault, FieldValidator, Group, Item, QuestionnaireChoices as _, ScalarField,
            ScalarKind,
        };

        standout_input::questionnaire::Questionnaire::new(
            "standout.new-project",
            vec![
                Item::from(Group::new(
                    "project",
                    "Project identity.",
                    vec![
                        ScalarField::new(
                            "project.name",
                            "What is the project name? It is also the destination directory.",
                            ScalarKind::String,
                        )
                        .with_validator(FieldValidator::new(
                            "crate-name.v1",
                            validate_project_name,
                        )),
                        ScalarField::new(
                            "project.executable",
                            "What is the executable name? Leave blank to reuse the project name.",
                            ScalarKind::String,
                        )
                        .with_dynamic_default(DynamicDefault::new(
                            "crate-name.v2",
                            executable_default,
                        ))
                        .with_validator(FieldValidator::new(
                            "crate-name.v2",
                            validate_executable_name,
                        )),
                    ],
                )),
                Item::from(Group::new(
                    "command",
                    "Initial command.",
                    vec![
                        Item::from(
                            ScalarField::new(
                                "command.name",
                                "What is the command name?",
                                ScalarKind::String,
                            )
                            .with_validator(FieldValidator::new(
                                "command-name.v1",
                                validate_command_answer,
                            )),
                        ),
                        Item::from(ScalarField::new(
                            "command.description",
                            "Describe the command in a sentence or two.",
                            ScalarKind::Text,
                        )),
                        Item::from(
                            Group::new(
                                "command.inputs",
                                "Describe a command input.",
                                vec![
                                    ScalarField::new(
                                        "command.inputs.name",
                                        "What is its name?",
                                        ScalarKind::String,
                                    )
                                    .with_validator(FieldValidator::new(
                                        "input-name.v1",
                                        validate_input_name,
                                    )),
                                    ScalarField::new(
                                        "command.inputs.value_type",
                                        "What type of value is it?",
                                        ScalarKind::String,
                                    )
                                    .one_of(InputValueType::choices().iter().copied())
                                    .with_default("string"),
                                    ScalarField::new(
                                        "command.inputs.cardinality",
                                        "How many values does it take?",
                                        ScalarKind::String,
                                    )
                                    .one_of(InputCardinality::choices().iter().copied())
                                    .with_dynamic_default(DynamicDefault::new(
                                        "input-cardinality-default.v1",
                                        cardinality_default,
                                    )),
                                    ScalarField::new(
                                        "command.inputs.sources",
                                        "Where can its value come from, in precedence order (comma-separated: argument, file, stdin)?",
                                        ScalarKind::String,
                                    )
                                    .with_dynamic_default(DynamicDefault::new(
                                        "input-sources.v2",
                                        sources_default,
                                    ))
                                    .with_validator(FieldValidator::new(
                                        "input-sources.v2",
                                        validate_sources_answer,
                                    )),
                                ],
                            )
                            .repeatable(1),
                        ),
                    ],
                )),
                Item::from(Group::new(
                    "result",
                    "Result shape.",
                    vec![
                        ScalarField::new(
                            "result.shape",
                            "Should the result be a message or a record?",
                            ScalarKind::String,
                        )
                        .one_of(ResultShape::choices().iter().copied())
                        .with_default("record"),
                        ScalarField::new(
                            "result.fields",
                            "Which fields should the record carry (comma-separated)?",
                            ScalarKind::String,
                        )
                        .optional()
                        .with_default("summary,count")
                        .active_when("result.shape", "record")
                        .with_validator(FieldValidator::new(
                            "record-fields.v1",
                            validate_record_fields_answer,
                        )),
                    ],
                )),
            ],
        )
        .unwrap()
    }

    fn decode_sheet(sheet: &str) -> Result<NewProjectAnswers, Vec<String>> {
        let questionnaire = questionnaire();
        let raw = questionnaire
            .parse_answer_sheet(sheet)
            .map_err(|diagnostics| {
                diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })?;
        NewProjectAnswers::from_raw_answers_with(&raw, new_project_form_rules)
            .map_err(|error| vec![error.to_string()])
    }

    fn minimal_sheet() -> String {
        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "hello-tool");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        fill(&sheet, "command.inputs.name", "name")
    }

    #[test]
    fn derived_questionnaire_preserves_stable_ids_and_typed_vocabularies() {
        let sheet = questionnaire().render_answer_sheet();

        assert!(sheet.contains("#! questionnaire: standout.new-project"));
        for id in [
            "project.name",
            "project.executable",
            "command.name",
            "command.description",
            "command.inputs",
            "command.inputs.name",
            "command.inputs.value_type",
            "command.inputs.cardinality",
            "command.inputs.sources",
            "result.shape",
            "result.fields",
        ] {
            assert!(sheet.contains(&format!("<id:{id}>")), "missing {id}");
        }
        assert!(sheet.contains("string, bool, or path"));
        assert!(sheet.contains("required, optional, repeated, or boolean"));
        assert!(sheet.contains("message or record"));
    }

    #[test]
    fn derived_wizard_schema_matches_hand_built_definition_and_fingerprint() {
        let derived = questionnaire();
        let hand_built = hand_built_questionnaire();

        assert_eq!(derived, hand_built);
        assert_eq!(derived.fingerprint(), hand_built.fingerprint());
    }

    #[test]
    fn answer_sheet_decodes_to_typed_struct_and_project_spec() {
        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "inspect-tool");
        let sheet = fill(&sheet, "command.name", "inspect");
        let sheet = fill(&sheet, "command.description", "Inspect document input");
        let sheet = fill(&sheet, "command.inputs.name", "document");
        let sheet = duplicate_inputs_block(&sheet);
        let sheet = fill_nth(&sheet, "command.inputs.name", "verbose", 1);
        let sheet = fill_nth(&sheet, "command.inputs.value_type", "bool", 1);
        let sheet = fill(&sheet, "result.fields", "summary,count,echo");

        let answers = decode_sheet(&sheet).unwrap();
        let spec = ProjectSpec::from_answers(answers).unwrap();

        assert_eq!(spec.executable_name, "inspect-tool");
        assert_eq!(spec.inputs.len(), 2);
        assert_eq!(
            spec.inputs[0].sources,
            vec![InputSource::Argument, InputSource::File, InputSource::Stdin]
        );
        assert_eq!(spec.inputs[1].value_type, InputValueType::Bool);
        assert_eq!(spec.inputs[1].cardinality, InputCardinality::Boolean);
        assert_eq!(spec.record_fields, vec!["summary", "count", "echo"]);
    }

    #[test]
    fn dynamic_defaults_apply_in_sheet_decode() {
        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "verbose");
        let sheet = fill(&sheet, "command.inputs.value_type", "bool");
        let sheet = fill(&sheet, "result.shape", "message");

        let spec = ProjectSpec::from_answers(decode_sheet(&sheet).unwrap()).unwrap();

        assert_eq!(spec.executable_name, "demo");
        assert_eq!(spec.inputs[0].cardinality, InputCardinality::Boolean);
        assert_eq!(spec.inputs[0].sources, vec![InputSource::Argument]);
        assert_eq!(spec.record_fields, Vec::<String>::new());
    }

    #[test]
    fn multiline_description_keeps_internal_line_breaks() {
        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(
            &sheet,
            "command.description",
            "Greet one value.\nIt spans two lines.",
        );
        let sheet = fill(&sheet, "command.inputs.name", "name");

        let answers = decode_sheet(&sheet).unwrap();

        assert_eq!(
            answers.command.description,
            "Greet one value.\nIt spans two lines."
        );
        assert!(ProjectSpec::from_answers(answers).is_ok());
    }

    #[test]
    fn field_and_typed_form_failures_accumulate_per_stage() {
        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "9bad");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "name");
        let sheet = fill(&sheet, "command.inputs.value_type", "integer");
        let sheet = fill(&sheet, "command.inputs.sources", "argument,teleport");

        let errors = decode_sheet(&sheet).unwrap_err();
        let error = errors.join("\n");

        assert!(error.contains("[project.name]"), "{error}");
        assert!(error.contains("[command.inputs[0].value_type]"), "{error}");
        assert!(error.contains("[command.inputs[0].sources]"), "{error}");

        let sheet = questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "document");
        let sheet = fill(&sheet, "command.inputs.value_type", "path");
        let sheet = fill(&sheet, "command.inputs.sources", "file");
        let sheet = duplicate_inputs_block(&sheet);
        let sheet = fill_nth(&sheet, "command.inputs.name", "document_file", 1);
        let sheet = fill_nth(&sheet, "command.inputs.value_type", "path", 1);
        let sheet = fill_nth(&sheet, "command.inputs.sources", "argument", 1);

        let errors = decode_sheet(&sheet).unwrap_err();
        let error = errors.join("\n");

        assert!(
            error.contains("path inputs only support argument source")
                && error.contains("command.inputs[0]"),
            "{error}"
        );
        assert!(error.contains("conflicts with input"), "{error}");
    }

    #[test]
    fn stale_fingerprint_rejects_with_regeneration_guidance() {
        let sheet = questionnaire().render_answer_sheet();
        let stale = sheet.replacen("#! fingerprint: sha256:", "#! fingerprint: sha256:00", 1);

        let errors = decode_sheet(&stale).unwrap_err();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("render a fresh answer sheet"));
    }

    #[test]
    fn file_and_stdin_sheets_decode_to_identical_answers_and_specs() {
        let sheet = minimal_sheet();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("answers.txt");
        fs::write(&path, &sheet).unwrap();
        let questionnaire = questionnaire();

        let from_file_raw = questionnaire
            .read_answer_sheet_file(&path, &standout_input::questionnaire::StandoutAnswerSheet)
            .unwrap();
        let from_stdin_raw = questionnaire
            .read_answer_sheet_stdin(
                &standout_input::MockStdin::piped(&sheet),
                &standout_input::questionnaire::StandoutAnswerSheet,
            )
            .unwrap();
        let from_file =
            NewProjectAnswers::from_raw_answers_with(&from_file_raw, new_project_form_rules)
                .unwrap();
        let from_stdin =
            NewProjectAnswers::from_raw_answers_with(&from_stdin_raw, new_project_form_rules)
                .unwrap();

        assert_eq!(from_file, from_stdin);
        assert_eq!(
            ProjectSpec::from_answers(from_file).unwrap(),
            ProjectSpec::from_answers(from_stdin).unwrap()
        );
    }

    #[test]
    #[serial(prompt_responder)]
    fn interactive_file_and_stdin_decode_to_identical_answers_and_specs() {
        let sheet = minimal_sheet();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("answers.txt");
        fs::write(&path, &sheet).unwrap();
        let questionnaire = questionnaire();

        let _guard = PromptResponderGuard::install([
            PromptResponse::text("hello-tool"),
            PromptResponse::Skip,
            PromptResponse::text("greet"),
            PromptResponse::text("Greet one value"),
            PromptResponse::text("name"),
            PromptResponse::Skip,
            PromptResponse::Skip,
            PromptResponse::Skip,
            PromptResponse::Skip,
            PromptResponse::Skip,
            PromptResponse::Skip,
        ]);
        let interactive_raw = questionnaire
            .collect_interactive_from(_guard.sources())
            .unwrap();
        let from_file_raw = questionnaire
            .read_answer_sheet_file(&path, &standout_input::questionnaire::StandoutAnswerSheet)
            .unwrap();
        let from_stdin_raw = questionnaire
            .read_answer_sheet_stdin(
                &standout_input::MockStdin::piped(&sheet),
                &standout_input::questionnaire::StandoutAnswerSheet,
            )
            .unwrap();

        let from_interactive =
            NewProjectAnswers::from_raw_answers_with(&interactive_raw, new_project_form_rules)
                .unwrap();
        let from_file =
            NewProjectAnswers::from_raw_answers_with(&from_file_raw, new_project_form_rules)
                .unwrap();
        let from_stdin =
            NewProjectAnswers::from_raw_answers_with(&from_stdin_raw, new_project_form_rules)
                .unwrap();

        assert_eq!(from_interactive, from_file);
        assert_eq!(from_interactive, from_stdin);
        let spec = ProjectSpec::from_answers(from_interactive).unwrap();
        assert_eq!(spec, ProjectSpec::from_answers(from_file).unwrap());
        assert_eq!(spec, ProjectSpec::from_answers(from_stdin).unwrap());
        assert_eq!(
            spec.inputs[0].sources,
            vec![InputSource::Argument, InputSource::File, InputSource::Stdin,]
        );
    }
}
