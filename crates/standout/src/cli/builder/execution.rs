//! Dispatch and execution methods for AppBuilder.
//!
//! This module contains methods for dispatching and running commands:
//! - `commands()` - dispatch macro integration
//! - `dispatch()` - match and execute handler
//! - `dispatch_from()` - parse args and dispatch
//! - `run()` - dispatch and print
//! - `run_to_string()` - dispatch and return

use crate::{write_binary_output, write_output, OutputDestination, OutputMode};
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

use super::{AppBuilder, PendingCommand};
use crate::cli::dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    DispatchOutput,
};
use crate::cli::group::{ErasedConfigRecipe, GroupBuilder, GroupEntry};
use crate::cli::handler::{CommandContext, RunResult};
use crate::cli::hooks::RenderedOutput;
use crate::SetupError;

impl AppBuilder {
    /// Registers commands from a dispatch closure (used by the `dispatch!` macro).
    ///
    /// This method accepts a closure that configures a [`GroupBuilder`] with commands
    /// and nested groups. It's typically used with the [`dispatch!`] macro:
    ///
    /// ```rust,ignore
    /// use standout::cli::{dispatch, App};
    ///
    /// App::builder()
    ///     .template_dir("templates")
    ///     .commands(dispatch! {
    ///         db: {
    ///             migrate => db::migrate,
    ///             backup => db::backup,
    ///         },
    ///         version => version,
    ///     })
    ///     .build()
    /// ```
    ///
    /// The closure receives an empty [`GroupBuilder`] and should return it with
    /// commands added. Each top-level entry becomes a command or group.
    pub fn commands<F>(mut self, configure: F) -> Result<Self, SetupError>
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());

        // Extract default command if set in the builder
        if let Some(ref default_cmd) = builder.default_command {
            self.default_command = Some(default_cmd.clone());
        }

        // Register all entries from the group builder with deferred closure creation
        for (name, entry) in builder.entries {
            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = handler
                        .template()
                        .map(String::from)
                        .unwrap_or_else(|| self.resolve_template(&name));

                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(name.clone(), hooks);
                    }

                    // Create a recipe for deferred closure creation
                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    // Check for duplicates
                    if self.pending_commands.borrow().contains_key(&name) {
                        return Err(SetupError::DuplicateCommand(name));
                    }

                    // Store pending command
                    self.pending_commands.borrow_mut().insert(
                        name,
                        PendingCommand {
                            recipe: Box::new(recipe),
                            template,
                        },
                    );
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&name, nested)?;
                }
            }
        }

        Ok(self)
    }

    /// Dispatches to a registered handler if one matches the command path.
    ///
    /// Returns `RunResult::Handled(output)` if a handler was found and executed,
    /// or `RunResult::NoMatch(matches)` if no handler matched.
    ///
    /// If hooks are registered for the command, they are executed:
    /// - Pre-dispatch hooks run before the handler
    /// - Post-dispatch hooks run after the handler but before rendering
    /// - Post-output hooks run after rendering
    ///
    /// Hook errors abort execution and return the error as handled output.
    pub fn dispatch(&self, matches: ArgMatches, output_mode: OutputMode) -> RunResult {
        // Ensure commands are finalized (creates dispatch closures with current theme)
        self.ensure_commands_finalized();

        // Build command path from matches
        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        // Look up handler
        let commands = self.get_commands();
        if let Some(dispatch) = commands.get(&path_str) {
            let ctx = CommandContext {
                output_mode,
                command_path: path,
            };

            // Get hooks for this command (used for pre-dispatch, post-dispatch, and post-output)
            let hooks = self.command_hooks.get(&path_str);

            // Run pre-dispatch hooks if registered
            if let Some(hooks) = hooks {
                if let Err(e) = hooks.run_pre_dispatch(&matches, &ctx) {
                    return RunResult::Handled(format!("Hook error: {}", e));
                }
            }

            // Get the subcommand matches for the deepest command
            let sub_matches = get_deepest_matches(&matches);

            // Run the handler (post-dispatch hooks are run inside dispatch function)
            let dispatch_output = match dispatch(sub_matches, &ctx, hooks) {
                Ok(output) => output,
                Err(e) => return RunResult::Handled(e),
            };

            // Convert to Output enum for post-output hooks
            let output = match dispatch_output {
                DispatchOutput::Text(s) => RenderedOutput::Text(s),
                DispatchOutput::Binary(b, f) => RenderedOutput::Binary(b, f),
                DispatchOutput::Silent => RenderedOutput::Silent,
            };

            // Run post-output hooks if registered
            let mut final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(&matches, &ctx, output) {
                    Ok(o) => o,
                    Err(e) => return RunResult::Handled(format!("Hook error: {}", e)),
                }
            } else {
                output
            };

            // Handle file output if configured
            if self.output_file_flag.is_some() {
                if let Some(path_str) = matches
                    .try_get_one::<String>("_output_file_path")
                    .unwrap_or(None)
                {
                    let path = PathBuf::from(path_str);
                    let dest = OutputDestination::File(path);

                    match &final_output {
                        RenderedOutput::Text(s) => {
                            if let Err(e) = write_output(s, &dest) {
                                return RunResult::Handled(format!("Error writing output: {}", e));
                            }
                            // Suppress further output
                            final_output = RenderedOutput::Silent;
                        }
                        RenderedOutput::Binary(b, _) => {
                            if let Err(e) = write_binary_output(b, &dest) {
                                return RunResult::Handled(format!("Error writing output: {}", e));
                            }
                            final_output = RenderedOutput::Silent;
                        }
                        RenderedOutput::Silent => {}
                    }
                }
            }

            // Convert back to RunResult
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
    /// This is the recommended entry point when using the command handler system.
    /// It augments the command with `--output` flag, parses arguments, and
    /// dispatches to registered handlers.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` if a registered handler processed the command
    /// - `RunResult::NoMatch(matches)` if no handler matched (for manual handling)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"]), "{{ . }}")
    ///     .dispatch_from(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::NoMatch(matches) => {
    ///         // Handle manually
    ///     }
    /// }
    /// ```
    pub fn dispatch_from<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        // Collect args to Vec<String> so we can potentially reparse with default command
        let args: Vec<String> = args
            .into_iter()
            .map(|a| a.into().to_string_lossy().into_owned())
            .collect();

        // Augment command with --output flag
        let augmented_cmd = self.augment_command_for_dispatch(cmd.clone());

        // Parse arguments
        let matches = match augmented_cmd.try_get_matches_from(&args) {
            Ok(m) => m,
            Err(e) => {
                // Return error as handled output
                return RunResult::Handled(e.to_string());
            }
        };

        // Check if we need to insert default command
        let matches = if !has_subcommand(&matches) && self.default_command.is_some() {
            let default_cmd = self.default_command.as_ref().unwrap();
            let new_args = insert_default_command(args, default_cmd);

            // Reparse with default command inserted
            let augmented_cmd = self.augment_command_for_dispatch(cmd);
            match augmented_cmd.try_get_matches_from(&new_args) {
                Ok(m) => m,
                Err(e) => return RunResult::Handled(e.to_string()),
            }
        } else {
            matches
        };

        // Extract output mode
        let output_mode = if self.output_flag.is_some() {
            match matches
                .get_one::<String>("_output_mode")
                .map(|s| s.as_str())
            {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                Some("yaml") => OutputMode::Yaml,
                Some("xml") => OutputMode::Xml,
                Some("csv") => OutputMode::Csv,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        };

        // Dispatch to handler
        self.dispatch(matches, output_mode)
    }

    /// Runs the CLI: parses arguments, dispatches to handlers, and prints output.
    ///
    /// This is the main entry point for command execution. It handles everything:
    /// parsing, dispatch, rendering, and output.
    ///
    /// # Returns
    ///
    /// - `true` if a handler processed and printed output
    /// - `false` if no handler matched (caller should handle manually)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output};
    ///
    /// let handled = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"])), "{{ . }}")?
    ///     .build()?
    ///     .run(cmd, std::env::args());
    ///
    /// if !handled {
    ///     // Handle unregistered commands manually
    /// }
    /// ```
    pub fn run<I, T>(&self, cmd: Command, args: I) -> bool
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
                // For binary output, write to stdout or the suggested file
                // By default, we write to the suggested filename
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
    /// Similar to `run()`, but returns the output instead of printing it.
    /// Useful for testing or when you need to capture and process the output.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` - Handler executed, output is the rendered string
    /// - `RunResult::Binary(bytes, filename)` - Handler produced binary output
    /// - `RunResult::NoMatch(matches)` - No handler matched
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"])), "{{ . }}")?
    ///     .build()?
    ///     .run_to_string(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::Binary(bytes, filename) => std::fs::write(filename, bytes)?,
    ///     RunResult::NoMatch(matches) => { /* handle manually */ }
    /// }
    /// ```
    pub fn run_to_string<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.dispatch_from(cmd, args)
    }

    /// Augments a command for dispatch (adds --output flag without help subcommand).
    fn augment_command_for_dispatch(&self, mut cmd: Command) -> Command {
        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser(["auto", "term", "text", "term-debug", "json"])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, term-debug, or json"),
            );
        }

        // Add output file flag if enabled
        if let Some(ref flag_name) = self.output_file_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_file_path")
                    .long(flag)
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
        }

        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::HandlerResult;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::cli::hooks::{HookError, Hooks, RenderedOutput};

    // ============================================================================
    // Dispatch Macro Integration Tests
    // ============================================================================

    #[test]
    fn test_dispatch_macro_simple() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                list => |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))
            })
            .unwrap();

        assert!(builder.has_command("list"));

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("items"));
    }

    #[test]
    fn test_dispatch_macro_with_groups() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                db: {
                    migrate => |_m, _ctx| Ok(HandlerOutput::Render(json!({"migrated": true}))),
                    backup => |_m, _ctx| Ok(HandlerOutput::Render(json!({"backed_up": true}))),
                },
                version => |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0"}))),
            })
            .unwrap();

        assert!(builder.has_command("db.migrate"));
        assert!(builder.has_command("db.backup"));
        assert!(builder.has_command("version"));

        // Test dispatch to nested command
        let cmd = Command::new("app")
            .subcommand(
                Command::new("db")
                    .subcommand(Command::new("migrate"))
                    .subcommand(Command::new("backup")),
            )
            .subcommand(Command::new("version"));

        let matches = cmd
            .clone()
            .try_get_matches_from(["app", "db", "migrate"])
            .unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);
        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("migrated"));
    }

    #[test]
    fn test_dispatch_macro_with_template() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                    template: "Count: {{ count }}",
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_macro_with_hooks() {
        use crate::dispatch;
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new()
            .commands(dispatch! {
                list => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    template: "{{ ok }}",
                    pre_dispatch: move |_, _| {
                        hook_called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    },
                }
            })
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_dispatch_macro_deeply_nested() {
        use crate::dispatch;
        use serde_json::json;

        let builder = AppBuilder::new()
            .commands(dispatch! {
                app: {
                    config: {
                        get => |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                        set => |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    },
                    start => |_m, _ctx| Ok(HandlerOutput::Render(json!({"started": true}))),
                },
            })
            .unwrap();

        assert!(builder.has_command("app.config.get"));
        assert!(builder.has_command("app.config.set"));
        assert!(builder.has_command("app.start"));
    }

    // ============================================================================
    // Core Dispatch Tests
    // ============================================================================

    #[test]
    fn test_dispatch_to_handler() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_unhandled_fallthrough() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "")
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(!result.is_handled());
        assert!(result.matches().is_some());
    }

    #[test]
    fn test_dispatch_json_output() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test", "value": 123}))),
                "{{ name }}: {{ value }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"value\": 123"));
    }

    #[test]
    fn test_dispatch_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "config.get",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                "{{ key }}",
            )
            .unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("value"));
    }

    #[test]
    fn test_dispatch_silent_result() {
        let builder = AppBuilder::new()
            .command("quiet", |_m, _ctx| Ok(HandlerOutput::<()>::Silent), "")
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("quiet"));

        let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_dispatch_error_result() {
        let builder = AppBuilder::new()
            .command(
                "fail",
                |_m, _ctx| Err::<HandlerOutput<()>, _>(anyhow::anyhow!("something went wrong")),
                "",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("fail"));

        let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Error:"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_dispatch_from_basic() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                "Items: {{ items }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_dispatch_from_with_json_flag() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output=json", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 5"));
    }

    #[test]
    fn test_dispatch_from_unhandled() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "")
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let result = builder.dispatch_from(cmd, ["app", "other"]);

        assert!(!result.is_handled());
    }

    // ============================================================================
    // Hook Execution Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_pre_dispatch_hook() {
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1}))),
                "{{ count }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().pre_dispatch(move |_, _ctx| {
                    hook_called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
        assert_eq!(result.output(), Some("1"));
    }

    #[test]
    fn test_dispatch_pre_dispatch_hook_abort() {
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| -> HandlerResult<()> {
                    panic!("Handler should not be called");
                },
                "",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("blocked by hook"))),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("blocked by hook"));
    }

    #[test]
    fn test_dispatch_with_post_output_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text) = output {
                        Ok(RenderedOutput::Text(text.to_uppercase()))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("HELLO"));
    }

    #[test]
    fn test_dispatch_post_output_hook_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "test"}))),
                "{{ msg }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text) = output {
                            Ok(RenderedOutput::Text(format!("[{}]", text)))
                        } else {
                            Ok(output)
                        }
                    })
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text) = output {
                            Ok(RenderedOutput::Text(text.to_uppercase()))
                        } else {
                            Ok(output)
                        }
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("[TEST]"));
    }

    #[test]
    fn test_dispatch_post_output_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _output| {
                    Err(HookError::post_output("post-processing failed"))
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("post-processing failed"));
    }

    #[test]
    fn test_dispatch_hooks_for_nested_command() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "config.get",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "secret"}))),
                "{{ value }}",
            )
            .unwrap()
            .hooks(
                "config.get",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(_) = output {
                        Ok(RenderedOutput::Text("***".into()))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("***"));
    }

    #[test]
    fn test_dispatch_no_hooks_for_command() {
        use serde_json::json;

        // Register hooks for "list" but dispatch "other"
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "list"}))),
                "{{ msg }}",
            )
            .unwrap()
            .command(
                "other",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "other"}))),
                "{{ msg }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _| {
                    panic!("Should not be called for 'other' command");
                }),
            );

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("other"));
    }

    #[test]
    fn test_dispatch_binary_output_with_hook() {
        let builder = AppBuilder::new()
            .command(
                "export",
                |_m, _ctx| -> HandlerResult<()> {
                    Ok(HandlerOutput::Binary {
                        data: vec![1, 2, 3],
                        filename: "out.bin".into(),
                    })
                },
                "",
            )
            .unwrap()
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Binary(mut bytes, filename) = output {
                        bytes.push(4);
                        Ok(RenderedOutput::Binary(bytes, filename))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("export"));

        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_binary());
        let (bytes, filename) = result.binary().unwrap();
        assert_eq!(bytes, &[1, 2, 3, 4]);
        assert_eq!(filename, "out.bin");
    }

    #[test]
    fn test_hooks_passed_to_built_standout() {
        let standout = AppBuilder::new()
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
            .build()
            .unwrap();

        assert!(standout.get_hooks("list").is_some());
        assert!(standout.get_hooks("other").is_none());
    }

    #[test]
    fn test_run_command_with_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let standout = AppBuilder::new()
            .hooks(
                "test",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text) = output {
                        Ok(RenderedOutput::Text(format!("wrapped: {}", text)))
                    } else {
                        Ok(output)
                    }
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            "{{ value }}",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("wrapped: 42"));
    }

    #[test]
    fn test_run_command_pre_dispatch_abort() {
        let standout = AppBuilder::new()
            .hooks(
                "test",
                Hooks::new().pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("access denied"))),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| {
                panic!("Handler should not be called");
            },
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("access denied"));
    }

    #[test]
    fn test_run_command_without_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let standout = AppBuilder::new().build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            |_m, _ctx| {
                Ok(HandlerOutput::Render(Data {
                    msg: "hello".into(),
                }))
            },
            "{{ msg }}",
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("hello"));
    }

    #[test]
    fn test_run_command_silent() {
        let standout = AppBuilder::new().build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Silent),
            "",
        );

        assert!(result.is_ok());
        assert!(result.unwrap().is_silent());
    }

    #[test]
    fn test_run_command_binary() {
        let standout = AppBuilder::new()
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    // Verify we receive binary output
                    assert!(output.is_binary());
                    Ok(output)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = standout.run_command::<_, ()>(
            "export",
            sub_matches,
            |_m, _ctx| {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                })
            },
            "",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD]);
        assert_eq!(filename, "data.bin");
    }

    // ============================================================================
    // Post-dispatch Hook Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_post_dispatch_hook() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                "Count: {{ count }}, Modified: {{ modified }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("modified".into(), json!(true));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Count: 5"));
        assert!(output.contains("Modified: true"));
    }

    #[test]
    fn test_dispatch_post_dispatch_hook_abort() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []}))),
                "{{ items }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    // Abort if no items
                    if data
                        .get("items")
                        .and_then(|v| v.as_array())
                        .map(|a| a.is_empty())
                        == Some(true)
                    {
                        return Err(HookError::post_dispatch("no items to display"));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("no items to display"));
    }

    #[test]
    fn test_dispatch_post_dispatch_chain() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": 1}))),
                "{{ value }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .post_dispatch(|_, _ctx, mut data| {
                        // First hook: multiply by 2
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) * 2);
                        }
                        Ok(data)
                    })
                    .post_dispatch(|_, _ctx, mut data| {
                        // Second hook: add 10
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) + 10);
                        }
                        Ok(data)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        // 1 * 2 = 2, 2 + 10 = 12
        assert_eq!(result.output(), Some("12"));
    }

    #[test]
    fn test_dispatch_all_three_hooks() {
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_order = Arc::new(AtomicUsize::new(0));
        let pre_order = call_order.clone();
        let post_dispatch_order = call_order.clone();
        let post_output_order = call_order.clone();

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .unwrap()
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(move |_, _ctx| {
                        assert_eq!(pre_order.fetch_add(1, Ordering::SeqCst), 0);
                        Ok(())
                    })
                    .post_dispatch(move |_, _ctx, data| {
                        assert_eq!(post_dispatch_order.fetch_add(1, Ordering::SeqCst), 1);
                        Ok(data)
                    })
                    .post_output(move |_, _ctx, output| {
                        assert_eq!(post_output_order.fetch_add(1, Ordering::SeqCst), 2);
                        Ok(output)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(call_order.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_run_command_with_post_dispatch_hook() {
        use serde::Serialize;
        use serde_json::json;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let standout = AppBuilder::new()
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("added_by_hook".into(), json!("yes"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            "value={{ value }}, added={{ added_by_hook }}",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("value=42, added=yes"));
    }

    #[test]
    fn test_run_command_post_dispatch_abort() {
        use crate::cli::hooks::HookPhase;
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            valid: bool,
        }

        let standout = AppBuilder::new()
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    if data.get("valid") == Some(&serde_json::json!(false)) {
                        return Err(HookError::post_dispatch("invalid data"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = standout.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { valid: false })),
            "{{ valid }}",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "invalid data");
        assert_eq!(err.phase, HookPhase::PostDispatch);
    }

    // ============================================================================
    // Default Command Tests
    // ============================================================================

    #[test]
    fn test_default_command_builder() {
        let builder = AppBuilder::new().default_command("list");

        assert_eq!(builder.default_command, Some("list".to_string()));
    }

    #[test]
    fn test_default_command_naked_invocation() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                "Items: {{ items }}",
            )
            .unwrap()
            .command(
                "add",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"added": true}))),
                "Added: {{ added }}",
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        // Naked invocation should dispatch to default command
        let result = builder.dispatch_from(cmd, ["app"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_default_command_with_options() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        // Naked invocation with --output flag should work
        let result = builder.dispatch_from(cmd, ["app", "--output=json"]);
        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_default_command_explicit_command_overrides() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .default_command("list")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "list"}))),
                "{{ cmd }}",
            )
            .unwrap()
            .command(
                "add",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "add"}))),
                "{{ cmd }}",
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add"));

        // Explicit command should override default
        let result = builder.dispatch_from(cmd, ["app", "add"]);
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("add"));
    }

    #[test]
    fn test_default_command_no_default_set() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []}))),
                "Items: {{ items }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        // Without default command, naked invocation should return NoMatch
        let result = builder.dispatch_from(cmd, ["app"]);
        assert!(!result.is_handled());
    }

    // ============================================================================
    // Output File Flag Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                "Count: {{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output-file-path", path_str, "list"]);

        assert!(result.is_handled());
        // Verify output is suppressed (silent)
        assert_eq!(result.output(), Some(""));

        // Verify file content
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "Count: 42");
    }

    #[test]
    fn test_dispatch_with_custom_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("out.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = AppBuilder::new()
            .output_file_flag(Some("save-to"))
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 99}))),
                "{{ count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--save-to", path_str, "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "99");
    }

    // ============================================================================
    // Theme Ordering Tests (issue #31 fix)
    // ============================================================================

    #[test]
    fn test_theme_ordering_command_before_theme() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        // Create a theme with a custom "late" style
        let theme = Theme::new().add("late", Style::new().bold());

        // BUG TEST: Register command BEFORE setting theme
        // This tests if the closure captures the theme at registration time
        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"}))),
                "[late]{{ name }}[/late]",
            )
            .unwrap()
            .theme(theme); // Theme set AFTER command registration

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "--output=term", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();

        // This will FAIL if closures capture theme at registration time
        // (because theme was None when .command() was called)
        assert!(
            !output.contains("[late?]"),
            "ORDERING BUG: Theme set after .command() was not applied - output: {}",
            output
        );
    }

    #[test]
    fn test_theme_passed_to_dispatch_closure() {
        use crate::Theme;
        use console::Style;
        use serde_json::json;

        // Create a theme with a "test_style" tag
        let theme = Theme::new().add("test_style", Style::new().bold());

        // Build with theme set BEFORE command registration
        let builder = AppBuilder::new()
            .theme(theme)
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"}))),
                "[test_style]{{ name }}[/test_style]",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = builder.dispatch_from(cmd, ["app", "--output=term", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();

        // If theme was passed correctly, there should be NO "[test_style?]" in output
        // (unknown style indicators appear when style is not found)
        assert!(
            !output.contains("[test_style?]"),
            "Theme was not passed to dispatch - output: {}",
            output
        );
    }
}
