mod validation;

use std::collections::HashMap;
use std::sync::Arc;

use super::decode::{AnswerValue, EarlierAnswers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    String,
    Text,
    Bool,
    Path,
}

impl ScalarKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ScalarKind::String => "string",
            ScalarKind::Text => "text",
            ScalarKind::Bool => "bool",
            ScalarKind::Path => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    OneOf(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub(crate) controller: String,
    pub(crate) expected: String,
}

impl Condition {
    pub fn controller(&self) -> &str {
        &self.controller
    }

    pub fn expected(&self) -> &str {
        &self.expected
    }
}

type ValidatorCheck = Arc<dyn Fn(&AnswerValue) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct FieldValidator {
    revision: String,
    check: ValidatorCheck,
}

impl FieldValidator {
    pub fn new(
        revision: impl Into<String>,
        check: impl Fn(&AnswerValue) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            revision: revision.into(),
            check: Arc::new(check),
        }
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

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

impl PartialEq for FieldValidator {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}

impl Eq for FieldValidator {}

type DefaultCompute = Arc<dyn Fn(&EarlierAnswers<'_>) -> String + Send + Sync>;

#[derive(Clone)]
pub struct DynamicDefault {
    revision: String,
    compute: DefaultCompute,
}

impl DynamicDefault {
    pub fn new(
        revision: impl Into<String>,
        compute: impl Fn(&EarlierAnswers<'_>) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            revision: revision.into(),
            compute: Arc::new(compute),
        }
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn compute(&self, earlier: &EarlierAnswers<'_>) -> String {
        (self.compute)(earlier)
    }
}

impl std::fmt::Debug for DynamicDefault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicDefault")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DynamicDefault {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}

impl Eq for DynamicDefault {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarField {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) kind: ScalarKind,
    pub(crate) optional: bool,
    pub(crate) default: Option<String>,
    pub(crate) dynamic_default: Option<DynamicDefault>,
    pub(crate) constraint: Option<Constraint>,
    pub(crate) condition: Option<Condition>,
    pub(crate) validator: Option<FieldValidator>,
}

impl ScalarField {
    pub fn new(id: impl Into<String>, prompt: impl Into<String>, kind: ScalarKind) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            kind,
            optional: false,
            default: None,
            dynamic_default: None,
            constraint: None,
            condition: None,
            validator: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn with_dynamic_default(mut self, dynamic_default: DynamicDefault) -> Self {
        self.dynamic_default = Some(dynamic_default);
        self
    }

    pub fn one_of(mut self, choices: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.constraint = Some(Constraint::OneOf(
            choices.into_iter().map(Into::into).collect(),
        ));
        self
    }

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

    pub fn with_validator(mut self, validator: FieldValidator) -> Self {
        self.validator = Some(validator);
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn kind(&self) -> ScalarKind {
        self.kind
    }

    pub fn is_optional(&self) -> bool {
        self.optional
    }

    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn dynamic_default(&self) -> Option<&DynamicDefault> {
        self.dynamic_default.as_ref()
    }

    pub fn constraint(&self) -> Option<&Constraint> {
        self.constraint.as_ref()
    }

    pub fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }

    pub fn validator(&self) -> Option<&FieldValidator> {
        self.validator.as_ref()
    }

    pub(crate) fn type_hint(&self) -> String {
        let mut hint = match &self.constraint {
            Some(Constraint::OneOf(choices)) => join_or(choices),
            None => self.kind.name().to_string(),
        };
        if self.optional {
            hint.push_str(", optional");
        }
        if let Some(condition) = &self.condition {
            hint.push_str(&format!(
                "; only when {} is {}",
                condition.controller, condition.expected
            ));
        }
        hint
    }
}

fn join_or(choices: &[String]) -> String {
    match choices {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        [head @ .., last] => format!("{}, or {last}", head.join(", ")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    pub(crate) min: usize,
    pub(crate) max: Option<usize>,
}

impl Repeat {
    pub fn min(&self) -> usize {
        self.min
    }

    pub fn max(&self) -> Option<usize> {
        self.max
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) children: Vec<Item>,
    pub(crate) repeat: Option<Repeat>,
}

impl Group {
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

    pub fn repeatable(mut self, min: usize) -> Self {
        self.repeat = Some(Repeat { min, max: None });
        self
    }

    pub fn max_occurrences(mut self, max: usize) -> Self {
        match &mut self.repeat {
            Some(repeat) => repeat.max = Some(max),
            None => {
                self.repeat = Some(Repeat {
                    min: 0,
                    max: Some(max),
                })
            }
        }
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn children(&self) -> &[Item] {
        &self.children
    }

    pub fn repeat(&self) -> Option<Repeat> {
        self.repeat
    }

    pub(crate) fn def_prefix(&self) -> String {
        format!("{}.", self.id)
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Field(ScalarField),
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
    pub fn id(&self) -> &str {
        match self {
            Item::Field(field) => field.id(),
            Item::Group(group) => group.id(),
        }
    }
}

pub(crate) fn path_join(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

pub(crate) fn child_segment<'a>(def_prefix: &str, id: &'a str) -> &'a str {
    id.strip_prefix(def_prefix).unwrap_or(id)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuestionnaireError {
    #[error("{reason}")]
    Structure { reason: String },

    #[error("{reason}")]
    Item { id: String, reason: String },
}

impl QuestionnaireError {
    pub(crate) fn structure(reason: impl Into<String>) -> Self {
        Self::Structure {
            reason: reason.into(),
        }
    }

    pub(crate) fn item(id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Item {
            id: id.into(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Questionnaire {
    id: String,
    items: Vec<Item>,
    meta: HashMap<String, NodeMeta>,
    fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeMeta {
    pub(crate) parent: Option<String>,
    pub(crate) group: bool,
}

impl Questionnaire {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub(crate) fn group_def(&self, id: &str) -> Option<&Group> {
        find_group(&self.items, id)
    }

    pub(crate) fn node_meta(&self, id: &str) -> Option<&NodeMeta> {
        self.meta.get(id)
    }
}

fn find_group<'a>(items: &'a [Item], id: &str) -> Option<&'a Group> {
    items.iter().find_map(|item| match item {
        Item::Field(_) => None,
        Item::Group(group) if group.id == id => Some(group),
        Item::Group(group) => find_group(&group.children, id),
    })
}
