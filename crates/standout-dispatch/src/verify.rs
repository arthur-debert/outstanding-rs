use clap::{ArgAction, Command};
use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgKind {
    Flag,
    RequiredArg,
    OptionalArg,
    VecArg,
}
impl fmt::Display for ArgKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgKind::Flag => write!(f, "boolean flag"),
            ArgKind::RequiredArg => write!(f, "required argument"),
            ArgKind::OptionalArg => write!(f, "optional argument"),
            ArgKind::VecArg => write!(f, "repeatable argument"),
        }
    }
}
#[derive(Debug, Clone)]
pub struct ExpectedArg {
    pub cli_name: String,
    pub rust_name: String,
    pub kind: ArgKind,
}
impl ExpectedArg {
    pub fn flag(cli_name: impl Into<String>, rust_name: impl Into<String>) -> Self {
        Self {
            cli_name: cli_name.into(),
            rust_name: rust_name.into(),
            kind: ArgKind::Flag,
        }
    }
    pub fn required_arg(cli_name: impl Into<String>, rust_name: impl Into<String>) -> Self {
        Self {
            cli_name: cli_name.into(),
            rust_name: rust_name.into(),
            kind: ArgKind::RequiredArg,
        }
    }
    pub fn optional_arg(cli_name: impl Into<String>, rust_name: impl Into<String>) -> Self {
        Self {
            cli_name: cli_name.into(),
            rust_name: rust_name.into(),
            kind: ArgKind::OptionalArg,
        }
    }
    pub fn vec_arg(cli_name: impl Into<String>, rust_name: impl Into<String>) -> Self {
        Self {
            cli_name: cli_name.into(),
            rust_name: rust_name.into(),
            kind: ArgKind::VecArg,
        }
    }
}
#[derive(Debug, Clone)]
pub enum ArgMismatch {
    MissingInCommand {
        cli_name: String,
        rust_name: String,
        expected_kind: ArgKind,
    },
    NotAFlag {
        cli_name: String,
        actual_action: String,
    },
    UnexpectedFlag {
        cli_name: String,
        expected_kind: ArgKind,
    },
    RequiredMismatch {
        cli_name: String,
        handler_required: bool,
        command_required: bool,
    },
}
impl fmt::Display for ArgMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgMismatch::MissingInCommand {
                cli_name,
                rust_name,
                expected_kind,
            } => {
                writeln!(f, "  Argument `{cli_name}` (parameter `{rust_name}`):")?;
                writeln!(f, "    - Handler expects: {expected_kind}")?;
                writeln!(f, "    - Command: argument not defined")?;
                writeln!(f)?;
                writeln!(f, "    Fix: Add the argument to your clap Command:")?;
                match expected_kind {
                    ArgKind::Flag => {
                        writeln!(
                            f,
                            "      .arg(Arg::new(\"{cli_name}\").long(\"{cli_name}\").action(ArgAction::SetTrue))"
                        )
                    }
                    ArgKind::RequiredArg => {
                        writeln!(
                            f,
                            "      .arg(Arg::new(\"{cli_name}\").long(\"{cli_name}\").required(true))"
                        )
                    }
                    ArgKind::OptionalArg => {
                        writeln!(
                            f,
                            "      .arg(Arg::new(\"{cli_name}\").long(\"{cli_name}\"))"
                        )
                    }
                    ArgKind::VecArg => {
                        writeln!(
                            f,
                            "      .arg(Arg::new(\"{cli_name}\").long(\"{cli_name}\").action(ArgAction::Append))"
                        )
                    }
                }
            }
            ArgMismatch::NotAFlag {
                cli_name,
                actual_action,
            } => {
                writeln!(f, "  Flag `{cli_name}`:")?;
                writeln!(f, "    - Handler expects: boolean flag (via get_flag)")?;
                writeln!(f, "    - Command defines: {actual_action}")?;
                writeln!(f)?;
                writeln!(f, "    Fix: Change the argument's action to SetTrue:")?;
                writeln!(
                    f,
                    "      .arg(Arg::new(\"{cli_name}\").long(\"{cli_name}\").action(ArgAction::SetTrue))"
                )
            }
            ArgMismatch::UnexpectedFlag {
                cli_name,
                expected_kind,
            } => {
                writeln!(f, "  Argument `{cli_name}`:")?;
                writeln!(f, "    - Handler expects: {expected_kind}")?;
                writeln!(f, "    - Command defines: boolean flag (SetTrue/SetFalse)")?;
                writeln!(f)?;
                writeln!(f, "    Fix: Either:")?;
                writeln!(
                    f,
                    "      - Change the handler parameter to `#[flag] {cli_name}: bool`"
                )?;
                writeln!(
                    f,
                    "      - Or change the command's action: .action(ArgAction::Set)"
                )
            }
            ArgMismatch::RequiredMismatch {
                cli_name,
                handler_required,
                command_required: _,
            } => {
                writeln!(f, "  Argument `{cli_name}`:")?;
                if *handler_required {
                    writeln!(f, "    - Handler expects: required argument")?;
                    writeln!(f, "    - Command defines: optional argument")?;
                    writeln!(f)?;
                    writeln!(f, "    Fix: Either:")?;
                    writeln!(
                        f,
                        "      - Change handler to `#[arg] {}: Option<T>`",
                        cli_name.replace('-', "_")
                    )?;
                    writeln!(f, "      - Or add `.required(true)` to the command arg")
                } else {
                    writeln!(f, "    - Handler expects: optional argument (Option<T>)")?;
                    writeln!(f, "    - Command defines: required argument")?;
                    writeln!(f)?;
                    writeln!(f, "    Fix: Either:")?;
                    writeln!(
                        f,
                        "      - Change handler to `#[arg] {}: T` (not Option)",
                        cli_name.replace('-', "_")
                    )?;
                    writeln!(
                        f,
                        "      - Or remove `.required(true)` from the command arg"
                    )
                }
            }
        }
    }
}
#[derive(Debug, Clone)]
pub struct HandlerMismatchError {
    pub handler_name: String,
    pub command_name: Option<String>,
    pub mismatches: Vec<ArgMismatch>,
}
impl std::error::Error for HandlerMismatchError {}
impl fmt::Display for HandlerMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cmd_desc = self
            .command_name
            .as_ref()
            .map(|n| format!(" for command `{n}`"))
            .unwrap_or_default();
        writeln!(
            f,
            "Handler `{}` is incompatible with clap Command{cmd_desc}",
            self.handler_name
        )?;
        writeln!(f)?;
        for mismatch in &self.mismatches {
            write!(f, "{mismatch}")?;
        }
        Ok(())
    }
}
fn is_flag_action(action: &ArgAction) -> bool {
    matches!(action, ArgAction::SetTrue | ArgAction::SetFalse)
}
fn describe_action(action: &ArgAction) -> String {
    match action {
        ArgAction::Set => "ArgAction::Set (single value)".to_string(),
        ArgAction::Append => "ArgAction::Append (multiple values)".to_string(),
        ArgAction::SetTrue => "ArgAction::SetTrue (boolean flag)".to_string(),
        ArgAction::SetFalse => "ArgAction::SetFalse (boolean flag)".to_string(),
        ArgAction::Count => "ArgAction::Count (counter)".to_string(),
        ArgAction::Help => "ArgAction::Help".to_string(),
        ArgAction::HelpShort => "ArgAction::HelpShort".to_string(),
        ArgAction::HelpLong => "ArgAction::HelpLong".to_string(),
        ArgAction::Version => "ArgAction::Version".to_string(),
        _ => "unknown action".to_string(),
    }
}
pub fn verify_handler_args(
    command: &Command,
    handler_name: &str,
    expected: &[ExpectedArg],
) -> Result<(), HandlerMismatchError> {
    let mut mismatches = Vec::new();
    for exp in expected {
        let arg = command
            .get_arguments()
            .find(|a| a.get_id() == exp.cli_name.as_str());
        match arg {
            None => {
                mismatches.push(ArgMismatch::MissingInCommand {
                    cli_name: exp.cli_name.clone(),
                    rust_name: exp.rust_name.clone(),
                    expected_kind: exp.kind.clone(),
                });
            }
            Some(arg) => {
                let action = arg.get_action();
                match exp.kind {
                    ArgKind::Flag => {
                        if !is_flag_action(action) {
                            mismatches.push(ArgMismatch::NotAFlag {
                                cli_name: exp.cli_name.clone(),
                                actual_action: describe_action(action),
                            });
                        }
                    }
                    ArgKind::RequiredArg => {
                        if is_flag_action(action) {
                            mismatches.push(ArgMismatch::UnexpectedFlag {
                                cli_name: exp.cli_name.clone(),
                                expected_kind: exp.kind.clone(),
                            });
                        } else if matches!(action, ArgAction::Count) {
                        } else if !arg.is_required_set() && arg.get_default_values().is_empty() {
                            mismatches.push(ArgMismatch::RequiredMismatch {
                                cli_name: exp.cli_name.clone(),
                                handler_required: true,
                                command_required: false,
                            });
                        }
                    }
                    ArgKind::OptionalArg => {
                        if is_flag_action(action) {
                            mismatches.push(ArgMismatch::UnexpectedFlag {
                                cli_name: exp.cli_name.clone(),
                                expected_kind: exp.kind.clone(),
                            });
                        } else if arg.is_required_set() {
                            mismatches.push(ArgMismatch::RequiredMismatch {
                                cli_name: exp.cli_name.clone(),
                                handler_required: false,
                                command_required: true,
                            });
                        }
                    }
                    ArgKind::VecArg => {
                        if is_flag_action(action) {
                            mismatches.push(ArgMismatch::UnexpectedFlag {
                                cli_name: exp.cli_name.clone(),
                                expected_kind: exp.kind.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(HandlerMismatchError {
            handler_name: handler_name.to_string(),
            command_name: Some(command.get_name().to_string()),
            mismatches,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;
    #[test]
    fn test_verify_matching_flag() {
        let command = Command::new("test").arg(
            Arg::new("verbose")
                .long("verbose")
                .action(ArgAction::SetTrue),
        );
        let expected = vec![ExpectedArg::flag("verbose", "verbose")];
        assert!(verify_handler_args(&command, "test_handler", &expected).is_ok());
    }
    #[test]
    fn test_verify_missing_arg() {
        let command = Command::new("test");
        let expected = vec![ExpectedArg::flag("verbose", "verbose")];
        let err = verify_handler_args(&command, "test_handler", &expected).unwrap_err();
        assert_eq!(err.mismatches.len(), 1);
        assert!(matches!(
            &err.mismatches[0],
            ArgMismatch::MissingInCommand { cli_name, .. } if cli_name == "verbose"
        ));
    }
    #[test]
    fn test_verify_wrong_action_for_flag() {
        let command =
            Command::new("test").arg(Arg::new("verbose").long("verbose").action(ArgAction::Set));
        let expected = vec![ExpectedArg::flag("verbose", "verbose")];
        let err = verify_handler_args(&command, "test_handler", &expected).unwrap_err();
        assert_eq!(err.mismatches.len(), 1);
        assert!(matches!(&err.mismatches[0], ArgMismatch::NotAFlag { .. }));
    }
    #[test]
    fn test_verify_required_mismatch() {
        let command =
            Command::new("test").arg(Arg::new("name").long("name").action(ArgAction::Set));
        let expected = vec![ExpectedArg::required_arg("name", "name")];
        let err = verify_handler_args(&command, "test_handler", &expected).unwrap_err();
        assert_eq!(err.mismatches.len(), 1);
        assert!(matches!(
            &err.mismatches[0],
            ArgMismatch::RequiredMismatch {
                handler_required: true,
                command_required: false,
                ..
            }
        ));
    }
    #[test]
    fn test_verify_optional_matches() {
        let command =
            Command::new("test").arg(Arg::new("filter").long("filter").action(ArgAction::Set));
        let expected = vec![ExpectedArg::optional_arg("filter", "filter")];
        assert!(verify_handler_args(&command, "test_handler", &expected).is_ok());
    }
    #[test]
    fn test_error_message_formatting() {
        let command =
            Command::new("list").arg(Arg::new("verbose").long("verbose").action(ArgAction::Set));
        let expected = vec![ExpectedArg::flag("verbose", "verbose")];
        let err = verify_handler_args(&command, "list_handler", &expected).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Handler `list_handler`"));
        assert!(msg.contains("command `list`"));
        assert!(msg.contains("Flag `verbose`"));
        assert!(msg.contains("ArgAction::SetTrue"));
    }
}
