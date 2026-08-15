//! Command dispatch logic.
//!
//! Core utilities for extracting command paths from clap ArgMatches
//! and managing the dispatch pipeline.

use clap::ArgMatches;

/// Extracts the command path from ArgMatches by following the subcommand chain.
///
/// For example, `myapp db migrate` produces `["db", "migrate"]`.
///
/// No name is privileged. A word the caller answers for itself — a framework
/// rendering its own `help`, say — never reaches these matches; a command
/// merely *called* `help` is a command like any other and is returned as one.
pub fn extract_command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = matches;

    while let Some((name, sub)) = current.subcommand() {
        path.push(name.to_string());
        current = sub;
    }

    path
}

/// Gets the deepest subcommand matches.
///
/// Traverses the subcommand chain and returns the ArgMatches
/// for the most deeply nested command.
pub fn get_deepest_matches(matches: &ArgMatches) -> &ArgMatches {
    let mut current = matches;

    while let Some((_, sub)) = current.subcommand() {
        current = sub;
    }

    current
}

/// Returns true if a subcommand was selected.
///
/// This reads a *parse result*, so there has to be one: a root that rejects the
/// line — because it requires arguments, or requires a subcommand — never gets
/// this far. "Was this invocation naked?" is still Clap's question to answer
/// for such a root, by parsing the line again permissively (see
/// `App::parse_with_default_command` in `standout`), never by reading the
/// argument list by hand.
pub fn has_subcommand(matches: &ArgMatches) -> bool {
    matches.subcommand().is_some()
}

/// Inserts a command name at position 1 (after program name) in the argument list.
///
/// For callers driving default-command substitution themselves — a partially
/// adopted CLI, most often. Standout's own parse path does the same insertion
/// on `OsString`s instead, so that substituting a command cannot mangle a
/// non-UTF8 argument beside it; this helper is for the `String`-typed argv a
/// caller already holds.
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

/// Converts a command path vector to a dot-separated string.
///
/// For example, `["db", "migrate"]` becomes `"db.migrate"`.
pub fn path_to_string(path: &[String]) -> String {
    path.join(".")
}

/// Parses a dot-separated command path string into a vector.
///
/// For example, `"db.migrate"` becomes `["db", "migrate"]`.
pub fn string_to_path(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split('.').map(String::from).collect()
    }
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
    fn test_has_subcommand_false() {
        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app"]).unwrap();
        assert!(!has_subcommand(&matches));
    }

    #[test]
    fn test_no_name_is_privileged() {
        // A command merely called `help` dispatches like any other; the word is
        // only special where its caller answers it before dispatch.
        let cmd = Command::new("app")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help"));

        let matches = cmd.try_get_matches_from(["app", "help"]).unwrap();
        assert!(has_subcommand(&matches));
        assert_eq!(extract_command_path(&matches), vec!["help"]);
    }

    #[test]
    fn test_insert_default_command() {
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
    fn test_path_to_string() {
        assert_eq!(
            path_to_string(&["db".into(), "migrate".into()]),
            "db.migrate"
        );
        assert_eq!(path_to_string(&["list".into()]), "list");
        assert_eq!(path_to_string(&[]), "");
    }

    #[test]
    fn test_string_to_path() {
        assert_eq!(string_to_path("db.migrate"), vec!["db", "migrate"]);
        assert_eq!(string_to_path("list"), vec!["list"]);
        assert_eq!(string_to_path(""), Vec::<String>::new());
    }
}
