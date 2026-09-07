use super::super::{CommandInput, InputSource, ProjectSpec};
use super::core::result_field_names;
use super::formatting::{quote, FN_CALL_WIDTH, MAX_WIDTH};
use super::pipeline::{expected_first_field, expected_primary_text};

pub(super) fn handler_expected_fields(spec: &ProjectSpec) -> String {
    let expected = expected_first_field(spec, "hello");
    result_field_names(spec)
        .into_iter()
        .map(|field| match field.as_str() {
            "message" | "summary" => {
                format!("                {field}: {}.into(),", quote(&expected.1))
            }
            "count" | "length" => {
                let value = expected_primary_text(&spec.inputs[0], "hello")
                    .chars()
                    .count()
                    .to_string();
                format!("                {field}: {}.into(),", quote(&value))
            }
            _ => {
                let value = expected_primary_text(&spec.inputs[0], "hello");
                format!("                {field}: {}.into(),", quote(&value))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn has_chain_inputs(spec: &ProjectSpec) -> bool {
    spec.inputs.iter().any(CommandInput::is_chain)
}

pub(super) fn has_file_source(spec: &ProjectSpec) -> bool {
    spec.inputs
        .iter()
        .any(|input| input.sources.contains(&InputSource::File))
}

pub(super) fn handler_imports(spec: &ProjectSpec) -> String {
    let mut imports = vec![
        format!("use {} as core;", spec.lib_crate),
        "use serde::Serialize;".to_string(),
        "use standout::handler;".to_string(),
    ];
    if has_file_source(spec) {
        imports.push("use clap::ArgMatches;".to_string());
    }
    if has_chain_inputs(spec) {
        imports.push(
            "use standout::cli::{CommandConfig, CommandContext, CommandContextInput, Output};"
                .to_string(),
        );
        let mut chain = vec!["InputChain"];
        if spec
            .inputs
            .iter()
            .any(|input| input.sources.contains(&InputSource::Argument))
        {
            chain.push("ArgSource");
        }
        if spec
            .inputs
            .iter()
            .any(|input| input.sources.contains(&InputSource::Stdin))
        {
            chain.push("StdinSource");
        }
        chain.sort_unstable();
        imports.push(match chain.as_slice() {
            [single] => format!("use standout::input::{single};"),
            several => format!("use standout::input::{{{}}};", several.join(", ")),
        });
    } else {
        imports.push("use standout::cli::Output;".to_string());
    }
    imports.sort();
    imports.join("\n")
}

pub(super) fn handler_signature(spec: &ProjectSpec) -> String {
    use unicode_width::UnicodeWidthStr;

    let mut params = spec
        .inputs
        .iter()
        .filter(|input| !input.is_chain())
        .map(CommandInput::handler_param)
        .collect::<Vec<_>>();
    if has_chain_inputs(spec) {
        params.push("#[ctx] ctx: &CommandContext".to_string());
    }
    let command_ident = spec.command_name.replace('-', "_");
    let return_type = format!("-> Result<Output<{}>, anyhow::Error> {{", spec.view_name);
    let one_line = format!(
        "pub(crate) fn {command_ident}({}) {return_type}",
        params.join(", ")
    );
    if one_line.width() <= MAX_WIDTH {
        return one_line;
    }
    let lines = params
        .iter()
        .map(|param| format!("    {param},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("pub(crate) fn {command_ident}(\n{lines}\n) {return_type}")
}

pub(super) fn handler_input_reads(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .filter(|input| input.is_chain())
        .map(|input| {
            format!(
                "    let {}: &String = ctx.input({})?;",
                input.name,
                quote(&input.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Empty when every value arrives as a typed parameter.
pub(super) fn command_inputs_fn(spec: &ProjectSpec) -> String {
    let chained = spec
        .inputs
        .iter()
        .filter(|input| input.is_chain())
        .collect::<Vec<_>>();
    let body = match chained.as_slice() {
        [] => return String::new(),
        [input] => format!(
            "    config.input(\n        {},\n        {},\n    )",
            quote(&input.name),
            input.chain_expr(8)
        ),
        inputs => {
            let entries = inputs
                .iter()
                .map(|input| {
                    format!(
                        "        .input(\n            {},\n            {},\n        )",
                        quote(&input.name),
                        input.chain_expr(12)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("    config\n{entries}")
        }
    };
    format!(
        "/// Where the command's values come from, tried in the order listed.\npub(crate) fn {}_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H> {{\n{body}\n}}",
        spec.command_name.replace('-', "_")
    )
}

/// The app's own `InputCollector`; it reports the `file` source kind and names the path on failure.
pub(super) fn file_source_item(spec: &ProjectSpec) -> String {
    if !has_file_source(spec) {
        return String::new();
    }
    r#"struct FileSource {
    arg: &'static str,
}

impl FileSource {
    fn new(arg: &'static str) -> Self {
        Self { arg }
    }
}

impl standout::input::InputCollector<String> for FileSource {
    fn name(&self) -> &'static str {
        "file"
    }

    fn is_available(&self, matches: &ArgMatches) -> bool {
        matches.get_one::<std::path::PathBuf>(self.arg).is_some()
    }

    fn collect(&self, matches: &ArgMatches) -> Result<Option<String>, standout::input::InputError> {
        let Some(path) = matches.get_one::<std::path::PathBuf>(self.arg) else {
            return Ok(None);
        };
        std::fs::read_to_string(path)
            .map(Some)
            .map_err(|error| standout::input::InputError::file(path.display().to_string(), error))
    }

    fn bind_sources(
        &self,
        _sources: &standout::input::InputSources,
    ) -> Option<Box<dyn standout::input::InputCollector<String>>> {
        None
    }
}"#
    .to_string()
}

fn handler_sample_value(index: usize) -> &'static str {
    if index == 0 {
        "hello"
    } else {
        "sample"
    }
}

pub(super) fn handler_call(spec: &ProjectSpec) -> String {
    use unicode_width::UnicodeWidthStr;

    let mut args = spec
        .inputs
        .iter()
        .enumerate()
        .filter(|(_, input)| !input.is_chain())
        .map(|(index, input)| input.handler_test_value(handler_sample_value(index)))
        .collect::<Vec<_>>();
    if has_chain_inputs(spec) {
        args.push("&ctx".to_string());
    }
    let command_ident = spec.command_name.replace('-', "_");
    let inline = args.join(", ");
    let one_line = format!("        let Output::Render(view) = {command_ident}({inline}).unwrap()");
    if inline.width() <= FN_CALL_WIDTH && one_line.width() <= MAX_WIDTH {
        if one_line.width() + " else {".width() <= MAX_WIDTH {
            return format!("{one_line} else {{");
        }
        return format!("{one_line}\n        else {{");
    }
    let lines = args
        .iter()
        .map(|arg| format!("            {arg},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "        let Output::Render(view) = {command_ident}(\n{lines}\n        )\n        .unwrap() else {{"
    )
}

fn source_kind_variant(source: InputSource) -> &'static str {
    match source {
        InputSource::Argument => "Arg",
        InputSource::File => "File",
        InputSource::Stdin => "Stdin",
    }
}

/// Stands in for pre-dispatch resolution: each value carries its first source's kind.
pub(super) fn handler_test_inputs(spec: &ProjectSpec) -> String {
    if !has_chain_inputs(spec) {
        return String::new();
    }
    let mut lines = vec![
        "        let mut ctx = CommandContext::default();".to_string(),
        "        let mut inputs = standout_input::Inputs::new();".to_string(),
    ];
    for (index, input) in spec.inputs.iter().enumerate() {
        if !input.is_chain() {
            continue;
        }
        lines.push(format!(
            "        inputs.insert(\n            {},\n            standout_input::ResolvedInput {{\n                value: {}.to_string(),\n                source: standout_input::InputSourceKind::{},\n            }},\n        );",
            quote(&input.name),
            quote(handler_sample_value(index)),
            source_kind_variant(input.sources[0])
        ));
    }
    lines.push("        ctx.extensions.insert(inputs);".to_string());
    lines.join("\n")
}
