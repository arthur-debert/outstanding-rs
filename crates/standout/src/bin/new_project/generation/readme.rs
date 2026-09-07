use super::super::{InputCardinality, InputSource, InputValueType, ProjectSpec};

pub(super) fn readme_input_policy(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .map(|input| format!("- {}.", input.policy_sentence()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn readme_validation_note(spec: &ProjectSpec) -> String {
    let inputs = spec
        .inputs
        .iter()
        .filter(|input| {
            input.value_type == InputValueType::String
                && input.cardinality == InputCardinality::Required
        })
        .map(|input| format!("`{}`", input.name))
        .collect::<Vec<_>>();
    match inputs.as_slice() {
        [] => String::new(),
        [input] => {
            format!("Blank values for the required string input {input} are rejected by the core operation.")
        }
        _ => format!(
            "Blank values for the required string inputs {} are rejected by the core operation.",
            inputs.join(", ")
        ),
    }
}

pub(super) fn readme_examples(spec: &ProjectSpec) -> String {
    let command = format!(
        "cargo run -p {} -- {}",
        spec.executable_name, spec.command_name
    );
    let args = spec
        .inputs
        .iter()
        .enumerate()
        .flat_map(|(index, input)| input.readme_args(if index == 0 { "VALUE" } else { "SAMPLE" }))
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = Vec::new();
    for input in &spec.inputs {
        if input.sources[0] == InputSource::File {
            lines.push(format!("printf '%s' VALUE > {}", input.sample_file_name()));
        }
    }
    let invocation = format!("{command} {args}").trim().to_string();
    if spec
        .inputs
        .iter()
        .any(|input| input.sources[0] == InputSource::Stdin)
    {
        lines.push(format!("printf '%s\\n' VALUE | {invocation}"));
        lines.push(format!("printf '%s\\n' VALUE | {invocation} --output json"));
    } else {
        lines.push(invocation.clone());
        lines.push(format!("{invocation} --output json"));
    }
    lines.join("\n")
}
