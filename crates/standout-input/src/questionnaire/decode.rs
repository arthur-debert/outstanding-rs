use std::collections::BTreeMap;

use super::definition::{
    child_segment, path_join, Constraint, Item, Questionnaire, ScalarField, ScalarKind,
};
use super::parse::RawAnswers;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerValue {
    Text(String),
    Bool(bool),
}

impl AnswerValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            AnswerValue::Text(s) => Some(s),
            AnswerValue::Bool(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            AnswerValue::Bool(b) => Some(*b),
            AnswerValue::Text(_) => None,
        }
    }

    pub(crate) fn canonical(&self) -> String {
        match self {
            AnswerValue::Text(s) => s.clone(),
            AnswerValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    values: BTreeMap<String, AnswerValue>,
    occurrences: BTreeMap<String, usize>,
}

impl Answers {
    pub fn get(&self, path: &str) -> Option<&AnswerValue> {
        self.values.get(path)
    }

    pub fn get_text(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(AnswerValue::as_text)
    }

    pub fn get_bool(&self, path: &str) -> Option<bool> {
        self.get(path).and_then(AnswerValue::as_bool)
    }

    pub fn occurrence_count(&self, group_path: &str) -> usize {
        self.occurrences.get(group_path).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormError {
    pub fields: Vec<String>,
    pub message: String,
}

impl FormError {
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

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ValidationDiagnostic {
    #[error("[{id}]: {message}")]
    Field { id: String, message: String },

    #[error("{}", form_display(.fields, .message))]
    Form {
        fields: Vec<String>,
        message: String,
    },
}

impl ValidationDiagnostic {
    pub(crate) fn field(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Field {
            id: id.into(),
            message: message.into(),
        }
    }
}

fn form_display(fields: &[String], message: &str) -> String {
    if fields.is_empty() {
        message.to_string()
    } else {
        format!("{message} (fields: {})", fields.join(", "))
    }
}

pub(crate) fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(true),
        "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

pub(crate) fn check_field_text(
    field: &ScalarField,
    path: &str,
    text: &str,
) -> Result<AnswerValue, ValidationDiagnostic> {
    let value = match field.kind() {
        ScalarKind::Text => AnswerValue::Text(text.to_string()),
        ScalarKind::String | ScalarKind::Path => {
            if text.contains('\n') {
                return Err(ValidationDiagnostic::field(
                    path,
                    format!("a {} answer must be a single line", field.kind().name()),
                ));
            }
            AnswerValue::Text(text.to_string())
        }
        ScalarKind::Bool => match parse_bool(text) {
            Some(b) => AnswerValue::Bool(b),
            None => {
                return Err(ValidationDiagnostic::field(
                    path,
                    "expected a yes/no answer (true, false, yes, no, y, or n)",
                ))
            }
        },
    };
    if let Some(Constraint::OneOf(choices)) = field.constraint() {
        let matches = value
            .as_text()
            .is_some_and(|t| choices.iter().any(|c| c == t));
        if !matches {
            return Err(ValidationDiagnostic::field(
                path,
                format!("the answer must be one of: {}.", choices.join(", ")),
            ));
        }
    }
    if let Some(validator) = field.validator() {
        if let Err(message) = validator.check(&value) {
            return Err(ValidationDiagnostic::field(path, message));
        }
    }
    Ok(value)
}

pub(crate) fn decode_field(
    field: &ScalarField,
    path: &str,
    raw: Option<&str>,
    computed: Option<&str>,
) -> Result<Option<AnswerValue>, ValidationDiagnostic> {
    let submitted = raw.map(str::trim).filter(|t| !t.is_empty());
    let effective = submitted.or(field.default()).or(computed);
    match effective {
        Some(text) => check_field_text(field, path, text).map(Some),
        None if field.is_optional() => Ok(None),
        None => Err(ValidationDiagnostic::field(
            path,
            "this question requires an answer.",
        )),
    }
}

pub(crate) enum FieldOutcome {
    Answered(AnswerValue),
    Omitted,
    Inactive,
    Errored,
}

#[derive(Debug, Clone)]
pub(crate) struct ScopeCtx {
    pub(crate) group_id: Option<String>,
    pub(crate) def_prefix: String,
    pub(crate) path_prefix: String,
}

impl ScopeCtx {
    pub(crate) fn root() -> Self {
        Self {
            group_id: None,
            def_prefix: String::new(),
            path_prefix: String::new(),
        }
    }

    pub(crate) fn child_path(&self, id: &str) -> String {
        path_join(&self.path_prefix, child_segment(&self.def_prefix, id))
    }
}

pub struct EarlierAnswers<'a> {
    questionnaire: &'a Questionnaire,
    chain: &'a [ScopeCtx],
    outcomes: &'a BTreeMap<String, FieldOutcome>,
}

impl<'a> EarlierAnswers<'a> {
    pub(crate) fn new(
        questionnaire: &'a Questionnaire,
        chain: &'a [ScopeCtx],
        outcomes: &'a BTreeMap<String, FieldOutcome>,
    ) -> Self {
        Self {
            questionnaire,
            chain,
            outcomes,
        }
    }

    pub fn get(&self, field_id: &str) -> Option<&AnswerValue> {
        let meta = self.questionnaire.node_meta(field_id)?;
        if meta.group {
            return None;
        }
        let scope = self
            .chain
            .iter()
            .rev()
            .find(|scope| scope.group_id.as_deref() == meta.parent.as_deref())?;
        match self.outcomes.get(&scope.child_path(field_id)) {
            Some(FieldOutcome::Answered(value)) => Some(value),
            _ => None,
        }
    }

    pub fn get_text(&self, field_id: &str) -> Option<&str> {
        self.get(field_id).and_then(AnswerValue::as_text)
    }

    pub fn get_bool(&self, field_id: &str) -> Option<bool> {
        self.get(field_id).and_then(AnswerValue::as_bool)
    }
}

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
                                diagnostics.push(ValidationDiagnostic::field(
                                    path.clone(),
                                    format!(
                                        "this question does not apply (it is asked only when {} is {}); remove its answer or change the controlling answer.",
                                        condition.controller(),
                                        condition.expected()
                                    ),
                                ));
                                FieldOutcome::Errored
                            }
                        }
                        Some(true) => {
                            let computed = field.dynamic_default().map(|dynamic| {
                                dynamic.compute(&EarlierAnswers::new(self, chain, outcomes))
                            });
                            match decode_field(field, &path, raw_value, computed.as_deref()) {
                                Ok(Some(value)) => FieldOutcome::Answered(value),
                                Ok(None) => FieldOutcome::Omitted,
                                Err(diagnostic) => {
                                    diagnostics.push(diagnostic);
                                    FieldOutcome::Errored
                                }
                            }
                        }
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
                                diagnostics.push(ValidationDiagnostic::field(
                                    base.clone(),
                                    format!(
                                        "{count} of at least {} required item(s) submitted. Copy a complete group block - its heading line and its questions - for each missing item.",
                                        repeat.min()
                                    ),
                                ));
                            }
                            if let Some(max) = repeat.max() {
                                if count > max {
                                    diagnostics.push(ValidationDiagnostic::field(
                                        base.clone(),
                                        format!(
                                            "{count} items submitted, but at most {max} are accepted. Remove the extra group block(s)."
                                        ),
                                    ));
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
