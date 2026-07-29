use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use minijinja::{context, Environment};

#[derive(Parser)]
#[command(name = "standout", about = "Standout project tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the smallest runnable Standout workspace.
    NewProject,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::NewProject => {
            let answers = prompt_answers(&mut io::stdin().lock(), &mut io::stdout())?;
            let spec = ProjectSpec::from_answers(answers)?;
            write_review(&spec, &mut io::stdout())?;
            if !confirm(&mut io::stdin().lock(), &mut io::stdout())? {
                println!("Generation cancelled.");
                return Ok(());
            }
            publish_project(&spec)?;
            println!("Created {}", spec.destination.display());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WizardAnswers {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProjectSpec {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
    lib_crate: String,
    operation_name: String,
    view_name: String,
    destination: PathBuf,
    standout_version: String,
    local_patch_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandInput {
    name: String,
    value_type: InputValueType,
    cardinality: InputCardinality,
    sources: Vec<InputSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputValueType {
    String,
    Bool,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputCardinality {
    Required,
    Optional,
    Repeated,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSource {
    Argument,
    File,
    Stdin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultShape {
    Message,
    Record,
}

impl ResultShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Record => "record",
        }
    }
}

impl ProjectSpec {
    fn from_answers(answers: WizardAnswers) -> Result<Self> {
        validate_crate_name(&answers.project_name, "project name")?;
        validate_crate_name(&answers.executable_name, "executable name")?;
        validate_ident(&answers.command_name.replace('-', "_"), "command name")?;
        if answers.inputs.is_empty() {
            bail!("at least one command input is required");
        }
        for input in &answers.inputs {
            input.validate()?;
        }
        validate_result_fields(answers.result_shape, &answers.record_fields)?;
        if answers.command_description.trim().is_empty() {
            bail!("command description cannot be empty");
        }

        let lib_crate = format!("{}lib", answers.project_name.replace('-', "_"));
        let command_ident = answers.command_name.replace('-', "_");
        let operation_name = format!("process_{command_ident}");
        let view_name = format!("{}View", pascal_case(&command_ident));
        Ok(Self {
            destination: PathBuf::from(&answers.project_name),
            project_name: answers.project_name,
            executable_name: answers.executable_name,
            command_name: answers.command_name,
            command_description: answers.command_description,
            inputs: answers.inputs,
            result_shape: answers.result_shape,
            record_fields: answers.record_fields,
            lib_crate,
            operation_name,
            view_name,
            standout_version: env!("CARGO_PKG_VERSION").to_string(),
            local_patch_root: None,
        })
    }

    fn tree(&self) -> Vec<String> {
        vec![
            "Cargo.toml".into(),
            format!("crates/{}/Cargo.toml", self.lib_crate),
            format!("crates/{}/src/lib.rs", self.lib_crate),
            format!("crates/{}/Cargo.toml", self.executable_name),
            format!("crates/{}/README.md", self.executable_name),
            format!("crates/{}/src/main.rs", self.executable_name),
            format!("crates/{}/src/cli.rs", self.executable_name),
            format!("crates/{}/src/handlers.rs", self.executable_name),
            format!(
                "crates/{}/src/templates/{}.jinja",
                self.executable_name, self.command_name
            ),
            format!(
                "crates/{}/src/styles/{}.css",
                self.executable_name, self.project_name
            ),
        ]
    }
}

impl CommandInput {
    fn validate(&self) -> Result<()> {
        validate_ident(&self.name, "input name")?;
        if self.sources.is_empty() {
            bail!("{} must allow at least one input source", self.name);
        }
        if self.cardinality == InputCardinality::Boolean {
            if self.value_type != InputValueType::Bool {
                bail!("{} uses boolean cardinality but is not bool", self.name);
            }
            if self.sources != [InputSource::Argument] {
                bail!("{} boolean flags only support argument source", self.name);
            }
        }
        if self.value_type == InputValueType::Bool && self.cardinality != InputCardinality::Boolean
        {
            bail!("{} bool inputs must use boolean cardinality", self.name);
        }
        if self.value_type == InputValueType::Path
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} path inputs only support argument source", self.name);
        }
        if self.cardinality == InputCardinality::Repeated
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} repeated inputs only support argument source", self.name);
        }
        Ok(())
    }

    fn policy_sentence(&self) -> String {
        let sources = self
            .sources
            .iter()
            .map(|source| match source {
                InputSource::Argument if self.cardinality == InputCardinality::Boolean => {
                    format!("--{}", self.name.replace('_', "-"))
                }
                InputSource::Argument => format!("--{}", self.name.replace('_', "-")),
                InputSource::File => format!("--{}-file", self.name.replace('_', "-")),
                InputSource::Stdin => "piped stdin".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", then ");
        format!("{} comes from {sources}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedFiles {
    files: BTreeMap<PathBuf, String>,
}

impl GeneratedFiles {
    fn render(spec: &ProjectSpec) -> Result<Self> {
        let mut env = Environment::new();
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

fn prompt_answers(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<WizardAnswers> {
    let project_name = prompt(input, output, "Project name")?;
    validate_crate_name(&project_name, "project name")?;

    let executable_name = prompt_default(input, output, "Executable name", &project_name)?;
    validate_crate_name(&executable_name, "executable name")?;

    let command_name = prompt(input, output, "Initial command name")?;
    validate_ident(&command_name.replace('-', "_"), "command name")?;

    let command_description = prompt(input, output, "One-sentence command description")?;
    if command_description.trim().is_empty() {
        bail!("command description cannot be empty");
    }

    let inputs = prompt_command_inputs(input, output)?;
    let result_shape = prompt_result_shape(input, output)?;
    let record_fields = match result_shape {
        ResultShape::Message => Vec::new(),
        ResultShape::Record => prompt_record_fields(input, output)?,
    };

    Ok(WizardAnswers {
        project_name,
        executable_name,
        command_name,
        command_description,
        inputs,
        result_shape,
        record_fields,
    })
}

fn prompt_command_inputs(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Vec<CommandInput>> {
    let count = prompt_default(input, output, "Number of command inputs", "1")?;
    let count: usize = count
        .parse()
        .context("number of command inputs must be an integer")?;
    if count == 0 {
        bail!("at least one command input is required");
    }

    let mut inputs = Vec::with_capacity(count);
    for index in 1..=count {
        writeln!(output, "Input {index}:")?;
        let name = prompt(input, output, "  Name")?;
        let value_type = prompt_input_value_type(input, output)?;
        let cardinality = prompt_input_cardinality(input, output, value_type)?;
        let sources = prompt_input_sources(input, output, value_type, cardinality)?;
        let command_input = CommandInput {
            name,
            value_type,
            cardinality,
            sources,
        };
        command_input.validate()?;
        inputs.push(command_input);
    }
    Ok(inputs)
}

fn prompt(input: &mut dyn BufRead, output: &mut dyn Write, label: &str) -> Result<String> {
    write!(output, "{label}: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let value = line.trim().to_string();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value)
}

fn prompt_default(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
    default: &str,
) -> Result<String> {
    write!(output, "{label} [{default}]: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let value = line.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn confirm(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<bool> {
    write!(output, "Generate this project? Type 'yes' to continue: ")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line.trim() == "yes")
}

fn prompt_result_shape(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<ResultShape> {
    let value = prompt_default(input, output, "Result shape (message/record)", "record")?;
    match value.as_str() {
        "message" => Ok(ResultShape::Message),
        "record" => Ok(ResultShape::Record),
        _ => bail!("result shape must be message or record"),
    }
}

fn prompt_record_fields(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<Vec<String>> {
    let value = prompt_default(
        input,
        output,
        "Record fields (comma-separated)",
        "summary,count",
    )?;
    parse_record_fields(&value)
}

fn prompt_input_value_type(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<InputValueType> {
    let value = prompt_default(input, output, "  Type (string/bool/path)", "string")?;
    match value.as_str() {
        "string" => Ok(InputValueType::String),
        "bool" => Ok(InputValueType::Bool),
        "path" => Ok(InputValueType::Path),
        _ => bail!("input type must be string, bool, or path"),
    }
}

fn prompt_input_cardinality(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    value_type: InputValueType,
) -> Result<InputCardinality> {
    let default = if value_type == InputValueType::Bool {
        "boolean"
    } else {
        "required"
    };
    let value = prompt_default(
        input,
        output,
        "  Cardinality (required/optional/repeated/boolean)",
        default,
    )?;
    match value.as_str() {
        "required" => Ok(InputCardinality::Required),
        "optional" => Ok(InputCardinality::Optional),
        "repeated" => Ok(InputCardinality::Repeated),
        "boolean" => Ok(InputCardinality::Boolean),
        _ => bail!("input cardinality must be required, optional, repeated, or boolean"),
    }
}

fn prompt_input_sources(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    value_type: InputValueType,
    cardinality: InputCardinality,
) -> Result<Vec<InputSource>> {
    let default = if value_type == InputValueType::String
        && matches!(
            cardinality,
            InputCardinality::Required | InputCardinality::Optional
        ) {
        "argument,file,stdin"
    } else {
        "argument"
    };
    let value = prompt_default(
        input,
        output,
        "  Sources in precedence order (argument,file,stdin)",
        default,
    )?;
    parse_input_sources(&value)
}

fn write_review(spec: &ProjectSpec, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "\nReview")?;
    writeln!(output, "Destination: {}", spec.destination.display())?;
    writeln!(output, "Generated tree:")?;
    for path in spec.tree() {
        writeln!(output, "  {path}")?;
    }
    writeln!(
        output,
        "Command syntax: {} {} {}",
        spec.executable_name,
        spec.command_name,
        spec.inputs
            .iter()
            .map(command_syntax_fragment)
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    writeln!(output, "Input policy:")?;
    for input in &spec.inputs {
        writeln!(output, "  - {}.", input.policy_sentence())?;
    }
    writeln!(
        output,
        "Core operation: {}::{}({})",
        spec.lib_crate,
        spec.operation_name,
        spec.inputs
            .iter()
            .map(core_signature_fragment)
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(
        output,
        "Output shape: {} {} renders human output and serializes as JSON.",
        spec.result_shape.as_str(),
        spec.view_name
    )?;
    writeln!(
        output,
        "Generated tests: core validation, typed handler mapping, TestHarness pipeline."
    )?;
    Ok(())
}

fn publish_project(spec: &ProjectSpec) -> Result<()> {
    if spec.destination.exists() {
        if !spec.destination.is_dir() {
            bail!(
                "destination {} already exists and is not a directory",
                spec.destination.display()
            );
        }
        if fs::read_dir(&spec.destination)?.next().is_some() {
            bail!(
                "destination {} already exists and is not empty",
                spec.destination.display()
            );
        }
    }

    let generated = GeneratedFiles::render(spec)?;
    let parent = spec.destination.parent().unwrap_or_else(|| Path::new("."));
    let name = spec
        .destination
        .file_name()
        .ok_or_else(|| anyhow!("destination must have a final path component"))?;
    let staging = parent.join(format!(
        ".{}.standout-new-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    if let Err(error) = write_generated_files(&staging, &generated) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if spec.destination.exists() {
        fs::remove_dir(&spec.destination).with_context(|| {
            format!(
                "destination {} must be empty before publish",
                spec.destination.display()
            )
        })?;
    }

    match fs::rename(&staging, &spec.destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error).with_context(|| {
                format!(
                    "failed to publish staged project to {}",
                    spec.destination.display()
                )
            })
        }
    }
}

fn write_generated_files(root: &Path, generated: &GeneratedFiles) -> Result<()> {
    for (path, body) in &generated.files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, body)?;
    }
    Ok(())
}

fn validate_crate_name(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        bail!("{label} may only contain letters, numbers, underscores, or hyphens");
    }
    Ok(())
}

fn validate_ident(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        bail!("{label} may only contain letters, numbers, or underscores");
    }
    if is_rust_keyword(value) {
        bail!("{label} cannot be a reserved Rust keyword");
    }
    Ok(())
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn parse_record_fields(value: &str) -> Result<Vec<String>> {
    let fields: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    validate_result_fields(ResultShape::Record, &fields)?;
    Ok(fields)
}

fn parse_input_sources(value: &str) -> Result<Vec<InputSource>> {
    let mut sources = Vec::new();
    for source in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let parsed = match source {
            "argument" | "arg" => InputSource::Argument,
            "file" => InputSource::File,
            "stdin" | "piped stdin" => InputSource::Stdin,
            _ => bail!("input source must be argument, file, or stdin"),
        };
        if sources.contains(&parsed) {
            bail!("input source {source} is declared more than once");
        }
        sources.push(parsed);
    }
    if sources.is_empty() {
        bail!("at least one input source is required");
    }
    Ok(sources)
}

fn validate_result_fields(shape: ResultShape, fields: &[String]) -> Result<()> {
    match shape {
        ResultShape::Message if !fields.is_empty() => {
            bail!("message results cannot declare record fields");
        }
        ResultShape::Message => {}
        ResultShape::Record => {
            if fields.is_empty() {
                bail!("record results must declare at least one field");
            }
            let mut seen = std::collections::BTreeSet::new();
            for field in fields {
                validate_ident(field, "record field")?;
                if !seen.insert(field) {
                    bail!("record field {field} is declared more than once");
                }
            }
        }
    }
    Ok(())
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn command_syntax_fragment(input: &CommandInput) -> String {
    match input.cardinality {
        InputCardinality::Boolean => format!("[--{}]", input.name.replace('_', "-")),
        InputCardinality::Required => {
            format!("--{} <{}>", input.name.replace('_', "-"), input.name)
        }
        InputCardinality::Optional => {
            format!("[--{} <{}>]", input.name.replace('_', "-"), input.name)
        }
        InputCardinality::Repeated => {
            format!("[--{} <{}>]...", input.name.replace('_', "-"), input.name)
        }
    }
}

fn core_signature_fragment(input: &CommandInput) -> String {
    format!("{}: {}", input.name, input.rust_type())
}

fn core_fn_signature(spec: &ProjectSpec) -> String {
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

fn core_primary_value(spec: &ProjectSpec) -> String {
    let primary = &spec.inputs[0];
    match (primary.value_type, primary.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            format!("{}.trim().to_string()", primary.name)
        }
        _ => format!("format!(\"{{:?}}\", &{})", primary.name),
    }
}

fn core_unused_inputs(spec: &ProjectSpec) -> String {
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

fn result_fields(spec: &ProjectSpec, visibility: &str) -> String {
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

fn result_init(spec: &ProjectSpec) -> String {
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

fn result_field_names(spec: &ProjectSpec) -> Vec<String> {
    match spec.result_shape {
        ResultShape::Message => vec!["message".into()],
        ResultShape::Record => spec.record_fields.clone(),
    }
}

fn view_from_fields(spec: &ProjectSpec) -> String {
    result_field_names(spec)
        .into_iter()
        .map(|field| format!("            {field}: value.{field},"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn core_valid_assertions(spec: &ProjectSpec) -> String {
    let sample = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Standout",
    };
    match spec.result_shape {
        ResultShape::Message => {
            let expected = expected_first_field(spec, sample).1;
            format!("        assert_eq!(result.message, {});", quote(&expected))
        }
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        assert_eq!(result.summary, \"Processed Standout\");".into(),
                "count" | "length" => format!("        assert_eq!(result.{field}, \"8\");"),
                other => format!("        assert_eq!(result.{other}, \"Standout\");"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_test_call(spec: &ProjectSpec, blank_primary: bool) -> String {
    if spec.inputs.len() == 1 {
        return format!(
            "{}({})",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], blank_primary)
        );
    }
    let args = spec
        .inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            format!(
                "            {},",
                core_test_arg(input, blank_primary && index == 0)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}(\n{args}\n        )", spec.operation_name)
}

fn core_sample_result(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!(
            "        let result = {}({}).unwrap();",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        )
    } else {
        format!(
            "        let result = {}\n        .unwrap();",
            core_test_call(spec, false)
        )
    }
}

fn core_invalid_test(spec: &ProjectSpec) -> String {
    let Some(index) = spec.inputs.iter().position(|input| {
        input.value_type == InputValueType::String
            && input.cardinality == InputCardinality::Required
    }) else {
        return format!(
            r#"    #[test]
    fn configured_inputs_are_accepted_by_the_core() {{
        let result = {}
        .unwrap();

        assert_eq!(result, result.clone());
    }}"#,
            core_test_call(spec, false)
        );
    };

    if spec.inputs.len() == 1 {
        return format!(
            r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!({}({}), Err(CoreError::EmptyInput));
    }}"#,
            spec.operation_name,
            core_test_arg(&spec.inputs[0], true)
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
            Err(CoreError::EmptyInput)
        );
    }}"#,
        spec.operation_name
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

fn handler_expected_fields(spec: &ProjectSpec) -> String {
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

fn expected_first_field(spec: &ProjectSpec, input: &str) -> (String, String) {
    let field = result_field_names(spec).remove(0);
    let input = expected_primary_text(&spec.inputs[0], input);
    let value = match field.as_str() {
        "message" | "summary" => format!("Processed {input}"),
        "count" | "length" => input.chars().count().to_string(),
        _ => input,
    };
    (field, value)
}

fn expected_primary_text(input: &CommandInput, value: &str) -> String {
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

fn sample_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("text"),
    ];
    args.extend(sample_command_args(spec, primary_value, false));
    args
}

fn sample_json_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("json"),
    ];
    args.extend(sample_command_args(
        spec,
        primary_value,
        !primary_can_use_argument(spec),
    ));
    args
}

fn sample_handler_args(spec: &ProjectSpec) -> Vec<String> {
    let mut args = vec![quote(&spec.executable_name), quote(&spec.command_name)];
    args.extend(sample_command_args(spec, "hello", false));
    args
}

fn sample_command_args(
    spec: &ProjectSpec,
    primary_value: &str,
    omit_stdin_primary: bool,
) -> Vec<String> {
    let mut args = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        if omit_stdin_primary && index == 0 {
            continue;
        }
        args.extend(input.sample_args(if index == 0 { primary_value } else { "sample" }));
    }
    args
}

fn primary_can_use_argument(spec: &ProjectSpec) -> bool {
    spec.inputs[0].sources.contains(&InputSource::Argument)
}

fn primary_can_use_stdin(spec: &ProjectSpec) -> bool {
    spec.inputs[0].sources.contains(&InputSource::Stdin)
}

fn generated_json_pipeline_test(spec: &ProjectSpec) -> String {
    let primary_value = if primary_can_use_stdin(spec) {
        "Grace"
    } else {
        match spec.inputs[0].value_type {
            InputValueType::Path => "config.toml",
            _ => "Grace",
        }
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let args = if primary_can_use_stdin(spec) {
        sample_json_cli_args(spec, "Grace")
    } else {
        sample_json_cli_args(spec, primary_value)
    };
    let harness = if primary_can_use_stdin(spec) {
        "TestHarness::new().no_color().piped_stdin(\"Grace\\n\")"
    } else {
        "TestHarness::new().no_color()"
    };
    format!(
        r#"    #[test]
    #[serial]
    fn pipeline_serializes_json_for_configured_inputs() {{
        let app = build_app().unwrap();

        let result = {harness}.run(
            &app,
            cli::command(),
            {args},
        );

        result.assert_success();
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});
    }}"#,
        args = rust_array(&args, 16, 70),
        expected = quote(&expected)
    )
}

fn readme_input_policy(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .map(|input| format!("- {}.", input.policy_sentence()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

fn rust_array(items: &[String], indent: usize, max_inline_len: usize) -> String {
    let inline = format!("[{}]", items.join(", "));
    if inline.len() <= max_inline_len {
        return inline;
    }
    let spaces = " ".repeat(indent);
    let mut output = String::from("[\n");
    for item in items {
        output.push_str(&spaces);
        output.push_str(item);
        output.push_str(",\n");
    }
    output.push_str(&" ".repeat(indent.saturating_sub(4)));
    output.push(']');
    output
}

impl CommandInput {
    fn rust_type(&self) -> &'static str {
        match (self.value_type, self.cardinality) {
            (InputValueType::String, InputCardinality::Required) => "String",
            (InputValueType::String, InputCardinality::Optional) => "Option<String>",
            (InputValueType::String, InputCardinality::Repeated) => "Vec<String>",
            (InputValueType::Bool, InputCardinality::Boolean) => "bool",
            (InputValueType::Path, InputCardinality::Required) => "std::path::PathBuf",
            (InputValueType::Path, InputCardinality::Optional) => "Option<std::path::PathBuf>",
            (InputValueType::Path, InputCardinality::Repeated) => "Vec<std::path::PathBuf>",
            _ => "String",
        }
    }

    fn cli_arg(&self) -> String {
        let long = self.name.replace('_', "-");
        let mut args = match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                format!("#[arg(long = \"{long}\", action = clap::ArgAction::SetTrue)]\n        {}: bool,", self.name)
            }
            (InputValueType::Path, InputCardinality::Required) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: std::path::PathBuf,", self.name)
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Option<std::path::PathBuf>,", self.name)
            }
            (InputValueType::Path, InputCardinality::Repeated) => {
                format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Vec<std::path::PathBuf>,", self.name)
            }
            (_, InputCardinality::Required) => {
                if self.sources == [InputSource::Argument] {
                    format!("#[arg(long = \"{long}\")]\n        {}: String,", self.name)
                } else {
                    format!(
                        "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                        self.name
                    )
                }
            }
            (_, InputCardinality::Optional) => {
                format!(
                    "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                    self.name
                )
            }
            (_, InputCardinality::Repeated) => {
                format!(
                    "#[arg(long = \"{long}\")]\n        {}: Vec<String>,",
                    self.name
                )
            }
            _ => unreachable!("validated input combinations are renderable"),
        };
        if self.value_type == InputValueType::String && self.sources.contains(&InputSource::File) {
            args.push_str(&format!(
                "\n        #[arg(long = \"{long}-file\", value_name = \"PATH\")]\n        {0}_file: Option<std::path::PathBuf>,",
                self.name
            ));
        }
        args
    }

    fn sample_args(&self, primary_value: &str) -> Vec<String> {
        let long = format!("--{}", self.name.replace('_', "-"));
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => vec![quote(&long)],
            (InputValueType::String, InputCardinality::Required | InputCardinality::Optional) => {
                if self.sources.contains(&InputSource::Argument) {
                    vec![quote(&long), quote(primary_value)]
                } else {
                    Vec::new()
                }
            }
            (InputValueType::String, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra"),
            ],
            (InputValueType::Path, InputCardinality::Required | InputCardinality::Optional) => {
                vec![quote(&long), quote(primary_value)]
            }
            (InputValueType::Path, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra.toml"),
            ],
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn core_call_arg(&self) -> String {
        self.name.clone()
    }

    fn core_validation(&self) -> Option<String> {
        if self.value_type == InputValueType::String
            && self.cardinality == InputCardinality::Required
        {
            Some(format!(
                "if {}.trim().is_empty() {{\n        return Err(CoreError::EmptyInput);\n    }}",
                self.name
            ))
        } else {
            None
        }
    }

    fn resolve_statement(&self) -> String {
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                format!("let {} = matches.get_flag(\"{}\");", self.name, self.name)
            }
            (InputValueType::Path, InputCardinality::Required) => format!(
                "let {} = matches\n        .get_one::<std::path::PathBuf>(\"{}\")\n        .cloned()\n        .ok_or_else(|| anyhow::anyhow!(\"{} is required\"))?;",
                self.name, self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Optional) => format!(
                "let {} = matches.get_one::<std::path::PathBuf>(\"{}\").cloned();",
                self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Repeated) => format!(
                "let {} = matches\n        .get_many::<std::path::PathBuf>(\"{}\")\n        .map(|values| values.cloned().collect())\n        .unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Repeated) => format!(
                "let {} = matches\n        .get_many::<String>(\"{}\")\n        .map(|values| values.cloned().collect())\n        .unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Optional) => {
                self.string_resolver(false)
            }
            (InputValueType::String, InputCardinality::Required) => {
                self.string_resolver(true)
            }
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn string_resolver(&self, required: bool) -> String {
        let mut lines = vec![format!("let mut {} = None;", self.name)];
        for source in &self.sources {
            match source {
                InputSource::Argument => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = matches.get_one::<String>(\"{0}\").cloned();\n    }}",
                    self.name
                )),
                InputSource::File => lines.push(format!(
                    "if {0}.is_none() {{\n        if let Some(path) = matches.get_one::<std::path::PathBuf>(\"{0}_file\") {{\n            {0} =\n                Some(std::fs::read_to_string(path).map_err(|error| {{\n                    anyhow::anyhow!(\"failed to read {{}}: {{error}}\", path.display())\n                }})?);\n        }}\n    }}",
                    self.name
                )),
                InputSource::Stdin => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = standout_input::read_if_piped()?;\n    }}",
                    self.name
                )),
            }
        }
        if required {
            lines.push(format!(
                "let {} = match {} {{\n        Some(value) => value,\n        None => return Err(anyhow::anyhow!(\"{} is required\")),\n    }};",
                self.name, self.name, self.name
            ));
        }
        lines.join("\n    ")
    }
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    Environment::new()
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
        resolve_inputs => spec.inputs.iter().map(CommandInput::resolve_statement).collect::<Vec<_>>().join("\n    "),
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
        pipeline_human_args => rust_array(&sample_cli_args(spec, "Ada"), 16, 70),
        pipeline_json_test => generated_json_pipeline_test(spec),
        handler_args => rust_array(&sample_handler_args(spec), 16, 60),
        template_body => template_body(spec),
        human_expected => quote(&expected_first_field(spec, "Ada").1),
        readme_input_policy => readme_input_policy(spec),
        command_syntax => spec.inputs.iter().map(command_syntax_fragment).collect::<Vec<_>>().join(" "),
        standout_version => spec.standout_version,
        local_patch_root => spec.local_patch_root.as_ref().map(|path| path.display().to_string()),
    }
}

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
    ("crates/{{ executable_name }}/README.md", "readme"),
    ("crates/{{ executable_name }}/src/main.rs", "main"),
    ("crates/{{ executable_name }}/src/cli.rs", "cli"),
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

const TEMPLATE_CATALOG: &[(&str, &str)] = &[
    (
        "workspace",
        r#"[workspace]
resolver = "2"
members = [
    "crates/{{ lib_crate }}",
    "crates/{{ executable_name }}",
]

{%- if local_patch_root %}
[patch.crates-io]
standout = { path = "{{ local_patch_root }}/crates/standout" }
standout-test = { path = "{{ local_patch_root }}/crates/standout-test" }
standout-dispatch = { path = "{{ local_patch_root }}/crates/standout-dispatch" }
standout-input = { path = "{{ local_patch_root }}/crates/standout-input" }
{%- endif %}
"#,
    ),
    (
        "core_manifest",
        r#"[package]
name = "{{ lib_package }}"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"
"#,
    ),
    (
        "core_lib",
        r#"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct {{ view_name }} {
{{ core_result_fields }}
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("{{ input_name }} cannot be empty")]
    EmptyInput,
}

/// Runs the CLI-free core operation for the generated command.
///
/// The caller supplies explicit values. This crate deliberately has no Clap,
/// Standout, template, terminal, environment, or CLI-view dependencies.
pub fn {{ operation_name }}{{ core_fn_signature }} -> Result<{{ view_name }}, CoreError> {
{%- if core_validations %}
    {{ core_validations }}
{%- endif %}
{%- if core_unused_inputs %}
    {{ core_unused_inputs }}
{%- endif %}
    let primary = {{ core_primary_value }};
    Ok({{ view_name }} {
{{ result_init }}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_returns_a_typed_result() {
{{ core_sample_result }}

{{ core_valid_assertions }}
    }

{{ core_invalid_test }}
}
"#,
    ),
    (
        "cli_manifest",
        r#"[package]
name = "{{ executable_name }}"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
standout = "{{ standout_version }}"
standout-dispatch = "{{ standout_version }}"
standout-input = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod handlers;

use anyhow::Result;
use standout::{embed_styles, embed_templates};

fn main() -> Result<()> {
    let app = build_app()?;
    app.run(cli::command(), std::env::args());
    Ok(())
}

fn build_app() -> Result<standout::cli::App> {
    Ok(standout::cli::App::builder()
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("{{ project_name }}")
        .command_with("{{ command_name }}", handlers::{{ command_ident }}__handler, |config| {
            config.template("{{ command_name }}.jinja")
        })?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serial_test::serial;
    use standout_test::TestHarness;

    #[test]
    #[serial]
    fn pipeline_renders_human_output_from_argument() {
        let app = build_app().unwrap();

        let result = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            {{ pipeline_human_args }},
        );

        result.assert_success();
        result.assert_stdout_contains({{ human_expected }});
    }

{{ pipeline_json_test }}
}
"#,
    ),
    (
        "cli",
        r#"use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "{{ executable_name }}", about = "{{ command_description }}")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// {{ command_description }}
    {{ command_variant }} {
        {{ cli_args }}
    },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
"#,
    ),
    (
        "handlers",
        r#"#![allow(non_snake_case)]

use clap::ArgMatches;
use {{ lib_crate }} as core;
use serde::Serialize;
use standout::cli::{CommandContext, Output};
use standout::handler;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct {{ view_name }} {
{{ cli_view_fields }}
}

impl From<core::{{ view_name }}> for {{ view_name }} {
    fn from(value: core::{{ view_name }}) -> Self {
        Self {
{{ view_from_fields }}
        }
    }
}

/// Adapts typed shell input into the CLI-free core operation.
///
/// The handler owns CLI-only source resolution, including file-content reads,
/// then returns data for Standout to render or serialize.
#[handler]
pub(crate) fn {{ command_ident }}(
    #[matches] matches: &ArgMatches,
    #[ctx] _ctx: &CommandContext,
) -> Result<Output<{{ view_name }}>, anyhow::Error> {
    {{ resolve_inputs }}
    let result = core::{{ operation_name }}({{ core_call_args }})?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_handler_maps_input_to_core_and_view() {
        let matches = crate::cli::command()
            .try_get_matches_from({{ handler_args }})
            .unwrap();
        let (_, matches) = matches.subcommand().unwrap();
        let ctx = CommandContext::default();

        let Output::Render(view) = {{ command_ident }}(matches, &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(
            view,
            {{ view_name }} {
{{ handler_expected_fields }}
            }
        );
    }
}
"#,
    ),
    ("template", r#"{{ template_body }}"#),
    (
        "style",
        r#".title {
  font-weight: bold;
  color: cyan;
}
"#,
    ),
    (
        "readme",
        r#"# {{ executable_name }}

This generated project is a Standout architecture starter, not a finished
application. The reusable `{{ lib_crate }}` crate owns the CLI-free operation
and validation. This binary crate owns Clap declarations, Standout wiring,
input policy, handlers, serializable view types, templates, styles, and process
execution.

Run the generated command:

```sh
cargo run -p {{ executable_name }} -- {{ command_name }} --{{ input_name }} VALUE
cargo run -p {{ executable_name }} -- {{ command_name }} --{{ input_name }} VALUE --output json
```

Command syntax:

```text
{{ executable_name }} {{ command_name }} {{ command_syntax }}
```

Input policy:

{{ readme_input_policy }}

Blank required string values are rejected by the core operation.

The generated `{{ result_shape }}` result is intentionally small. The handler
maps resolved shell input into `{{ lib_crate }}::{{ operation_name }}` and maps
the core result into the CLI-owned `{{ view_name }}`.

Verify the project with:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
```
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn required_string(name: impl Into<String>) -> CommandInput {
        CommandInput {
            name: name.into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
        }
    }

    fn sample_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "hello-tool".into(),
            executable_name: "hello-tool".into(),
            command_name: "greet".into(),
            command_description: "Greet one value".into(),
            inputs: vec![required_string("name")],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();
        spec.destination = root.join("hello-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("standout crate lives under crates/standout in the repository")
            .to_path_buf()
    }

    #[test]
    fn answers_validate_before_rendering() {
        let mut input = io::Cursor::new("1bad\n");
        let mut output = Vec::new();

        let error = prompt_answers(&mut input, &mut output).unwrap_err();

        assert!(error.to_string().contains("project name must start"));
    }

    #[test]
    fn questionnaire_collects_supported_input_matrix() {
        let answers = [
            "inspect-tool",
            "",
            "inspect",
            "Inspect document input",
            "4",
            "document",
            "string",
            "required",
            "argument,file,stdin",
            "verbose",
            "bool",
            "boolean",
            "argument",
            "tag",
            "string",
            "repeated",
            "argument",
            "config",
            "path",
            "optional",
            "argument",
            "record",
            "summary,count,echo",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();
        let spec = ProjectSpec::from_answers(answers).unwrap();

        assert_eq!(spec.executable_name, "inspect-tool");
        assert_eq!(spec.inputs.len(), 4);
        assert_eq!(
            spec.inputs[0].sources,
            vec![InputSource::Argument, InputSource::File, InputSource::Stdin,]
        );
        assert_eq!(spec.inputs[1].value_type, InputValueType::Bool);
        assert_eq!(spec.inputs[2].cardinality, InputCardinality::Repeated);
        assert_eq!(spec.inputs[3].value_type, InputValueType::Path);
    }

    #[test]
    fn input_source_aliases_cannot_declare_the_same_source_twice() {
        let duplicate = parse_input_sources("argument,arg").unwrap_err();

        assert!(duplicate.to_string().contains("declared more than once"));
    }

    #[test]
    fn project_spec_is_private_validated_model() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
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
    fn render_builds_files_in_memory_before_publish() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());

        let generated = GeneratedFiles::render(&spec).unwrap();

        assert!(generated.files.contains_key(Path::new("Cargo.toml")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello_toollib/src/lib.rs")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello-tool/README.md")));
        assert!(!spec.destination.exists());
    }

    #[test]
    fn result_fields_and_generated_identifiers_are_validated() {
        let duplicate = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "summary".into()],
        })
        .unwrap_err();
        let keyword = ProjectSpec::from_answers(WizardAnswers {
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
    fn render_omits_local_patch_paths_by_default() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(!manifest.contains("[patch.crates-io]"));
        assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
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

        let spec = ProjectSpec::from_answers(WizardAnswers {
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
        let error = ProjectSpec::from_answers(WizardAnswers {
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

        let error = ProjectSpec::from_answers(WizardAnswers {
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

    #[test]
    fn path_input_rendering_does_not_emit_string_validation() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let core = generated
            .files
            .get(Path::new("crates/demolib/src/lib.rs"))
            .unwrap();

        assert!(core.contains("config: std::path::PathBuf"));
        assert!(!core.contains("config.trim()"));
    }

    #[test]
    fn publish_refuses_non_empty_destination_without_partial_staging() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::create_dir_all(&spec.destination).unwrap();
        fs::write(spec.destination.join("keep.txt"), "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not empty"));
        assert_eq!(
            fs::read_to_string(spec.destination.join("keep.txt")).unwrap(),
            "existing"
        );
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn publish_refuses_file_destination_with_clear_error() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::write(&spec.destination, "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not a directory"));
        assert_eq!(fs::read_to_string(&spec.destination).unwrap(), "existing");
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn declined_confirmation_is_a_non_mutating_cancel() {
        let mut input = io::Cursor::new("no\n");
        let mut output = Vec::new();

        assert!(!confirm(&mut input, &mut output).unwrap());
    }

    fn rich_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "inspect-tool".into(),
            executable_name: "inspect-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect document input".into(),
            inputs: vec![
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
                    name: "tag".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Repeated,
                    sources: vec![InputSource::Argument],
                },
                CommandInput {
                    name: "config".into(),
                    value_type: InputValueType::Path,
                    cardinality: InputCardinality::Optional,
                    sources: vec![InputSource::Argument],
                },
            ],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into(), "echo".into()],
        })
        .unwrap();
        spec.destination = root.join("inspect-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn path_first_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "config-tool".into(),
            executable_name: "config-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect a config path".into(),
            inputs: vec![
                CommandInput {
                    name: "config".into(),
                    value_type: InputValueType::Path,
                    cardinality: InputCardinality::Required,
                    sources: vec![InputSource::Argument],
                },
                CommandInput {
                    name: "note".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Optional,
                    sources: vec![InputSource::Argument],
                },
            ],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join("config-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    #[test]
    fn generated_project_matrix_formats_checks_tests_and_runs() {
        let dir = TempDir::new().unwrap();
        let mut message = sample_spec(dir.path());
        message.result_shape = ResultShape::Message;
        message.record_fields.clear();
        let rich = rich_spec(dir.path());
        let path_first = path_first_spec(dir.path());

        for spec in [&message, &rich, &path_first] {
            publish_project(spec).unwrap();
            run_cargo(&spec.destination, ["fmt", "--check"]);
            run_cargo(&spec.destination, ["check", "--workspace"]);
            run_cargo(&spec.destination, ["test", "--workspace"]);
        }

        let message_human = run_binary(
            &message.destination,
            [
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "Ada",
            ],
        );
        let stdout = String::from_utf8(message_human.stdout).unwrap();
        assert!(stdout.contains("Processed Ada"));

        let human = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--verbose",
                "--tag",
                "alpha",
                "--tag",
                "beta",
                "--config",
                "settings.toml",
            ],
        );
        let stdout = String::from_utf8(human.stdout).unwrap();
        assert!(stdout.contains("Ada"));
        assert!(stdout.contains("Summary:"));
        assert!(stdout.contains("Echo: Ada"));

        let json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--output",
                "json",
            ],
        );
        let value = json_value(&json);
        assert_eq!(value["summary"], "Processed Ada");
        assert_eq!(value["count"], "3");
        assert_eq!(value["echo"], "Ada");

        let input_file = rich.destination.join("input.txt");
        fs::write(&input_file, "File Ada").unwrap();
        let file_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&file_json);
        assert_eq!(value["summary"], "Processed File Ada");
        assert_eq!(value["count"], "8");

        let stdin_json = run_binary_with_stdin(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--output",
                "json",
            ],
            "Pipe Ada\n",
        );
        let value = json_value(&stdin_json);
        assert_eq!(value["summary"], "Processed Pipe Ada");
        assert_eq!(value["count"], "8");

        let precedence_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Arg Ada",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&precedence_json);
        assert_eq!(value["summary"], "Processed Arg Ada");
        assert_eq!(value["count"], "7");

        let invalid = Command::new("cargo")
            .current_dir(&rich.destination)
            .args([
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "   ",
            ])
            .output()
            .unwrap();
        assert!(!invalid.status.success());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("document cannot be empty"));

        let missing = Command::new("cargo")
            .current_dir(&rich.destination)
            .args(["run", "-q", "-p", "inspect-tool", "--", "inspect"])
            .output()
            .unwrap();
        assert!(!missing.status.success());
        assert!(String::from_utf8_lossy(&missing.stderr).contains("document is required"));
    }

    fn run_cargo<const N: usize>(cwd: &Path, args: [&str; N]) {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_binary<const N: usize>(cwd: &Path, args: [&str; N]) -> std::process::Output {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "binary run failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn run_binary_with_stdin<const N: usize>(
        cwd: &Path,
        args: [&str; N],
        stdin: &str,
    ) -> std::process::Output {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "binary run with stdin failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn json_value(output: &std::process::Output) -> serde_json::Value {
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
