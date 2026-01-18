//! Command dispatch logic.
//!
//! Internal types and functions for dispatching commands to handlers.
//!
//! This module provides dispatch function types for both handler modes:
//!
//! - [`DispatchFn`]: Thread-safe dispatch using `Arc<dyn Fn + Send + Sync>`
//! - [`LocalDispatchFn`]: Local dispatch using `Rc<RefCell<dyn FnMut>>`

use clap::ArgMatches;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::cli::handler::CommandContext;
use crate::cli::hooks::Hooks;

/// Internal result type for dispatch functions.
pub(crate) enum DispatchOutput {
    /// Text output (rendered template or JSON)
    Text(String),
    /// Binary output (bytes, filename)
    Binary(Vec<u8>, String),
    /// No output (silent)
    Silent,
}

/// Type-erased dispatch function for thread-safe handlers.
///
/// Takes ArgMatches, CommandContext, and optional Hooks. The hooks parameter
/// allows post-dispatch hooks to run between handler execution and rendering.
///
/// Used with [`App`](super::App) and [`Handler`](super::handler::Handler).
pub(crate) type DispatchFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, Option<&Hooks>) -> Result<DispatchOutput, String>
        + Send
        + Sync,
>;

/// Type-erased dispatch function for local (single-threaded) handlers.
///
/// Unlike [`DispatchFn`], this:
/// - Uses `Rc<RefCell<_>>` instead of `Arc` (no thread-safety overhead)
/// - Uses `FnMut` instead of `Fn` (allows mutable state)
/// - Does NOT require `Send + Sync`
///
/// Used with [`LocalApp`](super::LocalApp) and [`LocalHandler`](super::handler::LocalHandler).
pub(crate) type LocalDispatchFn = Rc<
    RefCell<
        dyn FnMut(&ArgMatches, &CommandContext, Option<&Hooks>) -> Result<DispatchOutput, String>,
    >,
>;

/// Extracts the command path from ArgMatches by following subcommand chain.
pub(crate) fn extract_command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = matches;

    while let Some((name, sub)) = current.subcommand() {
        // Skip "help" as it's handled separately
        if name == "help" {
            break;
        }
        path.push(name.to_string());
        current = sub;
    }

    path
}

/// Gets the deepest subcommand matches.
pub(crate) fn get_deepest_matches(matches: &ArgMatches) -> &ArgMatches {
    let mut current = matches;

    while let Some((name, sub)) = current.subcommand() {
        if name == "help" {
            break;
        }
        current = sub;
    }

    current
}

/// Returns true if the matches contain a subcommand (excluding "help").
///
/// This is used to detect "naked" CLI invocations where no command was specified,
/// enabling default command behavior.
pub fn has_subcommand(matches: &ArgMatches) -> bool {
    matches
        .subcommand()
        .map(|(name, _)| name != "help")
        .unwrap_or(false)
}

/// Inserts a command name at position 1 (after program name) in the argument list.
///
/// This is used to implement default command support: when no subcommand is specified,
/// we insert the default command name and reparse.
///
/// # Example
///
/// ```ignore
/// let args = vec!["myapp".to_string(), "-v".to_string()];
/// let new_args = insert_default_command(args, "list");
/// assert_eq!(new_args, vec!["myapp", "list", "-v"]);
/// ```
pub fn insert_default_command<I, S>(args: I, command: &str) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut result: Vec<String> = args.into_iter().map(Into::into).collect();
    if !result.is_empty() {
        result.insert(1, command.to_string());
    } else {
        result.push(command.to_string());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    #[test]
    fn test_extract_command_path() {
        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let path = extract_command_path(&matches);

        assert_eq!(path, vec!["config", "get"]);
    }

    #[test]
    fn test_extract_command_path_single() {
        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let path = extract_command_path(&matches);

        assert_eq!(path, vec!["list"]);
    }

    #[test]
    fn test_extract_command_path_empty() {
        let cmd = Command::new("app");

        let matches = cmd.try_get_matches_from(["app"]).unwrap();
        let path = extract_command_path(&matches);

        assert!(path.is_empty());
    }

    #[test]
    fn test_has_subcommand_true() {
        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        assert!(has_subcommand(&matches));
    }

    #[test]
    fn test_has_subcommand_false_no_subcommand() {
        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app"]).unwrap();
        assert!(!has_subcommand(&matches));
    }

    #[test]
    fn test_has_subcommand_false_help() {
        // Use disable_help_subcommand to avoid conflict with clap's built-in help
        let cmd = Command::new("app")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help"));

        let matches = cmd.try_get_matches_from(["app", "help"]).unwrap();
        // "help" subcommand is excluded from has_subcommand check
        // because standout handles help separately
        assert!(!has_subcommand(&matches));
    }

    #[test]
    fn test_insert_default_command_basic() {
        let args = vec!["myapp", "-v"];
        let result = insert_default_command(args, "list");
        assert_eq!(result, vec!["myapp", "list", "-v"]);
    }

    #[test]
    fn test_insert_default_command_no_args() {
        let args = vec!["myapp"];
        let result = insert_default_command(args, "list");
        assert_eq!(result, vec!["myapp", "list"]);
    }

    #[test]
    fn test_insert_default_command_empty() {
        let args: Vec<String> = vec![];
        let result = insert_default_command(args, "list");
        assert_eq!(result, vec!["list"]);
    }

    #[test]
    fn test_insert_default_command_with_options() {
        let args = vec!["myapp", "--verbose", "--output", "json"];
        let result = insert_default_command(args, "status");
        assert_eq!(
            result,
            vec!["myapp", "status", "--verbose", "--output", "json"]
        );
    }

    #[test]
    fn test_insert_default_command_with_positional() {
        let args = vec!["myapp", "file.txt"];
        let result = insert_default_command(args, "cat");
        assert_eq!(result, vec!["myapp", "cat", "file.txt"]);
    }
}
