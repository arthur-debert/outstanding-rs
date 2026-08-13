//! Trait support for `#[derive(Questionnaire)]`.
//!
//! The derive macro lowers an application struct to the public
//! [`Questionnaire`] builder, then generates direct filling code from decoded
//! [`Answers`]. This keeps derived questionnaires on the same validation,
//! rendering, parsing, and fingerprinting path as hand-built definitions.

use super::{Answers, Questionnaire, QuestionnaireError, RawAnswers, ValidationDiagnostic};

/// A Rust enum-backed choice vocabulary for derived questionnaires.
///
/// Implemented by `standout-macros` for enums that derive
/// `QuestionnaireChoices`. The enum is then the single source for the
/// accepted answer strings, rendered hints, parsing, and display: a derived
/// questionnaire field of this enum type lowers to a string field constrained
/// with [`ScalarField::one_of`](super::ScalarField::one_of), and typed filling
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
        write!(f, "expected one of: {}", self.choices.to_vec().join(", "))
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
pub trait QuestionnaireInput: Sized {
    /// Construct the runtime questionnaire definition for this type.
    ///
    /// Construction errors are the normal [`QuestionnaireError`] values from
    /// the public builder and identify the invalid questionnaire or field ID.
    fn questionnaire() -> Result<Questionnaire, QuestionnaireError>;

    /// Fill this type from answers decoded by its own questionnaire.
    ///
    /// This method is generated code for the closed questionnaire type
    /// universe: scalars, `Option<T>`, and enum choices. Call
    /// [`from_raw_answers`](Self::from_raw_answers) for the public checked
    /// path: it decodes with the generated definition first, then fills only
    /// after field validation succeeds.
    #[doc(hidden)]
    fn from_decoded_answers(answers: &Answers) -> Self;

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
