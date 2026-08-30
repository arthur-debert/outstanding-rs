use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::decode::{check_field_text, AnswerValue, EarlierAnswers};
use super::fingerprint::compute_fingerprint;

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

struct FieldInfo {
    dfs: usize,
    chain: Vec<String>,
    kind: ScalarKind,
    constraint: Option<Constraint>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

impl Questionnaire {
    pub fn new(
        id: impl Into<String>,
        items: Vec<impl Into<Item>>,
    ) -> Result<Self, QuestionnaireError> {
        let id = id.into();
        if !valid_id(&id) {
            return Err(QuestionnaireError::structure(format!(
                "Invalid questionnaire ID '{id}': IDs must be non-empty and use only a-z, 0-9, '.', '_', '-'."
            )));
        }
        let mut items: Vec<Item> = items.into_iter().map(Into::into).collect();
        if items.is_empty() {
            return Err(QuestionnaireError::structure(
                "A questionnaire must declare at least one item (field or group).",
            ));
        }

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

        validate_fields(&mut items, &meta, &field_info)?;

        let fingerprint = compute_fingerprint(&id, &items);
        Ok(Self {
            id,
            items,
            meta,
            fingerprint,
        })
    }

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
            return Err(QuestionnaireError::structure(format!(
                "Invalid ID '{item_id}': IDs must be non-empty and use only a-z, 0-9, '.', '_', '-'."
            )));
        }
        if meta.contains_key(item_id) {
            return Err(QuestionnaireError::structure(format!(
                "Duplicate ID '{item_id}': stable IDs must be unique within a questionnaire."
            )));
        }
        if let Some(parent) = parent {
            let prefix = format!("{parent}.");
            if !item_id.starts_with(&prefix) || item_id.len() == prefix.len() {
                return Err(QuestionnaireError::structure(format!(
                    "Item '{item_id}' inside group '{parent}' must extend the group's ID ('{parent}.<segment>') so submitted occurrence paths stay derivable from definition IDs."
                )));
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
                    return Err(QuestionnaireError::structure(format!(
                        "Group '{}' declares no children: a group must contain at least one field or group.",
                        group.id
                    )));
                }
                if let Some(repeat) = group.repeat {
                    if repeat.min == 0 {
                        return Err(QuestionnaireError::item(
                            group.id.clone(),
                            format!("Invalid repeat bounds on group '{}': the minimum must be at least 1 — rendering emits exactly the minimum number of blocks, and a sheet needs one complete block to copy (declare repeatable(min) before max_occurrences)", group.id),
                        ));
                    }
                    if let Some(max) = repeat.max {
                        if max < repeat.min {
                            return Err(QuestionnaireError::item(
                                group.id.clone(),
                                format!(
                                    "Invalid repeat bounds on group '{}': the maximum ({max}) is below the minimum ({})",
                                    group.id, repeat.min
                                ),
                            ));
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
                        return Err(QuestionnaireError::item(
                            field.id.clone(),
                            format!("Field '{}' attaches a validator with an empty revision: the revision is the validator's semantic identity and must be non-empty.", field.id),
                        ));
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

fn validate_constraint(field: &ScalarField) -> Result<(), QuestionnaireError> {
    let Some(Constraint::OneOf(choices)) = &field.constraint else {
        return Ok(());
    };
    let invalid = |reason: &str| {
        QuestionnaireError::item(
            field.id.clone(),
            format!("Invalid constraint on field '{}': {reason}", field.id),
        )
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

fn validate_condition(
    field_id: &str,
    condition: &mut Condition,
    meta: &HashMap<String, NodeMeta>,
    field_info: &HashMap<String, FieldInfo>,
) -> Result<(), QuestionnaireError> {
    let invalid = |reason: String| {
        QuestionnaireError::item(
            field_id,
            format!("Invalid condition on field '{field_id}': {reason}"),
        )
    };
    let dependent = &field_info[field_id];
    let Some(controller) = field_info.get(&condition.controller) else {
        if meta.contains_key(&condition.controller) {
            return Err(invalid(format!(
                "controller '{}' is a group; a controller must be a scalar field",
                condition.controller
            )));
        }
        return Err(QuestionnaireError::item(
            field_id,
            format!(
                "Field '{field_id}' is conditioned on unknown field '{}'.",
                condition.controller
            ),
        ));
    };
    let enclosing = controller.chain.len() <= dependent.chain.len()
        && dependent.chain[..controller.chain.len()] == controller.chain[..];
    if !enclosing {
        return Err(QuestionnaireError::item(
            field_id,
            format!(
                "Field '{field_id}' is conditioned on '{}', which is not in an enclosing scope. A controller must be declared in the same group as the dependent field or in one of its enclosing groups.",
                condition.controller
            ),
        ));
    }
    if controller.dfs > dependent.dfs {
        return Err(QuestionnaireError::item(
            field_id,
            format!(
                "Field '{field_id}' is conditioned on '{}', which is declared after it. Declare the controlling field first.",
                condition.controller
            ),
        ));
    }
    let controller_kind = &controller.kind;
    let controller_constraint = &controller.constraint;
    if *controller_kind == ScalarKind::Bool {
        match super::decode::parse_bool(&condition.expected) {
            Some(value) => condition.expected = if value { "true" } else { "false" }.to_string(),
            None => {
                return Err(invalid(format!(
                    "controller '{}' is a bool, but the expected value is not a yes/no value",
                    condition.controller
                )))
            }
        }
    } else if let Some(Constraint::OneOf(choices)) = controller_constraint {
        if !choices.contains(&condition.expected) {
            return Err(invalid(format!(
                "controller '{}' never accepts the expected value (its choices are: {})",
                condition.controller,
                choices.join(", ")
            )));
        }
    } else if condition.expected.is_empty() || condition.expected != condition.expected.trim() {
        return Err(invalid(format!(
            "controller '{}' never decodes to the expected value (decoded answers are non-blank and carry no outer whitespace)",
            condition.controller
        )));
    }
    Ok(())
}

fn validate_default(field: &ScalarField) -> Result<(), QuestionnaireError> {
    if let Some(dynamic) = &field.dynamic_default {
        if field.default.is_some() {
            return Err(QuestionnaireError::item(
                field.id.clone(),
                format!("Field '{}' declares both a static and a dynamic default: a field takes one or the other, never both.", field.id),
            ));
        }
        if dynamic.revision().is_empty() {
            return Err(QuestionnaireError::item(
                field.id.clone(),
                format!("Field '{}' attaches a dynamic default with an empty revision: the revision is the dynamic default's semantic identity and must be non-empty.", field.id),
            ));
        }
    }
    let Some(default) = &field.default else {
        return Ok(());
    };
    let invalid = |reason: String| {
        QuestionnaireError::item(
            field.id.clone(),
            format!("Invalid default on field '{}': {reason}", field.id),
        )
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
            "a default must be a single line (it renders pre-filled below the question line)"
                .to_string(),
        ));
    }
    if let Err(diagnostic) = check_field_text(field, field.id(), default) {
        return Err(invalid(format!(
            "the default does not decode cleanly: {diagnostic}"
        )));
    }
    Ok(())
}
