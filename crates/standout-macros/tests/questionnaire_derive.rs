use std::path::PathBuf;
use std::sync::Arc;

use serial_test::serial;
use standout_input::env::MockStdin;
use standout_input::questionnaire::QuestionnaireChoices as _;
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, AnswerValue, DynamicDefault, EarlierAnswers, FieldValidator,
    Questionnaire as RuntimeQuestionnaire, QuestionnaireError, QuestionnaireInput,
    QuestionnaireInputError, ScalarField, ScalarKind, ValidationDiagnostic,
};
use standout_input::{
    reset_default_prompt_responder, set_default_prompt_responder, PromptResponse, ScriptedResponder,
};
use standout_macros::{Questionnaire, QuestionnaireChoices};

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.profile")]
struct Profile {
    /// What is your project called?
    name: String,

    /// Where should the project live?
    #[question(id = "project.path", default = "/tmp/demo")]
    root: PathBuf,

    /// Use Docker?
    #[question(default = "yes")]
    docker: bool,

    /// Add any setup notes.
    ///
    /// Later paragraphs are reserved and do not render today.
    #[question(prose)]
    notes: String,

    /// Optional nickname?
    nickname: Option<String>,

    /// Optional config path?
    config: Option<PathBuf>,
}

fn hand_built_profile() -> RuntimeQuestionnaire {
    RuntimeQuestionnaire::new(
        "demo.profile",
        vec![
            ScalarField::new("name", "What is your project called?", ScalarKind::String),
            ScalarField::new(
                "project.path",
                "Where should the project live?",
                ScalarKind::Path,
            )
            .with_default("/tmp/demo"),
            ScalarField::new("docker", "Use Docker?", ScalarKind::Bool).with_default("yes"),
            ScalarField::new("notes", "Add any setup notes.", ScalarKind::Text),
            ScalarField::new("nickname", "Optional nickname?", ScalarKind::String).optional(),
            ScalarField::new("config", "Optional config path?", ScalarKind::Path).optional(),
        ],
    )
    .unwrap()
}

fn answer(sheet: &str, id: &str, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out = Vec::new();
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line.to_string());
        i += 1;
        if !found && line.trim_end().ends_with(&tag) {
            found = true;
            if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                i += 1;
            }
            out.push(answer.to_string());
        }
    }
    assert!(found, "answer sheet has no question line for {tag}");
    out.join("\n") + "\n"
}

fn answer_occurrence(sheet: &str, id: &str, occurrence: usize, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out = Vec::new();
    let mut seen = 0;
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line.to_string());
        i += 1;
        if line.trim_end().ends_with(&tag) {
            if seen == occurrence {
                found = true;
                if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                    i += 1;
                }
                out.push(answer.to_string());
            }
            seen += 1;
        }
    }
    assert!(
        found,
        "answer sheet has no occurrence {occurrence} for question line {tag}"
    );
    out.join("\n") + "\n"
}

fn duplicate_only_group_block(sheet: &str, group_id: &str) -> String {
    let tag = format!("<id:{group_id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim_end().ends_with(&tag))
        .expect("answer sheet has no group line");
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| line.trim_end().ends_with(&tag).then_some(index))
        .unwrap_or(lines.len());
    let block = lines[start..end].join("\n");
    format!("{}\n{}\n", sheet.trim_end(), block)
}

fn edited_sheet(questionnaire: &RuntimeQuestionnaire) -> String {
    let sheet = questionnaire.render_answer_sheet();
    let sheet = answer(&sheet, "name", "demo");
    let sheet = answer(&sheet, "notes", "line one\nline two");
    answer(&sheet, "config", "/etc/demo")
}

fn expected_profile() -> Profile {
    Profile {
        name: "demo".to_string(),
        root: PathBuf::from("/tmp/demo"),
        docker: true,
        notes: "line one\nline two".to_string(),
        nickname: None,
        config: Some(PathBuf::from("/etc/demo")),
    }
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.command")]
struct CommandQuestionnaire {
    /// Command metadata?
    metadata: CommandMetadata,

    /// Command inputs?
    #[question(min = 2, max = 3)]
    inputs: Vec<CommandInput>,

    /// Tags?
    tags: Vec<String>,

    /// Include paths?
    include_paths: Vec<PathBuf>,

    /// Flags?
    #[question(repeated, min = 2, max = 3)]
    flags: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.metadata")]
struct CommandMetadata {
    /// Owner?
    owner: String,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.input")]
struct CommandInput {
    /// Input name?
    name: String,

    /// Required?
    #[question(default = "no")]
    required: bool,
}

fn hand_built_command_questionnaire() -> RuntimeQuestionnaire {
    use standout_input::questionnaire::{Group, Item};

    RuntimeQuestionnaire::new(
        "demo.command",
        vec![
            Item::from(Group::new(
                "metadata",
                "Command metadata?",
                vec![ScalarField::new(
                    "metadata.owner",
                    "Owner?",
                    ScalarKind::String,
                )],
            )),
            Item::from(
                Group::new(
                    "inputs",
                    "Command inputs?",
                    vec![
                        ScalarField::new("inputs.name", "Input name?", ScalarKind::String),
                        ScalarField::new("inputs.required", "Required?", ScalarKind::Bool)
                            .with_default("no"),
                    ],
                )
                .repeatable(2)
                .max_occurrences(3),
            ),
            Item::from(ScalarField::new("tags", "Tags?", ScalarKind::String)),
            Item::from(ScalarField::new(
                "include_paths",
                "Include paths?",
                ScalarKind::Path,
            )),
            Item::from(
                Group::new(
                    "flags",
                    "Flags?",
                    vec![ScalarField::new(
                        "flags.value",
                        "Flags?",
                        ScalarKind::String,
                    )],
                )
                .repeatable(2)
                .max_occurrences(3),
            ),
        ],
    )
    .unwrap()
}

fn edited_command_sheet(questionnaire: &RuntimeQuestionnaire) -> String {
    let sheet = questionnaire.render_answer_sheet();
    let sheet = answer(&sheet, "metadata.owner", "platform");
    let sheet = answer_occurrence(&sheet, "inputs.name", 0, "source");
    let sheet = answer_occurrence(&sheet, "inputs.required", 0, "yes");
    let sheet = answer_occurrence(&sheet, "inputs.name", 1, "target");
    let sheet = answer(&sheet, "tags", "server, worker");
    let sheet = answer(&sheet, "include_paths", "src, /tmp/data");
    let sheet = answer_occurrence(&sheet, "flags.value", 0, "--verbose");
    answer_occurrence(&sheet, "flags.value", 1, "--dry-run")
}

fn expected_command() -> CommandQuestionnaire {
    CommandQuestionnaire {
        metadata: CommandMetadata {
            owner: "platform".to_string(),
        },
        inputs: vec![
            CommandInput {
                name: "source".to_string(),
                required: true,
            },
            CommandInput {
                name: "target".to_string(),
                required: false,
            },
        ],
        tags: vec!["server".to_string(), "worker".to_string()],
        include_paths: vec![PathBuf::from("src"), PathBuf::from("/tmp/data")],
        flags: vec!["--verbose".to_string(), "--dry-run".to_string()],
    }
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.copy")]
struct CopyableInputs {
    /// Command inputs?
    #[question(max = 2)]
    inputs: Vec<CommandInput>,
}

struct ResponderGuard;

impl ResponderGuard {
    fn install(responses: impl IntoIterator<Item = PromptResponse>) -> Self {
        set_default_prompt_responder(Arc::new(ScriptedResponder::new(responses)));
        Self
    }
}

impl Drop for ResponderGuard {
    fn drop(&mut self) {
        reset_default_prompt_responder();
    }
}

#[test]
fn derived_definition_matches_hand_built_definition_and_fingerprint() {
    let derived = Profile::questionnaire().unwrap();
    let hand_built = hand_built_profile();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[test]
fn nested_and_repeatable_definition_matches_hand_built_definition_and_fingerprint() {
    let derived = CommandQuestionnaire::questionnaire().unwrap();
    let hand_built = hand_built_command_questionnaire();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[derive(Questionnaire)]
#[question(id = "demo.invalid")]
#[allow(dead_code)]
struct InvalidFieldId {
    /// Name?
    #[question(id = "Project.Name")]
    name: String,
}

#[test]
fn derived_definitions_surface_public_builder_errors() {
    let err = InvalidFieldId::questionnaire().unwrap_err();

    assert_eq!(err, QuestionnaireError::InvalidId("Project.Name".into()));
    assert!(err.to_string().contains("Project.Name"));
}

#[test]
fn rendered_sheet_round_trips_to_the_typed_struct() {
    let questionnaire = Profile::questionnaire().unwrap();
    let raw = questionnaire
        .parse_answer_sheet(&edited_sheet(&questionnaire))
        .unwrap();
    let profile = Profile::from_raw_answers(&raw).unwrap();

    assert_eq!(profile, expected_profile());
}

#[test]
fn nested_repeatable_and_scalar_vec_sheet_round_trips_to_the_typed_struct() {
    let questionnaire = CommandQuestionnaire::questionnaire().unwrap();
    let raw = questionnaire
        .parse_answer_sheet(&edited_command_sheet(&questionnaire))
        .unwrap();
    let command = CommandQuestionnaire::from_raw_answers(&raw).unwrap();

    assert_eq!(command, expected_command());
}

#[test]
fn copied_repeatable_block_round_trips_in_occurrence_order() {
    let questionnaire = CopyableInputs::questionnaire().unwrap();
    let sheet = duplicate_only_group_block(&questionnaire.render_answer_sheet(), "inputs");
    let sheet = answer_occurrence(&sheet, "inputs.name", 0, "first");
    let sheet = answer_occurrence(&sheet, "inputs.name", 1, "second");

    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();
    let decoded = CopyableInputs::from_raw_answers(&raw).unwrap();

    assert_eq!(
        decoded.inputs,
        vec![
            CommandInput {
                name: "first".to_string(),
                required: false,
            },
            CommandInput {
                name: "second".to_string(),
                required: false,
            },
        ]
    );
}

#[test]
fn derived_repeatable_groups_surface_occurrence_bound_diagnostics() {
    let questionnaire = CopyableInputs::questionnaire().unwrap();
    let preamble_only = questionnaire
        .render_answer_sheet()
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let raw = questionnaire.parse_answer_sheet(&preamble_only).unwrap();
    let err = CopyableInputs::from_raw_answers(&raw).unwrap_err();

    assert!(matches!(
        &err,
        QuestionnaireInputError::Validation(diagnostics)
            if matches!(
                diagnostics.as_slice(),
                [ValidationDiagnostic::TooFewOccurrences { path, minimum: 1, found: 0 }]
                    if path == "inputs"
            )
    ));

    let too_many = duplicate_only_group_block(
        &duplicate_only_group_block(&questionnaire.render_answer_sheet(), "inputs"),
        "inputs",
    );
    let too_many = answer_occurrence(&too_many, "inputs.name", 0, "first");
    let too_many = answer_occurrence(&too_many, "inputs.name", 1, "second");
    let too_many = answer_occurrence(&too_many, "inputs.name", 2, "third");
    let raw = questionnaire.parse_answer_sheet(&too_many).unwrap();
    let err = CopyableInputs::from_raw_answers(&raw).unwrap_err();

    assert!(matches!(
        &err,
        QuestionnaireInputError::Validation(diagnostics)
            if diagnostics.iter().any(|diagnostic| matches!(
                diagnostic,
                ValidationDiagnostic::TooManyOccurrences { path, maximum: 2, found: 3 }
                    if path == "inputs"
            ))
    ));
}

#[test]
fn typed_decode_accepts_file_and_stdin_raw_answers() {
    let questionnaire = Profile::questionnaire().unwrap();
    let sheet = edited_sheet(&questionnaire);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("answers.txt");
    std::fs::write(&path, &sheet).unwrap();

    let from_file = questionnaire.read_answer_sheet_file(&path).unwrap();
    let from_stdin = questionnaire
        .read_answer_sheet_stdin_with(&MockStdin::piped(sheet))
        .unwrap();

    assert_eq!(
        Profile::from_raw_answers(&from_file).unwrap(),
        expected_profile()
    );
    assert_eq!(
        Profile::from_raw_answers(&from_stdin).unwrap(),
        expected_profile()
    );
}

#[test]
#[serial(prompt_responder)]
fn typed_decode_accepts_interactive_raw_answers() {
    let questionnaire = Profile::questionnaire().unwrap();
    let _guard = ResponderGuard::install([
        PromptResponse::text("demo"),
        PromptResponse::Skip,
        PromptResponse::Skip,
        PromptResponse::text("line one\nline two"),
        PromptResponse::Skip,
        PromptResponse::text("/etc/demo"),
    ]);

    let raw = questionnaire.collect_interactive().unwrap();
    assert_eq!(Profile::from_raw_answers(&raw).unwrap(), expected_profile());
}

#[derive(Questionnaire)]
#[question(id = "demo.docs")]
#[allow(dead_code)]
struct WrappedDocs {
    /// Rustfmt may wrap this prompt over
    /// more than one source line.
    ///
    /// This paragraph is reserved.
    name: String,
}

#[derive(Questionnaire)]
#[question(id = "demo.docs")]
#[allow(dead_code)]
struct UnwrappedDocs {
    /// Rustfmt may wrap this prompt over more than one source line.
    ///
    /// This paragraph is reserved too.
    name: String,
}

#[test]
fn doc_comments_use_the_unwrapped_first_paragraph_only() {
    let wrapped = WrappedDocs::questionnaire().unwrap().render_answer_sheet();
    let unwrapped = UnwrappedDocs::questionnaire()
        .unwrap()
        .render_answer_sheet();

    assert_eq!(wrapped, unwrapped);
    assert!(wrapped.contains("Rustfmt may wrap this prompt over more than one source line."));
    assert!(!wrapped.contains("reserved"));
}

#[derive(Debug, Questionnaire)]
#[question(id = "demo.single-line")]
#[allow(dead_code)]
struct SingleLineDefault {
    /// A short value?
    value: String,

    /// Prose value?
    #[question(prose)]
    prose: String,
}

#[test]
fn prose_accepts_multiline_while_unmarked_strings_reject_it() {
    let questionnaire = SingleLineDefault::questionnaire().unwrap();
    let ok_sheet = answer(
        &answer(&questionnaire.render_answer_sheet(), "value", "short"),
        "prose",
        "line one\nline two",
    );
    assert!(SingleLineDefault::from_raw_answers(
        &questionnaire.parse_answer_sheet(&ok_sheet).unwrap()
    )
    .is_ok());

    let bad_sheet = answer(
        &answer(
            &questionnaire.render_answer_sheet(),
            "value",
            "line one\nline two",
        ),
        "prose",
        "text",
    );
    let err =
        SingleLineDefault::from_raw_answers(&questionnaire.parse_answer_sheet(&bad_sheet).unwrap())
            .unwrap_err();
    assert!(matches!(
        &err,
        QuestionnaireInputError::Validation(diagnostics)
            if matches!(
                diagnostics.as_slice(),
                [ValidationDiagnostic::InvalidValue { id, .. }] if id == "value"
            )
    ));
    let message = err.to_string();
    assert!(message.contains("derived questionnaire answers are invalid"));
    assert!(message.contains("[value]: a string answer must be a single line"));
}

#[test]
fn stale_fingerprint_rejects_the_round_trip() {
    let questionnaire = Profile::questionnaire().unwrap();
    let sheet = edited_sheet(&questionnaire).replace("sha256:", "sha256:0000");
    let diagnostics = questionnaire.parse_answer_sheet(&sheet).unwrap_err();
    assert!(matches!(
        diagnostics.as_slice(),
        [AnswerSheetDiagnostic::FingerprintMismatch { .. }]
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum PackageKind {
    CliApp,
    #[question(rename = "library")]
    LibraryCrate,
    InternalTool,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.choices")]
struct ChoiceProfile {
    /// Package style?
    #[question(choice, default = "cli-app")]
    kind: PackageKind,

    /// Optional fallback style?
    #[question(choice)]
    fallback: Option<PackageKind>,
}

fn hand_built_choice_profile() -> RuntimeQuestionnaire {
    RuntimeQuestionnaire::new(
        "demo.choices",
        vec![
            ScalarField::new("kind", "Package style?", ScalarKind::String)
                .one_of(["cli-app", "library", "internal-tool"])
                .with_default("cli-app"),
            ScalarField::new("fallback", "Optional fallback style?", ScalarKind::String)
                .optional()
                .one_of(["cli-app", "library", "internal-tool"]),
        ],
    )
    .unwrap()
}

#[test]
fn choice_enum_generates_display_parse_and_declared_choices() {
    assert_eq!(
        PackageKind::choices(),
        &["cli-app", "library", "internal-tool"]
    );
    assert_eq!(PackageKind::CliApp.to_string(), "cli-app");
    assert_eq!(
        "library".parse::<PackageKind>().unwrap(),
        PackageKind::LibraryCrate
    );

    let err = "unknown".parse::<PackageKind>().unwrap_err();
    assert_eq!(err.choices(), PackageKind::choices());
    assert!(err.to_string().contains("cli-app, library, internal-tool"));
}

#[test]
fn enum_choice_field_matches_hand_built_one_of_and_decodes_to_enum() {
    let derived = ChoiceProfile::questionnaire().unwrap();
    let hand_built = hand_built_choice_profile();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());

    let sheet = answer(&derived.render_answer_sheet(), "fallback", "internal-tool");
    assert!(sheet.contains("cli-app, library, or internal-tool"));
    let raw = derived.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        ChoiceProfile::from_raw_answers(&raw).unwrap(),
        ChoiceProfile {
            kind: PackageKind::CliApp,
            fallback: Some(PackageKind::InternalTool),
        }
    );
}

#[derive(Questionnaire)]
#[question(id = "demo.choice.default")]
#[allow(dead_code)]
struct InvalidChoiceDefault {
    /// Package style?
    #[question(choice, default = "desktop")]
    kind: PackageKind,
}

#[test]
fn enum_choice_defaults_must_name_a_declared_choice() {
    let err = InvalidChoiceDefault::questionnaire().unwrap_err();

    assert!(matches!(
        err,
        QuestionnaireError::InvalidDefault { ref id, .. } if id == "kind"
    ));
    assert!(err.to_string().contains("cli-app, library, internal-tool"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceOrderA {
    Alpha,
    Beta,
    Gamma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceOrderB {
    Gamma,
    Alpha,
    Beta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceRenamed {
    Alpha,
    #[question(rename = "release")]
    Beta,
    Gamma,
}

#[derive(Questionnaire)]
#[question(id = "demo.choice.fingerprint")]
#[allow(dead_code)]
struct ChoiceFingerprintA {
    /// Stage?
    #[question(choice)]
    stage: ChoiceOrderA,
}

#[derive(Questionnaire)]
#[question(id = "demo.choice.fingerprint")]
#[allow(dead_code)]
struct ChoiceFingerprintB {
    /// Stage?
    #[question(choice)]
    stage: ChoiceOrderB,
}

#[derive(Questionnaire)]
#[question(id = "demo.choice.fingerprint")]
#[allow(dead_code)]
struct ChoiceFingerprintRenamed {
    /// Stage?
    #[question(choice)]
    stage: ChoiceRenamed,
}

#[test]
fn choice_order_is_cosmetic_but_renaming_is_semantic_for_fingerprints() {
    let a = ChoiceFingerprintA::questionnaire().unwrap();
    let b = ChoiceFingerprintB::questionnaire().unwrap();
    let renamed = ChoiceFingerprintRenamed::questionnaire().unwrap();

    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_ne!(a.fingerprint(), renamed.fingerprint());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum RuntimeMode {
    Local,
    Docker,
}

fn image_default(earlier: &EarlierAnswers<'_>) -> String {
    match earlier.get_text("runtime") {
        Some("docker") => "debian:stable".to_string(),
        _ => "localhost".to_string(),
    }
}

fn image_default_v2(_: &EarlierAnswers<'_>) -> String {
    "debian:bookworm".to_string()
}

fn validate_image(value: &AnswerValue) -> Result<(), String> {
    let Some(text) = value.as_text() else {
        return Ok(());
    };
    if text.contains(':') {
        Ok(())
    } else {
        Err("the image must include a tag".to_string())
    }
}

fn validate_image_v2(_: &AnswerValue) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.hooks")]
struct HookedProfile {
    /// Runtime?
    #[question(choice)]
    runtime: RuntimeMode,

    /// Container image?
    #[question(
        active_when(field = "runtime", is = "docker"),
        default_with = image_default,
        validate = validate_image,
        revision = "image-hooks-v1"
    )]
    image: Option<String>,
}

fn hand_built_hooked_profile() -> RuntimeQuestionnaire {
    RuntimeQuestionnaire::new(
        "demo.hooks",
        vec![
            ScalarField::new("runtime", "Runtime?", ScalarKind::String).one_of(["local", "docker"]),
            ScalarField::new("image", "Container image?", ScalarKind::String)
                .optional()
                .with_dynamic_default(DynamicDefault::new("image-hooks-v1", image_default))
                .active_when("runtime", "docker")
                .with_validator(FieldValidator::new("image-hooks-v1", validate_image)),
        ],
    )
    .unwrap()
}

#[test]
fn behavior_hooks_match_hand_built_definition_and_fingerprint() {
    let derived = HookedProfile::questionnaire().unwrap();
    let hand_built = hand_built_hooked_profile();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[test]
fn behavior_hooks_round_trip_through_sheet_decode() {
    let questionnaire = HookedProfile::questionnaire().unwrap();

    let sheet = answer(&questionnaire.render_answer_sheet(), "runtime", "docker");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        HookedProfile::from_raw_answers(&raw).unwrap(),
        HookedProfile {
            runtime: RuntimeMode::Docker,
            image: Some("debian:stable".to_string()),
        }
    );

    let sheet = answer(&questionnaire.render_answer_sheet(), "runtime", "local");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        HookedProfile::from_raw_answers(&raw).unwrap(),
        HookedProfile {
            runtime: RuntimeMode::Local,
            image: None,
        }
    );

    let sheet = answer(&sheet, "image", "debian:stable");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();
    assert!(matches!(
        HookedProfile::from_raw_answers(&raw).unwrap_err(),
        QuestionnaireInputError::Validation(diagnostics)
            if diagnostics.iter().any(|diagnostic| matches!(
                diagnostic,
                ValidationDiagnostic::InactiveAnswered { id, .. } if id == "image"
            ))
    ));

    let sheet = answer(&questionnaire.render_answer_sheet(), "runtime", "docker");
    let sheet = answer(&sheet, "image", "debian");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();
    assert!(matches!(
        HookedProfile::from_raw_answers(&raw).unwrap_err(),
        QuestionnaireInputError::Validation(diagnostics)
            if diagnostics.iter().any(|diagnostic| matches!(
                diagnostic,
                ValidationDiagnostic::FieldValidation { id, message }
                    if id == "image" && message == "the image must include a tag"
            ))
    ));
}

#[test]
#[serial(prompt_responder)]
fn behavior_hooks_round_trip_through_interactive_decode() {
    let questionnaire = HookedProfile::questionnaire().unwrap();
    let _guard = ResponderGuard::install([
        PromptResponse::text("docker"),
        PromptResponse::Skip, // image: blank -> computed default
    ]);
    let raw = questionnaire.collect_interactive().unwrap();

    assert_eq!(
        HookedProfile::from_raw_answers(&raw).unwrap(),
        HookedProfile {
            runtime: RuntimeMode::Docker,
            image: Some("debian:stable".to_string()),
        }
    );
}

#[derive(Debug, Questionnaire)]
#[question(id = "demo.hooks")]
#[allow(dead_code)]
struct HookedProfileRevisionV2 {
    /// Runtime?
    #[question(choice)]
    runtime: RuntimeMode,

    /// Container image?
    #[question(
        active_when(field = "runtime", is = "docker"),
        default_with = image_default_v2,
        validate = validate_image,
        revision = "image-hooks-v2"
    )]
    image: Option<String>,
}

#[derive(Debug, Questionnaire)]
#[question(id = "demo.hooks")]
#[allow(dead_code)]
struct HookedProfileValidatorRevisionV2 {
    /// Runtime?
    #[question(choice)]
    runtime: RuntimeMode,

    /// Container image?
    #[question(
        active_when(field = "runtime", is = "docker"),
        default_with = image_default,
        validate = validate_image_v2,
        revision = "validator-v2"
    )]
    image: Option<String>,
}

#[test]
fn hook_revisions_participate_in_the_fingerprint() {
    let base = HookedProfile::questionnaire().unwrap();
    let default_revision = HookedProfileRevisionV2::questionnaire().unwrap();
    let validator_revision = HookedProfileValidatorRevisionV2::questionnaire().unwrap();

    assert_ne!(base.fingerprint(), default_revision.fingerprint());
    assert_ne!(base.fingerprint(), validator_revision.fingerprint());
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.conditional.optional.scalar")]
struct ConditionalScalarProfile {
    /// Enabled?
    enabled: bool,

    /// Name?
    #[question(active_when(field = "enabled", is = "yes"))]
    name: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.conditional.optional.choice")]
struct ConditionalChoiceProfile {
    /// Enabled?
    enabled: bool,

    /// Package style?
    #[question(choice, active_when(field = "enabled", is = "yes"))]
    kind: Option<PackageKind>,
}

#[test]
fn inactive_optional_conditional_scalar_and_choice_fields_fill_as_none() {
    let scalar = ConditionalScalarProfile::questionnaire().unwrap();
    let sheet = answer(&scalar.render_answer_sheet(), "enabled", "no");
    let raw = scalar.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        ConditionalScalarProfile::from_raw_answers(&raw).unwrap(),
        ConditionalScalarProfile {
            enabled: false,
            name: None,
        }
    );

    let choice = ConditionalChoiceProfile::questionnaire().unwrap();
    let sheet = answer(&choice.render_answer_sheet(), "enabled", "no");
    let raw = choice.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        ConditionalChoiceProfile::from_raw_answers(&raw).unwrap(),
        ConditionalChoiceProfile {
            enabled: false,
            kind: None,
        }
    );
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.nested.controllers")]
struct NestedControllerPlan {
    /// Global runtime?
    #[question(id = "global.runtime", choice)]
    global_runtime: RuntimeMode,

    /// Services?
    #[question(min = 2, max = 2)]
    services: Vec<NestedControllerService>,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.nested.controllers.service")]
struct NestedControllerService {
    /// Service runtime?
    #[question(id = "service.runtime", choice)]
    runtime: RuntimeMode,

    /// Details?
    details: NestedControllerDetails,
}

#[derive(Debug, PartialEq, Eq, Questionnaire)]
#[question(id = "demo.nested.controllers.details")]
struct NestedControllerDetails {
    /// Image inherited from the global runtime?
    #[question(active_when(field = "global_runtime", is = "docker"))]
    inherited_image: Option<String>,

    /// Image inherited from the current service runtime?
    #[question(active_when(field = "runtime", is = "docker"))]
    service_image: Option<String>,
}

fn hand_built_nested_controller_plan() -> RuntimeQuestionnaire {
    use standout_input::questionnaire::{Group, Item};

    RuntimeQuestionnaire::new(
        "demo.nested.controllers",
        vec![
            Item::from(
                ScalarField::new("global.runtime", "Global runtime?", ScalarKind::String)
                    .one_of(["local", "docker"]),
            ),
            Item::from(
                Group::new(
                    "services",
                    "Services?",
                    vec![
                        Item::from(
                            ScalarField::new(
                                "services.service.runtime",
                                "Service runtime?",
                                ScalarKind::String,
                            )
                            .one_of(["local", "docker"]),
                        ),
                        Item::from(Group::new(
                            "services.details",
                            "Details?",
                            vec![
                                ScalarField::new(
                                    "services.details.inherited_image",
                                    "Image inherited from the global runtime?",
                                    ScalarKind::String,
                                )
                                .optional()
                                .active_when("global.runtime", "docker"),
                                ScalarField::new(
                                    "services.details.service_image",
                                    "Image inherited from the current service runtime?",
                                    ScalarKind::String,
                                )
                                .optional()
                                .active_when("services.service.runtime", "docker"),
                            ],
                        )),
                    ],
                )
                .repeatable(2)
                .max_occurrences(2),
            ),
        ],
    )
    .unwrap()
}

#[test]
fn nested_active_when_resolves_enclosing_rust_field_names_to_stable_ids() {
    let derived = NestedControllerPlan::questionnaire().unwrap();
    let hand_built = hand_built_nested_controller_plan();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[test]
fn nested_active_when_uses_the_current_repeated_group_occurrence() {
    let questionnaire = NestedControllerPlan::questionnaire().unwrap();
    let sheet = answer(
        &questionnaire.render_answer_sheet(),
        "global.runtime",
        "docker",
    );
    let sheet = answer_occurrence(&sheet, "services.service.runtime", 0, "docker");
    let sheet = answer_occurrence(&sheet, "services.details.inherited_image", 0, "global-one");
    let sheet = answer_occurrence(&sheet, "services.details.service_image", 0, "service-one");
    let sheet = answer_occurrence(&sheet, "services.service.runtime", 1, "local");
    let sheet = answer_occurrence(&sheet, "services.details.inherited_image", 1, "global-two");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();

    assert_eq!(
        NestedControllerPlan::from_raw_answers(&raw).unwrap(),
        NestedControllerPlan {
            global_runtime: RuntimeMode::Docker,
            services: vec![
                NestedControllerService {
                    runtime: RuntimeMode::Docker,
                    details: NestedControllerDetails {
                        inherited_image: Some("global-one".to_string()),
                        service_image: Some("service-one".to_string()),
                    },
                },
                NestedControllerService {
                    runtime: RuntimeMode::Local,
                    details: NestedControllerDetails {
                        inherited_image: Some("global-two".to_string()),
                        service_image: None,
                    },
                },
            ],
        }
    );
}

#[derive(Questionnaire)]
#[question(id = "demo.empty-default-revision")]
#[allow(dead_code)]
struct EmptyDefaultRevision {
    /// Name?
    #[question(default_with = image_default, revision = "")]
    name: String,
}

#[derive(Questionnaire)]
#[question(id = "demo.empty-validator-revision")]
#[allow(dead_code)]
struct EmptyValidatorRevision {
    /// Name?
    #[question(validate = validate_image, revision = "")]
    name: String,
}

#[test]
fn empty_hook_revisions_are_rejected_by_the_builder() {
    assert!(matches!(
        EmptyDefaultRevision::questionnaire().unwrap_err(),
        QuestionnaireError::EmptyDefaultRevision { id } if id == "name"
    ));
    assert!(matches!(
        EmptyValidatorRevision::questionnaire().unwrap_err(),
        QuestionnaireError::EmptyValidatorRevision { id } if id == "name"
    ));
}

#[derive(Questionnaire)]
#[question(id = "demo.unknown-controller")]
#[allow(dead_code)]
struct UnknownController {
    /// Name?
    #[question(active_when(field = "ghost", is = "yes"))]
    name: Option<String>,
}

#[derive(Questionnaire)]
#[question(id = "demo.later-controller")]
#[allow(dead_code)]
struct LaterController {
    /// Name?
    #[question(active_when(field = "enabled", is = "yes"))]
    name: Option<String>,

    /// Enabled?
    enabled: bool,
}

#[derive(Questionnaire)]
#[question(id = "demo.scope")]
#[allow(dead_code)]
struct ScopedController {
    /// First?
    first: ScopedFirst,

    /// Second?
    second: ScopedSecond,
}

#[derive(Questionnaire)]
#[question(id = "demo.scope.first")]
#[allow(dead_code)]
struct ScopedFirst {
    /// Enabled?
    enabled: bool,
}

#[derive(Questionnaire)]
#[question(id = "demo.scope.second")]
#[allow(dead_code)]
struct ScopedSecond {
    /// Name?
    #[question(active_when(field = "first.enabled", is = "yes"))]
    name: Option<String>,
}

#[derive(Questionnaire)]
#[question(id = "demo.never")]
#[allow(dead_code)]
struct NeverMatchingController {
    /// Runtime?
    #[question(choice)]
    runtime: RuntimeMode,

    /// Image?
    #[question(active_when(field = "runtime", is = "podman"))]
    image: Option<String>,
}

#[test]
fn active_when_surfaces_builder_controller_diagnostics() {
    assert!(matches!(
        UnknownController::questionnaire().unwrap_err(),
        QuestionnaireError::UnknownConditionController { controller, .. } if controller == "ghost"
    ));
    assert!(matches!(
        LaterController::questionnaire().unwrap_err(),
        QuestionnaireError::ConditionOrder { controller, .. } if controller == "enabled"
    ));
    assert!(matches!(
        ScopedController::questionnaire().unwrap_err(),
        QuestionnaireError::ConditionScope { controller, .. } if controller == "first.enabled"
    ));
    assert!(matches!(
        NeverMatchingController::questionnaire().unwrap_err(),
        QuestionnaireError::InvalidCondition { id, .. } if id == "image"
    ));
}

#[test]
fn compile_failures_cover_attribute_misuse() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/questionnaire/*.rs");
}
