use clap::{CommandFactory, Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tdoo", about = "A tiny todo list - the Standout sample app")]
pub(crate) struct Cli {
    /// Optional so a naked `tdoo` parses successfully and reaches Standout's
    /// default-command resolution (see `app::build`) instead of failing as a
    /// clap usage error.
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Add a new todo. Title comes from --title or piped stdin.
    Add {
        #[arg(short, long)]
        title: Option<String>,
    },
    /// List todos. By default only pending ones; pass --all for everything.
    List {
        #[arg(short, long)]
        all: bool,
    },
    /// Mark a todo done.
    Done { id: u32 },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}

/// Resolves CLI configuration before constructing the core store.
pub(crate) fn resolve_store_path() -> PathBuf {
    store_path_from_env(
        std::env::var_os("TODO_FILE"),
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
    )
}

fn store_path_from_env(
    todo_file: Option<OsString>,
    home: Option<OsString>,
    user_profile: Option<OsString>,
) -> PathBuf {
    if let Some(path) = todo_file {
        return PathBuf::from(path);
    }
    home.or(user_profile)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todos.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_todo_file_takes_precedence() {
        let path = store_path_from_env(
            Some("custom.json".into()),
            Some("/home/alice".into()),
            Some("C:\\Users\\Alice".into()),
        );

        assert_eq!(path, PathBuf::from("custom.json"));
    }

    #[test]
    fn home_precedes_windows_user_profile() {
        let path = store_path_from_env(
            None,
            Some("/home/alice".into()),
            Some("C:\\Users\\Alice".into()),
        );

        assert_eq!(path, PathBuf::from("/home/alice").join(".todos.json"));
    }

    #[test]
    fn windows_user_profile_is_a_portable_fallback() {
        let path = store_path_from_env(None, None, Some("C:\\Users\\Alice".into()));

        assert_eq!(path, PathBuf::from("C:\\Users\\Alice").join(".todos.json"));
    }

    #[test]
    fn missing_home_variables_fall_back_to_current_directory() {
        let path = store_path_from_env(None, None, None);

        assert_eq!(path, PathBuf::from(".").join(".todos.json"));
    }
}
