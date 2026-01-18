//! Local (single-threaded) app for mutable handlers.
//!
//! This module provides [`LocalApp`] for CLI applications that need
//! `FnMut` handlers with `&mut self` access to state.
//!
//! # When to Use
//!
//! Use `LocalApp` when:
//! - Your handlers need `&mut self` access to state
//! - You want to avoid `Arc<Mutex<_>>` wrappers
//! - Your CLI is single-threaded (the common case)
//!
//! # Example
//!
//! ```rust,ignore
//! use standout::cli::{LocalApp, Output};
//!
//! struct Database {
//!     records: Vec<Record>,
//! }
//!
//! impl Database {
//!     fn add(&mut self, r: Record) { self.records.push(r); }
//! }
//!
//! let mut db = Database { records: vec![] };
//!
//! LocalApp::builder()
//!     .command("add", |m, ctx| {
//!         db.add(Record::new(m.get_one::<String>("name").unwrap()));
//!         Ok(Output::Silent)
//!     }, "")
//!     .build()?
//!     .run(cmd, args);
//! ```

use std::collections::HashMap;

use clap::{ArgMatches, Command};
use serde::Serialize;

use crate::setup::SetupError;
use crate::{OutputMode, Theme};

use super::core::AppCore;
use super::dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    DispatchOutput, LocalDispatchFn,
};
use super::handler::{CommandContext, RunResult};
use super::hooks::RenderedOutput;
use super::local_builder::LocalAppBuilder;

/// Local (single-threaded) CLI application.
///
/// Unlike [`App`](super::App), this type:
/// - Uses `FnMut` handlers instead of `Fn`
/// - Does NOT require `Send + Sync` on handlers
/// - Allows handlers to capture `&mut` references to state
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{LocalApp, Output};
///
/// let mut counter = 0u32;
///
/// LocalApp::builder()
///     .command("increment", |m, ctx| {
///         counter += 1;
///         Ok(Output::Render(counter))
///     }, "{{ count }}")
///     .build()?
///     .run(cmd, args);
/// ```
///
/// # Comparison with App
///
/// | Aspect | `App` | `LocalApp` |
/// |--------|-------|------------|
/// | Handler type | `Fn + Send + Sync` | `FnMut` |
/// | State mutation | Via `Arc<Mutex<_>>` | Direct |
/// | Thread safety | Yes | No |
/// | Use case | Libraries, async | Simple CLIs |
pub struct LocalApp {
    /// Shared core configuration and functionality.
    pub(crate) core: AppCore,
    /// Registered command handlers.
    pub(crate) commands: HashMap<String, LocalDispatchFn>,
}

impl LocalApp {
    /// Creates a new builder for constructing a LocalApp instance.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let app = LocalApp::builder()
    ///     .command("list", handler, template)
    ///     .build()?;
    /// ```
    pub fn builder() -> LocalAppBuilder {
        LocalAppBuilder::new()
    }

    // =========================================================================
    // Delegated accessors (from AppCore)
    // =========================================================================

    /// Returns the current output mode.
    pub fn output_mode(&self) -> OutputMode {
        self.core.output_mode()
    }

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&super::hooks::Hooks> {
        self.core.get_hooks(path)
    }

    /// Returns the default theme, if configured.
    pub fn theme(&self) -> Option<&Theme> {
        self.core.theme()
    }

    /// Returns the names of all available templates.
    ///
    /// Returns an empty iterator if no template registry is configured.
    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.core.template_names()
    }

    /// Returns the names of all available themes.
    ///
    /// Returns an empty vector if no stylesheet registry is configured.
    pub fn theme_names(&self) -> Vec<String> {
        self.core.theme_names()
    }

    /// Gets a theme by name from the stylesheet registry.
    ///
    /// This allows using themes other than the default at runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if no stylesheet registry is configured or if the theme
    /// is not found.
    pub fn get_theme(&mut self, name: &str) -> Result<Theme, SetupError> {
        self.core.get_theme(name)
    }

    // =========================================================================
    // Rendering (delegated to AppCore)
    // =========================================================================

    /// Renders a template by name with the given data.
    ///
    /// Looks up the template in the registry and renders it.
    /// Supports `{% include %}` directives via the template registry.
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        self.core.render(template, data, mode)
    }

    /// Renders an inline template string with the given data.
    ///
    /// Unlike `render`, this takes the template content directly.
    /// Still supports `{% include %}` if a template registry is configured.
    pub fn render_inline<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        self.core.render_inline(template, data, mode)
    }

    // =========================================================================
    // Dispatch
    // =========================================================================

    /// Dispatches to a registered handler if one matches the command path.
    ///
    /// Note: This method takes `&mut self` because local handlers may mutate state.
    pub fn dispatch(&mut self, matches: ArgMatches, output_mode: OutputMode) -> RunResult {
        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        if let Some(dispatch) = self.commands.get(&path_str) {
            let ctx = CommandContext {
                output_mode,
                command_path: path,
            };

            let hooks = self.core.get_hooks(&path_str);

            // Run pre-dispatch hooks
            if let Some(hooks) = hooks {
                if let Err(e) = hooks.run_pre_dispatch(&matches, &ctx) {
                    return RunResult::Handled(format!("Hook error: {}", e));
                }
            }

            let sub_matches = get_deepest_matches(&matches);

            // Run the handler (needs mutable borrow)
            let dispatch_output = {
                let mut dispatch_fn = dispatch.borrow_mut();
                match dispatch_fn(sub_matches, &ctx, hooks) {
                    Ok(output) => output,
                    Err(e) => return RunResult::Handled(e),
                }
            };

            // Convert to Output enum for post-output hooks
            let output = match dispatch_output {
                DispatchOutput::Text(s) => RenderedOutput::Text(s),
                DispatchOutput::Binary(b, f) => RenderedOutput::Binary(b, f),
                DispatchOutput::Silent => RenderedOutput::Silent,
            };

            // Run post-output hooks
            let final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(&matches, &ctx, output) {
                    Ok(o) => o,
                    Err(e) => return RunResult::Handled(format!("Hook error: {}", e)),
                }
            } else {
                output
            };

            match final_output {
                RenderedOutput::Text(s) => RunResult::Handled(s),
                RenderedOutput::Binary(b, f) => RunResult::Binary(b, f),
                RenderedOutput::Silent => RunResult::Handled(String::new()),
            }
        } else {
            RunResult::NoMatch(matches)
        }
    }

    /// Parses arguments and dispatches to registered handlers.
    ///
    /// Note: This method takes `&mut self` because local handlers may mutate state.
    pub fn dispatch_from<I, T>(&mut self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args: Vec<String> = args
            .into_iter()
            .map(|a| a.into().to_string_lossy().into_owned())
            .collect();

        let augmented_cmd = self.core.augment_command(cmd.clone());

        let matches = match augmented_cmd.try_get_matches_from(&args) {
            Ok(m) => m,
            Err(e) => return RunResult::Handled(e.to_string()),
        };

        // Check if we need to insert default command
        let matches = if !has_subcommand(&matches) && self.core.default_command().is_some() {
            let default_cmd = self.core.default_command().unwrap();
            let new_args = insert_default_command(args, default_cmd);

            let augmented_cmd = self.core.augment_command(cmd);
            match augmented_cmd.try_get_matches_from(&new_args) {
                Ok(m) => m,
                Err(e) => return RunResult::Handled(e.to_string()),
            }
        } else {
            matches
        };

        // Extract output mode using core
        let output_mode = self.core.extract_output_mode(&matches);

        self.dispatch(matches, output_mode)
    }

    /// Runs the CLI: parses arguments, dispatches to handlers, and prints output.
    ///
    /// Note: This method takes `&mut self` because local handlers may mutate state.
    ///
    /// # Returns
    ///
    /// - `true` if a handler processed and printed output
    /// - `false` if no handler matched
    pub fn run<I, T>(&mut self, cmd: Command, args: I) -> bool
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        match self.dispatch_from(cmd, args) {
            RunResult::Handled(output) => {
                if !output.is_empty() {
                    println!("{}", output);
                }
                true
            }
            RunResult::Binary(bytes, filename) => {
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    eprintln!("Error writing {}: {}", filename, e);
                } else {
                    eprintln!("Wrote {} bytes to {}", bytes.len(), filename);
                }
                true
            }
            RunResult::NoMatch(_) => false,
        }
    }

    /// Runs the CLI and returns the rendered output as a string.
    ///
    /// Note: This method takes `&mut self` because local handlers may mutate state.
    pub fn run_to_string<I, T>(&mut self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.dispatch_from(cmd, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::Output;
    use serde_json::json;

    #[test]
    fn test_local_app_dispatch() {
        let mut counter = 0u32;

        let mut app = LocalApp::builder()
            .command(
                "increment",
                move |_m, _ctx| {
                    counter += 1;
                    Ok(Output::Render(json!({"count": counter})))
                },
                "{{ count }}",
            )
            .build()
            .unwrap();

        let cmd = Command::new("test").subcommand(Command::new("increment"));

        // First call
        let result = app.dispatch_from(cmd.clone(), ["test", "increment"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("1"));

        // Second call - counter should increment
        let result = app.dispatch_from(cmd.clone(), ["test", "increment"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("2"));
    }

    #[test]
    fn test_local_app_mutable_struct() {
        use crate::cli::handler::HandlerResult;
        use crate::cli::LocalHandler;

        struct Counter {
            count: u32,
        }

        impl LocalHandler for Counter {
            type Output = serde_json::Value;

            fn handle(
                &mut self,
                _m: &ArgMatches,
                _ctx: &CommandContext,
            ) -> HandlerResult<serde_json::Value> {
                self.count += 1;
                Ok(Output::Render(json!({"count": self.count})))
            }
        }

        let mut app = LocalApp::builder()
            .command_handler("count", Counter { count: 0 }, "{{ count }}")
            .build()
            .unwrap();

        let cmd = Command::new("test").subcommand(Command::new("count"));

        // Multiple calls accumulate state
        let _ = app.dispatch_from(cmd.clone(), ["test", "count"]);
        let _ = app.dispatch_from(cmd.clone(), ["test", "count"]);
        let result = app.dispatch_from(cmd.clone(), ["test", "count"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("3"));
    }

    #[test]
    fn test_local_app_no_match() {
        let app = LocalApp::builder()
            .command("list", |_m, _ctx| Ok(Output::Render(json!({}))), "")
            .build()
            .unwrap();

        let cmd = Command::new("test")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let mut app = app;
        let result = app.dispatch_from(cmd, ["test", "other"]);
        assert!(!result.is_handled());
    }
}
