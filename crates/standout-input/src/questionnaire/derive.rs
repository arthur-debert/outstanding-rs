//! Trait support for `#[derive(Questionnaire)]`.
//!
//! The derive macro lowers an application struct to the public
//! [`Questionnaire`] builder, then generates direct filling code from decoded
//! [`Answers`]. This keeps derived questionnaires on the same validation,
//! rendering, parsing, and fingerprinting path as hand-built definitions.

use super::{
    Answers, FormError, Item, Questionnaire, QuestionnaireError, RawAnswers, ValidationDiagnostic,
};

/// A Rust enum-backed choice vocabulary for derived questionnaires.
///
/// Implemented by `standout-macros` for enums that derive
/// `QuestionnaireChoices`. The enum is then the single source for the
/// accepted answer strings, rendered hints, parsing, and display: a derived
/// questionnaire field of this enum type marked `#[question(choice)]` lowers
/// to a string field constrained with
/// [`ScalarField::one_of`](super::ScalarField::one_of), and typed filling
/// parses the validated answer back into the enum.
pub trait QuestionnaireChoices:
    Sized + std::str::FromStr<Err = QuestionnaireChoiceParseError> + std::fmt::Display
{
    /// The declared user-facing choices for this enum.
    fn choices() -> &'static [&'static str];
}

/// Error returned when parsing an undeclared enum choice.
///
/// The invalid submitted value is intentionally not retained or displayed;
/// diagnostics elsewhere in the questionnaire pipeline follow the same
/// no-echo rule for answer values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionnaireChoiceParseError {
    choices: &'static [&'static str],
}

impl QuestionnaireChoiceParseError {
    /// Create an error for a vocabulary with the given declared choices.
    pub const fn new(choices: &'static [&'static str]) -> Self {
        Self { choices }
    }

    /// The choices accepted by the enum parser.
    pub fn choices(&self) -> &'static [&'static str] {
        self.choices
    }
}

impl std::fmt::Display for QuestionnaireChoiceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "expected one of: {}", self.choices.join(", "))
    }
}

impl std::error::Error for QuestionnaireChoiceParseError {}

/// A derived questionnaire definition and typed filler.
///
/// Implemented by `standout-macros` for structs that derive
/// `Questionnaire`. The generated implementation has two pieces:
///
/// - [`questionnaire`](Self::questionnaire), which constructs the runtime
///   definition through [`Questionnaire::new`] so all existing construction
///   invariants remain load-bearing.
/// - [`from_decoded_answers`](Self::from_decoded_answers), which directly
///   materializes the struct from successfully decoded [`Answers`] without
///   involving serde or stringly application conversion code.
/// - [`from_raw_answers_with`](Self::from_raw_answers_with), which lets an
///   application add whole-form rules over the filled struct while keeping
///   field decoding, defaults, conditions, and validators on the shared
///   runtime path.
///
/// The derive also emits hidden helpers used to lower and fill nested
/// questionnaire structs. They keep nested fields on the same stable
/// group-prefix and occurrence-path model as the public runtime definition.
/// An `active_when` controller must name a field of the same derived struct;
/// the macro resolves it to the field's stable definition ID at expansion
/// time.
pub trait QuestionnaireInput: Sized {
    /// Construct the runtime questionnaire definition for this type.
    ///
    /// Construction errors are the normal [`QuestionnaireError`] values from
    /// the public builder and identify the invalid questionnaire or field ID.
    fn questionnaire() -> Result<Questionnaire, QuestionnaireError>;

    /// Fill this type from answers decoded by its own questionnaire.
    ///
    /// This method is generated code for the closed questionnaire type
    /// universe: scalars, `Option<T>`, scalar `Vec<T>`, enum choices, nested
    /// structs, and repeatable groups. Conditional fields are represented as
    /// `Option<T>` because inactive decoded answers are absent. Call
    /// [`from_raw_answers`](Self::from_raw_answers) for the public checked
    /// path: it decodes with the generated definition first, then fills only
    /// after field validation succeeds.
    #[doc(hidden)]
    fn from_decoded_answers(answers: &Answers) -> Self;

    /// Build this type's items under `prefix`.
    ///
    /// Generated implementations use `prefix` to make nested struct field IDs
    /// extend their enclosing group ID. Manual implementations may keep the
    /// default root-only behavior unless they need nested reuse. The default
    /// implementation panics for non-empty prefixes so accidental nested reuse
    /// fails at the call site instead of constructing root-scoped items.
    #[doc(hidden)]
    fn questionnaire_items(prefix: &str) -> Vec<Item> {
        assert!(
            prefix.is_empty(),
            "manual QuestionnaireInput implementation cannot be nested unless questionnaire_items is overridden"
        );
        Self::questionnaire()
            .expect("manual QuestionnaireInput implementation cannot be nested unless questionnaire_items is overridden")
            .items()
            .to_vec()
    }

    /// Fill this type from answers rooted at `prefix`.
    ///
    /// Generated implementations use `prefix` to read fields inside nested and
    /// repeatable group occurrences. Manual implementations may keep the
    /// default root-only behavior unless they need nested reuse. The default
    /// implementation panics for non-empty prefixes so accidental nested reuse
    /// fails at the call site instead of reading root-scoped answers.
    #[doc(hidden)]
    fn from_decoded_answers_at(answers: &Answers, prefix: &str) -> Self {
        assert!(
            prefix.is_empty(),
            "manual QuestionnaireInput implementation cannot be nested unless from_decoded_answers_at is overridden"
        );
        Self::from_decoded_answers(answers)
    }

    /// Decode raw answers with this type's generated definition and return
    /// the filled struct.
    ///
    /// Definition construction runs through the public builder, then the
    /// normal shared decode pipeline validates fields, defaults,
    /// optionality, conditions, and validators before the generated filler
    /// constructs the struct.
    fn from_raw_answers(raw: &RawAnswers) -> Result<Self, QuestionnaireInputError> {
        let questionnaire = Self::questionnaire().map_err(QuestionnaireInputError::Definition)?;
        let answers = questionnaire
            .decode_answers(raw)
            .map_err(QuestionnaireInputError::Validation)?;
        Ok(Self::from_decoded_answers(&answers))
    }

    /// Decode raw answers, fill the struct, then run typed whole-form rules.
    ///
    /// Use this when a questionnaire has constraints that span fields after
    /// the field-level stage has already produced typed values. The `form`
    /// closure receives the filled struct and returns [`FormError`] values
    /// that are reported as normal validation diagnostics. The form closure
    /// does not run when field decoding fails.
    fn from_raw_answers_with<F>(raw: &RawAnswers, form: F) -> Result<Self, QuestionnaireInputError>
    where
        F: FnOnce(&Self) -> Vec<FormError>,
    {
        let questionnaire = Self::questionnaire().map_err(QuestionnaireInputError::Definition)?;
        let answers = questionnaire
            .decode_answers(raw)
            .map_err(QuestionnaireInputError::Validation)?;
        let value = Self::from_decoded_answers(&answers);
        let form_errors = form(&value);
        if form_errors.is_empty() {
            return Ok(value);
        }
        Err(QuestionnaireInputError::Validation(
            form_errors
                .into_iter()
                .map(|error| ValidationDiagnostic::Form {
                    fields: error.fields,
                    message: error.message,
                })
                .collect(),
        ))
    }
}

/// Errors returned while decoding and filling a derived questionnaire.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuestionnaireInputError {
    /// The generated definition failed public builder validation.
    #[error("derived questionnaire definition is invalid: {0}")]
    Definition(QuestionnaireError),

    /// The submitted answers failed the shared validation pipeline; the
    /// display message includes value-safe diagnostics from that pipeline.
    #[error("derived questionnaire answers are invalid: {}", validation_diagnostics_display(.0))]
    Validation(Vec<ValidationDiagnostic>),
}

fn validation_diagnostics_display(diagnostics: &[ValidationDiagnostic]) -> String {
    if diagnostics.is_empty() {
        return "no diagnostics reported".to_string();
    }

    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}
