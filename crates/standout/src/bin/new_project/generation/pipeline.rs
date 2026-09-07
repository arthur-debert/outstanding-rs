use super::super::{CommandInput, InputCardinality, InputSource, InputValueType, ProjectSpec};
use super::core::result_field_names;
use super::formatting::{quote, rust_array, FN_CALL_WIDTH};

pub(super) fn expected_first_field(spec: &ProjectSpec, input: &str) -> (String, String) {
    let field = result_field_names(spec).remove(0);
    let input = expected_primary_text(&spec.inputs[0], input);
    let value = match field.as_str() {
        "message" | "summary" => format!("Processed {input}"),
        "count" | "length" => input.chars().count().to_string(),
        _ => input,
    };
    (field, value)
}

pub(super) fn expected_primary_text(input: &CommandInput, value: &str) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => value.to_string(),
        (InputValueType::String, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::String, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra\"]")
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => format!("\"{value}\""),
        (InputValueType::Path, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::Path, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra.toml\"]")
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}

/// The human page is what a bare invocation renders, so it names no `--output`.
pub(super) fn sample_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![quote(&spec.executable_name), quote(&spec.command_name)];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_json_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("json"),
    ];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_command_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        args.extend(input.sample_args_for_source(
            input.sources[0],
            if index == 0 { primary_value } else { "sample" },
        ));
    }
    args
}

/// The chained `TestHarness` setup calls the samples need, in order, for the
/// caller to join.
fn harness_setup_calls(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut calls = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        let value = if index == 0 { primary_value } else { "sample" };
        match input.sources[0] {
            InputSource::File => calls.push(format!(
                ".fixture({}, {})",
                quote(&input.sample_file_name()),
                quote(value)
            )),
            InputSource::Stdin => {
                calls.push(format!(".piped_stdin({})", quote(&format!("{value}\n"))))
            }
            InputSource::Argument => {}
        }
    }
    calls
}

fn harness_expression(calls: &[String]) -> String {
    match calls.len() {
        0 | 1 => format!("TestHarness::new(){}", calls.join("")),
        _ => format!(
            "TestHarness::new()\n            {}",
            calls.join("\n            ")
        ),
    }
}

/// The harness binding and the `run` call, as two statements: a one-call chain
/// is what rustfmt formats predictably.
pub(super) fn generated_harness_run(
    spec: &ProjectSpec,
    primary_value: &str,
    args: &[String],
) -> String {
    let harness = harness_expression(&harness_setup_calls(spec, primary_value));
    let inline_args = format!("&app, cli::command(), [{}]", args.join(", "));
    let run = if inline_args.len() <= FN_CALL_WIDTH {
        format!("        let result = harness.run({inline_args});")
    } else {
        format!(
            "        let result = harness.run(\n            &app,\n            cli::command(),\n            {},\n        );",
            rust_array(args, 16, 63)
        )
    };
    format!("        let harness = {harness};\n{run}")
}

pub(super) fn generated_json_pipeline_test(spec: &ProjectSpec) -> String {
    let primary_value = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Grace",
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let args = sample_json_cli_args(spec, primary_value);
    let run = generated_harness_run(spec, primary_value, &args);
    format!(
        r#"    #[test]
    #[serial]
    fn pipeline_serializes_json_for_configured_inputs() {{
        let (_user_dir, app) = test_app();

{run}

        result.assert_success();
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});
    }}"#,
        expected = quote(&expected)
    )
}

pub(super) fn generated_config_test(spec: &ProjectSpec) -> String {
    let primary_value = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Grace",
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let executable = quote(&spec.executable_name);
    let file = quote(&format!("{}.toml", spec.executable_name));
    let mut calls = harness_setup_calls(spec, primary_value);
    calls.push(format!(".fixture({file}, CONFIG_FILE)"));
    let harness = harness_expression(&calls);
    let mut args = vec![executable.clone(), quote(&spec.command_name)];
    args.extend(sample_command_args(spec, primary_value));
    let inline_args = format!("&app, cli::command(), [{}]", args.join(", "));
    let run = if inline_args.len() <= FN_CALL_WIDTH {
        format!("        let result = harness.run({inline_args});")
    } else {
        format!(
            "        let result = harness.run(\n            &app,\n            cli::command(),\n            {},\n        );",
            rust_array(&args, 16, 63)
        )
    };
    format!(
        r#"    #[test]
    #[serial]
    fn term_output_in_the_config_file_selects_json() {{
        let (_user_dir, app) = test_app();
        let harness = {harness};
{run}

        result.assert_success();
        assert_eq!(result.output_mode(), Representation::Json);
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});

        let shown = TestHarness::new()
            .fixture({file}, CONFIG_FILE)
            .run(
                &app,
                cli::command(),
                [{executable}, "config", "get", "term.output"],
            );

        shown.assert_success();
        shown.assert_stdout_contains("term.output = json");
    }}"#,
        expected = quote(&expected)
    )
}
