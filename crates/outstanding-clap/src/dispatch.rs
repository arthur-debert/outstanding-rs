//! Command dispatch logic.
//!
//! Internal types and functions for dispatching commands to handlers.

use clap::ArgMatches;
use std::sync::Arc;

use crate::handler::CommandContext;
use crate::hooks::Hooks;

/// Internal result type for dispatch functions.
pub(crate) enum DispatchOutput {
    /// Text output (rendered template or JSON)
    Text(String),
    /// Binary output (bytes, filename)
    Binary(Vec<u8>, String),
    /// No output (silent)
    Silent,
}

/// Type-erased dispatch function.
///
/// Takes ArgMatches, CommandContext, and optional Hooks. The hooks parameter
/// allows post-dispatch hooks to run between handler execution and rendering.
pub(crate) type DispatchFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, Option<&Hooks>) -> Result<DispatchOutput, String>
        + Send
        + Sync,
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
}
