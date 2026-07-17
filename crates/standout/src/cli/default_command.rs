//! Default-command resolution for naked invocations.
//!
//! A *naked* invocation is a successful parse that selected no subcommand:
//! `myapp`, `myapp --verbose`. Standout can substitute a command for it in two
//! additive layers:
//!
//! - [`AppBuilder::default_command`](crate::cli::App::default_command) — a
//!   static name, used for every naked invocation.
//! - [`AppBuilder::default_command_with`](crate::cli::App::default_command_with)
//!   — a resolver that picks a name per invocation from non-consuming facts.
//!
//! Both layers funnel through [`resolve`], which is the single decision point
//! for the integrated dispatch path (`dispatch_from` / `run` / `run_to_string`)
//! and for configured parsing (`get_matches_from` / `parse_from`). Consumers
//! that parse first and build dispatch state afterwards therefore see the same
//! command a naked `run()` would have selected.
//!
//! # Facts, not input
//!
//! [`DefaultCommandContext`] exposes only what is needed to choose a command:
//! the parsed root matches, read-only app state, and whether stdin is a
//! terminal. Stdin is *never* read during resolution — the terminal check is
//! the existing non-consuming [`StdinReader::is_terminal`] seam, so piped-empty
//! and piped-with-data are both distinguishable from a terminal without
//! consuming a byte. A handler's `InputChain` still reads stdin normally
//! afterwards.
//!
//! # Ordering
//!
//! Resolution runs only after an initially successful parse that produced no
//! subcommand, which keeps Clap authoritative:
//!
//! - Explicit and nested commands short-circuit resolution entirely.
//! - Help and version short-circuit inside Clap before resolution is reached.
//! - Invalid syntax stays a Clap usage error; the resolver never sees it.
//!
//! # Failure
//!
//! A resolver naming a command the CLI does not have is an application bug, and
//! is reported as a typed failure ([`UnknownDefaultCommand`]) rather than a
//! panic — per the "invalid configuration must never panic at runtime" pillar in
//! `docs/dev/design-guidelines.md`.

use clap::{ArgMatches, Command};
use standout_input::env::{DefaultStdin, StdinReader};
use std::rc::Rc;

use crate::cli::app::find_subcommand;
use crate::cli::handler::Extensions;
use crate::cli::App;

impl App {
    /// Decides which command this naked invocation means, if any.
    ///
    /// The single entry point both parse paths call, so `dispatch_from` and
    /// `get_matches_from` can never disagree. See [`resolve`] for the rules.
    ///
    /// `cmd` is the un-augmented root command; augmentation only adds flags, so
    /// it carries the same command names as what was parsed.
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
    /// The root [`ArgMatches`] of the successful naked parse.
    ///
    /// Contains global flags and root-level arguments. It has no subcommand —
    /// that is what makes the invocation naked.
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
/// `cmd` is the un-augmented root command, used to validate resolver output.
///
/// # Precedence
///
/// 1. An invocation that already selected a subcommand — explicit, nested, or
///    `help` — is not naked and resolves to `None`.
/// 2. A configured resolver runs and, if it returns a name, wins.
/// 3. Otherwise the static default (if any) applies, exactly as before.
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
    fn no_default_configured_resolves_to_none() {
        let resolved = resolve(&app_cmd(), &naked_matches(), &Extensions::new(), None, None);
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn explicit_command_is_not_naked() {
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
        let err = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("nope"))),
            None,
        )
        .expect_err("an unknown command name must not resolve");

        assert_eq!(err.name, "nope");
        assert_eq!(err.app, "myapp");
        assert_eq!(err.known, vec!["list", "add"]);

        // The diagnostic must name the bug and point at the way out.
        let msg = err.to_string();
        assert!(msg.contains("returned `nope`"), "{msg}");
        assert!(msg.contains("is not a command of `myapp`"), "{msg}");
        assert!(msg.contains("list, add"), "{msg}");
        assert!(msg.contains("Return `None`"), "{msg}");
    }

    #[test]
    fn an_unknown_resolver_output_does_not_fall_back_to_the_static_default() {
        // Silently substituting the static default would hide the bug.
        let err = resolve(
            &app_cmd(),
            &naked_matches(),
            &Extensions::new(),
            Some(&resolver_returning(Some("nope"))),
            Some("list"),
        );
        assert!(err.is_err());
    }

    #[test]
    fn resolver_reads_root_matches() {
        let cmd = app_cmd().arg(
            clap::Arg::new("all")
                .long("all")
                .action(clap::ArgAction::SetTrue),
        );
        let matches = cmd
            .clone()
            .try_get_matches_from(["myapp", "--all"])
            .unwrap();

        let resolver: DefaultCommandResolver = Rc::new(|ctx| {
            if ctx.matches().get_flag("all") {
                Some("list".to_string())
            } else {
                Some("add".to_string())
            }
        });

        let resolved = resolve(&cmd, &matches, &Extensions::new(), Some(&resolver), None);
        assert_eq!(resolved.unwrap().as_deref(), Some("list"));
    }

    #[test]
    fn resolver_reads_app_state() {
        struct Mode(&'static str);
        let mut state = Extensions::new();
        state.insert(Mode("add"));

        let resolver: DefaultCommandResolver =
            Rc::new(|ctx| ctx.app_state::<Mode>().map(|mode| mode.0.to_string()));

        let resolved = resolve(&app_cmd(), &naked_matches(), &state, Some(&resolver), None);
        assert_eq!(resolved.unwrap().as_deref(), Some("add"));
    }

    #[test]
    fn context_reports_the_stdin_terminal_fact_without_consuming() {
        let terminal = MockStdin::terminal();
        let matches = naked_matches();
        let state = Extensions::new();
        let ctx = DefaultCommandContext {
            matches: &matches,
            app_state: &state,
            stdin: &terminal,
        };
        assert!(ctx.stdin_is_terminal());
        assert!(!ctx.stdin_is_piped());

        let piped = MockStdin::piped("data");
        let ctx = DefaultCommandContext {
            matches: &matches,
            app_state: &state,
            stdin: &piped,
        };
        assert!(!ctx.stdin_is_terminal());
        assert!(ctx.stdin_is_piped());

        // Piped-but-empty is a pipe, not a terminal — knowable without reading.
        let empty = MockStdin::piped_empty();
        let ctx = DefaultCommandContext {
            matches: &matches,
            app_state: &state,
            stdin: &empty,
        };
        assert!(!ctx.stdin_is_terminal());
        assert!(ctx.stdin_is_piped());
    }
}
