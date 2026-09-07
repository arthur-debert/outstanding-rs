use super::*;
use crate::new_project::{InputCardinality, InputSource, InputValueType, ProjectSpec, ResultShape};
use serial_test::serial;
use standout_input::{
    questionnaire::QuestionnaireInput, InputSources, PromptResponse, ScriptedResponder,
};
use std::fs;
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
        NewProjectAnswers::from_raw_answers_with(&from_file_raw, new_project_form_rules).unwrap();
    let from_stdin =
        NewProjectAnswers::from_raw_answers_with(&from_stdin_raw, new_project_form_rules).unwrap();

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
        NewProjectAnswers::from_raw_answers_with(&interactive_raw, new_project_form_rules).unwrap();
    let from_file =
        NewProjectAnswers::from_raw_answers_with(&from_file_raw, new_project_form_rules).unwrap();
    let from_stdin =
        NewProjectAnswers::from_raw_answers_with(&from_stdin_raw, new_project_form_rules).unwrap();

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
