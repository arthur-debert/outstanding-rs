use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum RuntimeMode {
    #[question(rename = "local")]
    Local,
    #[question(rename = "docker")]
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
                ValidationDiagnostic::Field { id, message }
                    if id == "image" && message.contains("does not apply")
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
                ValidationDiagnostic::Field { id, message }
                    if id == "image" && message == "the image must include a tag"
            ))
    ));
}

#[test]
fn behavior_hooks_round_trip_through_interactive_decode() {
    let questionnaire = HookedProfile::questionnaire().unwrap();
    let _guard = ResponderGuard::install([
        PromptResponse::text("docker"),
        PromptResponse::Skip, // image: blank -> computed default
    ]);
    let raw = questionnaire
        .collect_interactive_from(_guard.sources())
        .unwrap();

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

    /// Container image?
    #[question(active_when(field = "runtime", is = "docker"))]
    image: Option<String>,
}

fn hand_built_nested_controller_plan() -> RuntimeQuestionnaire {
    use standout_input::questionnaire::{Group, Item};

    RuntimeQuestionnaire::new(
        "demo.nested.controllers",
        vec![Item::from(
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
                    Item::from(
                        ScalarField::new("services.image", "Container image?", ScalarKind::String)
                            .optional()
                            .active_when("services.service.runtime", "docker"),
                    ),
                ],
            )
            .repeatable(2)
            .max_occurrences(2),
        )],
    )
    .unwrap()
}

#[test]
fn nested_active_when_resolves_same_struct_rust_field_names_to_stable_ids() {
    let derived = NestedControllerPlan::questionnaire().unwrap();
    let hand_built = hand_built_nested_controller_plan();

    assert_eq!(derived, hand_built);
    assert_eq!(derived.fingerprint(), hand_built.fingerprint());
}

#[test]
fn nested_active_when_uses_the_current_repeated_group_occurrence() {
    let questionnaire = NestedControllerPlan::questionnaire().unwrap();
    let sheet = questionnaire.render_answer_sheet();
    let sheet = answer_occurrence(&sheet, "services.service.runtime", 0, "docker");
    let sheet = answer_occurrence(&sheet, "services.image", 0, "debian:stable");
    let sheet = answer_occurrence(&sheet, "services.service.runtime", 1, "local");
    let raw = questionnaire.parse_answer_sheet(&sheet).unwrap();

    assert_eq!(
        NestedControllerPlan::from_raw_answers(&raw).unwrap(),
        NestedControllerPlan {
            services: vec![
                NestedControllerService {
                    runtime: RuntimeMode::Docker,
                    image: Some("debian:stable".to_string()),
                },
                NestedControllerService {
                    runtime: RuntimeMode::Local,
                    image: None,
                },
            ],
        }
    );
}

mod shadowed_string_hygiene {
    use super::*;

    #[allow(dead_code)]
    type String = PathBuf;

    #[derive(Questionnaire)]
    #[question(id = "demo.shadowed-string")]
    #[allow(dead_code)]
    struct ShadowedRoot {
        /// Child?
        child: ShadowedChild,
    }

    #[derive(Questionnaire)]
    #[question(id = "demo.shadowed-string.child")]
    #[allow(dead_code)]
    struct ShadowedChild {
        /// Enabled?
        enabled: bool,

        /// Visible?
        #[question(active_when(field = "enabled", is = "yes"))]
        visible: Option<bool>,
    }

    #[test]
    fn generated_context_uses_std_string_when_string_is_shadowed() {
        let questionnaire = ShadowedRoot::questionnaire().unwrap();

        assert_eq!(questionnaire.id(), "demo.shadowed-string");
    }
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
    let err = EmptyDefaultRevision::questionnaire().unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "name"));
    assert!(err
        .to_string()
        .contains("attaches a dynamic default with an empty revision"));
    let err = EmptyValidatorRevision::questionnaire().unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "name"));
    assert!(err
        .to_string()
        .contains("attaches a validator with an empty revision"));
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
    let err = LaterController::questionnaire().unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "name"));
    assert!(err
        .to_string()
        .contains("conditioned on 'enabled', which is declared after it"));
    let err = NeverMatchingController::questionnaire().unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "image"));
    assert!(err
        .to_string()
        .contains("Invalid condition on field 'image'"));
}
