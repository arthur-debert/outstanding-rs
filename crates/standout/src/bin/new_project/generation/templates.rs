pub(super) const TEMPLATE_CATALOG: &[(&str, &str)] = &[
    (
        "workspace",
        r#"[workspace]
resolver = "3"
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
    #[error("{field} cannot be empty")]
    EmptyInput { field: &'static str },
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
clapfig = "{{ clapfig_version }}"
serde = { version = "1", features = ["derive"] }
standout = "{{ standout_version }}"
standout-dispatch = "{{ standout_version }}"
standout-input = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
tempfile = "3"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod config;
mod handlers;

use anyhow::Result;
use standout::{embed_styles, embed_templates};

fn main() -> Result<()> {
    let app = build_app(clapfig::SearchPath::Platform)?;
    app.run(cli::command(), std::env::args());
    Ok(())
}

fn build_app(user_scope: clapfig::SearchPath) -> Result<standout::cli::App> {
    Ok(standout::cli::App::builder()
        .name(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("{{ project_name }}")
        .config(config::builder(user_scope))
        .term_settings(|config: &config::Config| &config.term)
        .commands(cli::Commands::dispatch_config())?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serial_test::serial;
    use standout::Representation;
    use standout_test::TestHarness;
    use tempfile::TempDir;

    const CONFIG_FILE: &str = "[term]\noutput = \"json\"\n";

    fn test_app() -> (TempDir, standout::cli::App) {
        let user_dir = TempDir::new().unwrap();
        let app = build_app(clapfig::SearchPath::Path(user_dir.path().to_path_buf())).unwrap();
        (user_dir, app)
    }

    #[test]
    #[serial]
    fn pipeline_renders_human_output_from_argument() {
        let (_user_dir, app) = test_app();

{{ pipeline_human_run }}

        result.assert_success();
        result.assert_stdout_contains({{ human_expected }});
    }

{{ pipeline_json_test }}

{{ config_test }}
}
"#,
    ),
    (
        "cli",
        r#"use clap::{CommandFactory, Parser, Subcommand};
use standout::cli::Dispatch;

#[derive(Parser)]
{{ cli_command_attribute }}
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = crate::handlers)]
pub(crate) enum Commands {
    /// {{ command_description }}
    #[command(name = "{{ command_name }}")]
    {{ dispatch_attribute }}
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
        "config",
        r#"use serde::{Deserialize, Serialize};
use standout::TermSettings;

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
pub(crate) struct Config {
    pub(crate) term: TermSettings,
}

pub(crate) fn builder(user_scope: clapfig::SearchPath) -> clapfig::TypedBuilder<Config> {
    clapfig::Clapfig::typed::<Config>()
        .app_name("{{ executable_name }}")
        .add_search_path(user_scope.clone())
        .add_search_path(clapfig::SearchPath::Cwd)
        .persist_scope("local", clapfig::SearchPath::Cwd)
        .persist_scope("global", user_scope)
}
"#,
    ),
    (
        "handlers",
        r#"{{ handler_imports }}

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
{%- if file_source_item %}

{{ file_source_item }}
{%- endif %}
{%- if command_inputs_fn %}

{{ command_inputs_fn }}
{%- endif %}

/// Adapts typed shell input into the CLI-free core operation.
///
/// Values that can come from more than one place are resolved before dispatch
/// by the command's input chains; the rest arrive as typed parameters. The
/// handler returns data for Standout to render or serialize.
#[handler]
{{ handler_signature }}
{%- if handler_input_reads %}
{{ handler_input_reads }}
{%- endif %}
    let result = core::{{ operation_name }}({{ core_call_args }})?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_handler_maps_input_to_core_and_view() {
{%- if handler_test_inputs %}
{{ handler_test_inputs }}

{%- endif %}
{{ handler_call }}
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
{{ readme_examples }}
```

Command syntax:

```text
{{ executable_name }} {{ command_name }} {{ command_syntax }}
```

Input policy:

{{ readme_input_policy }}

{%- if readme_validation_note %}

{{ readme_validation_note }}
{%- endif %}

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
