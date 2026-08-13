//! Shared decoding and validation of raw answers.
//!
//! Every collection path — interactive prompts, named files, and explicit
//! stdin — normalizes to [`RawAnswers`] and runs through the functions here,
//! so equivalent raw text always decodes to the same value with the same
//! diagnostics. There are no adapter-specific conversions or messages.
//!
//! Decoding one field applies, in order: blank resolution (a blank answer
//! resolves to the declared default first; an optional blank without a
//! default is an omission; a required blank without a default is a
//! missing-value error), kind conversion, constraint checking, and the
//! application's [`FieldValidator`](super::FieldValidator). Whole-document
//! decoding ([`Questionnaire::decode_answers`]) additionally evaluates
//! conditional applicability and accumulates every independent diagnostic
//! instead of stopping at the first; the application's whole-form rules
//! join the same accumulated list via
//! [`Questionnaire::decode_answers_with`].
//!
//! Diagnostics identify fields by stable ID and never echo submitted values;
//! see the [module documentation](crate::questionnaire) for why.

use std::collections::BTreeMap;

use super::definition::{Constraint, Questionnaire, ScalarField, ScalarKind};
use super::parse::RawAnswers;

/// A decoded, field-validated answer value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerValue {
    /// The value of a `String`, `Text`, or `Path` field.
    Text(String),
    /// The value of a `Bool` field.
    Bool(bool),
}

impl AnswerValue {
    /// The text content, for `String` / `Text` / `Path` fields.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            AnswerValue::Text(s) => Some(s),
            AnswerValue::Bool(_) => None,
        }
    }

    /// The boolean content, for `Bool` fields.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            AnswerValue::Bool(b) => Some(*b),
            AnswerValue::Text(_) => None,
        }
    }

    /// Canonical string form, used to evaluate conditions: text verbatim,
    /// bools as `true` / `false`.
    pub(crate) fn canonical(&self) -> String {
        match self {
            AnswerValue::Text(s) => s.clone(),
            AnswerValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }
}

/// The decoded, validated answers for one questionnaire submission.
///
/// Contains one entry per *answered* field. Omitted optional fields and
/// inactive conditional fields are absent. Conversion into application
/// domain types starts from here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    values: BTreeMap<String, AnswerValue>,
}

impl Answers {
    /// The decoded value for a stable field ID, if the field was answered.
    pub fn get(&self, field_id: &str) -> Option<&AnswerValue> {
        self.values.get(field_id)
    }

    /// The text value for a `String` / `Text` / `Path` field, if answered.
    pub fn get_text(&self, field_id: &str) -> Option<&str> {
        self.get(field_id).and_then(AnswerValue::as_text)
    }

    /// The boolean value for a `Bool` field, if answered.
    pub fn get_bool(&self, field_id: &str) -> Option<bool> {
        self.get(field_id).and_then(AnswerValue::as_bool)
    }

    /// Iterate over `(field_id, value)` pairs, ordered by field ID.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AnswerValue)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of answered fields.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether no field was answered.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// One whole-form error returned by an application form validator.
///
/// Mapped into [`ValidationDiagnostic::Form`] so form-level findings
/// accumulate in the same list as field-level ones. The message should
/// describe the rule without echoing submitted values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormError {
    /// Stable IDs of the fields involved (may be empty for a global rule).
    pub fields: Vec<String>,
    /// User-facing description of the violated rule.
    pub message: String,
}

impl FormError {
    /// Create a form error over the given fields.
    pub fn new(
        fields: impl IntoIterator<Item = impl Into<String>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            fields: fields.into_iter().map(Into::into).collect(),
            message: message.into(),
        }
    }
}

/// One problem found while decoding and validating raw answers.
///
/// Diagnostics identify fields by stable ID and describe the violated rule;
/// they never echo the submitted value, since answers may be sensitive.
/// Independent diagnostics accumulate: a batch submission reports everything
/// actionable in one pass.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ValidationDiagnostic {
    /// A required, active field has no answer (blank or absent, no default).
    #[error("[{id}]: this question requires an answer.")]
    MissingAnswer {
        /// The unanswered field.
        id: String,
    },

    /// An inactive conditional field was populated.
    #[error("[{id}]: this question does not apply (it is asked only when {controller} is {expected}); remove its answer or change the controlling answer.")]
    InactiveAnswered {
        /// The populated inactive field.
        id: String,
        /// Its controlling field.
        controller: String,
        /// The canonical value that would activate it.
        expected: String,
    },

    /// The answer text does not convert to the field's kind.
    #[error("[{id}]: {reason}")]
    InvalidValue {
        /// The field whose answer failed conversion.
        id: String,
        /// What the kind expects (never the submitted value).
        reason: String,
    },

    /// The converted answer is not one of the declared choices.
    #[error("[{id}]: the answer must be one of: {}.", allowed.join(", "))]
    ConstraintViolation {
        /// The constrained field.
        id: String,
        /// The declared choices (definition data, safe to echo).
        allowed: Vec<String>,
    },

    /// The application's field validator rejected the converted answer.
    #[error("[{id}]: {message}")]
    FieldValidation {
        /// The rejected field.
        id: String,
        /// The validator's user-facing message.
        message: String,
    },

    /// An application whole-form rule was violated.
    #[error("{}", form_display(.fields, .message))]
    Form {
        /// Stable IDs of the fields involved (may be empty).
        fields: Vec<String>,
        /// The rule's user-facing message.
        message: String,
    },
}

/// Render a whole-form diagnostic: the rule's message, plus the involved
/// stable field IDs when the rule names any.
fn form_display(fields: &[String], message: &str) -> String {
    if fields.is_empty() {
        message.to_string()
    } else {
        format!("{message} (fields: {})", fields.join(", "))
    }
}

/// Parse the shared boolean vocabulary: `true`/`false`/`yes`/`no`/`y`/`n`,
/// case-insensitive. This is the single bool decoder for every collection
/// path and for canonicalizing condition expected values.
pub(crate) fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(true),
        "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

/// Convert non-blank answer text through kind conversion, constraint
/// checking, and the application validator.
///
/// Shared by every collection path and by definition-time default
/// validation. Diagnostic reasons never include `text`.
pub(crate) fn check_field_text(
    field: &ScalarField,
    text: &str,
) -> Result<AnswerValue, ValidationDiagnostic> {
    let value = match field.kind() {
        ScalarKind::Text => AnswerValue::Text(text.to_string()),
        ScalarKind::String | ScalarKind::Path => {
            if text.contains('\n') {
                return Err(ValidationDiagnostic::InvalidValue {
                    id: field.id().to_string(),
                    reason: format!("a {} answer must be a single line", field.kind().name()),
                });
            }
            AnswerValue::Text(text.to_string())
        }
        ScalarKind::Bool => match parse_bool(text) {
            Some(b) => AnswerValue::Bool(b),
            None => {
                return Err(ValidationDiagnostic::InvalidValue {
                    id: field.id().to_string(),
                    reason: "expected a yes/no answer (true, false, yes, no, y, or n)".to_string(),
                })
            }
        },
    };
    if let Some(Constraint::OneOf(choices)) = field.constraint() {
        let matches = value
            .as_text()
            .is_some_and(|t| choices.iter().any(|c| c == t));
        if !matches {
            return Err(ValidationDiagnostic::ConstraintViolation {
                id: field.id().to_string(),
                allowed: choices.clone(),
            });
        }
    }
    if let Some(validator) = field.validator() {
        if let Err(message) = validator.check(&value) {
            return Err(ValidationDiagnostic::FieldValidation {
                id: field.id().to_string(),
                message,
            });
        }
    }
    Ok(value)
}

/// Decode one active field's raw answer.
///
/// `raw` is the trimmed answer text, or `None` when the field is absent from
/// the submission. Blank resolves through the declared default first; a
/// blank without a default is an omission (`Ok(None)`) when optional and a
/// [`ValidationDiagnostic::MissingAnswer`] when required.
pub(crate) fn decode_field(
    field: &ScalarField,
    raw: Option<&str>,
) -> Result<Option<AnswerValue>, ValidationDiagnostic> {
    let submitted = raw.map(str::trim).filter(|t| !t.is_empty());
    let effective = submitted.or(field.default());
    match effective {
        Some(text) => check_field_text(field, text).map(Some),
        None if field.is_optional() => Ok(None),
        None => Err(ValidationDiagnostic::MissingAnswer {
            id: field.id().to_string(),
        }),
    }
}

/// What happened to one field during a whole-document decode pass.
pub(crate) enum FieldOutcome {
    /// Decoded and validated to a value.
    Answered(AnswerValue),
    /// Active and optional, left blank without a default.
    Omitted,
    /// Condition unsatisfied; the field was (correctly) not answered.
    Inactive,
    /// This field (or its controller chain) produced a diagnostic, so
    /// dependents cannot be judged and are skipped without diagnostics.
    Errored,
}

/// Evaluate a field's applicability from earlier outcomes.
///
/// `Some(true)` / `Some(false)` when applicability is known; `None` when the
/// controller (or its chain) errored, so the field cannot be judged.
pub(crate) fn is_active(
    field: &ScalarField,
    outcomes: &BTreeMap<String, FieldOutcome>,
) -> Option<bool> {
    let Some(condition) = field.condition() else {
        return Some(true);
    };
    match outcomes.get(condition.controller()) {
        Some(FieldOutcome::Answered(value)) => Some(value.canonical() == condition.expected()),
        Some(FieldOutcome::Omitted) | Some(FieldOutcome::Inactive) => Some(false),
        Some(FieldOutcome::Errored) | None => None,
    }
}

impl Questionnaire {
    /// Decode and validate a complete raw submission.
    ///
    /// Fields are processed in declaration order (controllers precede their
    /// dependents by construction). For each field: applicability is
    /// evaluated from earlier decoded values; an active field decodes via
    /// [the shared field pipeline](Self::decode_answers) — default
    /// resolution, kind conversion, constraints, application validator; an
    /// inactive field must be blank or hold its untouched pre-filled
    /// default, otherwise it is reported as populated-but-inapplicable.
    ///
    /// All independent diagnostics accumulate: one pass reports every
    /// missing value, conversion failure, constraint violation, field-
    /// validation failure, and populated inactive field together. A field
    /// whose controller errored is skipped without piling on speculative
    /// diagnostics.
    ///
    /// # Errors
    ///
    /// The accumulated [`ValidationDiagnostic`] list, identifying fields by
    /// stable ID without echoing submitted values.
    pub fn decode_answers(&self, raw: &RawAnswers) -> Result<Answers, Vec<ValidationDiagnostic>> {
        let mut outcomes: BTreeMap<String, FieldOutcome> = BTreeMap::new();
        let mut diagnostics = Vec::new();

        for field in self.fields() {
            let raw_value = raw.get(field.id());
            let outcome = match is_active(field, &outcomes) {
                None => FieldOutcome::Errored,
                Some(false) => {
                    let blank = raw_value.is_none_or(|t| t.trim().is_empty());
                    let untouched_default =
                        field.default().is_some() && raw_value == field.default();
                    if blank || untouched_default {
                        FieldOutcome::Inactive
                    } else {
                        let condition = field.condition().expect("inactive implies condition");
                        diagnostics.push(ValidationDiagnostic::InactiveAnswered {
                            id: field.id().to_string(),
                            controller: condition.controller().to_string(),
                            expected: condition.expected().to_string(),
                        });
                        FieldOutcome::Errored
                    }
                }
                Some(true) => match decode_field(field, raw_value) {
                    Ok(Some(value)) => FieldOutcome::Answered(value),
                    Ok(None) => FieldOutcome::Omitted,
                    Err(diagnostic) => {
                        diagnostics.push(diagnostic);
                        FieldOutcome::Errored
                    }
                },
            };
            outcomes.insert(field.id().to_string(), outcome);
        }

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        let values = outcomes
            .into_iter()
            .filter_map(|(id, outcome)| match outcome {
                FieldOutcome::Answered(value) => Some((id, value)),
                _ => None,
            })
            .collect();
        Ok(Answers { values })
    }

    /// Decode and validate a complete raw submission, then run the
    /// application's whole-form rules over the successful result.
    ///
    /// Field-level diagnostics behave exactly as in
    /// [`decode_answers`](Self::decode_answers). When the field stage
    /// succeeds, `form` runs once over the decoded [`Answers`] and every
    /// returned [`FormError`] accumulates as a
    /// [`ValidationDiagnostic::Form`] — so a batch submission reports all of
    /// its independent form-level findings together, in the same list and
    /// format as field-level ones. Whole-form rules do not run over a
    /// submission with field-level failures: they would be judging values
    /// that do not exist.
    ///
    /// # Errors
    ///
    /// The accumulated [`ValidationDiagnostic`] list from whichever stages
    /// could run.
    pub fn decode_answers_with<F>(
        &self,
        raw: &RawAnswers,
        form: F,
    ) -> Result<Answers, Vec<ValidationDiagnostic>>
    where
        F: FnOnce(&Answers) -> Vec<FormError>,
    {
        let answers = self.decode_answers(raw)?;
        let form_errors = form(&answers);
        if form_errors.is_empty() {
            return Ok(answers);
        }
        Err(form_errors
            .into_iter()
            .map(|e| ValidationDiagnostic::Form {
                fields: e.fields,
                message: e.message,
            })
            .collect())
    }
}
