//! The pager rule: which command pages a run's human output, how it is
//! started, and what happens when it will not start or stops reading.
//!
//! The command comes from the environment only — `<APP>_PAGER`, then `PAGER`
//! — so no configuration file can name a program the framework will execute.
//! `<APP>` is the name the application gave the builder, so an application
//! that never named itself is paged by `PAGER` alone.
//! An unset variable and an empty winning value both mean "do not page":
//! there is no built-in `less` or `more` to fall back to. The value is a
//! shell word list, not a program name, so it runs through `sh -c`; Windows
//! has no such shell and never pages.
//!
//! `page` reports which of three things happened rather than deciding for the
//! caller: the pager read the bytes, it could not start (the caller still owes
//! the user those bytes on stdout), or it stopped reading early (the bytes are
//! spent and the run keeps its own exit status).

use std::io::Write;
use std::process::{Command, Stdio};

/// What `sh` exits with when it cannot execute the command it was given.
const SHELL_CANNOT_EXEC: i32 = 127;

/// Set for the child only when the parent has not: `less` quits on one screen,
/// keeps ANSI colors and leaves the page on the terminal; `lv` keeps its colors.
const CHILD_ENV: [(&str, &str); 2] = [("LESS", "FRX"), ("LV", "-c")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pager {
    command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagerOutcome {
    Paged,
    CouldNotStart,
    ReaderLeft,
}

impl Pager {
    /// The pager a run already decided on, named by its own delivery record.
    pub(crate) fn named(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    pub(crate) fn command(&self) -> &str {
        &self.command
    }

    pub(crate) fn resolve(app_name: Option<&str>) -> Option<Self> {
        if cfg!(windows) {
            return None;
        }
        Self::resolve_from(app_name, |name| {
            std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
        })
    }

    fn resolve_from(app_name: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let value = app_name
            .and_then(|app_name| var(&app_pager_var(app_name)))
            .or_else(|| var("PAGER"))?;
        let command = value.trim();
        (!command.is_empty()).then(|| Self {
            command: command.to_string(),
        })
    }

    pub(crate) fn page(&self, content: &str) -> PagerOutcome {
        self.page_with(content, &child_env(|key| std::env::var_os(key).is_some()))
    }

    fn page_with(&self, content: &str, env: &[(&str, &str)]) -> PagerOutcome {
        let mut command = Command::new("sh");
        command.arg("-c").arg(&self.command).stdin(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        let Ok(mut child) = command.spawn() else {
            return PagerOutcome::CouldNotStart;
        };
        let unread = match child.stdin.take() {
            Some(mut stdin) => stdin.write_all(content.as_bytes()).is_err(),
            None => false,
        };
        match child.wait() {
            Ok(status) if status.code() == Some(SHELL_CANNOT_EXEC) => PagerOutcome::CouldNotStart,
            _ if unread => PagerOutcome::ReaderLeft,
            _ => PagerOutcome::Paged,
        }
    }
}

/// The application's name upper-cased, with every character outside `A-Z0-9`
/// as `_`.
fn app_pager_var(app_name: &str) -> String {
    let mut name: String = app_name
        .chars()
        .map(|c| {
            let c = c.to_ascii_uppercase();
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .collect();
    name.push_str("_PAGER");
    name
}

fn child_env(is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, &'static str)> {
    CHILD_ENV
        .into_iter()
        .filter(|(key, _)| !is_set(key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn resolve(app_name: Option<&str>, vars: &HashMap<String, String>) -> Option<Pager> {
        Pager::resolve_from(app_name, |name| vars.get(name).cloned())
    }

    #[test]
    fn the_application_variable_outranks_pager() {
        let vars = env(&[("MYAPP_PAGER", "sed -n 1p"), ("PAGER", "less")]);
        assert_eq!(resolve(Some("myapp"), &vars).unwrap().command, "sed -n 1p");
    }

    #[test]
    fn pager_answers_when_the_application_variable_is_unset() {
        let vars = env(&[("PAGER", "less -FRX")]);
        assert_eq!(resolve(Some("myapp"), &vars).unwrap().command, "less -FRX");
    }

    #[test]
    fn an_empty_winning_value_pages_nothing() {
        let vars = env(&[("MYAPP_PAGER", ""), ("PAGER", "less")]);
        assert_eq!(resolve(Some("myapp"), &vars), None);
        assert_eq!(resolve(Some("myapp"), &env(&[("PAGER", "   ")])), None);
    }

    #[test]
    fn neither_variable_set_pages_nothing() {
        assert_eq!(resolve(Some("myapp"), &env(&[])), None);
    }

    #[test]
    fn an_application_that_never_named_itself_reads_pager_alone() {
        let vars = env(&[("MYAPP_PAGER", "sed -n 1p"), ("PAGER", "less")]);
        assert_eq!(resolve(None, &vars).unwrap().command, "less");
        assert_eq!(resolve(None, &env(&[("MYAPP_PAGER", "less")])), None);
    }

    #[test]
    fn the_variable_name_upper_cases_the_application_and_replaces_the_rest() {
        assert_eq!(app_pager_var("myapp"), "MYAPP_PAGER");
        assert_eq!(app_pager_var("my-app"), "MY_APP_PAGER");
        assert_eq!(app_pager_var("my.app 2"), "MY_APP_2_PAGER");
        assert_eq!(app_pager_var("café"), "CAF__PAGER");
    }

    #[test]
    fn the_child_gets_the_reader_settings_the_parent_left_unset() {
        assert_eq!(child_env(|_| false), vec![("LESS", "FRX"), ("LV", "-c")]);
        assert_eq!(child_env(|key| key == "LESS"), vec![("LV", "-c")]);
        assert!(child_env(|_| true).is_empty());
    }

    /// Windows has no `sh` to run the command through, so `resolve` answers
    /// `None` whatever the environment names.
    #[cfg(windows)]
    #[test]
    #[serial_test::serial]
    fn windows_pages_nothing_the_environment_names() {
        let _env = standout_test::ScopedEnv::new().set("PAGER", "more");
        assert_eq!(Pager::resolve(Some("myapp")), None);
    }

    #[cfg(unix)]
    fn pager(command: &str) -> Pager {
        Pager::named(command)
    }

    #[cfg(unix)]
    #[test]
    fn a_pager_that_reads_everything_is_paged() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen");
        let outcome = pager(&format!("cat > {}", seen.display()))
            .page_with("one\ntwo\n", &child_env(|_| false));

        assert_eq!(outcome, PagerOutcome::Paged);
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), "one\ntwo\n");
    }

    #[cfg(unix)]
    #[test]
    fn the_reader_settings_reach_the_pager() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen");
        let outcome = pager(&format!(
            "cat > /dev/null; printf '%s %s' \"$LESS\" \"$LV\" > {}",
            seen.display()
        ))
        .page_with("page", &child_env(|_| false));

        assert_eq!(outcome, PagerOutcome::Paged);
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), "FRX -c");
    }

    #[cfg(unix)]
    #[test]
    fn a_pager_that_cannot_start_says_so() {
        assert_eq!(
            pager("/nonexistent/pager 2>/dev/null").page_with("page", &[]),
            PagerOutcome::CouldNotStart
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_pager_that_stops_reading_says_so() {
        let content = "x".repeat(1024 * 1024);
        assert_eq!(
            pager("head -c 1 > /dev/null").page_with(&content, &[]),
            PagerOutcome::ReaderLeft
        );
    }
}
