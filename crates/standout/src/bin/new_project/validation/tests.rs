use super::*;
use crate::new_project::test_support::{required_string, TestProjectAnswers};
use crate::new_project::{ProjectSpec, ResultShape};

#[test]
fn input_source_aliases_cannot_declare_the_same_source_twice() {
    let duplicate = parse_input_sources("argument,arg").unwrap_err();

    assert!(duplicate.to_string().contains("declared more than once"));
}

#[test]
fn project_spec_is_private_validated_model() {
    let spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![required_string("document")],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();

    assert_eq!(spec.lib_crate, "demolib");
    assert_eq!(spec.operation_name, "process_inspect");
    assert_eq!(spec.view_name, "InspectView");
}

#[test]
fn result_fields_and_generated_identifiers_are_validated() {
    let duplicate = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![required_string("document")],
        result_shape: ResultShape::Record,
        record_fields: vec!["summary".into(), "summary".into()],
    })
    .unwrap_err();
    let keyword = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "match".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![required_string("document")],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap_err();

    assert!(duplicate.to_string().contains("declared more than once"));
    assert!(keyword.to_string().contains("reserved Rust keyword"));
}

#[test]
fn generated_flags_cannot_collide_across_inputs() {
    let answers_with = |inputs| TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs,
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    };

    for reserved in ["output", "output_file_path", "help"] {
        let error =
            ProjectSpec::from_answers(answers_with(vec![required_string(reserved)])).unwrap_err();
        assert!(error.to_string().contains("reserved framework/Clap flag"));
        assert!(error
            .to_string()
            .contains(&format!("--{}", reserved.replace('_', "-"))));
    }

    let derived_collision = ProjectSpec::from_answers(answers_with(vec![
        required_string("document"),
        CommandInput {
            name: "document_file".into(),
            value_type: InputValueType::Path,
            cardinality: InputCardinality::Optional,
            sources: vec![InputSource::Argument],
        },
    ]))
    .unwrap_err();
    assert!(derived_collision.to_string().contains("--document-file"));
    assert!(derived_collision.to_string().contains("conflicts"));
}

#[test]
fn validates_supported_typed_cardinality_source_combinations() {
    let rich_inputs = vec![
        CommandInput {
            name: "document".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
        },
        CommandInput {
            name: "verbose".into(),
            value_type: InputValueType::Bool,
            cardinality: InputCardinality::Boolean,
            sources: vec![InputSource::Argument],
        },
        CommandInput {
            name: "config".into(),
            value_type: InputValueType::Path,
            cardinality: InputCardinality::Optional,
            sources: vec![InputSource::Argument],
        },
    ];

    let spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: rich_inputs,
        result_shape: ResultShape::Record,
        record_fields: vec!["summary".into(), "count".into()],
    })
    .unwrap();

    assert_eq!(
        spec.inputs[0].policy_sentence(),
        "document comes from --document, then --document-file, then piped stdin"
    );
}

#[test]
fn rejects_unsupported_input_combinations_before_rendering() {
    let error = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![CommandInput {
            name: "enabled".into(),
            value_type: InputValueType::Bool,
            cardinality: InputCardinality::Optional,
            sources: vec![InputSource::Argument],
        }],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("bool inputs must use boolean cardinality"));

    let error = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![CommandInput {
            name: "config".into(),
            value_type: InputValueType::Path,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File],
        }],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("path inputs only support argument source"));
}

/// An optional value with a second source has no blessed spelling, so the wizard refuses it.
#[test]
fn an_optional_input_cannot_take_a_second_source() {
    let error = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![CommandInput {
            name: "note".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Optional,
            sources: vec![InputSource::Argument, InputSource::Stdin],
        }],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("optional inputs only support argument source"));
}
