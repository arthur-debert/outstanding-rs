//! Questionnaire definitions: stable identities plus cosmetic wording.
//!
//! A [`Questionnaire`] is an application-owned, static description of the
//! information to collect. The definition carries two very different kinds of
//! data and the split is the point:
//!
//! - **Semantic** (identity-bearing): the questionnaire ID, each field's
//!   stable ID, its [`ScalarKind`], and its optionality. These feed the
//!   [fingerprint](Questionnaire::fingerprint) and determine how a rendered
//!   sheet is parsed.
//! - **Cosmetic** (presentation-only): question wording and field order.
//!   Changing them never changes answer identity or the fingerprint.
//!
//! Definitions are validated at construction: [`Questionnaire::new`] rejects
//! empty or malformed IDs and duplicate field IDs, so every constructed
//! questionnaire can render and parse without further checks.

use super::fingerprint::compute_fingerprint;

/// The kind of value a scalar field collects.
///
/// The kind is *semantic*: it participates in the questionnaire
/// [fingerprint](Questionnaire::fingerprint) and drives the rendered type
/// hint. It does not change how raw answer text is captured — both kinds
/// round-trip text with outer whitespace trimmed and internal line breaks
/// preserved. Decoding raw text into typed values is application/later-stage
/// work, not part of the answer-sheet boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// A short, single-line-oriented value (rendered hint: `string`).
    String,
    /// Free-form prose that may span several lines (rendered hint: `text`).
    Text,
}

impl ScalarKind {
    /// Stable name used in the fingerprint canonical form and type hints.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ScalarKind::String => "string",
            ScalarKind::Text => "text",
        }
    }
}

/// One scalar question in a questionnaire.
///
/// The `id` is the stable machine identity rendered as the bracketed token
/// (`[project.name]`); the `prompt` is human wording and may be edited freely
/// without affecting compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarField {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) kind: ScalarKind,
    pub(crate) optional: bool,
}

impl ScalarField {
    /// Create a required scalar field with a stable `id`, human `prompt`
    /// wording, and answer `kind`.
    ///
    /// The `id` is validated when the field is passed to
    /// [`Questionnaire::new`], not here.
    pub fn new(id: impl Into<String>, prompt: impl Into<String>, kind: ScalarKind) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            kind,
            optional: false,
        }
    }

    /// Mark this field as optional (a blank answer means omission).
    ///
    /// Optionality is semantic: it changes the fingerprint.
    pub fn optional(mut self) -> Self {
        self.optional = true;
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

    /// Whether a blank answer means omission rather than a missing value.
    pub fn is_optional(&self) -> bool {
        self.optional
    }

    /// The cosmetic type hint rendered after the bracketed ID.
    pub(crate) fn type_hint(&self) -> String {
        if self.optional {
            format!("{}, optional", self.kind.name())
        } else {
            self.kind.name().to_string()
        }
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
}

/// An application-owned questionnaire definition.
///
/// See the [module documentation](crate::questionnaire) for the ownership
/// boundary and the rendered answer-sheet format.
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
    /// sheet's preamble. `fields` are rendered and parsed in the given order,
    /// but order is cosmetic: reordering fields does not change the
    /// [fingerprint](Self::fingerprint).
    ///
    /// # Errors
    ///
    /// Returns a [`QuestionnaireError`] for an invalid questionnaire or field
    /// ID, a duplicate field ID, or an empty field list.
    pub fn new(
        id: impl Into<String>,
        fields: Vec<ScalarField>,
    ) -> Result<Self, QuestionnaireError> {
        let id = id.into();
        if !valid_id(&id) {
            return Err(QuestionnaireError::InvalidQuestionnaireId(id));
        }
        if fields.is_empty() {
            return Err(QuestionnaireError::NoFields);
        }
        let mut seen: Vec<&str> = Vec::with_capacity(fields.len());
        for field in &fields {
            if !valid_id(&field.id) {
                return Err(QuestionnaireError::InvalidFieldId(field.id.clone()));
            }
            if seen.contains(&field.id.as_str()) {
                return Err(QuestionnaireError::DuplicateFieldId(field.id.clone()));
            }
            seen.push(&field.id);
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
    /// definition — questionnaire ID, field IDs, kinds, and optionality. It
    /// deliberately excludes wording, presentation order, and everything else
    /// cosmetic, so copy-editing a questionnaire never invalidates existing
    /// answer sheets. It is **not** an authenticity or tamper-proofing
    /// mechanism.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Look up a field by stable ID.
    pub(crate) fn field(&self, id: &str) -> Option<&ScalarField> {
        self.fields.iter().find(|f| f.id == id)
    }
}
