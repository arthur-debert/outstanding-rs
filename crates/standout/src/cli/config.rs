use std::any::Any;
use std::path::Path;

use clap::Command;
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
            json!({ "path": path }),
        ),
        ConfigResult::SchemaWritten { path } => (
            format!("schema written to {}", path.display()),
            json!({ "path": path }),
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

fn typed_value(value: &clapfig::value::Value) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()))
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

    fn handle(&self, action: &ConfigAction) -> Result<ConfigResult, ClapfigError>;
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
        let mut builder = self.builder.clone();
        for (key, raw) in overrides {
            builder = builder.cli_override_str(key, raw);
        }
        let config = builder.build_resolver()?.resolve_at(dir)?;
        let term = self.term.as_ref().map(|accessor| accessor(&config).clone());
        Ok(ResolvedConfig::new(config, term))
    }

    fn handle(&self, action: &ConfigAction) -> Result<ConfigResult, ClapfigError> {
        self.builder.clone().handle(action)
    }
}

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
                extensions.insert(*value);
            },
            term,
        }
    }

    pub(crate) fn install(self, extensions: &mut Extensions) {
        (self.install)(self.value, extensions);
        if let Some(term) = self.term {
            extensions.insert(term);
        }
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
    fn an_override_pair_splits_at_the_first_equals() {
        assert_eq!(
            parse_override_pair("a.b=c=d").unwrap(),
            ("a.b".to_string(), "c=d".to_string())
        );
        assert!(parse_override_pair("novalue").is_err());
        assert!(parse_override_pair("=v").is_err());
    }
}
