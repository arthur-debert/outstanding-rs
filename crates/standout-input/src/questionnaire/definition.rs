//! Questionnaire definitions: stable identities plus cosmetic wording.
//!
//! A [`Questionnaire`] is an application-owned, static description of the
//! information to collect: a tree of [`Item`]s, where each item is either a
//! [`ScalarField`] or a [`Group`] of nested items (optionally repeatable
//! within declared bounds). The definition carries two very different kinds
//! of data and the split is the point:
//!
//! - **Semantic** (identity-bearing): the questionnaire ID, each field's and
//!   group's stable ID, group structure and [`Repeat`] bounds, each field's
//!   [`ScalarKind`], its optionality, its declared default, its
//!   [`Constraint`], its conditional-applicability [`Condition`], and the
//!   revision of any attached [`FieldValidator`]. These feed the
//!   [fingerprint](Questionnaire::fingerprint) and determine how answers are
//!   decoded and validated.
//! - **Cosmetic** (presentation-only): question wording and item order.
//!   Changing them never changes answer identity or the fingerprint.
//!
//! Definitions are validated at construction: [`Questionnaire::new`] rejects
//! empty or malformed IDs, duplicate IDs, empty groups, invalid repeat
//! bounds, child IDs that do not extend their group's ID, invalid defaults
//! and constraints, and conditions that reference unknown, later-declared,
//! or out-of-scope fields, so every constructed questionnaire can render,
//! parse, and decode without further checks.
//!
//! # Definition IDs vs occurrence paths
//!
//! Every field and group declares one *stable definition ID* such as
//! `command.inputs.name`. A submitted answer instead lives at an *occurrence
//! path*: for scalar fields and fields inside non-repeatable groups the path
//! equals the definition ID, while each occurrence of a repeatable group
//! inserts a zero-based index — the second submitted input's name is
//! `command.inputs[1].name`. Definition IDs never carry indexes; indexes
//! belong to an answer instance, which is why every child of a group must
//! extend its group's ID (`command.inputs` → `command.inputs.name`): the
//! occurrence path is then always derivable from the definition.

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
    /// The controller must live in the same group as this field or in one of
    /// its enclosing groups (so every submitted occurrence resolves it
    /// unambiguously); a controller inside a repeatable group gates its
    /// dependents *per occurrence*. An inactive field may stay blank (or
    /// keep its untouched pre-filled default) even when required; a
    /// *populated* inactive field is a validation error. Conditions are
    /// semantic: they change the fingerprint.
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

/// Repeat bounds for a repeatable [`Group`]: at least `min` occurrences
/// (rendering emits exactly `min` blank blocks) and, when declared, at most
/// `max`.
///
/// Bounds are semantic: they participate in the
/// [fingerprint](Questionnaire::fingerprint). The minimum must be at least 1
/// so a rendered sheet always contains one complete block to copy; an
/// entirely optional list of items should live behind a conditional field or
/// an optional child instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    pub(crate) min: usize,
    pub(crate) max: Option<usize>,
}

impl Repeat {
    /// The minimum number of occurrences (also the rendered count).
    pub fn min(&self) -> usize {
        self.min
    }

    /// The maximum number of occurrences, if bounded.
    pub fn max(&self) -> Option<usize> {
        self.max
    }
}

/// A named group of nested questionnaire items.
///
/// The `id` is the stable machine identity rendered as the bracketed token
/// (`[command.inputs]`); the `prompt` is human wording and may be edited
/// freely. Every child's ID must extend the group's ID with a `.` segment
/// (`command.inputs` → `command.inputs.name`), which keeps submitted
/// occurrence paths derivable from definition IDs.
///
/// A plain group ([`Group::new`]) renders and is answered exactly once — it
/// structures related questions under one heading. A *repeatable* group
/// ([`Group::repeatable`]) is answered once per submitted occurrence:
/// rendering emits exactly the declared minimum number of blank blocks, and
/// a person adds items by copying a complete block (its heading line and its
/// questions). The number of submitted items is inferred from occurrences of
/// the stable group header, never from display numbers or wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) children: Vec<Item>,
    pub(crate) repeat: Option<Repeat>,
}

impl Group {
    /// Create a non-repeatable group with a stable `id`, human `prompt`
    /// wording, and nested `children`.
    ///
    /// The `id`, the child-ID prefix rule, and all nested declarations are
    /// validated when the group is passed to [`Questionnaire::new`], not
    /// here.
    pub fn new(
        id: impl Into<String>,
        prompt: impl Into<String>,
        children: impl IntoIterator<Item = impl Into<Item>>,
    ) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            children: children.into_iter().map(Into::into).collect(),
            repeat: None,
        }
    }

    /// Make this group repeatable with at least `min` occurrences
    /// (`min >= 1`; rendering emits exactly `min` blank blocks).
    ///
    /// Repeat bounds are semantic: they change the fingerprint.
    pub fn repeatable(mut self, min: usize) -> Self {
        self.repeat = Some(Repeat { min, max: None });
        self
    }

    /// Bound a repeatable group to at most `max` occurrences.
    ///
    /// Only meaningful after [`repeatable`](Self::repeatable);
    /// [`Questionnaire::new`] rejects a maximum on a non-repeatable group or
    /// a maximum below the minimum. Semantic: changes the fingerprint.
    pub fn max_occurrences(mut self, max: usize) -> Self {
        match &mut self.repeat {
            Some(repeat) => repeat.max = Some(max),
            // Recorded as an impossible bound so `Questionnaire::new` can
            // reject it with a precise error instead of silently ignoring it.
            None => {
                self.repeat = Some(Repeat {
                    min: 0,
                    max: Some(max),
                })
            }
        }
        self
    }

    /// The stable group ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The human wording (cosmetic).
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The nested items, in presentation order.
    pub fn children(&self) -> &[Item] {
        &self.children
    }

    /// The repeat bounds, or `None` for a group answered exactly once.
    pub fn repeat(&self) -> Option<Repeat> {
        self.repeat
    }

    /// The definition-ID prefix every child extends (`<id>.`).
    pub(crate) fn def_prefix(&self) -> String {
        format!("{}.", self.id)
    }

    /// The cosmetic type hint rendered after the bracketed ID.
    pub(crate) fn type_hint(&self) -> String {
        match self.repeat {
            None => "section".to_string(),
            Some(Repeat { min, max: None }) => {
                format!("repeatable section, minimum {min}")
            }
            Some(Repeat {
                min,
                max: Some(max),
            }) => format!("repeatable section, minimum {min}, maximum {max}"),
        }
    }
}

/// One node of a questionnaire definition: a scalar question or a group of
/// nested items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A single scalar question.
    Field(ScalarField),
    /// A (possibly repeatable) group of nested items.
    Group(Group),
}

impl From<ScalarField> for Item {
    fn from(field: ScalarField) -> Self {
        Item::Field(field)
    }
}

impl From<Group> for Item {
    fn from(group: Group) -> Self {
        Item::Group(group)
    }
}

impl Item {
    /// The stable ID of this item, field or group alike.
    pub fn id(&self) -> &str {
        match self {
            Item::Field(field) => field.id(),
            Item::Group(group) => group.id(),
        }
    }
}

/// Join an occurrence-path prefix and a child segment (`""` prefixes join to
/// the bare segment, so root items keep their definition IDs as paths).
pub(crate) fn path_join(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

/// The path segment a child contributes under its group's definition-ID
/// prefix (`command.inputs.` + `command.inputs.name` → `name`).
pub(crate) fn child_segment<'a>(def_prefix: &str, id: &'a str) -> &'a str {
    id.strip_prefix(def_prefix).unwrap_or(id)
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

    /// A field or group ID is empty or contains characters outside
    /// `a-z`, `0-9`, `.`, `_`, `-`.
    #[error("Invalid ID '{0}': IDs must be non-empty and use only a-z, 0-9, '.', '_', '-'.")]
    InvalidId(String),

    /// Two items (fields or groups) declare the same stable ID.
    #[error("Duplicate ID '{0}': stable IDs must be unique within a questionnaire.")]
    DuplicateId(String),

    /// The questionnaire declares no items.
    #[error("A questionnaire must declare at least one item (field or group).")]
    NoFields,

    /// A group declares no children.
    #[error("Group '{0}' declares no children: a group must contain at least one field or group.")]
    EmptyGroup(String),

    /// A group child's ID does not extend the group's ID.
    #[error("Item '{child}' inside group '{group}' must extend the group's ID ('{group}.<segment>') so submitted occurrence paths stay derivable from definition IDs.")]
    GroupChildId {
        /// The enclosing group.
        group: String,
        /// The offending child ID.
        child: String,
    },

    /// A group declares impossible repeat bounds.
    #[error("Invalid repeat bounds on group '{id}': {reason}")]
    InvalidRepeat {
        /// The group carrying the bounds.
        id: String,
        /// Why the bounds are invalid.
        reason: String,
    },

    /// A condition references a field outside the dependent field's own
    /// scope chain. A controller must live in the same group occurrence or
    /// an enclosing one, so that every submitted occurrence can resolve it
    /// unambiguously.
    #[error("Field '{id}' is conditioned on '{controller}', which is not in an enclosing scope. A controller must be declared in the same group as the dependent field or in one of its enclosing groups.")]
    ConditionScope {
        /// The dependent field.
        id: String,
        /// The out-of-scope controller ID.
        controller: String,
    },

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
    items: Vec<Item>,
    /// Every declared ID mapped to its structural position, for scope
    /// resolution during parsing and decoding.
    meta: HashMap<String, NodeMeta>,
    fingerprint: String,
}

/// Structural position of one declared ID within the definition tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeMeta {
    /// The enclosing group's ID, or `None` at the questionnaire root.
    pub(crate) parent: Option<String>,
    /// Whether the ID names a group (vs a scalar field).
    pub(crate) group: bool,
}

/// Per-field facts gathered during the structural pass, consumed by the
/// semantic pass to validate and canonicalize conditions.
struct FieldInfo {
    /// Depth-first declaration index (controllers must come first).
    dfs: usize,
    /// The chain of enclosing group IDs, outermost first.
    chain: Vec<String>,
    kind: ScalarKind,
    constraint: Option<Constraint>,
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
    /// sheet's preamble. `items` — scalar fields and (possibly repeatable)
    /// groups; a `Vec<ScalarField>` works directly for a flat questionnaire
    /// — are rendered, collected, and decoded in the given order, but order
    /// is cosmetic for identity: reordering items does not change the
    /// [fingerprint](Self::fingerprint). The one ordering rule is
    /// structural: a conditional field's controller must be declared before
    /// it, in the same group or an enclosing one.
    ///
    /// Condition expected values are canonicalized here (a bool controller's
    /// `"yes"` becomes `"true"`), so equivalent declarations fingerprint
    /// identically.
    ///
    /// # Errors
    ///
    /// Returns a [`QuestionnaireError`] for an invalid questionnaire or item
    /// ID, a duplicate ID, an empty item list, an empty group, invalid
    /// repeat bounds, a child ID that does not extend its group's ID, or an
    /// invalid default, constraint, condition, or validator revision.
    pub fn new(
        id: impl Into<String>,
        items: Vec<impl Into<Item>>,
    ) -> Result<Self, QuestionnaireError> {
        let id = id.into();
        if !valid_id(&id) {
            return Err(QuestionnaireError::InvalidQuestionnaireId(id));
        }
        let mut items: Vec<Item> = items.into_iter().map(Into::into).collect();
        if items.is_empty() {
            return Err(QuestionnaireError::NoFields);
        }

        // Structural pass: IDs, duplicates, group shape, repeat bounds.
        let mut meta = HashMap::new();
        let mut field_info = HashMap::new();
        collect_structure(
            &items,
            None,
            &mut Vec::new(),
            &mut meta,
            &mut field_info,
            &mut 0,
        )?;

        // Semantic pass: constraints, defaults, validators, conditions.
        validate_fields(&mut items, &meta, &field_info)?;

        let fingerprint = compute_fingerprint(&id, &items);
        Ok(Self {
            id,
            items,
            meta,
            fingerprint,
        })
    }

    /// The stable questionnaire ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The declared items, in presentation order.
    pub fn items(&self) -> &[Item] {
        &self.items
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

    /// Look up a group anywhere in the tree by stable ID.
    pub(crate) fn group_def(&self, id: &str) -> Option<&Group> {
        find_group(&self.items, id)
    }

    /// Structural position of a declared ID, or `None` for unknown IDs.
    pub(crate) fn node_meta(&self, id: &str) -> Option<&NodeMeta> {
        self.meta.get(id)
    }
}

/// Depth-first group lookup by stable ID.
fn find_group<'a>(items: &'a [Item], id: &str) -> Option<&'a Group> {
    items.iter().find_map(|item| match item {
        Item::Field(_) => None,
        Item::Group(group) if group.id == id => Some(group),
        Item::Group(group) => find_group(&group.children, id),
    })
}

/// Structural validation walk: ID shape and uniqueness, the child-ID prefix
/// rule, group non-emptiness, and repeat bounds. Fills `meta` (structural
/// position per ID) and `field_info` (per-field facts for the semantic
/// pass).
fn collect_structure(
    items: &[Item],
    parent: Option<&str>,
    chain: &mut Vec<String>,
    meta: &mut HashMap<String, NodeMeta>,
    field_info: &mut HashMap<String, FieldInfo>,
    dfs: &mut usize,
) -> Result<(), QuestionnaireError> {
    for item in items {
        let item_id = item.id();
        if !valid_id(item_id) {
            return Err(QuestionnaireError::InvalidId(item_id.to_string()));
        }
        if meta.contains_key(item_id) {
            return Err(QuestionnaireError::DuplicateId(item_id.to_string()));
        }
        if let Some(parent) = parent {
            let prefix = format!("{parent}.");
            if !item_id.starts_with(&prefix) || item_id.len() == prefix.len() {
                return Err(QuestionnaireError::GroupChildId {
                    group: parent.to_string(),
                    child: item_id.to_string(),
                });
            }
        }
        meta.insert(
            item_id.to_string(),
            NodeMeta {
                parent: parent.map(str::to_string),
                group: matches!(item, Item::Group(_)),
            },
        );
        *dfs += 1;
        match item {
            Item::Field(field) => {
                field_info.insert(
                    field.id.clone(),
                    FieldInfo {
                        dfs: *dfs,
                        chain: chain.clone(),
                        kind: field.kind,
                        constraint: field.constraint.clone(),
                    },
                );
            }
            Item::Group(group) => {
                if group.children.is_empty() {
                    return Err(QuestionnaireError::EmptyGroup(group.id.clone()));
                }
                if let Some(repeat) = group.repeat {
                    if repeat.min == 0 {
                        return Err(QuestionnaireError::InvalidRepeat {
                            id: group.id.clone(),
                            reason: "the minimum must be at least 1 — rendering emits exactly the minimum number of blocks, and a sheet needs one complete block to copy (declare repeatable(min) before max_occurrences)".to_string(),
                        });
                    }
                    if let Some(max) = repeat.max {
                        if max < repeat.min {
                            return Err(QuestionnaireError::InvalidRepeat {
                                id: group.id.clone(),
                                reason: format!(
                                    "the maximum ({max}) is below the minimum ({})",
                                    repeat.min
                                ),
                            });
                        }
                    }
                }
                chain.push(group.id.clone());
                collect_structure(
                    &group.children,
                    Some(&group.id),
                    chain,
                    meta,
                    field_info,
                    dfs,
                )?;
                chain.pop();
            }
        }
    }
    Ok(())
}

/// Semantic validation walk: per-field constraints, defaults, validator
/// revisions, and conditions (classified and canonicalized against the
/// structural pass's facts).
fn validate_fields(
    items: &mut [Item],
    meta: &HashMap<String, NodeMeta>,
    field_info: &HashMap<String, FieldInfo>,
) -> Result<(), QuestionnaireError> {
    for item in items {
        match item {
            Item::Field(field) => {
                validate_constraint(field)?;
                let field_id = field.id.clone();
                if let Some(condition) = &mut field.condition {
                    validate_condition(&field_id, condition, meta, field_info)?;
                }
                if let Some(validator) = &field.validator {
                    if validator.revision().is_empty() {
                        return Err(QuestionnaireError::EmptyValidatorRevision {
                            id: field.id.clone(),
                        });
                    }
                }
                validate_default(field)?;
            }
            Item::Group(group) => {
                validate_fields(&mut group.children, meta, field_info)?;
            }
        }
    }
    Ok(())
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

/// Check a condition's controller — it must be an earlier-declared field in
/// the dependent's own scope chain — and rewrite the expected value into
/// canonical form.
fn validate_condition(
    field_id: &str,
    condition: &mut Condition,
    meta: &HashMap<String, NodeMeta>,
    field_info: &HashMap<String, FieldInfo>,
) -> Result<(), QuestionnaireError> {
    let dependent = &field_info[field_id];
    let Some(controller) = field_info.get(&condition.controller) else {
        if meta.contains_key(&condition.controller) {
            return Err(QuestionnaireError::InvalidCondition {
                id: field_id.to_string(),
                reason: format!(
                    "controller '{}' is a group; a controller must be a scalar field",
                    condition.controller
                ),
            });
        }
        return Err(QuestionnaireError::UnknownConditionController {
            id: field_id.to_string(),
            controller: condition.controller.clone(),
        });
    };
    // The controller's scope chain must enclose (or equal) the dependent's,
    // so every submitted occurrence resolves the controller unambiguously.
    let enclosing = controller.chain.len() <= dependent.chain.len()
        && dependent.chain[..controller.chain.len()] == controller.chain[..];
    if !enclosing {
        return Err(QuestionnaireError::ConditionScope {
            id: field_id.to_string(),
            controller: condition.controller.clone(),
        });
    }
    if controller.dfs > dependent.dfs {
        return Err(QuestionnaireError::ConditionOrder {
            id: field_id.to_string(),
            controller: condition.controller.clone(),
        });
    }
    let controller_kind = &controller.kind;
    let controller_constraint = &controller.constraint;
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
    if let Err(diagnostic) = check_field_text(field, field.id(), default) {
        return Err(invalid(format!(
            "the default does not decode cleanly: {diagnostic}"
        )));
    }
    Ok(())
}
