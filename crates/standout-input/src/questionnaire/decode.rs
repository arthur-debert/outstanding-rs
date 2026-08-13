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
//! Nested and repeated values run through the exact same pipeline as
//! scalars: decoding walks the definition tree, visits every submitted
//! occurrence of each repeatable group, and addresses each value by its
//! *occurrence path* (`command.inputs[1].name`) — the stable definition ID
//! with a zero-based index per enclosing repeatable-group occurrence.
//! Occurrence counts outside a group's declared [`Repeat`](super::Repeat)
//! bounds are reported here as structural diagnostics, alongside the value
//! diagnostics of the occurrences that do exist.
//!
//! Diagnostics identify fields by stable occurrence path and never echo
//! submitted values; see the [module documentation](crate::questionnaire)
//! for why.

use std::collections::BTreeMap;

use super::definition::{
    child_segment, path_join, Constraint, Item, Questionnaire, ScalarField, ScalarKind,
};
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
/// Contains one entry per *answered* occurrence path — the stable field ID,
/// with a zero-based index per enclosing repeatable-group occurrence
/// (`command.inputs[1].name`). Omitted optional fields and inactive
/// conditional fields are absent. Repeatable-group occurrence counts are
/// carried alongside ([`occurrence_count`](Self::occurrence_count)), so an
/// application can iterate submitted items without guessing from key shapes.
/// Conversion into application domain types starts from here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    values: BTreeMap<String, AnswerValue>,
    occurrences: BTreeMap<String, usize>,
}

impl Answers {
    /// The decoded value at an occurrence path (for fields outside
    /// repeatable groups: the stable field ID), if it was answered.
    pub fn get(&self, path: &str) -> Option<&AnswerValue> {
        self.values.get(path)
    }

    /// The text value for a `String` / `Text` / `Path` field, if answered.
    pub fn get_text(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(AnswerValue::as_text)
    }

    /// The boolean value for a `Bool` field, if answered.
    pub fn get_bool(&self, path: &str) -> Option<bool> {
        self.get(path).and_then(AnswerValue::as_bool)
    }

    /// How many occurrences of a repeatable group were submitted, addressed
    /// by the group's occurrence path base — `command.inputs` at the root,
    /// `command.inputs[0].flags` for a group nested in another occurrence.
    pub fn occurrence_count(&self, group_path: &str) -> usize {
        self.occurrences.get(group_path).copied().unwrap_or(0)
    }

    /// Iterate over `(occurrence_path, value)` pairs, ordered by path.
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
/// Diagnostics identify fields by stable occurrence path (for fields outside
/// repeatable groups: the stable field ID) and describe the violated rule;
/// they never echo the submitted value, since answers may be sensitive.
/// Independent diagnostics accumulate: a batch submission reports everything
/// actionable in one pass.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ValidationDiagnostic {
    /// A required, active field has no answer (blank or absent, no default).
    #[error("[{id}]: this question requires an answer.")]
    MissingAnswer {
        /// The unanswered field's occurrence path.
        id: String,
    },

    /// An inactive conditional field was populated.
    #[error("[{id}]: this question does not apply (it is asked only when {controller} is {expected}); remove its answer or change the controlling answer.")]
    InactiveAnswered {
        /// The populated inactive field's occurrence path.
        id: String,
        /// Its controlling field.
        controller: String,
        /// The canonical value that would activate it.
        expected: String,
    },

    /// The answer text does not convert to the field's kind.
    #[error("[{id}]: {reason}")]
    InvalidValue {
        /// The occurrence path of the field whose answer failed conversion.
        id: String,
        /// What the kind expects (never the submitted value).
        reason: String,
    },

    /// The converted answer is not one of the declared choices.
    #[error("[{id}]: the answer must be one of: {}.", allowed.join(", "))]
    ConstraintViolation {
        /// The constrained field's occurrence path.
        id: String,
        /// The declared choices (definition data, safe to echo).
        allowed: Vec<String>,
    },

    /// The application's field validator rejected the converted answer.
    #[error("[{id}]: {message}")]
    FieldValidation {
        /// The rejected field's occurrence path.
        id: String,
        /// The validator's user-facing message.
        message: String,
    },

    /// A repeatable group has fewer submitted occurrences than its declared
    /// minimum.
    #[error("[{path}]: {found} of at least {minimum} required item(s) submitted. Copy a complete group block - its heading line and its questions - for each missing item.")]
    TooFewOccurrences {
        /// The group's occurrence path base.
        path: String,
        /// The declared minimum.
        minimum: usize,
        /// The submitted occurrence count.
        found: usize,
    },

    /// A repeatable group has more submitted occurrences than its declared
    /// maximum.
    #[error("[{path}]: {found} items submitted, but at most {maximum} are accepted. Remove the extra group block(s).")]
    TooManyOccurrences {
        /// The group's occurrence path base.
        path: String,
        /// The declared maximum.
        maximum: usize,
        /// The submitted occurrence count.
        found: usize,
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
/// checking, and the application validator. `path` is the occurrence path
/// used in diagnostics (the field's own ID outside repeated groups).
///
/// Shared by every collection path and by definition-time default
/// validation. Diagnostic reasons never include `text`.
pub(crate) fn check_field_text(
    field: &ScalarField,
    path: &str,
    text: &str,
) -> Result<AnswerValue, ValidationDiagnostic> {
    let value = match field.kind() {
        ScalarKind::Text => AnswerValue::Text(text.to_string()),
        ScalarKind::String | ScalarKind::Path => {
            if text.contains('\n') {
                return Err(ValidationDiagnostic::InvalidValue {
                    id: path.to_string(),
                    reason: format!("a {} answer must be a single line", field.kind().name()),
                });
            }
            AnswerValue::Text(text.to_string())
        }
        ScalarKind::Bool => match parse_bool(text) {
            Some(b) => AnswerValue::Bool(b),
            None => {
                return Err(ValidationDiagnostic::InvalidValue {
                    id: path.to_string(),
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
                id: path.to_string(),
                allowed: choices.clone(),
            });
        }
    }
    if let Some(validator) = field.validator() {
        if let Err(message) = validator.check(&value) {
            return Err(ValidationDiagnostic::FieldValidation {
                id: path.to_string(),
                message,
            });
        }
    }
    Ok(value)
}

/// Decode one active field's raw answer, identified in diagnostics by its
/// occurrence `path`.
///
/// `raw` is the trimmed answer text, or `None` when the field is absent from
/// the submission. Blank resolves through the declared default first; a
/// blank without a default is an omission (`Ok(None)`) when optional and a
/// [`ValidationDiagnostic::MissingAnswer`] when required.
pub(crate) fn decode_field(
    field: &ScalarField,
    path: &str,
    raw: Option<&str>,
) -> Result<Option<AnswerValue>, ValidationDiagnostic> {
    let submitted = raw.map(str::trim).filter(|t| !t.is_empty());
    let effective = submitted.or(field.default());
    match effective {
        Some(text) => check_field_text(field, path, text).map(Some),
        None if field.is_optional() => Ok(None),
        None => Err(ValidationDiagnostic::MissingAnswer {
            id: path.to_string(),
        }),
    }
}

/// What happened to one field occurrence during a whole-document decode
/// pass. Keyed by occurrence path.
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

/// One open definition scope during a decode or collection walk: the root,
/// or a group occurrence.
#[derive(Debug, Clone)]
pub(crate) struct ScopeCtx {
    /// The scope's group ID (`None` at the questionnaire root).
    pub(crate) group_id: Option<String>,
    /// The definition-ID prefix children of this scope extend (`""` at the
    /// root, `<group_id>.` inside a group).
    pub(crate) def_prefix: String,
    /// The occurrence path prefix of this scope (`""`, `command`, or
    /// `command.inputs[1]`).
    pub(crate) path_prefix: String,
}

impl ScopeCtx {
    /// The root scope every walk starts from.
    pub(crate) fn root() -> Self {
        Self {
            group_id: None,
            def_prefix: String::new(),
            path_prefix: String::new(),
        }
    }

    /// The occurrence path of a direct child of this scope.
    pub(crate) fn child_path(&self, id: &str) -> String {
        path_join(&self.path_prefix, child_segment(&self.def_prefix, id))
    }
}

/// Resolve a condition controller's occurrence path against the current
/// scope chain: the controller lives in whichever open scope declares it as
/// a direct child (construction guarantees that scope is on the chain).
pub(crate) fn controller_path(
    questionnaire: &Questionnaire,
    chain: &[ScopeCtx],
    controller: &str,
) -> String {
    let parent = questionnaire
        .node_meta(controller)
        .expect("conditions are validated at construction")
        .parent
        .as_deref();
    let scope = chain
        .iter()
        .rev()
        .find(|scope| scope.group_id.as_deref() == parent)
        .expect("the controller's scope encloses the dependent's");
    scope.child_path(controller)
}

/// Evaluate a field's applicability from earlier outcomes, resolving its
/// controller within the current scope chain (a controller inside a
/// repeatable group controls per occurrence).
///
/// `Some(true)` / `Some(false)` when applicability is known; `None` when the
/// controller (or its chain) errored, so the field cannot be judged.
pub(crate) fn is_active(
    questionnaire: &Questionnaire,
    field: &ScalarField,
    chain: &[ScopeCtx],
    outcomes: &BTreeMap<String, FieldOutcome>,
) -> Option<bool> {
    let Some(condition) = field.condition() else {
        return Some(true);
    };
    let path = controller_path(questionnaire, chain, condition.controller());
    match outcomes.get(&path) {
        Some(FieldOutcome::Answered(value)) => Some(value.canonical() == condition.expected()),
        Some(FieldOutcome::Omitted) | Some(FieldOutcome::Inactive) => Some(false),
        Some(FieldOutcome::Errored) | None => None,
    }
}

impl Questionnaire {
    /// Decode and validate a complete raw submission.
    ///
    /// The definition tree is walked in declaration order (controllers
    /// precede their dependents by construction), visiting every submitted
    /// occurrence of each repeatable group. For each field occurrence:
    /// applicability is evaluated from earlier decoded values in the same
    /// scope chain; an active field decodes via the shared field pipeline —
    /// default resolution, kind conversion, constraints, application
    /// validator; an inactive field must be blank or hold its untouched
    /// pre-filled default, otherwise it is reported as
    /// populated-but-inapplicable. Repeatable-group occurrence counts
    /// outside the declared bounds are reported as structural diagnostics
    /// while the occurrences that do exist still decode.
    ///
    /// All independent diagnostics accumulate: one pass reports every
    /// missing value, conversion failure, constraint violation, field-
    /// validation failure, populated inactive field, and occurrence-bound
    /// violation together. A field whose controller errored is skipped
    /// without piling on speculative diagnostics.
    ///
    /// # Errors
    ///
    /// The accumulated [`ValidationDiagnostic`] list, identifying fields by
    /// stable occurrence path without echoing submitted values.
    pub fn decode_answers(&self, raw: &RawAnswers) -> Result<Answers, Vec<ValidationDiagnostic>> {
        let mut outcomes: BTreeMap<String, FieldOutcome> = BTreeMap::new();
        let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
        let mut diagnostics = Vec::new();

        self.decode_items(
            self.items(),
            &mut vec![ScopeCtx::root()],
            raw,
            &mut outcomes,
            &mut occurrences,
            &mut diagnostics,
        );

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        let values = outcomes
            .into_iter()
            .filter_map(|(path, outcome)| match outcome {
                FieldOutcome::Answered(value) => Some((path, value)),
                _ => None,
            })
            .collect();
        Ok(Answers {
            values,
            occurrences,
        })
    }

    /// Decode one scope's items; `chain` is the open scope chain, innermost
    /// last (used to build occurrence paths and resolve controllers).
    fn decode_items(
        &self,
        items: &[Item],
        chain: &mut Vec<ScopeCtx>,
        raw: &RawAnswers,
        outcomes: &mut BTreeMap<String, FieldOutcome>,
        occurrences: &mut BTreeMap<String, usize>,
        diagnostics: &mut Vec<ValidationDiagnostic>,
    ) {
        for item in items {
            match item {
                Item::Field(field) => {
                    let path = chain
                        .last()
                        .expect("chain starts rooted")
                        .child_path(field.id());
                    let raw_value = raw.get(&path);
                    let outcome = match is_active(self, field, chain, outcomes) {
                        None => FieldOutcome::Errored,
                        Some(false) => {
                            let blank = raw_value.is_none_or(|t| t.trim().is_empty());
                            let untouched_default =
                                field.default().is_some() && raw_value == field.default();
                            if blank || untouched_default {
                                FieldOutcome::Inactive
                            } else {
                                let condition =
                                    field.condition().expect("inactive implies condition");
                                diagnostics.push(ValidationDiagnostic::InactiveAnswered {
                                    id: path.clone(),
                                    controller: condition.controller().to_string(),
                                    expected: condition.expected().to_string(),
                                });
                                FieldOutcome::Errored
                            }
                        }
                        Some(true) => match decode_field(field, &path, raw_value) {
                            Ok(Some(value)) => FieldOutcome::Answered(value),
                            Ok(None) => FieldOutcome::Omitted,
                            Err(diagnostic) => {
                                diagnostics.push(diagnostic);
                                FieldOutcome::Errored
                            }
                        },
                    };
                    outcomes.insert(path, outcome);
                }
                Item::Group(group) => {
                    let base = chain
                        .last()
                        .expect("chain starts rooted")
                        .child_path(group.id());
                    match group.repeat() {
                        None => {
                            chain.push(ScopeCtx {
                                group_id: Some(group.id().to_string()),
                                def_prefix: group.def_prefix(),
                                path_prefix: base,
                            });
                            self.decode_items(
                                group.children(),
                                chain,
                                raw,
                                outcomes,
                                occurrences,
                                diagnostics,
                            );
                            chain.pop();
                        }
                        Some(repeat) => {
                            let count = raw.occurrence_count(&base);
                            if count < repeat.min() {
                                diagnostics.push(ValidationDiagnostic::TooFewOccurrences {
                                    path: base.clone(),
                                    minimum: repeat.min(),
                                    found: count,
                                });
                            }
                            if let Some(max) = repeat.max() {
                                if count > max {
                                    diagnostics.push(ValidationDiagnostic::TooManyOccurrences {
                                        path: base.clone(),
                                        maximum: max,
                                        found: count,
                                    });
                                }
                            }
                            if count > 0 {
                                occurrences.insert(base.clone(), count);
                            }
                            for index in 0..count {
                                chain.push(ScopeCtx {
                                    group_id: Some(group.id().to_string()),
                                    def_prefix: group.def_prefix(),
                                    path_prefix: format!("{base}[{index}]"),
                                });
                                self.decode_items(
                                    group.children(),
                                    chain,
                                    raw,
                                    outcomes,
                                    occurrences,
                                    diagnostics,
                                );
                                chain.pop();
                            }
                        }
                    }
                }
            }
        }
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
