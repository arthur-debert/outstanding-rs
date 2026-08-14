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
//! Both layers funnel through [`resolve`], which is the single decision point
//! for the integrated dispatch path (`dispatch_from` / `run` / `run_to_string`)
//! and for configured parsing (`get_matches_from` / `parse_from`). Consumers
//! that parse first and build dispatch state afterwards therefore see the same
//! command a naked `run()` would have selected.
//!
//! The module also owns the lexical scan that ordering rests on, because more
//! than resolution needs it: [`select`] reads one level — what does the token
//! in command position name? — and [`command_path`] walks the whole chain for
//! callers that need the command a line *targets*, help rendering among them.
//! One scan, so an option's value, an alias, and a help request cannot be read
//! one way for selection and another way for help.
//!
//! # Facts, not input
//!
//! [`DefaultCommandContext`] exposes only what is needed to choose a command:
//! read-only app state and whether stdin is a terminal (plus `std::env` for
//! env-derived facts, which never went through Clap). It does *not* expose
//! parse results: resolution happens before parsing, so there are none to hand
//! it. Stdin is *never* read during resolution — the terminal check is the
//! existing non-consuming [`StdinReader::is_terminal`] seam, so piped-empty and
//! piped-with-data are both distinguishable from a terminal without consuming a
//! byte. A handler's `InputChain` still reads stdin normally afterwards.
//!
//! # Ordering
//!
//! Selection is *name-first*: the token in command position is read as a name
//! before anything is parsed. If it names a command, that command is what the
//! line means; only when no name matches is the default command inserted and
//! the line handed to Clap. See ADR-0018.
//!
//! - A named command — explicit, nested, or `help` — is selected lexically, so
//!   the root's required arguments never fire before the name is understood.
//! - A root help or version request (`--help`, `-h`, `--version`, `-V`) is not a
//!   naked invocation: it short-circuits inside Clap, and inserting a default
//!   command would silently retarget it at that command's help.
//! - Everything after selection stays Clap's: invalid syntax is its usage error,
//!   produced by the single authoritative parse rather than guessed at from a
//!   rejected one. A resolver therefore *does* run for a line that will not
//!   parse — its answer is a function of the command name alone, not of whether
//!   the rest of the line is valid — but it cannot change the diagnostic.
//!
//! # Failure
//!
//! A resolver naming a command the CLI does not have is an application bug, and
//! is reported as a typed failure ([`UnknownDefaultCommand`]) rather than a
//! panic — per the "invalid configuration must never panic at runtime" pillar in
//! `docs/dev/design-guidelines.md`.

use clap::Command;
use standout_input::env::{DefaultStdin, StdinReader};
use std::rc::Rc;

use crate::cli::app::find_subcommand;
use crate::cli::handler::Extensions;
use crate::cli::App;

impl App {
    /// Decides which command this invocation means, if any.
    ///
    /// The single entry point both parse paths call, so `dispatch_from` and
    /// `get_matches_from` can never disagree. See [`resolve`] for the rules.
    ///
    /// `cmd` must be the augmented root command each path is about to parse
    /// with: selection reads command names off it, so a name it does not carry
    /// is not a name that line can mean.
    pub(crate) fn resolve_default_command(
        &self,
        cmd: &Command,
        args: &[String],
    ) -> Result<Option<String>, UnknownDefaultCommand> {
        resolve(
            cmd,
            args,
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
/// Deliberately narrow: a resolver chooses a command name *before* the line is
/// parsed, so it sees read-only app state and the stdin terminal fact — plus
/// `std::env` for env-derived facts, which never went through Clap. It cannot
/// see parse results, read stdin, mutate state, or influence parsing.
pub struct DefaultCommandContext<'a> {
    app_state: &'a Extensions,
    stdin: &'a dyn StdinReader,
}

impl<'a> DefaultCommandContext<'a> {
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

/// What the raw argument list says about which command was asked for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Selection<'a> {
    /// A command was named in command position. `name` is its canonical name
    /// (an alias resolves to it) and `index` is where the token sits in `args`.
    Named { name: &'a str, index: usize },
    /// A root help or version request. Clap answers it; nothing is inserted.
    RootShortCircuit,
    /// No command was named: the line is naked and may take a default command.
    Naked,
}

/// Reads the command name off the raw argument list, without parsing.
///
/// The scan walks options until it reaches command position — the first token
/// that is not an option or an option's value — because a global flag written
/// before the command (`app --loud list`) does not make the line naked. Option
/// values are skipped using `cmd`'s own arg definitions, so `--output json`
/// cannot be mistaken for a `json` command. `--` ends the options and hands the
/// rest to the positionals, so nothing after it can name a command.
pub(crate) fn select<'a>(cmd: &'a Command, args: &[String]) -> Selection<'a> {
    let mut index = 1;
    while let Some(token) = args.get(index) {
        if token == "--" {
            return Selection::Naked;
        }

        if let Some(long) = token.strip_prefix("--") {
            if long == "help" && !cmd.is_disable_help_flag_set() {
                return Selection::RootShortCircuit;
            }
            if long == "version" && has_version_flag(cmd) {
                return Selection::RootShortCircuit;
            }
            index += 1;
            // `--flag=value` carries its value inline; `--flag value` eats the
            // next token.
            if !long.contains('=') && long_takes_value(cmd, long) {
                index += 1;
            }
            continue;
        }

        // A bare `-` is the conventional name for stdin, not an option.
        if token.starts_with('-') && token.len() > 1 {
            match short_cluster(cmd, token) {
                ShortCluster::ShortCircuit => return Selection::RootShortCircuit,
                ShortCluster::EatsNextToken => index += 2,
                ShortCluster::SelfContained => index += 1,
            }
            continue;
        }

        return match find_subcommand(cmd, token) {
            Some(sub) => Selection::Named {
                name: sub.get_name(),
                index,
            },
            None => Selection::Naked,
        };
    }

    Selection::Naked
}

/// Reads the whole command chain off the raw argument list, without parsing.
///
/// [`select`] answers one level; this walks them, descending into each command
/// it names and reading the next level against *that* command's options.
/// `myapp db migrate --help` yields `["db", "migrate"]`; a line that names
/// nothing yields an empty path, which is the root.
///
/// Sharing the scan with [`select`] is the whole point. A walk that merely
/// skipped anything starting with `-` would read an option's *value* as a
/// command name — `myapp --output-file-path list --help` would render `list`'s
/// help instead of the root's — and would stride straight past the help request
/// that provoked the walk in the first place.
///
/// The command is built first so that global arguments declared on the root are
/// visible at every level: `--output json` has to eat its value wherever it is
/// written. Building is done on a copy, leaving the caller's command untouched.
pub(crate) fn command_path(cmd: &Command, args: &[String]) -> Vec<String> {
    let mut built = cmd.clone();
    built.build();

    let mut path = Vec::new();
    let mut current = &built;
    let mut rest = args;

    while let Selection::Named { name, index } = select(current, rest) {
        // `select` read this name off `current`, so the lookup answers; a
        // graceful stop is still cheaper than an invariant that can panic.
        let Some(sub) = find_subcommand(current, name) else {
            break;
        };
        path.push(name.to_string());
        current = sub;
        // The name sits at `index`, so the next level starts after it. `index`
        // is at least 1, which is what makes the walk terminate.
        rest = &rest[index..];
    }

    path
}

/// Whether `cmd` still answers `--version` / `-V` itself.
fn has_version_flag(cmd: &Command) -> bool {
    !cmd.is_disable_version_flag_set()
        && (cmd.get_version().is_some() || cmd.get_long_version().is_some())
}

/// Whether the long option `name` consumes the following token as its value.
fn long_takes_value(cmd: &Command, name: &str) -> bool {
    cmd.get_arguments()
        .find(|arg| {
            arg.get_long() == Some(name)
                || arg
                    .get_all_aliases()
                    .is_some_and(|aliases| aliases.contains(&name))
        })
        .is_some_and(|arg| arg.get_action().takes_values())
}

/// What a cluster of short options (`-abc`) does to the scan position.
enum ShortCluster {
    /// It asked for root help or version.
    ShortCircuit,
    /// Its last option takes a value, which is the next token (`-o json`).
    EatsNextToken,
    /// Every value it needs is inside the cluster (`-ojson`, or no values).
    SelfContained,
}

/// Classifies a short-option cluster the way Clap reads one.
fn short_cluster(cmd: &Command, token: &str) -> ShortCluster {
    let mut chars = token.chars().skip(1).peekable();
    while let Some(short) = chars.next() {
        if short == 'h' && !cmd.is_disable_help_flag_set() {
            return ShortCluster::ShortCircuit;
        }
        if short == 'V' && has_version_flag(cmd) {
            return ShortCluster::ShortCircuit;
        }
        let takes_value = cmd
            .get_arguments()
            .find(|arg| arg.get_short() == Some(short))
            .is_some_and(|arg| arg.get_action().takes_values());
        if takes_value {
            // The rest of the cluster is the value if there is any rest.
            return if chars.peek().is_some() {
                ShortCluster::SelfContained
            } else {
                ShortCluster::EatsNextToken
            };
        }
    }
    ShortCluster::SelfContained
}

/// Decides which command a naked invocation means, if any.
///
/// Returns the command name to insert after the program name, or `None` to
/// leave the argument list alone.
///
/// `cmd` is the augmented root command about to be parsed: it supplies the
/// command names selection reads, the option shapes the scan skips over, and
/// the validation of resolver output.
///
/// # Precedence
///
/// 1. A line that names a command in command position — explicit, nested, or
///    `help` — is not naked and resolves to `None`.
/// 2. A root help or version request is not naked either: Clap answers it, and
///    inserting a default command would retarget it at that command's help.
/// 3. A configured resolver runs and, if it returns a name, wins.
/// 4. Otherwise the static default (if any) applies.
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
    args: &[String],
    app_state: &Extensions,
    resolver: Option<&DefaultCommandResolver>,
    static_default: Option<&str>,
) -> Result<Option<String>, UnknownDefaultCommand> {
    match select(cmd, args) {
        Selection::Named { .. } | Selection::RootShortCircuit => return Ok(None),
        Selection::Naked => {}
    }

    if let Some(resolver) = resolver {
        let stdin = DefaultStdin;
        let ctx = DefaultCommandContext {
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
            .version("1.0")
            .subcommand(Command::new("list").alias("ls"))
            .subcommand(Command::new("add"))
    }

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn resolver_returning(name: Option<&'static str>) -> DefaultCommandResolver {
        Rc::new(move |_ctx| name.map(String::from))
    }

    fn resolve_for(args: &[&str], static_default: Option<&str>) -> Option<String> {
        resolve(
            &app_cmd(),
            &argv(args),
            &Extensions::new(),
            None,
            static_default,
        )
        .unwrap()
    }

    // --- name-first selection ---------------------------------------------

    #[test]
    fn a_command_name_in_command_position_is_selected() {
        let cmd = app_cmd();
        assert_eq!(
            select(&cmd, &argv(&["myapp", "add"])),
            Selection::Named {
                name: "add",
                index: 1
            }
        );
    }

    #[test]
    fn an_alias_selects_its_canonical_command() {
        let cmd = app_cmd();
        assert_eq!(
            select(&cmd, &argv(&["myapp", "ls"])),
            Selection::Named {
                name: "list",
                index: 1
            }
        );
    }

    #[test]
    fn options_written_before_the_command_do_not_hide_it() {
        // A global flag ahead of the command name is still that command.
        let cmd = app_cmd().arg(
            clap::Arg::new("loud")
                .long("loud")
                .global(true)
                .action(clap::ArgAction::SetTrue),
        );
        assert_eq!(
            select(&cmd, &argv(&["myapp", "--loud", "list"])),
            Selection::Named {
                name: "list",
                index: 2
            }
        );
    }

    #[test]
    fn an_option_value_is_never_read_as_a_command_name() {
        // `--pick list` names no command: `list` is the option's value.
        let cmd = app_cmd().arg(
            clap::Arg::new("pick")
                .long("pick")
                .short('p')
                .action(clap::ArgAction::Set),
        );
        assert_eq!(
            select(&cmd, &argv(&["myapp", "--pick", "list"])),
            Selection::Naked
        );
        assert_eq!(
            select(&cmd, &argv(&["myapp", "-p", "list"])),
            Selection::Naked
        );
        // Attached values leave the cluster self-contained.
        assert_eq!(
            select(&cmd, &argv(&["myapp", "-plist", "add"])),
            Selection::Named {
                name: "add",
                index: 2
            }
        );
        assert_eq!(
            select(&cmd, &argv(&["myapp", "--pick=x", "add"])),
            Selection::Named {
                name: "add",
                index: 2
            }
        );
    }

    #[test]
    fn nothing_after_the_escape_can_name_a_command() {
        let cmd = app_cmd();
        assert_eq!(
            select(&cmd, &argv(&["myapp", "--", "list"])),
            Selection::Naked
        );
    }

    #[test]
    fn a_word_that_names_nothing_leaves_the_line_naked() {
        let cmd = app_cmd();
        assert_eq!(
            select(&cmd, &argv(&["myapp", "whatever"])),
            Selection::Naked
        );
    }

    #[test]
    fn root_help_and_version_short_circuit() {
        let cmd = app_cmd();
        for args in [
            &["myapp", "--help"][..],
            &["myapp", "-h"][..],
            &["myapp", "--version"][..],
            &["myapp", "-V"][..],
        ] {
            assert_eq!(
                select(&cmd, &argv(args)),
                Selection::RootShortCircuit,
                "{args:?}"
            );
        }
    }

    #[test]
    fn a_command_before_a_help_flag_still_wins() {
        // `myapp list --help` is help *for `list`*, so the command is selected.
        let cmd = app_cmd();
        assert_eq!(
            select(&cmd, &argv(&["myapp", "list", "--help"])),
            Selection::Named {
                name: "list",
                index: 1
            }
        );
    }

    // --- the whole command chain -------------------------------------------

    /// A nested CLI with the two option shapes a naive scan gets wrong: a
    /// global long option that takes a value, and a short one that takes a
    /// value.
    fn nested_cmd() -> Command {
        Command::new("myapp")
            .version("1.0")
            .arg(
                clap::Arg::new("out")
                    .long("output")
                    .global(true)
                    .action(clap::ArgAction::Set),
            )
            .arg(
                clap::Arg::new("file")
                    .short('f')
                    .action(clap::ArgAction::Set),
            )
            .subcommand(Command::new("list").alias("ls"))
            .subcommand(Command::new("db").subcommand(Command::new("migrate")))
    }

    #[test]
    fn the_chain_is_read_one_level_at_a_time() {
        assert_eq!(
            command_path(&nested_cmd(), &argv(&["myapp", "db", "migrate", "--help"])),
            vec!["db".to_string(), "migrate".to_string()]
        );
    }

    #[test]
    fn a_chain_step_resolves_an_alias_to_its_command() {
        assert_eq!(
            command_path(&nested_cmd(), &argv(&["myapp", "ls", "--help"])),
            vec!["list".to_string()]
        );
    }

    #[test]
    fn an_option_value_is_never_read_as_a_step_in_the_chain() {
        // `--output list` names no command: `list` is the option's value, so
        // the help request is the root's. A scan that only skipped tokens
        // starting with `-` would answer `list` here.
        assert!(command_path(
            &nested_cmd(),
            &argv(&["myapp", "--output", "list", "--help"])
        )
        .is_empty());

        // The same for a short option that takes a value.
        assert!(command_path(&nested_cmd(), &argv(&["myapp", "-f", "list", "--help"])).is_empty());

        // An attached value leaves the token self-contained, so what follows is
        // read as a command again.
        assert_eq!(
            command_path(
                &nested_cmd(),
                &argv(&["myapp", "--output=json", "list", "--help"])
            ),
            vec!["list".to_string()]
        );
    }

    #[test]
    fn a_global_option_keeps_its_value_at_every_level() {
        // `--output` is declared on the root and reaches `db`, so `migrate` is
        // its value and the help request is `db`'s — not `db migrate`'s.
        assert_eq!(
            command_path(
                &nested_cmd(),
                &argv(&["myapp", "db", "--output", "migrate", "--help"])
            ),
            vec!["db".to_string()]
        );
    }

    #[test]
    fn the_walk_stops_where_the_help_request_is() {
        // Help was asked for before any command was named, so it is the root's;
        // a scan that skipped flags would keep walking and answer `list`.
        for args in [
            &["myapp", "--help", "list"][..],
            &["myapp", "-h", "list"][..],
        ] {
            assert!(
                command_path(&nested_cmd(), &argv(args)).is_empty(),
                "{args:?}"
            );
        }
    }

    #[test]
    fn nothing_after_the_escape_is_a_step_in_the_chain() {
        assert!(command_path(&nested_cmd(), &argv(&["myapp", "--", "list", "--help"])).is_empty());
    }

    #[test]
    fn a_word_that_names_nothing_ends_the_chain() {
        assert_eq!(
            command_path(&nested_cmd(), &argv(&["myapp", "db", "nope", "--help"])),
            vec!["db".to_string()]
        );
    }

    // --- resolution --------------------------------------------------------

    #[test]
    fn static_default_applies_to_a_naked_invocation() {
        assert_eq!(
            resolve_for(&["myapp"], Some("list")).as_deref(),
            Some("list")
        );
    }

    #[test]
    fn no_default_configured_resolves_to_none() {
        assert_eq!(resolve_for(&["myapp"], None), None);
    }

    #[test]
    fn a_named_command_is_not_naked() {
        let resolved = resolve(
            &app_cmd(),
            &argv(&["myapp", "add"]),
            &Extensions::new(),
            Some(&resolver_returning(Some("list"))),
            Some("list"),
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn a_root_help_request_is_not_naked() {
        // Inserting a default here would render *that command's* help instead.
        let resolved = resolve(
            &app_cmd(),
            &argv(&["myapp", "--help"]),
            &Extensions::new(),
            Some(&resolver_returning(Some("list"))),
            Some("list"),
        );
        assert_eq!(resolved.unwrap(), None);
    }

    #[test]
    fn invalid_syntax_still_resolves_a_default() {
        // Selection is a function of the command name alone: a line that will
        // not parse is still naked, and Clap reports the usage error after.
        assert_eq!(
            resolve_for(&["myapp", "--nonexistent"], Some("list")).as_deref(),
            Some("list")
        );
    }

    #[test]
    fn resolver_wins_over_the_static_default() {
        let resolved = resolve(
            &app_cmd(),
            &argv(&["myapp"]),
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
            &argv(&["myapp"]),
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
            &argv(&["myapp"]),
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
            &argv(&["myapp"]),
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
            &argv(&["myapp"]),
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
            &argv(&["myapp"]),
            &Extensions::new(),
            Some(&resolver_returning(Some("nope"))),
            Some("list"),
        );
        assert!(err.is_err());
    }

    #[test]
    fn resolver_reads_app_state() {
        struct Mode(&'static str);
        let mut state = Extensions::new();
        state.insert(Mode("add"));

        let resolver: DefaultCommandResolver =
            Rc::new(|ctx| ctx.app_state::<Mode>().map(|mode| mode.0.to_string()));

        let resolved = resolve(&app_cmd(), &argv(&["myapp"]), &state, Some(&resolver), None);
        assert_eq!(resolved.unwrap().as_deref(), Some("add"));
    }

    #[test]
    fn context_reports_the_stdin_terminal_fact_without_consuming() {
        let terminal = MockStdin::terminal();
        let state = Extensions::new();
        let ctx = DefaultCommandContext {
            app_state: &state,
            stdin: &terminal,
        };
        assert!(ctx.stdin_is_terminal());
        assert!(!ctx.stdin_is_piped());

        let piped = MockStdin::piped("data");
        let ctx = DefaultCommandContext {
            app_state: &state,
            stdin: &piped,
        };
        assert!(!ctx.stdin_is_terminal());
        assert!(ctx.stdin_is_piped());

        // Piped-but-empty is a pipe, not a terminal — knowable without reading.
        let empty = MockStdin::piped_empty();
        let ctx = DefaultCommandContext {
            app_state: &state,
            stdin: &empty,
        };
        assert!(!ctx.stdin_is_terminal());
        assert!(ctx.stdin_is_piped());
    }
}
