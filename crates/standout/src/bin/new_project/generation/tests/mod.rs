use super::*;
use crate::new_project::test_support::*;
use crate::new_project::{
    CommandInput, InputCardinality, InputSource, InputValueType, ProjectSpec, ResultShape,
};
use std::path::Path;
use tempfile::TempDir;

mod manifests;
mod project_matrix;

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
fn render_omits_local_patch_paths_by_default() {
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

    let generated = GeneratedFiles::render(&spec).unwrap();
    let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

    assert!(!manifest.contains("[patch.crates-io]"));
    assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
}

#[test]
fn local_patch_paths_are_escaped_as_toml_basic_string_content() {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: "Inspect one value".into(),
        inputs: vec![required_string("document")],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();
    spec.local_patch_root = Some(PathBuf::from(r#"C:\Users\Ada "Q"\standout"#));

    let generated = GeneratedFiles::render(&spec).unwrap();
    let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

    assert!(manifest
        .contains(r#"standout = { path = "C:\\Users\\Ada \"Q\"\\standout/crates/standout" }"#));
    assert!(!manifest.contains(r#"path = "C:\Users"#));
}

#[test]
fn generated_sources_commands_and_validation_preserve_the_validated_model() {
    let file_only = CommandInput {
        name: "document".into(),
        value_type: InputValueType::String,
        cardinality: InputCardinality::Required,
        sources: vec![InputSource::File],
    };
    assert_eq!(
        command_syntax_fragment(&file_only),
        "--document-file <PATH>"
    );
    assert_eq!(
        command_syntax_fragment(&CommandInput {
            name: "tag".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Repeated,
            sources: vec![InputSource::Argument],
        }),
        "[--tag <tag>]..."
    );

    let spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "send_email".into(),
        command_description: "Send one message".into(),
        inputs: vec![
            file_only,
            CommandInput {
                name: "subject".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Stdin],
            },
        ],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();

    let generated = GeneratedFiles::render(&spec).unwrap();
    let cli = generated
        .files
        .get(Path::new("crates/demo/src/cli.rs"))
        .unwrap();
    let handlers = generated
        .files
        .get(Path::new("crates/demo/src/handlers.rs"))
        .unwrap();
    let core = generated
        .files
        .get(Path::new("crates/demolib/src/lib.rs"))
        .unwrap();
    let readme = generated
        .files
        .get(Path::new("crates/demo/README.md"))
        .unwrap();

    assert!(cli.contains("#[command(name = \"send_email\")]"));
    assert!(cli.contains("long = \"document-file\""));
    assert!(!cli.contains("long = \"document\""));
    assert!(!cli.contains("long = \"subject\""));
    assert!(handlers.contains("    fn typed_handler_maps_input_to_core_and_view()"));
    assert!(core.contains("CoreError::EmptyInput { field: \"document\" }"));
    assert!(core.contains("CoreError::EmptyInput { field: \"subject\" }"));
    assert!(readme.contains("demo send_email --document-file <PATH> <piped stdin>"));
}

#[test]
fn path_input_rendering_does_not_emit_string_validation() {
    let spec = ProjectSpec::from_answers(TestProjectAnswers {
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

fn generated_source(generated: &GeneratedFiles, path: &str) -> String {
    generated
        .files
        .get(Path::new(path))
        .unwrap_or_else(|| panic!("{path} is generated"))
        .clone()
}

// The negative assertions alone would pass on output that dropped the concern entirely.
#[test]
fn generated_project_uses_the_blessed_idioms() {
    let dir = TempDir::new().unwrap();
    let generated = GeneratedFiles::render(&rich_questionnaire_spec(dir.path())).unwrap();
    let cli = generated_source(&generated, "crates/inspect-tool/src/cli.rs");
    let main = generated_source(&generated, "crates/inspect-tool/src/main.rs");
    let handlers = generated_source(&generated, "crates/inspect-tool/src/handlers.rs");

    assert!(cli.contains("#[derive(Subcommand, Dispatch)]"));
    assert!(cli.contains("#[dispatch(handlers = crate::handlers)]"));
    assert!(cli.contains("#[dispatch(pure, default, inputs = crate::handlers::inspect_inputs)]"));
    assert!(main.contains(".commands(cli::Commands::dispatch_config())?"));
    assert!(main.contains("build_app(clapfig::SearchPath::Platform)?"));
    assert!(main.contains("fn build_app(user_scope: clapfig::SearchPath)"));
    assert!(main.contains(".config(config::builder(user_scope))"));
    assert!(main.contains("build_app(clapfig::SearchPath::Path(user_dir.path().to_path_buf()))"));
    assert!(main.contains(".term_settings(|config: &config::Config| &config.term)"));
    assert!(main.contains("fn term_output_in_the_config_file_selects_json()"));
    assert!(main.contains(".fixture(\"inspect-tool.toml\", CONFIG_FILE)"));
    assert!(main.contains("[\"inspect-tool\", \"config\", \"get\", \"term.output\"]"));

    let config = generated_source(&generated, "crates/inspect-tool/src/config.rs");
    assert!(config.contains("#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]"));
    assert!(config.contains("pub(crate) term: TermSettings,"));
    assert!(config.contains(".app_name(\"inspect-tool\")"));
    assert!(config.contains("fn builder(user_scope: clapfig::SearchPath)"));
    assert!(config.contains(".add_search_path(user_scope.clone())"));
    assert!(config.contains(".add_search_path(clapfig::SearchPath::Cwd)"));
    assert!(config.contains(".persist_scope(\"global\", user_scope)"));

    assert!(handlers.contains("#[handler]"));
    assert!(handlers.contains("#[flag] verbose: bool"));
    assert!(handlers.contains("#[arg] config: Option<std::path::PathBuf>"));

    assert!(cli.contains("#[derive(Parser)]"));
    assert!(cli.contains("Cli::command()"));

    assert!(main.contains(".templates(embed_templates!(\"src/templates\"))"));

    assert!(main.contains(".styles(embed_styles!(\"src/styles\"))"));
    assert!(main.contains(".default_theme(\"inspect-tool\")"));

    assert!(cli.contains("inputs = crate::handlers::inspect_inputs"));
    assert!(handlers
        .contains("pub(crate) fn inspect_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H>"));
    assert!(handlers.contains("InputChain::<String>::new()"));
    assert!(handlers.contains(".try_source(ArgSource::new(\"document\"))"));
    assert!(handlers.contains(".try_source(StdinSource::new())"));
    assert!(handlers.contains("ctx.input(\"document\")?"));

    assert!(main.contains("app.run(cli::command(), std::env::args());"));
    assert!(main.contains(".version(env!(\"CARGO_PKG_VERSION\"))"));

    assert!(!main.contains("command_with"));
    assert!(!main.contains("FnHandler"));
    assert!(!main.contains("AppBuilder::default"));
    assert!(!main.contains("template_name"));
    assert!(!main.contains("help_handling"));
    assert!(!main.contains("Theme::"));
    assert!(!handlers.contains("#[matches]"));
    assert!(!handlers.contains("matches.get_one::<String>"));
    assert!(!cli.contains("#[derive(Subcommand)]"));
}

#[test]
fn file_source_reports_file_provenance_and_names_an_unreadable_path() {
    let dir = TempDir::new().unwrap();
    let generated = GeneratedFiles::render(&file_only_spec(dir.path())).unwrap();
    let handlers = generated_source(&generated, "crates/file-tool/src/handlers.rs");

    assert!(handlers.contains("    fn name(&self) -> &'static str {\n        \"file\"\n    }"));
    assert!(
        handlers.contains("standout::input::InputError::file(path.display().to_string(), error)")
    );
    assert!(!handlers.contains("InputError::parse(self.arg"));
    assert!(handlers.contains("source: standout_input::InputSourceKind::File,"));
}

#[test]
fn argument_only_input_stays_a_typed_handler_parameter() {
    let dir = TempDir::new().unwrap();
    let spec = single_input_spec(
        dir.path(),
        "bool-tool",
        CommandInput {
            name: "verbose".into(),
            value_type: InputValueType::Bool,
            cardinality: InputCardinality::Boolean,
            sources: vec![InputSource::Argument],
        },
    );
    let generated = GeneratedFiles::render(&spec).unwrap();
    let cli = generated_source(&generated, "crates/bool-tool/src/cli.rs");
    let handlers = generated_source(&generated, "crates/bool-tool/src/handlers.rs");

    assert!(handlers.contains("pub(crate) fn inspect(#[flag] verbose: bool)"));
    assert!(!handlers.contains("InputChain"));
    assert!(!handlers.contains("CommandContext"));
    assert!(!cli.contains("inputs = "));
}

#[test]
fn an_underscored_input_name_spells_out_the_argument_id() {
    let dir = TempDir::new().unwrap();
    let spec = single_input_spec(
        dir.path(),
        "note-tool",
        CommandInput {
            name: "note_text".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument],
        },
    );
    let generated = GeneratedFiles::render(&spec).unwrap();
    let handlers = generated_source(&generated, "crates/note-tool/src/handlers.rs");

    assert!(handlers.contains("#[arg(name = \"note_text\")] note_text: String"));
}
