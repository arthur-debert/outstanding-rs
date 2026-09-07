use super::super::{CommandInput, InputCardinality, InputValueType, ProjectSpec, ResultShape};
use super::formatting::quote;

pub(in crate::new_project) fn core_signature_fragment(input: &CommandInput) -> String {
    format!("{}: {}", input.name, input.rust_type())
}

pub(super) fn core_fn_signature(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!("({})", core_signature_fragment(&spec.inputs[0]))
    } else {
        let params = spec
            .inputs
            .iter()
            .map(|input| format!("    {},", core_signature_fragment(input)))
            .collect::<Vec<_>>()
            .join("\n");
        format!("(\n{params}\n)")
    }
}

pub(super) fn core_primary_value(spec: &ProjectSpec) -> String {
    let primary = &spec.inputs[0];
    match (primary.value_type, primary.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            format!("{}.trim().to_string()", primary.name)
        }
        _ => format!("format!(\"{{:?}}\", &{})", primary.name),
    }
}

pub(super) fn core_unused_inputs(spec: &ProjectSpec) -> String {
    let names = spec
        .inputs
        .iter()
        .skip(1)
        .map(|input| format!("&{}", input.name))
        .collect::<Vec<_>>();
    if names.is_empty() {
        String::new()
    } else if names.len() == 1 {
        format!("let _ = {};", names[0])
    } else {
        format!("let _ = ({});", names.join(", "))
    }
}

pub(super) fn result_fields(spec: &ProjectSpec, visibility: &str) -> String {
    match spec.result_shape {
        ResultShape::Message => format!("    {visibility}message: String,"),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| format!("    {visibility}{field}: String,"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub(super) fn result_init(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "        message: format!(\"Processed {primary}\"),".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        summary: format!(\"Processed {primary}\"),".into(),
                "count" | "length" => {
                    format!("        {field}: primary.chars().count().to_string(),")
                }
                other => format!("        {other}: primary.clone(),"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub(super) fn result_field_names(spec: &ProjectSpec) -> Vec<String> {
    match spec.result_shape {
        ResultShape::Message => vec!["message".into()],
        ResultShape::Record => spec.record_fields.clone(),
    }
}

pub(super) fn view_from_fields(spec: &ProjectSpec) -> String {
    result_field_names(spec)
        .into_iter()
        .map(|field| format!("            {field}: value.{field},"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn core_valid_assertions(spec: &ProjectSpec) -> String {
    let primary = match (spec.inputs[0].value_type, spec.inputs[0].cardinality) {
        (InputValueType::String, InputCardinality::Required) => "Standout".to_string(),
        (InputValueType::String, InputCardinality::Optional) => "Some(\"optional\")".to_string(),
        (InputValueType::String, InputCardinality::Repeated) => "[\"alpha\", \"beta\"]".to_string(),
        (InputValueType::Bool, InputCardinality::Boolean) => "true".to_string(),
        (InputValueType::Path, InputCardinality::Required) => "\"config.toml\"".to_string(),
        (InputValueType::Path, InputCardinality::Optional) => "Some(\"config.toml\")".to_string(),
        (InputValueType::Path, InputCardinality::Repeated) => "[\"one.toml\"]".to_string(),
        _ => unreachable!("validated input combinations are renderable"),
    };
    match spec.result_shape {
        ResultShape::Message => {
            let expected = format!("Processed {primary}");
            format!("        assert_eq!(result.message, {});", quote(&expected))
        }
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => format!(
                    "        assert_eq!(result.summary, {});",
                    quote(&format!("Processed {primary}"))
                ),
                "count" | "length" => format!(
                    "        assert_eq!(result.{field}, {});",
                    quote(&primary.chars().count().to_string())
                ),
                other => format!("        assert_eq!(result.{other}, {});", quote(&primary)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_test_call(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        return format!(
            "{}({})",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        );
    }
    let args = spec
        .inputs
        .iter()
        .map(|input| format!("            {},", core_test_arg(input, false)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}(\n{args}\n        )", spec.operation_name)
}

pub(super) fn core_sample_result(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!(
            "        let result = {}({}).unwrap();",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        )
    } else {
        format!(
            "        let result = {}\n        .unwrap();",
            core_test_call(spec)
        )
    }
}

pub(super) fn core_invalid_test(spec: &ProjectSpec) -> String {
    let Some(index) = spec.inputs.iter().position(|input| {
        input.value_type == InputValueType::String
            && input.cardinality == InputCardinality::Required
    }) else {
        let call = core_test_call(spec);
        let result = if spec.inputs.len() == 1 {
            format!("        let result = {call}.unwrap();")
        } else {
            format!("        let result = {call}\n        .unwrap();")
        };
        return format!(
            r#"    #[test]
    fn configured_inputs_are_accepted_by_the_core() {{
{result}

        assert_eq!(result, result.clone());
    }}"#
        );
    };

    if spec.inputs.len() == 1 {
        return format!(
            r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}({}),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
            spec.operation_name,
            core_test_arg(&spec.inputs[0], true),
            quote(&spec.inputs[index].name)
        );
    }

    let args = spec
        .inputs
        .iter()
        .enumerate()
        .map(|(input_index, input)| {
            format!(
                "                {},",
                core_test_arg(input, input_index == index)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}(
{args}
            ),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
        spec.operation_name,
        quote(&spec.inputs[index].name)
    )
}

fn core_test_arg(input: &CommandInput, blank: bool) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            if blank {
                "\"\".to_string()".into()
            } else {
                "\"Standout\".to_string()".into()
            }
        }
        (InputValueType::String, InputCardinality::Optional) => {
            if blank {
                "Some(\"\".to_string())".into()
            } else {
                "Some(\"optional\".to_string())".into()
            }
        }
        (InputValueType::String, InputCardinality::Repeated) => {
            "vec![\"alpha\".to_string(), \"beta\".to_string()]".into()
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => {
            "std::path::PathBuf::from(\"config.toml\")".into()
        }
        (InputValueType::Path, InputCardinality::Optional) => {
            "Some(std::path::PathBuf::from(\"config.toml\"))".into()
        }
        (InputValueType::Path, InputCardinality::Repeated) => {
            "vec![std::path::PathBuf::from(\"one.toml\")]".into()
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}
