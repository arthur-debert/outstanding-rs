//! Where clapfig meets the run pipeline.
//!
//! An application hands `App::builder().config(...)` a `clapfig::TypedBuilder<C>`
//! for its own settings struct. `App` is not generic, so the builder is held behind
//! [`ConfigSeam`], a trait object that knows how to clone it, apply the override
//! flag's `key=value` pairs, build a resolver, resolve at a directory into a
//! `Box<dyn Any>`, and run a `ConfigAction` for the injected `config` command.
//! [`ResolvedConfig`] carries the erased struct plus an installer that puts it back
//! into `ctx.extensions` under its real type, where `ctx.config::<C>()` finds it.
//!
//! Resolution happens once per run, after clap parses and only when the parsed
//! path names a registered handler: `--help`, usage errors, the help word and the
//! `config` tree never read the app's configuration, so a broken file cannot take
//! them down, and the override pairs exist before they are needed. The resolver is
//! built per run rather than at `App::build()` because clapfig snapshots the process
//! environment inside `build_resolver`, and a test sets its environment after the
//! app is built.
//!
//! `[term]` is opt-in through `term_settings(accessor)`: clapfig has no reserved
//! section and this module does not look for a field by name in someone else's
//! struct. [`TermSettings`] holds the keys the framework reads for itself;
//! `output` fills `extract_output_mode`'s fallback arm when `--output` was not typed.
//!
//! The `config` command is clapfig's `ConfigCommand` tree, installed beside the help
//! word. clapfig executes the action; this module only projects the `ConfigResult`
//! into `Output` so it rides the normal render path, which is what makes
//! `--output json` apply and keeps integers as numbers. Template and schema output
//! are artifacts. Its file option is spelled `--file` because `--output` is the
//! global mode flag.
//!
//! A `ClapfigError` keeps clapfig's words (`render_plain`) and becomes
//! `RunErrorKind::Config`; the diagnostic range is filled only when clapfig has a
//! real position.

use std::any::Any;
use std::path::Path;

use clap::{Arg, Command};
use clapfig::value::Value;
use clapfig::{
    ClapfigError, ConfigAction, ConfigCommand, ConfigResult, DocumentRoot, TypedBuilder,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cli::builder::TemplateRef;
use crate::cli::handler::{Artifact, Diagnostic, Extensions, Output, RunError, RunErrorKind};
use crate::setup::SetupError;
use crate::OutputMode;

pub(crate) const CONFIG_COMMAND: &str = "config";

pub(crate) fn config_command() -> ConfigCommand {
    ConfigCommand::new().output_long("file")
}

pub(crate) fn config_command_tree() -> Command {
    config_command().as_command(CONFIG_COMMAND)
}

pub(crate) fn claims_config_command(cmd: &Command) -> bool {
    cmd.get_name() == CONFIG_COMMAND || cmd.get_all_aliases().any(|alias| alias == CONFIG_COMMAND)
}

pub(crate) fn claims_config_path(path: &str) -> bool {
    path == CONFIG_COMMAND || path.starts_with("config.")
}

pub(crate) fn config_command_collision(claim: &str) -> SetupError {
    SetupError::Config(format!(
        "config — {claim}, and standout installs a `config` command of its own for the \
         configuration given to .config(...). Rename the application's command, or call \
         .no_config_command() to keep the name"
    ))
}

pub(crate) fn config_option_collision(claim: &str) -> SetupError {
    SetupError::Config(format!(
        "config — {claim}, and the `config` command standout installs for the configuration \
         given to .config(...) takes it on its own tree, where clap would propagate the global. \
         Rename the flag, or call .no_config_command() to keep it"
    ))
}

pub(crate) fn config_tree_takes_long(flag: &str) -> bool {
    config_tree_args(&config_command_tree()).any(|arg| arg_longs(arg).any(|long| long == flag))
}

pub(crate) fn config_tree_claim(global: &Arg) -> Option<String> {
    let tree = config_command_tree();
    let claim = config_tree_args(&tree).find_map(|taken| {
        if let Some(long) = arg_longs(global).find(|long| arg_longs(taken).any(|t| t == *long)) {
            return Some(format!("`--{long}`"));
        }
        if let Some(short) = arg_shorts(global).find(|short| arg_shorts(taken).any(|t| t == *short))
        {
            return Some(format!("`-{short}`"));
        }
        (global.get_id() == taken.get_id()).then(|| format!("the id `{}`", global.get_id()))
    });
    claim
}

fn config_tree_args(cmd: &Command) -> Box<dyn Iterator<Item = &Arg> + '_> {
    Box::new(
        cmd.get_arguments()
            .chain(cmd.get_subcommands().flat_map(config_tree_args)),
    )
}

fn arg_longs(arg: &Arg) -> impl Iterator<Item = &str> {
    arg.get_long()
        .into_iter()
        .chain(arg.get_all_aliases().unwrap_or_default())
}

fn arg_shorts(arg: &Arg) -> impl Iterator<Item = char> + '_ {
    arg.get_short()
        .into_iter()
        .chain(arg.get_all_short_aliases().unwrap_or_default())
}

const LINE_TEMPLATE: &str = "{{ line }}";

pub(crate) fn config_result_output(
    result: ConfigResult,
    output_mode: OutputMode,
) -> (Output<serde_json::Value>, TemplateRef) {
    let (line, structured) = match result {
        ConfigResult::Template(text) | ConfigResult::Schema(text) => {
            return (
                Output::Artifact(Artifact::new(text.into_bytes()).allow_stdout()),
                TemplateRef::Inline(LINE_TEMPLATE.to_string()),
            )
        }
        ConfigResult::Listing { entries, rendered } => {
            let object = entries
                .into_iter()
                .map(|(key, value)| (key, typed_value(&value)))
                .collect::<serde_json::Map<_, _>>();
            (rendered, serde_json::Value::Object(object))
        }
        ConfigResult::KeyValue {
            key,
            value,
            rendered,
            ..
        } => (rendered, json!({ key: typed_value(&value) })),
        ConfigResult::ValueSet {
            key,
            value,
            rendered,
        } => (rendered, json!({ "key": key, "value": value })),
        ConfigResult::ValueUnset { key } => (format!("{key} unset"), json!({ "key": key })),
        ConfigResult::TemplateWritten { path } => (
            format!("template written to {}", path.display()),
            json!({ "path": path.display().to_string() }),
        ),
        ConfigResult::SchemaWritten { path } => (
            format!("schema written to {}", path.display()),
            json!({ "path": path.display().to_string() }),
        ),
    };
    let data = if output_mode.is_structured() {
        structured
    } else {
        json!({ "line": escape_style_tags(&line) })
    };
    (
        Output::Render(data),
        TemplateRef::Inline(LINE_TEMPLATE.to_string()),
    )
}

fn typed_value(value: &Value) -> serde_json::Value {
    match value {
        Value::String(text) => serde_json::Value::String(text.clone()),
        Value::Integer(int) => serde_json::Value::from(*int),
        Value::Float(float) => serde_json::Number::from_f64(*float).map_or_else(
            || serde_json::Value::String(value.to_string()),
            serde_json::Value::Number,
        ),
        Value::Boolean(flag) => serde_json::Value::Bool(*flag),
        Value::Datetime(_) => serde_json::Value::String(value.to_string()),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(typed_value).collect()),
        Value::Map(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), typed_value(value)))
                .collect(),
        ),
    }
}

fn escape_style_tags(text: &str) -> String {
    text.replace('[', "\\[").replace(']', "\\]")
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, clapfig::Schema)]
pub struct TermSettings {
    pub output: Option<TermOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clapfig::Schema)]
#[serde(rename_all = "kebab-case")]
pub enum TermOutput {
    Auto,
    Term,
    Text,
    TermDebug,
    Json,
    Yaml,
    Csv,
    Ndjson,
}

impl From<TermOutput> for OutputMode {
    fn from(output: TermOutput) -> Self {
        match output {
            TermOutput::Auto => OutputMode::Auto,
            TermOutput::Term => OutputMode::Term,
            TermOutput::Text => OutputMode::Text,
            TermOutput::TermDebug => OutputMode::TermDebug,
            TermOutput::Json => OutputMode::Json,
            TermOutput::Yaml => OutputMode::Yaml,
            TermOutput::Csv => OutputMode::Csv,
            TermOutput::Ndjson => OutputMode::Ndjson,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no `{type_name}` configuration was resolved for this run; register one with `App::builder().config(...)`")]
pub struct MissingConfig {
    pub type_name: &'static str,
}

pub(crate) type TermAccessor<C> = Box<dyn Fn(&C) -> &TermSettings>;

pub(crate) trait ConfigSeam {
    fn attach_term_accessor(&mut self, accessor: Box<dyn Any>) -> Result<(), SetupError>;

    fn resolve_at(
        &self,
        overrides: &[(String, String)],
        dir: &Path,
    ) -> Result<ResolvedConfig, ClapfigError>;

    fn handle(
        &self,
        action: &ConfigAction,
        overrides: &[(String, String)],
    ) -> Result<ConfigResult, ClapfigError>;
}

pub(crate) struct TypedSeam<C: DocumentRoot> {
    builder: TypedBuilder<C>,
    term: Option<TermAccessor<C>>,
}

impl<C: DocumentRoot> TypedSeam<C> {
    pub(crate) fn new(builder: TypedBuilder<C>) -> Self {
        Self {
            builder,
            term: None,
        }
    }

    fn builder_with(&self, overrides: &[(String, String)]) -> TypedBuilder<C> {
        let mut builder = self.builder.clone();
        for (key, raw) in overrides {
            builder = builder.cli_override_str(key, raw);
        }
        builder
    }
}

impl<C: DocumentRoot + DeserializeOwned + 'static> ConfigSeam for TypedSeam<C> {
    fn attach_term_accessor(&mut self, accessor: Box<dyn Any>) -> Result<(), SetupError> {
        match accessor.downcast::<TermAccessor<C>>() {
            Ok(accessor) => {
                self.term = Some(*accessor);
                Ok(())
            }
            Err(_) => Err(SetupError::Config(format!(
                "term_settings accessor does not read `{}`, the type given to .config(...)",
                std::any::type_name::<C>()
            ))),
        }
    }

    fn resolve_at(
        &self,
        overrides: &[(String, String)],
        dir: &Path,
    ) -> Result<ResolvedConfig, ClapfigError> {
        let config = self
            .builder_with(overrides)
            .build_resolver()?
            .resolve_at(dir)?;
        let term = self.term.as_ref().map(|accessor| accessor(&config).clone());
        Ok(ResolvedConfig::new(config, term))
    }

    fn handle(
        &self,
        action: &ConfigAction,
        overrides: &[(String, String)],
    ) -> Result<ConfigResult, ClapfigError> {
        // clapfig validates a write candidate through every layer, so an
        // override would vouch for a file the next run cannot load.
        let builder = match action {
            ConfigAction::List { .. } | ConfigAction::Get { .. } => self.builder_with(overrides),
            _ => self.builder.clone(),
        };
        builder.handle(action)
    }
}

pub(crate) struct ResolvedApp<C>(pub(crate) C);

pub(crate) struct ResolvedConfig {
    value: Box<dyn Any>,
    install: fn(Box<dyn Any>, &mut Extensions),
    pub(crate) term: Option<TermSettings>,
}

impl ResolvedConfig {
    fn new<C: 'static>(value: C, term: Option<TermSettings>) -> Self {
        Self {
            value: Box::new(value),
            install: |value, extensions| {
                let value = value
                    .downcast::<C>()
                    .expect("a ResolvedConfig installs the type it was built from");
                extensions.insert(ResolvedApp(*value));
            },
            term,
        }
    }

    pub(crate) fn install(self, extensions: &mut Extensions) {
        (self.install)(self.value, extensions);
    }
}

pub(crate) fn config_run_error(error: &ClapfigError) -> RunError {
    let prose = clapfig::render::render_plain(error);
    let run_error = RunError::new(prose, RunErrorKind::Config);
    match config_error_position(error) {
        Some((file, line, column)) => {
            let diagnostic = run_error.diagnostic();
            let diagnostic = Diagnostic::error(diagnostic.summary)
                .detail(diagnostic.detail)
                .range(file, line, column);
            run_error.with_diagnostic(diagnostic)
        }
        None => run_error,
    }
}

fn config_error_position(error: &ClapfigError) -> Option<(String, u64, u64)> {
    match error {
        ClapfigError::UnknownKeys(infos) => {
            let info = infos.first()?;
            if info.line == 0 {
                return None;
            }
            let column = info
                .span
                .zip(info.source.as_deref())
                .map_or(1, |(span, source)| line_and_column(source, span.start).1);
            Some((info.path.display().to_string(), info.line as u64, column))
        }
        ClapfigError::ParseError {
            path,
            source,
            source_text,
        } => {
            let span = source.parse_span()?;
            let (line, column) = line_and_column(source_text.as_deref()?, span.start);
            Some((path.display().to_string(), line, column))
        }
        _ => None,
    }
}

fn line_and_column(source: &str, offset: usize) -> (u64, u64) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(crate) fn parse_override_pair(raw: &str) -> Result<(String, String), String> {
    match raw.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_string(), value.to_string())),
        _ => Err(format!("expected KEY=VALUE, got `{raw}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::builder::OUTPUT_MODE_FLAG_VALUES;

    #[test]
    fn term_output_spellings_are_the_output_flag_values() {
        let variants = [
            TermOutput::Auto,
            TermOutput::Term,
            TermOutput::Text,
            TermOutput::TermDebug,
            TermOutput::Json,
            TermOutput::Yaml,
            TermOutput::Csv,
            TermOutput::Ndjson,
        ];
        let spelled: Vec<String> = variants
            .iter()
            .map(|variant| {
                serde_json::to_value(variant)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(spelled, OUTPUT_MODE_FLAG_VALUES);
        for (variant, spelling) in variants.iter().zip(OUTPUT_MODE_FLAG_VALUES) {
            let parsed: TermOutput = serde_json::from_value(spelling.into()).unwrap();
            assert_eq!(parsed, *variant);
        }
    }

    #[test]
    fn an_unknown_key_without_a_span_starts_its_line() {
        let info = clapfig::error::UnknownKeyInfo {
            key: "bogus".into(),
            path: "app.toml".into(),
            line: 3,
            source: None,
            env_var: None,
            span: None,
            url_key: None,
            override_key: None,
            input_type: None,
        };
        let position = config_error_position(&ClapfigError::UnknownKeys(vec![info]));
        assert_eq!(position, Some(("app.toml".to_string(), 3, 1)));
    }

    #[test]
    fn typed_values_project_datetimes_and_non_finite_floats_as_strings() {
        let stamp: clapfig::value::Datetime = "1979-05-27T07:32:00Z".parse().unwrap();
        let mut inner = clapfig::value::Map::new();
        inner.insert("when".into(), Value::Datetime(stamp));
        inner.insert("ratio".into(), Value::Float(f64::NAN));
        let value = Value::Array(vec![
            Value::Map(inner),
            Value::Float(f64::INFINITY),
            Value::Float(-f64::INFINITY),
            Value::Float(1.5),
            Value::Integer(7),
            Value::Boolean(true),
            Value::String("s".into()),
        ]);
        assert_eq!(
            typed_value(&value),
            json!([
                { "when": "1979-05-27T07:32:00Z", "ratio": "nan" },
                "inf",
                "-inf",
                1.5,
                7,
                true,
                "s"
            ])
        );
    }

    #[test]
    fn a_written_file_confirmation_carries_its_path_as_text() {
        let result = ConfigResult::TemplateWritten {
            path: std::path::PathBuf::from("generated.toml"),
        };
        let (output, _) = config_result_output(result, OutputMode::Json);
        let Output::Render(data) = output else {
            panic!("a confirmation renders");
        };
        assert_eq!(data, json!({ "path": "generated.toml" }));
    }

    #[test]
    fn the_config_tree_claims_root_globals_by_long_short_and_id() {
        let cases = [
            (Arg::new("x").long("scope"), Some("`--scope`")),
            (Arg::new("x").long("file"), Some("`--file`")),
            (Arg::new("x").long("force"), Some("`--force`")),
            (Arg::new("x").long("zone").alias("scope"), Some("`--scope`")),
            (Arg::new("x").short('o'), Some("`-o`")),
            (Arg::new("x").short('x').short_alias('o'), Some("`-o`")),
            (Arg::new("output").long("out"), Some("the id `output`")),
            (Arg::new("x").long("output"), None),
            (Arg::new("x").long("set"), None),
        ];
        for (arg, expected) in cases {
            assert_eq!(config_tree_claim(&arg).as_deref(), expected, "{arg:?}");
        }
        assert!(config_tree_takes_long("scope") && !config_tree_takes_long("output"));
    }

    #[test]
    fn an_override_pair_splits_at_the_first_equals() {
        assert_eq!(
            parse_override_pair("a.b=c=d").unwrap(),
            ("a.b".to_string(), "c=d".to_string())
        );
        assert!(parse_override_pair("novalue").is_err());
        assert!(parse_override_pair("=v").is_err());
    }
}
