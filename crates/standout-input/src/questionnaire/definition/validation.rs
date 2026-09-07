use super::super::decode::check_field_text;
use super::super::fingerprint::compute_fingerprint;
use super::{
    Condition, Constraint, Item, NodeMeta, Questionnaire, QuestionnaireError, ScalarField,
    ScalarKind,
};
use std::collections::{HashMap, HashSet};

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
        match super::super::decode::parse_bool(&condition.expected) {
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
