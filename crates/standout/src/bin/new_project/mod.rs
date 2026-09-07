use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde_json::json;
use standout::cli::{
    App, CommandContext, CommandContextInput, FnHandler, HandlerResult, Output as HandlerOutput,
};
use standout::embed_templates;

mod generation;
mod publish;
mod questionnaire;
#[cfg(test)]
mod test_support;
mod validation;

use publish::{publish_project, write_review};
use questionnaire::*;
use validation::{
    pascal_case, validate_crate_name, validate_generated_flags, validate_ident,
    validate_result_fields,
};

pub(super) fn build_app() -> Result<App> {
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
