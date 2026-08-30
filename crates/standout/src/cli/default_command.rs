//! Default-command resolution for naked invocations (`myapp`, `myapp
//! --verbose` — no command named).
//!
//! [`AppBuilder::default_command`](crate::cli::AppBuilder::default_command)
//! substitutes a static name; `default_command_with` resolves one per
//! invocation from non-consuming facts ([`DefaultCommandContext`]: parsed
//! root matches, read-only app state, whether stdin is a terminal — stdin is
//! never read here, only checked, so a handler's `InputChain` still sees it
//! fresh). Both funnel through [`resolve`], reached from every parse path, so
//! `dispatch_from`/`run`/`run_to_string` and `get_matches_from`/`parse_from`
//! never disagree about which command a line meant.
//!
//! Whether a command was named is decided by clap's own
//! [`ArgMatches::subcommand`], not a manual scan — so `--`, `--flag=value`,
//! short clusters, aliases, and global args behave exactly as elsewhere. See
//! ADR-0018.
//!
//! A resolver naming a command the CLI doesn't have is an application bug,
//! reported as [`UnknownDefaultCommand`] rather than a panic.

use clap::{ArgMatches, Command};
use standout_input::env::StdinReader;
use std::ffi::OsString;
use std::rc::Rc;

use crate::cli::app::find_subcommand;
use crate::cli::handler::Extensions;
use crate::cli::App;

pub(crate) enum ParseFailure {
    Clap(clap::Error),
    UnknownDefault(UnknownDefaultCommand),
}

impl App {
    pub(crate) fn parse_with_default_command(
        &self,
        cmd: &Command,
        args: &[OsString],
        stdin: &dyn StdinReader,
    ) -> Result<ArgMatches, ParseFailure> {
        match cmd.clone().try_get_matches_from(args) {
            Ok(matches) => {
                if matches.subcommand().is_some() {
                    return Ok(matches);
                }
                match self.resolve_default_command(cmd, &matches, stdin) {
                    Err(e) => Err(ParseFailure::UnknownDefault(e)),
                    Ok(None) => Ok(matches),
                    Ok(Some(name)) => self.reparse_with_command(cmd, args, &name),
                }
            }
            Err(error) => {
                if !error.use_stderr() {
                    return Err(ParseFailure::Clap(error));
                }
                let Some(probe) = self.probe_naked(cmd, args) else {
                    return Err(ParseFailure::Clap(error));
                };
                match self.resolve_default_command(cmd, &probe, stdin) {
                    Err(e) => Err(ParseFailure::UnknownDefault(e)),
                    Ok(None) => Err(ParseFailure::Clap(error)),
                    Ok(Some(name)) => self.reparse_with_command(cmd, args, &name),
                }
            }
        }
    }

    fn probe_naked(&self, cmd: &Command, args: &[OsString]) -> Option<ArgMatches> {
        if self.default_command.is_none() && self.default_command_resolver.is_none() {
            return None;
        }

        let matches = cmd
            .clone()
            .subcommand_required(false)
            .ignore_errors(true)
            .try_get_matches_from(args)
            .ok()?;

        matches.subcommand().is_none().then_some(matches)
    }

    fn reparse_with_command(
        &self,
        cmd: &Command,
        args: &[OsString],
        name: &str,
    ) -> Result<ArgMatches, ParseFailure> {
        let mut amended = args.to_vec();
        amended.insert(amended.len().min(1), OsString::from(name));

        cmd.clone()
            .try_get_matches_from(&amended)
            .map_err(ParseFailure::Clap)
    }

    pub(crate) fn resolve_default_command(
        &self,
        cmd: &Command,
        matches: &ArgMatches,
        stdin: &dyn StdinReader,
    ) -> Result<Option<String>, UnknownDefaultCommand> {
        resolve(
            cmd,
            matches,
            &self.app_state,
            self.default_command_resolver.as_ref(),
            self.default_command.as_deref(),
            stdin,
        )
    }
}

#[derive(Debug, Clone)]
pub struct UnknownDefaultCommand {
    pub name: String,
    pub known: Vec<String>,
    pub app: String,
}

impl std::fmt::Display for UnknownDefaultCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "default command resolver returned `{}`, which is not a command of `{}`. \
             Known commands: [{}]. Return `None` to decline instead of naming an unknown command.",
            self.name,
            self.app,
            self.known.join(", ")
        )
    }
}

impl std::error::Error for UnknownDefaultCommand {}

pub struct DefaultCommandContext<'a> {
    matches: &'a ArgMatches,
    app_state: &'a Extensions,
    stdin: &'a dyn StdinReader,
}

impl<'a> DefaultCommandContext<'a> {
    pub fn matches(&self) -> &'a ArgMatches {
        self.matches
    }

    pub fn app_state<T: 'static>(&self) -> Option<&'a T> {
        self.app_state.get::<T>()
    }

    pub fn stdin_is_terminal(&self) -> bool {
        self.stdin.is_terminal()
    }

    pub fn stdin_is_piped(&self) -> bool {
        !self.stdin_is_terminal()
    }
}

pub type DefaultCommandResolver = Rc<dyn Fn(&DefaultCommandContext<'_>) -> Option<String>>;

pub(crate) fn resolve(
    cmd: &Command,
    matches: &ArgMatches,
    app_state: &Extensions,
    resolver: Option<&DefaultCommandResolver>,
    static_default: Option<&str>,
    stdin: &dyn StdinReader,
) -> Result<Option<String>, UnknownDefaultCommand> {
    if matches.subcommand().is_some() {
        return Ok(None);
    }

    if let Some(resolver) = resolver {
        let ctx = DefaultCommandContext {
            matches,
            app_state,
            stdin,
        };
        if let Some(name) = resolver(&ctx) {
            return check_known_command(cmd, name).map(Some);
        }
    }

    Ok(static_default.map(String::from))
}

fn check_known_command(cmd: &Command, name: String) -> Result<String, UnknownDefaultCommand> {
    if find_subcommand(cmd, &name).is_some() {
        return Ok(name);
    }

    Err(UnknownDefaultCommand {
        name,
        known: cmd
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect(),
        app: cmd.get_name().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use standout_input::env::{MockStdin, RealStdin};

    fn app_cmd() -> Command {
        Command::new("myapp")
            .subcommand(Command::new("list").alias("ls"))
            .subcommand(Command::new("add"))
    }

    fn naked_matches() -> ArgMatches {
        app_cmd().try_get_matches_from(["myapp"]).unwrap()
    }

    fn resolver_returning(name: Option<&'static str>) -> DefaultCommandResolver {
        Rc::new(move |_ctx| name.map(String::from))
    }

    #[test]
    fn static_default_applies_to_a_naked_invocation() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            None,
            Some("list"),
            &RealStdin,
        );
        assert_eq!(resolved.unwrap().as_deref(), Some("list"));
    }

    #[test]
    fn a_selected_subcommand_is_never_naked() {
        let matches = app_cmd().try_get_matches_from(["myapp", "add"]).unwrap();
        let resolved = resolve(
            &app_cmd(),
            &matches,
            &Extensions::new(),
            Some(&resolver_returning(Some("list"))),
            Some("list"),
            &RealStdin,
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn resolver_wins_over_the_static_default() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("add"))),
            Some("list"),
            &RealStdin,
        );
        assert_eq!(resolved.unwrap().as_deref(), Some("add"));
    }

    #[test]
    fn declining_resolver_falls_back_to_the_static_default() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(None)),
            Some("list"),
            &RealStdin,
        );
        assert_eq!(resolved.unwrap().as_deref(), Some("list"));
    }

    #[test]
    fn declining_resolver_without_a_static_default_resolves_to_none() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(None)),
            None,
            &RealStdin,
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn no_default_configured_resolves_to_none() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            None,
            None,
            &RealStdin,
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn an_alias_is_a_known_command() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("ls"))),
            None,
            &RealStdin,
        );
        assert_eq!(resolved.unwrap().as_deref(), Some("ls"));
    }

    #[test]
    fn unknown_resolver_output_is_a_typed_error() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("nope"))),
            None,
            &RealStdin,
        );
        let error = resolved.expect_err("an unknown command must not resolve");
        assert_eq!(error.name, "nope");
        assert_eq!(error.app, "myapp");
        assert!(error.known.contains(&"list".to_string()));
    }

    #[test]
    fn an_unknown_resolver_output_does_not_fall_back_to_the_static_default() {
        let resolved = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("nope"))),
            Some("list"),
            &RealStdin,
        );
        assert!(resolved.is_err());
    }

    #[test]
    fn the_resolver_reads_the_root_matches() {
        let cmd = app_cmd().arg(
            clap::Arg::new("all")
                .long("all")
                .action(clap::ArgAction::SetTrue),
        );
        let resolver: DefaultCommandResolver = Rc::new(|ctx| {
            Some(if ctx.matches().get_flag("all") {
                "list".to_string()
            } else {
                "add".to_string()
            })
        });

        let naked = cmd.clone().try_get_matches_from(["myapp"]).unwrap();
        assert_eq!(
            resolve(
                &cmd,
                &naked,
                &Extensions::new(),
                Some(&resolver),
                None,
                &RealStdin
            )
            .unwrap()
            .as_deref(),
            Some("add")
        );

        let flagged = cmd
            .clone()
            .try_get_matches_from(["myapp", "--all"])
            .unwrap();
        assert_eq!(
            resolve(
                &cmd,
                &flagged,
                &Extensions::new(),
                Some(&resolver),
                None,
                &RealStdin,
            )
            .unwrap()
            .as_deref(),
            Some("list")
        );
    }

    #[test]
    fn context_reports_the_stdin_terminal_fact_without_consuming() {
        let matches = naked_matches();
        let state = Extensions::new();
        let piped = MockStdin::piped("payload");
        let ctx = DefaultCommandContext {
            matches: &matches,
            app_state: &state,
            stdin: &piped,
        };
        assert!(ctx.stdin_is_piped());
        assert!(!ctx.stdin_is_terminal());

        let terminal = MockStdin::terminal();
        let ctx = DefaultCommandContext {
            matches: &matches,
            app_state: &state,
            stdin: &terminal,
        };
        assert!(ctx.stdin_is_terminal());
    }
}
