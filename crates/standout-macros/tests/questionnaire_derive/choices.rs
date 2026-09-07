use super::*;
use standout_input::questionnaire::QuestionnaireChoices as _;

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
        QuestionnaireError::Item { ref id, .. } if id == "kind"
    ));
    assert!(err.to_string().contains("Invalid default on field 'kind'"));
    assert!(err.to_string().contains("cli-app, library, internal-tool"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceOrderA {
    #[question(rename = "alpha")]
    Alpha,
    #[question(rename = "beta")]
    Beta,
    #[question(rename = "gamma")]
    Gamma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceOrderB {
    #[question(rename = "gamma")]
    Gamma,
    #[question(rename = "alpha")]
    Alpha,
    #[question(rename = "beta")]
    Beta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
enum ChoiceRenamed {
    #[question(rename = "alpha")]
    Alpha,
    #[question(rename = "release")]
    Beta,
    #[question(rename = "gamma")]
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
