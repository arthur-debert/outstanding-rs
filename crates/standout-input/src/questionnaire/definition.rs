//! Questionnaire definitions: stable identities plus cosmetic wording.
//!
//! A [`Questionnaire`] is an application-owned, static description of the
//! information to collect. The definition carries two very different kinds of
//! data and the split is the point:
//!
//! - **Semantic** (identity-bearing): the questionnaire ID, each field's
//!   stable ID, its [`ScalarKind`], its optionality, its declared default,
//!   its [`Constraint`], its conditional-applicability [`Condition`], and the
//!   revision of any attached [`FieldValidator`]. These feed the
//!   [fingerprint](Questionnaire::fingerprint) and determine how answers are
//!   decoded and validated.
//! - **Cosmetic** (presentation-only): question wording and field order.
//!   Changing them never changes answer identity or the fingerprint.
//!
//! Definitions are validated at construction: [`Questionnaire::new`] rejects
//! empty or malformed IDs, duplicate field IDs, invalid defaults and
//! constraints, and conditions that reference unknown or later-declared
//! fields, so every constructed questionnaire can render, parse, and decode
//! without further checks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::decode::{check_field_text, AnswerValue};
use super::fingerprint::compute_fingerprint;

/// The kind of value a scalar field collects.
///
/// The kind is *semantic*: it participates in the questionnaire
/// [fingerprint](Questionnaire::fingerprint), drives the rendered type hint,
/// and selects the shared decoder every collection path uses. Interactive,
/// file, and stdin answers for the same field always run through the same
/// kind decoder, so equivalent raw text decodes identically everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// A short, single-line value (rendered hint: `string`). A multi-line
    /// answer is a decode error.
    String,
    /// Free-form prose that may span several lines (rendered hint: `text`).
    Text,
    /// A yes/no value (rendered hint: `bool`). Decodes `true`/`false`/
    /// `yes`/`no`/`y`/`n` case-insensitively.
    Bool,
    /// A filesystem path (rendered hint: `path`). Decoded as a single-line
    /// string; no filesystem checks are performed at decode time.
    Path,
}

impl ScalarKind {
    /// Stable name used in the fingerprint canonical form and type hints.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ScalarKind::String => "string",
            ScalarKind::Text => "text",
            ScalarKind::Bool => "bool",
            ScalarKind::Path => "path",
        }
    }
}

/// A semantic constraint on the values a field accepts.
///
/// Constraints are checked by the shared decoder after kind conversion, so
/// every collection path enforces them identically. They participate in the
/// [fingerprint](Questionnaire::fingerprint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// The decoded answer must equal one of these values exactly.
    ///
    /// Choices must be unique, non-blank single lines with no outer
    /// whitespace (answers are trimmed before matching, so anything else is
    /// unsatisfiable); [`Questionnaire::new`] rejects violations. Choice
    /// order is presentation-only (the fingerprint sorts it); the set of
    /// choices is semantic.
    OneOf(Vec<String>),
}

/// A static conditional-applicability rule: this field is asked (and may be
/// required) only when a previously declared *controller* field decoded to
/// an expected value.
///
/// Conditions are semantic: they participate in the
/// [fingerprint](Questionnaire::fingerprint). The expected value is stored
/// canonically (for a bool controller, `true`/`false` — so declaring
/// `"yes"` and `"true"` produce the same fingerprint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub(crate) controller: String,
    pub(crate) expected: String,
}

impl Condition {
    /// The stable ID of the controlling field.
    pub fn controller(&self) -> &str {
        &self.controller
    }

    /// The canonical expected value that activates the dependent field.
    pub fn expected(&self) -> &str {
        &self.expected
    }
}

/// The shared closure type behind a [`FieldValidator`]: judges a decoded
/// value, returning a user-facing message on rejection.
type ValidatorCheck = Arc<dyn Fn(&AnswerValue) -> Result<(), String> + Send + Sync>;

/// An application-supplied field validator with an explicit semantic
/// revision.
///
/// The closure runs in the shared decode stage, so interactive, file, and
/// stdin answers are validated identically. Because closure semantics cannot
/// be fingerprinted, the application declares an explicit `revision` string
/// that *does* enter the [fingerprint](Questionnaire::fingerprint): bump the
/// revision whenever the validator's accepted values change, and previously
/// rendered sheets are invalidated exactly like any other semantic change.
///
/// Error messages returned by the closure are shown to users in diagnostics;
/// they should describe the rule without echoing the submitted value.
#[derive(Clone)]
pub struct FieldValidator {
    revision: String,
    check: ValidatorCheck,
}

impl FieldValidator {
    /// Create a validator with a semantic `revision` and a `check` that
    /// returns a user-facing message on rejection.
    pub fn new(
        revision: impl Into<String>,
        check: impl Fn(&AnswerValue) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            revision: revision.into(),
            check: Arc::new(check),
        }
    }

    /// The semantic revision that participates in the fingerprint.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Run the validator against a decoded value.
    pub(crate) fn check(&self, value: &AnswerValue) -> Result<(), String> {
        (self.check)(value)
    }
}

impl std::fmt::Debug for FieldValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldValidator")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

/// Equality compares the semantic revision only; the closure itself is not
/// comparable, and the revision is the declared semantic identity.
impl PartialEq for FieldValidator {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}

impl Eq for FieldValidator {}

/// One scalar question in a questionnaire.
///
/// The `id` is the stable machine identity rendered as the bracketed token
/// (`[project.name]`); the `prompt` is human wording and may be edited freely
/// without affecting compatibility. Everything else — kind, optionality,
/// default, constraint, condition, and validator revision — is semantic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarField {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) kind: ScalarKind,
    pub(crate) optional: bool,
    pub(crate) default: Option<String>,
    pub(crate) constraint: Option<Constraint>,
    pub(crate) condition: Option<Condition>,
    pub(crate) validator: Option<FieldValidator>,
}

impl ScalarField {
    /// Create a required scalar field with a stable `id`, human `prompt`
    /// wording, and answer `kind`.
    ///
    /// The `id` and all declared properties are validated when the field is
    /// passed to [`Questionnaire::new`], not here.
    pub fn new(id: impl Into<String>, prompt: impl Into<String>, kind: ScalarKind) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            kind,
            optional: false,
            default: None,
            constraint: None,
            condition: None,
            validator: None,
        }
    }

    /// Mark this field as optional (a blank answer without a default means
    /// omission rather than a missing-value error).
    ///
    /// Optionality is semantic: it changes the fingerprint.
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Declare a default value.
    ///
    /// The renderer pre-fills the default on the answer marker line, and
    /// during decoding any blank answer resolves to the default *before*
    /// optionality is considered. Defaults must be a single line with no
    /// outer whitespace (parsed answers are trimmed) and must themselves
    /// decode cleanly. Defaults are semantic: they change the fingerprint.
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Constrain the answer to one of the given values.
    ///
    /// Checked after kind conversion by the shared decoder. Not applicable
    /// to [`ScalarKind::Bool`] fields. Semantic: changes the fingerprint.
    pub fn one_of(mut self, choices: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.constraint = Some(Constraint::OneOf(
            choices.into_iter().map(Into::into).collect(),
        ));
        self
    }

    /// Make this field applicable only when the earlier-declared `controller`
    /// field decodes to `expected`.
    ///
    /// An inactive field may stay blank (or keep its untouched pre-filled
    /// default) even when required; a *populated* inactive field is a
    /// validation error. Conditions are semantic: they change the
    /// fingerprint.
    pub fn active_when(
        mut self,
        controller: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        self.condition = Some(Condition {
            controller: controller.into(),
            expected: expected.into(),
        });
        self
    }

    /// Attach an application validator with an explicit semantic revision.
    ///
    /// See [`FieldValidator`] for the revision contract.
    pub fn with_validator(mut self, validator: FieldValidator) -> Self {
        self.validator = Some(validator);
        self
    }

    /// The stable field ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The human wording (cosmetic).
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The answer kind (semantic).
    pub fn kind(&self) -> ScalarKind {
        self.kind
    }

    /// Whether a blank answer without a default means omission rather than a
    /// missing value.
    pub fn is_optional(&self) -> bool {
        self.optional
    }

    /// The declared default, if any (semantic).
    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// The declared constraint, if any (semantic).
    pub fn constraint(&self) -> Option<&Constraint> {
        self.constraint.as_ref()
    }

    /// The conditional-applicability rule, if any (semantic).
    pub fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }

    /// The attached application validator, if any (its revision is
    /// semantic).
    pub fn validator(&self) -> Option<&FieldValidator> {
        self.validator.as_ref()
    }

    /// The cosmetic type hint rendered after the bracketed ID.
    ///
    /// Presentation-only: choices render in "a, b, or c" style, optionality
    /// and conditions are spelled out, and any square brackets in condition
    /// values are stripped so a hint can never look like a field header to
    /// the parser.
    pub(crate) fn type_hint(&self) -> String {
        let mut hint = match &self.constraint {
            Some(Constraint::OneOf(choices)) => join_or(choices),
            None => self.kind.name().to_string(),
        };
        if self.optional {
            hint.push_str(", optional");
        }
        if let Some(condition) = &self.condition {
            let expected: String = condition
                .expected
                .chars()
                .filter(|c| !matches!(c, '[' | ']'))
                .collect();
            hint.push_str(&format!(
                "; only when {} is {}",
                condition.controller, expected
            ));
        }
        hint
    }
}

/// Join choices in prose style: `a`, `a or b`, `a, b, or c`.
fn join_or(choices: &[String]) -> String {
    match choices {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        [head @ .., last] => format!("{}, or {last}", head.join(", ")),
    }
}

/// A definition-time validation error.
///
/// Produced by [`Questionnaire::new`]; a constructed questionnaire is always
/// internally consistent.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuestionnaireError {
    /// The questionnaire ID is empty or contains characters outside
    /// `a-z`, `0-9`, `.`, `_`, `-`.
    #[error("Invalid questionnaire ID '{0}': IDs must be non-empty and use only a-z, 0-9, '.', '_', '-'.")]
    InvalidQuestionnaireId(String),

    /// A field ID is empty or contains characters outside
    /// `a-z`, `0-9`, `.`, `_`, `-`.
    #[error("Invalid field ID '{0}': IDs must be non-empty and use only a-z, 0-9, '.', '_', '-'.")]
    InvalidFieldId(String),

    /// Two fields declare the same stable ID.
    #[error("Duplicate field ID '{0}': stable IDs must be unique within a questionnaire.")]
    DuplicateFieldId(String),

    /// The questionnaire declares no fields.
    #[error("A questionnaire must declare at least one field.")]
    NoFields,

    /// A declared constraint is invalid for its field.
    #[error("Invalid constraint on field '{id}': {reason}")]
    InvalidConstraint {
        /// The field carrying the constraint.
        id: String,
        /// Why the constraint is invalid.
        reason: String,
    },

    /// A declared default is invalid for its field.
    #[error("Invalid default on field '{id}': {reason}")]
    InvalidDefault {
        /// The field carrying the default.
        id: String,
        /// Why the default is invalid.
        reason: String,
    },

    /// A condition references a field the questionnaire does not declare.
    #[error("Field '{id}' is conditioned on unknown field '{controller}'.")]
    UnknownConditionController {
        /// The dependent field.
        id: String,
        /// The missing controller ID.
        controller: String,
    },

    /// A condition references a field declared after the dependent field.
    /// Controllers must be declared first so answers can be collected and
    /// decoded in a single pass.
    #[error("Field '{id}' is conditioned on '{controller}', which is declared after it. Declare the controlling field first.")]
    ConditionOrder {
        /// The dependent field.
        id: String,
        /// The later-declared controller ID.
        controller: String,
    },

    /// A condition's expected value can never match its controller.
    #[error("Invalid condition on field '{id}': {reason}")]
    InvalidCondition {
        /// The dependent field.
        id: String,
        /// Why the expected value can never match.
        reason: String,
    },

    /// An attached validator declares an empty semantic revision.
    #[error("Field '{id}' attaches a validator with an empty revision: the revision is the validator's semantic identity and must be non-empty.")]
    EmptyValidatorRevision {
        /// The field carrying the validator.
        id: String,
    },
}

/// An application-owned questionnaire definition.
///
/// See the [module documentation](crate::questionnaire) for the ownership
/// boundary, the rendered answer-sheet format, and the collection and
/// decoding model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Questionnaire {
    id: String,
    fields: Vec<ScalarField>,
    fingerprint: String,
}

/// Returns `true` when `id` is a valid stable identifier.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

impl Questionnaire {
    /// Create a validated questionnaire definition.
    ///
    /// `id` is the stable questionnaire identity written into every rendered
    /// sheet's preamble. `fields` are rendered, collected, and decoded in the
    /// given order, but order is cosmetic for identity: reordering fields
    /// does not change the [fingerprint](Self::fingerprint). The one ordering
    /// rule is structural: a conditional field's controller must be declared
    /// before it.
    ///
    /// Condition expected values are canonicalized here (a bool controller's
    /// `"yes"` becomes `"true"`), so equivalent declarations fingerprint
    /// identically.
    ///
    /// # Errors
    ///
    /// Returns a [`QuestionnaireError`] for an invalid questionnaire or field
    /// ID, a duplicate field ID, an empty field list, or an invalid default,
    /// constraint, condition, or validator revision.
    pub fn new(
        id: impl Into<String>,
        mut fields: Vec<ScalarField>,
    ) -> Result<Self, QuestionnaireError> {
        let id = id.into();
        if !valid_id(&id) {
            return Err(QuestionnaireError::InvalidQuestionnaireId(id));
        }
        if fields.is_empty() {
            return Err(QuestionnaireError::NoFields);
        }
        let mut seen: HashSet<String> = HashSet::with_capacity(fields.len());
        let all_ids: HashSet<String> = fields.iter().map(|f| f.id.clone()).collect();
        // Kind and choices of already-validated (earlier) fields, for
        // canonicalizing and checking conditions in one pass.
        let mut earlier: HashMap<String, (ScalarKind, Option<Constraint>)> = HashMap::new();

        for field in &mut fields {
            if !valid_id(&field.id) {
                return Err(QuestionnaireError::InvalidFieldId(field.id.clone()));
            }
            if !seen.insert(field.id.clone()) {
                return Err(QuestionnaireError::DuplicateFieldId(field.id.clone()));
            }
            validate_constraint(field)?;
            if let Some(condition) = &mut field.condition {
                canonicalize_condition(&field.id, condition, &earlier, &all_ids)?;
            }
            if let Some(validator) = &field.validator {
                if validator.revision().is_empty() {
                    return Err(QuestionnaireError::EmptyValidatorRevision {
                        id: field.id.clone(),
                    });
                }
            }
            validate_default(field)?;
            earlier.insert(field.id.clone(), (field.kind, field.constraint.clone()));
        }

        let fingerprint = compute_fingerprint(&id, &fields);
        Ok(Self {
            id,
            fields,
            fingerprint,
        })
    }

    /// The stable questionnaire ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The declared fields, in presentation order.
    pub fn fields(&self) -> &[ScalarField] {
        &self.fields
    }

    /// The semantic fingerprint (`sha256:<hex>`).
    ///
    /// The fingerprint is a compatibility checksum over the *semantic*
    /// definition — questionnaire ID and each field's stable ID, kind,
    /// optionality, default, constraint, condition, and validator revision.
    /// It deliberately excludes wording, presentation order, and everything
    /// else cosmetic, so copy-editing a questionnaire never invalidates
    /// existing answer sheets, while any change to accepted answers reliably
    /// does. It is **not** an authenticity or tamper-proofing mechanism.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Look up a field by stable ID.
    pub(crate) fn field(&self, id: &str) -> Option<&ScalarField> {
        self.fields.iter().find(|f| f.id == id)
    }
}

/// Reject constraints that can never apply to their field.
fn validate_constraint(field: &ScalarField) -> Result<(), QuestionnaireError> {
    let Some(Constraint::OneOf(choices)) = &field.constraint else {
        return Ok(());
    };
    let invalid = |reason: &str| QuestionnaireError::InvalidConstraint {
        id: field.id.clone(),
        reason: reason.to_string(),
    };
    if field.kind == ScalarKind::Bool {
        return Err(invalid("a bool field cannot declare choices"));
    }
    if choices.is_empty() {
        return Err(invalid("the choice list is empty"));
    }
    let mut unique = HashSet::new();
    for choice in choices {
        if choice.trim().is_empty() || choice.contains('\n') {
            return Err(invalid("choices must be non-blank single lines"));
        }
        if choice != choice.trim() {
            return Err(invalid(
                "choices must carry no outer whitespace (answers are trimmed before matching, so such a choice is unsatisfiable)",
            ));
        }
        if !unique.insert(choice.as_str()) {
            return Err(invalid("choices must be unique"));
        }
    }
    Ok(())
}

/// Check a condition against its (earlier-declared) controller and rewrite
/// the expected value into canonical form.
fn canonicalize_condition(
    field_id: &str,
    condition: &mut Condition,
    earlier: &HashMap<String, (ScalarKind, Option<Constraint>)>,
    all_ids: &HashSet<String>,
) -> Result<(), QuestionnaireError> {
    let Some((controller_kind, controller_constraint)) = earlier.get(&condition.controller) else {
        if all_ids.contains(&condition.controller) {
            return Err(QuestionnaireError::ConditionOrder {
                id: field_id.to_string(),
                controller: condition.controller.clone(),
            });
        }
        return Err(QuestionnaireError::UnknownConditionController {
            id: field_id.to_string(),
            controller: condition.controller.clone(),
        });
    };
    if *controller_kind == ScalarKind::Bool {
        match super::decode::parse_bool(&condition.expected) {
            Some(value) => condition.expected = if value { "true" } else { "false" }.to_string(),
            None => {
                return Err(QuestionnaireError::InvalidCondition {
                    id: field_id.to_string(),
                    reason: format!(
                        "controller '{}' is a bool, but the expected value is not a yes/no value",
                        condition.controller
                    ),
                })
            }
        }
    } else if let Some(Constraint::OneOf(choices)) = controller_constraint {
        if !choices.contains(&condition.expected) {
            return Err(QuestionnaireError::InvalidCondition {
                id: field_id.to_string(),
                reason: format!(
                    "controller '{}' never accepts the expected value (its choices are: {})",
                    condition.controller,
                    choices.join(", ")
                ),
            });
        }
    } else if condition.expected.is_empty() || condition.expected != condition.expected.trim() {
        return Err(QuestionnaireError::InvalidCondition {
            id: field_id.to_string(),
            reason: format!(
                "controller '{}' never decodes to the expected value (decoded answers are non-blank and carry no outer whitespace)",
                condition.controller
            ),
        });
    }
    Ok(())
}

/// Reject defaults that could not survive the shared decoder, the
/// single-line marker rendering, or the render/parse round trip (outer
/// whitespace is trimmed away at parse time).
fn validate_default(field: &ScalarField) -> Result<(), QuestionnaireError> {
    let Some(default) = &field.default else {
        return Ok(());
    };
    let invalid = |reason: String| QuestionnaireError::InvalidDefault {
        id: field.id.clone(),
        reason,
    };
    if default.trim().is_empty() {
        return Err(invalid("a default must be non-blank".to_string()));
    }
    if default != default.trim() {
        return Err(invalid(
            "a default must carry no outer whitespace (parsed answers are trimmed, so it could never survive a render/parse round trip)"
                .to_string(),
        ));
    }
    if default.contains('\n') {
        return Err(invalid(
            "a default must be a single line (it renders on the answer marker line)".to_string(),
        ));
    }
    if let Err(diagnostic) = check_field_text(field, default) {
        return Err(invalid(format!(
            "the default does not decode cleanly: {diagnostic}"
        )));
    }
    Ok(())
}
