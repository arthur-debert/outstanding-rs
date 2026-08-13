//! Trait support for `#[derive(Questionnaire)]`.
//!
//! The derive macro lowers an application struct to the public
//! [`Questionnaire`] builder, then generates direct filling code from decoded
//! [`Answers`]. This keeps derived questionnaires on the same validation,
//! rendering, parsing, and fingerprinting path as hand-built definitions.

use super::{Answers, Item, Questionnaire, QuestionnaireError, RawAnswers, ValidationDiagnostic};

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
///
/// The derive also emits hidden, prefix-aware helpers used to lower and fill
/// nested questionnaire structs. They keep nested fields on the same stable
/// group-prefix and occurrence-path model as the public runtime definition.
pub trait QuestionnaireInput: Sized {
    /// Construct the runtime questionnaire definition for this type.
    ///
    /// Construction errors are the normal [`QuestionnaireError`] values from
    /// the public builder and identify the invalid questionnaire or field ID.
    fn questionnaire() -> Result<Questionnaire, QuestionnaireError>;

    /// Fill this type from answers decoded by its own questionnaire.
    ///
    /// This method is generated code for the closed questionnaire type
    /// universe. Call [`from_raw_answers`](Self::from_raw_answers) for the
    /// public checked path: it decodes with the generated definition first,
    /// then fills only after field validation succeeds.
    #[doc(hidden)]
    fn from_decoded_answers(answers: &Answers) -> Self;

    /// Build this type's items under `prefix`.
    ///
    /// Generated implementations use `prefix` to make nested struct field IDs
    /// extend their enclosing group ID. Manual implementations may keep the
    /// default root-only behavior unless they need nested reuse.
    #[doc(hidden)]
    fn questionnaire_items(prefix: &str) -> Vec<Item> {
        let _ = prefix;
        Self::questionnaire()
            .expect("manual QuestionnaireInput implementation cannot be nested unless questionnaire_items is overridden")
            .items()
            .to_vec()
    }

    /// Fill this type from answers rooted at `prefix`.
    ///
    /// Generated implementations use `prefix` to read fields inside nested and
    /// repeatable group occurrences. Manual implementations may keep the
    /// default root-only behavior unless they need nested reuse.
    #[doc(hidden)]
    fn from_decoded_answers_at(answers: &Answers, prefix: &str) -> Self {
        let _ = prefix;
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
