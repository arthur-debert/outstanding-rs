use anyhow::Result;
use standout_input::questionnaire::{AnswerValue, EarlierAnswers, FormError};

use super::validation::{
    parse_input_sources, parse_record_fields, validate_crate_name, validate_generated_flags,
    validate_ident,
};
use super::{CommandInput, CommandInputAnswers, NewProjectAnswers, ResultAnswers, ResultShape};

#[cfg(test)]
mod tests;

pub(super) fn executable_default(answers: &EarlierAnswers<'_>) -> String {
    answers
        .get_text("project.name")
        .unwrap_or_default()
        .to_string()
}

pub(super) fn cardinality_default(answers: &EarlierAnswers<'_>) -> String {
    if answers.get_text("command.inputs.value_type") == Some("bool") {
        "boolean"
    } else {
        "required"
    }
    .to_string()
}

pub(super) fn sources_default(answers: &EarlierAnswers<'_>) -> String {
    let value_type = answers.get_text("command.inputs.value_type");
    let cardinality = answers.get_text("command.inputs.cardinality");
    if value_type == Some("string") && matches!(cardinality, Some("required") | Some("optional")) {
        "argument,file,stdin"
    } else {
        "argument"
    }
    .to_string()
}

pub(super) fn validate_project_name(value: &AnswerValue) -> Result<(), String> {
    validate_crate_name(value.as_text().unwrap_or_default(), "project name")
        .map_err(|error| error.to_string())
}

pub(super) fn validate_executable_name(value: &AnswerValue) -> Result<(), String> {
    validate_crate_name(value.as_text().unwrap_or_default(), "executable name")
        .map_err(|error| error.to_string())
}

pub(super) fn validate_command_answer(value: &AnswerValue) -> Result<(), String> {
    validate_ident(
        &value.as_text().unwrap_or_default().replace('-', "_"),
        "command name",
    )
    .map_err(|error| error.to_string())
}

pub(super) fn validate_input_name(value: &AnswerValue) -> Result<(), String> {
    validate_ident(value.as_text().unwrap_or_default(), "input name")
        .map_err(|error| error.to_string())
}

pub(super) fn validate_sources_answer(value: &AnswerValue) -> Result<(), String> {
    parse_input_sources(value.as_text().unwrap_or_default())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(super) fn validate_record_fields_answer(value: &AnswerValue) -> Result<(), String> {
    parse_record_fields(value.as_text().unwrap_or_default())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(super) fn new_project_form_rules(answers: &NewProjectAnswers) -> Vec<FormError> {
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

pub(super) fn command_inputs_from_answers(
    inputs: &[CommandInputAnswers],
) -> Result<Vec<CommandInput>> {
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

pub(super) fn record_fields_from_answers(result: &ResultAnswers) -> Result<Vec<String>> {
    match result.shape {
        ResultShape::Message => Ok(Vec::new()),
        ResultShape::Record => parse_record_fields(result.fields.as_deref().unwrap_or_default()),
    }
}
