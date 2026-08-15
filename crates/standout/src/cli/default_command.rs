//! Default-command resolution for naked invocations.
//!
//! A *naked* invocation is one that names no command: `myapp`,
//! `myapp --verbose`. Standout can substitute a command for it in two additive
//! layers:
//!
//! - [`AppBuilder::default_command`](crate::cli::App::default_command) — a
//!   static name, used for every naked invocation.
//! - [`AppBuilder::default_command_with`](crate::cli::App::default_command_with)
//!   — a resolver that picks a name per invocation from non-consuming facts.
//!
//! Both layers funnel through [`resolve`], and both parse paths reach it
//! through [`App::parse_with_default_command`], so the integrated dispatch path
//! (`dispatch_from` / `run` / `run_to_string`) and configured parsing
//! (`get_matches_from` / `parse_from`) can never disagree about which command a
//! line meant.
//!
//! # Clap decides what the line says
//!
//! Nothing here reads the raw argument list. Whether a command was named is
//! [`ArgMatches::subcommand`]'s answer, not a scan's — which is what keeps
//! option values, `--`, inline `--flag=value`, short clusters, aliases, and
//! global arguments behaving here exactly as they behave everywhere else in the
//! CLI. See ADR-0018.
//!
//! # Facts, not input
//!
//! [`DefaultCommandContext`] exposes what is needed to choose a command: the
//! parsed root matches, read-only app state, and whether stdin is a terminal.
//! Stdin is *never* read during resolution — the terminal check is the
//! non-consuming [`StdinReader::is_terminal`] seam, so piped-empty and
//! piped-with-data are both distinguishable from a terminal without consuming a
//! byte. A handler's `InputChain` still reads stdin normally afterwards.
//!
//! # Failure
//!
//! A resolver naming a command the CLI does not have is an application bug, and
//! is reported as a typed failure ([`UnknownDefaultCommand`]) rather than a
//! panic — per the "invalid configuration must never panic at runtime" pillar in
//! `docs/dev/design-guidelines.md`.

use clap::{ArgMatches, Command};
use standout_input::env::{DefaultStdin, StdinReader};
use std::ffi::OsString;
use std::rc::Rc;

use crate::cli::app::find_subcommand;
use crate::cli::handler::Extensions;
use crate::cli::App;

/// Why a parse produced no matches.
///
/// Both parse paths project this onto their own failure type, so a line that
/// fails means the same thing whichever entry point met it.
pub(crate) enum ParseFailure {
    /// Clap rejected the line, or wants to display something instead of
    /// parsing it. Its own `use_stderr` split says which.
    Clap(clap::Error),
    /// A resolver named a command the CLI does not have.
    UnknownDefault(UnknownDefaultCommand),
}

impl App {
    /// Parses `args`, substituting a default command for a naked line.
    ///
    /// The single parse entry point for both paths. `cmd` must be the augmented
    /// root command: it is what gets parsed, and what resolver output is
    /// validated against.
    ///
    /// # What decides
    ///
    /// Clap does, twice at most:
    ///
    /// 1. The authoritative parse runs. If it succeeds and selected a
    ///    subcommand, that is the command — no default applies.
    /// 2. If it succeeds and selected none, the line is naked and resolution
    ///    runs on those matches.
    /// 3. If it *fails*, the line may still be naked — `myapp --all` fails at a
    ///    root that has no `--all` even though `--all` belongs to the default
    ///    command, and a root that requires a subcommand rejects every naked
    ///    line by construction. [`probe_naked`](Self::probe_naked) asks Clap
    ///    itself whether a command was named, and resolution runs if none was.
    ///
    /// A substituted command means one more authoritative parse of the amended
    /// line, whose result — success or failure — is final.
    ///
    /// Arguments stay [`OsString`]s from end to end. They are the caller's
    /// verbatim `args_os()` on their way to Clap, which is the only thing that
    /// has to understand them; a path that is not valid UTF-8 is a real
    /// argument on Unix and must reach the handler as it was typed.
    pub(crate) fn parse_with_default_command(
        &self,
        cmd: &Command,
        args: &[OsString],
    ) -> Result<ArgMatches, ParseFailure> {
        match cmd.clone().try_get_matches_from(args) {
            Ok(matches) => {
                if matches.subcommand().is_some() {
                    return Ok(matches);
                }
                match self.resolve_default_command(cmd, &matches) {
                    Err(e) => Err(ParseFailure::UnknownDefault(e)),
                    Ok(None) => Ok(matches),
                    Ok(Some(name)) => self.reparse_with_command(cmd, args, &name),
                }
            }
            Err(error) => {
                // A display request (`--help`, `--version`) is Clap answering
                // the line, not refusing it: there is nothing to substitute
                // into. Clap's own typed split says which this is.
                if !error.use_stderr() {
                    return Err(ParseFailure::Clap(error));
                }
                let Some(probe) = self.probe_naked(cmd, args) else {
                    return Err(ParseFailure::Clap(error));
                };
                match self.resolve_default_command(cmd, &probe) {
                    Err(e) => Err(ParseFailure::UnknownDefault(e)),
                    Ok(None) => Err(ParseFailure::Clap(error)),
                    Ok(Some(name)) => self.reparse_with_command(cmd, args, &name),
                }
            }
        }
    }

    /// Asks Clap whether a refused line named a command.
    ///
    /// The question is only worth asking when a default command could answer
    /// it, so a CLI that configures none never pays for this parse.
    ///
    /// The probe relaxes exactly two things: `subcommand_required`, because a
    /// root that demands a command rejects the naked line this exists to
    /// detect, and error collection, because the line is already known to be
    /// invalid — the point is not to parse it but to learn whether Clap sees a
    /// command name in it. Clap does the reading either way, which is what
    /// makes the answer agree with the authoritative parse's.
    ///
    /// **Caveat, deliberate:** `ignore_errors` stops collecting at the first
    /// error, so a line that is invalid *before* the command name — `myapp
    /// --nonexistent list` — probes as naked. Such a line then takes the
    /// default and fails at the authoritative parse below, which is the parse
    /// whose diagnostic the user should be reading anyway.
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

    /// Re-parses the line with `name` inserted as its command.
    ///
    /// The name goes in after the program name, which is what
    /// [`insert_default_command`](crate::cli::insert_default_command) does for
    /// callers holding `String`s. Done here on the `OsString`s directly, so
    /// substituting a command cannot mangle the arguments around it.
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

    /// Decides which command this naked invocation means, if any.
    ///
    /// See [`resolve`] for the rules.
    pub(crate) fn resolve_default_command(
        &self,
        cmd: &Command,
        matches: &ArgMatches,
    ) -> Result<Option<String>, UnknownDefaultCommand> {
        resolve(
            cmd,
            matches,
            &self.app_state,
            self.default_command_resolver.as_ref(),
            self.default_command.as_deref(),
        )
    }
}

/// A resolver named a command the CLI does not have.
///
/// An application bug rather than a user mistake, so it carries its own
/// diagnostic instead of surfacing as a Clap usage error that would blame the
/// user for it. Each parse path maps this to its own typed failure —
/// [`RunErrorKind::DefaultCommand`](crate::cli::RunErrorKind::DefaultCommand)
/// for dispatch, a Clap error for configured parsing.
#[derive(Debug, Clone)]
pub struct UnknownDefaultCommand {
    /// The name the resolver returned.
    pub name: String,
    /// The command names it could have returned.
    pub known: Vec<String>,
    /// The root command's name.
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

/// The facts a default-command resolver may consult.
///
/// Deliberately narrow: a resolver chooses a command name, so it sees the root
/// matches (globals, flags), read-only app state, and the stdin terminal fact.
/// It cannot read stdin, mutate state, or influence parsing.
pub struct DefaultCommandContext<'a> {
    matches: &'a ArgMatches,
    app_state: &'a Extensions,
    stdin: &'a dyn StdinReader,
}

impl<'a> DefaultCommandContext<'a> {
    /// The root [`ArgMatches`] the decision is made from.
    ///
    /// Contains global flags and root-level arguments, and no subcommand — that
    /// is what makes the invocation naked. For a line Clap refused, these are
    /// the permissive probe's matches (see
    /// [`probe_naked`](crate::cli::App::probe_naked)), so a flag written after
    /// whatever Clap objected to may be absent.
    pub fn matches(&self) -> &'a ArgMatches {
        self.matches
    }

    /// Borrows app-level state of type `T`, if registered via
    /// [`app_state`](crate::cli::App::app_state).
    ///
    /// Read-only: resolution happens before dispatch and must not mutate.
    pub fn app_state<T: 'static>(&self) -> Option<&'a T> {
        self.app_state.get::<T>()
    }

    /// Whether stdin is an interactive terminal.
    ///
    /// Non-consuming. `false` covers both piped-with-data and piped-but-empty;
    /// use it to select a piped entry point without reading the pipe.
    pub fn stdin_is_terminal(&self) -> bool {
        self.stdin.is_terminal()
    }

    /// Whether stdin is redirected (a pipe, a file, `/dev/null`).
    ///
    /// The inverse of [`stdin_is_terminal`](Self::stdin_is_terminal), spelled
    /// for the common "did someone pipe to me?" policy. `true` for
    /// piped-but-empty as well as piped-with-data: emptiness is only knowable
    /// by reading, which resolution never does.
    pub fn stdin_is_piped(&self) -> bool {
        !self.stdin_is_terminal()
    }
}

/// A per-invocation default-command chooser.
///
/// Returns the command name to run for a naked invocation, or `None` to decline
/// (falling back to the static default, if one is configured).
pub type DefaultCommandResolver = Rc<dyn Fn(&DefaultCommandContext<'_>) -> Option<String>>;

/// Decides which command a naked invocation means, if any.
///
/// Returns the command name to insert after the program name, or `None` to
/// leave the argument list alone.
///
/// `cmd` is the root command, used to validate resolver output.
///
/// # Precedence
///
/// 1. An invocation that already selected a subcommand — explicit, nested, or
///    `help` — is not naked and resolves to `None`.
/// 2. A configured resolver runs and, if it returns a name, wins.
/// 3. Otherwise the static default (if any) applies.
///
/// # Errors
///
/// [`UnknownDefaultCommand`] if the resolver returns a name that is not a
/// subcommand (or alias) of `cmd`. Validation is against Clap's command names
/// rather than Standout's registered handlers, so it means the same thing on
/// the parse-only path (which has no dispatch state) and leaves partial
/// adoption coherent: a Clap command Standout does not handle resolves and
/// yields `NoMatch`, exactly as if it had been typed explicitly.
///
/// The static default is intentionally *not* validated here: it predates this
/// module and its existing behaviour (an unknown name reaches Clap) is part of
/// the contract its users already have.
pub(crate) fn resolve(
    cmd: &Command,
    matches: &ArgMatches,
    app_state: &Extensions,
    resolver: Option<&DefaultCommandResolver>,
    static_default: Option<&str>,
) -> Result<Option<String>, UnknownDefaultCommand> {
    // Anything Clap already routed to a subcommand — including `help` — takes
    // precedence over every default.
    if matches.subcommand().is_some() {
        return Ok(None);
    }

    if let Some(resolver) = resolver {
        let stdin = DefaultStdin;
        let ctx = DefaultCommandContext {
            matches,
            app_state,
            stdin: &stdin,
        };
        if let Some(name) = resolver(&ctx) {
            return check_known_command(cmd, name).map(Some);
        }
    }

    Ok(static_default.map(String::from))
}

/// Returns `name` if it is a subcommand or alias of `cmd`.
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
    use standout_input::env::MockStdin;

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
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn no_default_configured_resolves_to_none() {
        let resolved = resolve(&app_cmd(), &naked_matches(), &Extensions::new(), None, None);
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
        );
        assert!(resolved.is_err());
    }

    #[test]
    fn the_resolver_reads_the_root_matches() {
        // The decision is made from a parse, so the parse's own facts are
        // available to make it with.
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
            resolve(&cmd, &naked, &Extensions::new(), Some(&resolver), None)
                .unwrap()
                .as_deref(),
            Some("add")
        );

        let flagged = cmd
            .clone()
            .try_get_matches_from(["myapp", "--all"])
            .unwrap();
        assert_eq!(
            resolve(&cmd, &flagged, &Extensions::new(), Some(&resolver), None)
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
