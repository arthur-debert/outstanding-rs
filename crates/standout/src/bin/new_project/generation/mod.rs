use super::validation::pascal_case;
use super::{CommandInput, ProjectSpec, ResultShape};
use anyhow::{Context, Result};
use minijinja::context;
use standout_render::template::new_environment;
use std::collections::BTreeMap;
use std::path::PathBuf;

mod core;
mod formatting;
mod handlers;
mod inputs;
mod pipeline;
mod readme;
mod templates;
#[cfg(test)]
mod tests;

pub(super) use core::core_signature_fragment;
use core::*;
use formatting::*;
use handlers::*;
pub(super) use inputs::command_syntax_fragment;
use pipeline::*;
use readme::*;
use templates::TEMPLATE_CATALOG;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GeneratedFiles {
    pub(super) files: BTreeMap<PathBuf, String>,
}

impl GeneratedFiles {
    pub(super) fn render(spec: &ProjectSpec) -> Result<Self> {
        let mut env = code_generation_environment();
        for (name, source) in TEMPLATE_CATALOG {
            env.add_template(name, source)
                .with_context(|| format!("template {name} is malformed"))?;
        }

        let mut files = BTreeMap::new();
        for (path_template, template_name) in FILE_MAP {
            let path = render_inline(path_template, spec)?;
            let mut body = env
                .get_template(template_name)?
                .render(model(spec))
                .with_context(|| format!("template {template_name} is missing model data"))?;
            if !body.ends_with('\n') {
                body.push('\n');
            }
            files.insert(PathBuf::from(path), body);
        }
        Ok(Self { files })
    }
}

fn template_body(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "[title]{{ message }}[/title]\n".into(),
        ResultShape::Record => {
            let mut lines = vec![format!(
                "[title]{{{{ {} }}}}[/title]",
                spec.record_fields[0]
            )];
            for field in &spec.record_fields {
                lines.push(format!("{}: {{{{ {} }}}}", pascal_case(field), field));
            }
            lines.join("\n") + "\n"
        }
    }
}

fn code_generation_environment() -> minijinja::Environment<'static> {
    let mut environment = new_environment();
    environment.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    environment.set_formatter(|output, _state, value| {
        output.write_str(&standout_render::template::stringify(value))?;
        Ok(())
    });
    environment
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    code_generation_environment()
        .template_from_str(template)?
        .render(model(spec))
        .with_context(|| format!("path template {template} is missing model data"))
}

fn model(spec: &ProjectSpec) -> minijinja::Value {
    let primary = &spec.inputs[0];
    context! {
        project_name => spec.project_name,
        executable_name => spec.executable_name,
        command_name => spec.command_name,
        command_ident => spec.command_name.replace('-', "_"),
        command_variant => pascal_case(&spec.command_name.replace('-', "_")),
        command_description => spec.command_description,
        cli_command_attribute => cli_command_attribute(spec),
        input_name => primary.name,
        inputs => spec.inputs.iter().map(|input| {
            context! {
                name => input.name,
                cli_arg => input.cli_arg(),
                core_call_arg => input.core_call_arg(),
                rust_type => input.rust_type(),
                policy => input.policy_sentence(),
            }
        }).collect::<Vec<_>>(),
        core_params => spec.inputs.iter().map(core_signature_fragment).collect::<Vec<_>>().join(", "),
        core_fn_signature => core_fn_signature(spec),
        core_call_args => spec.inputs.iter().map(CommandInput::core_call_arg).collect::<Vec<_>>().join(", "),
        core_validations => spec.inputs.iter().filter_map(CommandInput::core_validation).collect::<Vec<_>>().join("\n    "),
        core_unused_inputs => core_unused_inputs(spec),
        cli_args => spec.inputs.iter().map(CommandInput::cli_arg).collect::<Vec<_>>().join("\n        "),
        dispatch_attribute => dispatch_attribute(spec),
        lib_crate => spec.lib_crate,
        lib_package => spec.lib_crate.replace('_', "-"),
        operation_name => spec.operation_name,
        view_name => spec.view_name,
        result_shape => spec.result_shape.as_str(),
        core_primary_value => core_primary_value(spec),
        core_result_fields => result_fields(spec, "pub "),
        cli_view_fields => result_fields(spec, "pub(crate) "),
        result_init => result_init(spec),
        view_from_fields => view_from_fields(spec),
        core_valid_assertions => core_valid_assertions(spec),
        core_sample_result => core_sample_result(spec),
        core_invalid_test => core_invalid_test(spec),
        handler_expected_fields => handler_expected_fields(spec),
        handler_imports => handler_imports(spec),
        handler_signature => handler_signature(spec),
        handler_input_reads => handler_input_reads(spec),
        handler_call => handler_call(spec),
        handler_test_inputs => handler_test_inputs(spec),
        command_inputs_fn => command_inputs_fn(spec),
        file_source_item => file_source_item(spec),
        pipeline_human_run => generated_harness_run(spec, "Ada", &sample_cli_args(spec, "Ada")),
        pipeline_json_test => generated_json_pipeline_test(spec),
        template_body => template_body(spec),
        human_expected => quote(&expected_first_field(spec, "Ada").1),
        readme_input_policy => readme_input_policy(spec),
        readme_validation_note => readme_validation_note(spec),
        readme_examples => readme_examples(spec),
        command_syntax => spec.inputs.iter().map(command_syntax_fragment).collect::<Vec<_>>().join(" "),
        standout_version => spec.standout_version,
        clapfig_version => CLAPFIG_VERSION,
        config_test => generated_config_test(spec),
        local_patch_root => spec.local_patch_root.as_deref().map(toml_basic_string_content),
    }
}

const CLAPFIG_VERSION: &str = "0.26";

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
    ("crates/{{ executable_name }}/README.md", "readme"),
    ("crates/{{ executable_name }}/src/main.rs", "main"),
    ("crates/{{ executable_name }}/src/cli.rs", "cli"),
    ("crates/{{ executable_name }}/src/config.rs", "config"),
    ("crates/{{ executable_name }}/src/handlers.rs", "handlers"),
    (
        "crates/{{ executable_name }}/src/templates/{{ command_name }}.jinja",
        "template",
    ),
    (
        "crates/{{ executable_name }}/src/styles/{{ project_name }}.css",
        "style",
    ),
];
