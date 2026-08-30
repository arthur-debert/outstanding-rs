use super::{
    Answers, FormError, Item, Questionnaire, QuestionnaireError, RawAnswers, ValidationDiagnostic,
};

pub trait QuestionnaireChoices:
    Sized + std::str::FromStr<Err = QuestionnaireChoiceParseError> + std::fmt::Display
{
    fn choices() -> &'static [&'static str];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionnaireChoiceParseError {
    choices: &'static [&'static str],
}

impl QuestionnaireChoiceParseError {
    pub const fn new(choices: &'static [&'static str]) -> Self {
        Self { choices }
    }

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

pub trait QuestionnaireInput: Sized {
    fn questionnaire() -> Result<Questionnaire, QuestionnaireError>;

    #[doc(hidden)]
    fn from_decoded_answers(answers: &Answers) -> Self;

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

    #[doc(hidden)]
    fn from_decoded_answers_at(answers: &Answers, prefix: &str) -> Self {
        assert!(
            prefix.is_empty(),
            "manual QuestionnaireInput implementation cannot be nested unless from_decoded_answers_at is overridden"
        );
        Self::from_decoded_answers(answers)
    }

    fn from_raw_answers(raw: &RawAnswers) -> Result<Self, QuestionnaireInputError> {
        let questionnaire = Self::questionnaire().map_err(QuestionnaireInputError::Definition)?;
        let answers = questionnaire
            .decode_answers(raw)
            .map_err(QuestionnaireInputError::Validation)?;
        Ok(Self::from_decoded_answers(&answers))
    }

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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuestionnaireInputError {
    #[error("derived questionnaire definition is invalid: {0}")]
    Definition(QuestionnaireError),

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
