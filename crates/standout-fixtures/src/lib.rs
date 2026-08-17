//! The downstream-shaped fixture the help and rendering suites assert against.
//!
//! Every help test used to hand-build its own [`App`] *and* its own
//! [`clap::Command`] and keep the two in sync by hand, so three files
//! re-declared a near-identical `--staged`/`range` shape and each one could
//! drift from the others. Worse, the copies were all *smaller* than a real
//! CLI: none of them carried the properties that co-occur downstream — an app
//! theme beside enabled help handling, an enumerated option beside a presence
//! flag, subcommands beside a positional — which is exactly the combination
//! the themed-help defect cluster lived in.
//!
//! This crate is one definition of that shape, handed out as a matched pair:
//! [`Fixture::app`] and [`Fixture::command`] are built together from the same
//! [`Downstream`] configuration, so a caller cannot desynchronize them the way
//! two hand-written functions could.
//!
//! # What the shape carries, and why
//!
//! - **Three subcommands**, so the COMMANDS section and its grouping render
//!   rather than being suppressed as they are on a flat root.
//! - **A positional with an explicit value name** (`RANGE`) and **a `SetTrue`
//!   flag** (`--staged`), the pair whose rows must not look alike: a presence
//!   flag takes no value and advertises no possible values (#301).
//! - **A valued option with a metavar** (`--threshold <RATIO>`) and **a valued
//!   option without one** (`--pattern`), which must still show clap's fallback
//!   metavar (#302).
//! - **An enumerated option with a default** (`--summary`), whose default and
//!   possible-value rows are what a template with no slot for them drops
//!   (#298). Its vocabulary — `brief`/`full`/`none` — is deliberately unlike
//!   standout's own `--output`, so an assertion can tell the app's enumerated
//!   option from the framework's.
//! - **A long option name** (`--output-file-path`, standout's own) that
//!   exceeds the old fixed name column, which is what collided with its
//!   description (#297).
//! - **An app theme that is deliberately incomplete**: it defines the app's
//!   own vocabulary and none of the tags the help template emits, the shape
//!   that rendered `[header?]` markers when the app theme replaced the help
//!   theme instead of overlaying it (#303).
//! - **`help_handling(true)` and registered topics**, so the `help` word has
//!   somewhere to go and the COMMANDS section has a reason to list it (#299).
//!
//! # Shapes
//!
//! [`Downstream::flat`] drops the subcommands and makes "a range or `--staged`"
//! a required [`ArgGroup`] — the root whose requirements fire before clap will
//! route a bare word, which is where the `help` word was unreachable. It is the
//! same argument declaration as the nested shape, so the two cannot disagree
//! about what an option is called.
//!
//! ```
//! use standout_fixtures::downstream;
//!
//! let fixture = downstream().build();
//! assert_eq!(fixture.command().get_name(), "lookma");
//! ```

use clap::{Arg, ArgAction, ArgGroup, Command};
use console::Style;
use serde_json::json;
use standout::cli::{App, Output};
use standout::topics::{Topic, TopicType};
use standout::Theme;

/// The name the fixture's root command answers to.
pub const NAME: &str = "lookma";

/// Which root shape the fixture wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A root with subcommands: the shape that has a COMMANDS section.
    Nested,
    /// A flat root whose positional and flag form one required group: the
    /// shape whose requirements fire before a bare word can be routed.
    Flat,
}

/// Starts a [`Downstream`] configuration with the defaults below.
pub fn downstream() -> Downstream {
    Downstream::new()
}

/// The configuration one [`Fixture`] is built from.
///
/// The defaults are the full downstream shape: subcommands, the incomplete app
/// theme, help handling, the `help` word opted in, and topics. Each modifier
/// removes exactly one property, so a test that needs the shape *without* a
/// theme (or without the word) still gets the same arguments and the same
/// handlers as everybody else.
#[derive(Debug, Clone)]
#[must_use = "a Downstream is a configuration; call build() for the fixture"]
pub struct Downstream {
    shape: Shape,
    help_word: bool,
    theme: bool,
    topics: bool,
}

impl Downstream {
    /// The full shape: nested, themed, with the `help` word opted in and
    /// topics registered.
    pub fn new() -> Self {
        Self {
            shape: Shape::Nested,
            help_word: true,
            theme: true,
            topics: true,
        }
    }

    /// Drops the subcommands and makes the positional and `--staged` a
    /// required group.
    pub fn flat(mut self) -> Self {
        self.shape = Shape::Flat;
        self
    }

    /// Leaves `help_word` off, so the install policy decides the word on the
    /// command's shape alone.
    pub fn without_help_word(mut self) -> Self {
        self.help_word = false;
        self
    }

    /// Leaves the app theme unset, so help renders from the default help theme
    /// with nothing overlaid.
    pub fn without_theme(mut self) -> Self {
        self.theme = false;
        self
    }

    /// Registers no topics, which on a flat root leaves the `help` word with
    /// nowhere to go — the shape whose COMMANDS section would otherwise list
    /// only the machinery that printed it.
    pub fn without_topics(mut self) -> Self {
        self.topics = false;
        self
    }

    /// Builds the matched [`App`]/[`Command`] pair.
    pub fn build(&self) -> Fixture {
        Fixture {
            app: self.app(),
            command: self.command(),
        }
    }

    fn app(&self) -> App {
        let mut builder = App::builder().help_handling(true).help_word(self.help_word);

        if self.topics {
            builder = builder
                .add_topic(Topic::new(
                    "Ranges",
                    "A range is two revisions separated by two dots.",
                    TopicType::Text,
                    Some("ranges".to_string()),
                ))
                .add_topic(Topic::new(
                    "Naming",
                    "A change is named for what it does, not for the files it touches.",
                    TopicType::Text,
                    Some("naming".to_string()),
                ));
        }

        if self.theme {
            builder = builder.theme(incomplete_theme());
        }

        let builder = match self.shape {
            Shape::Nested => builder
                .command(
                    "review",
                    |m, _ctx| {
                        Ok(Output::Render(json!({
                            "hunk": m.get_one::<String>("hunk").cloned().unwrap_or_default(),
                        })))
                    },
                    "reviewed {{ hunk }}",
                )
                .unwrap()
                .command("stat", |_m, _ctx| Ok(Output::Render(json!({}))), "stat")
                .unwrap()
                .command(
                    "export",
                    |_m, _ctx| Ok(Output::Render(json!({}))),
                    "exported",
                )
                .unwrap(),
            // A flat root's only handler is the root's, as `command_with("", …)`
            // apps are shaped. It echoes the range so a test can tell a word
            // that reached the positional from one the framework intercepted.
            Shape::Flat => builder
                .command(
                    "",
                    |m, _ctx| {
                        Ok(Output::Render(json!({
                            "range": m.get_one::<String>("range").cloned().unwrap_or_default(),
                        })))
                    },
                    "range={{ range }}",
                )
                .unwrap(),
        };

        builder.build().unwrap()
    }

    fn command(&self) -> Command {
        let root = Command::new(NAME)
            .about("Diff a git range")
            .long_about("Diff a git range.\n\nNames a change the way a human would.")
            .arg(
                Arg::new("range")
                    .value_name("RANGE")
                    .help("Git range to diff, e.g. main..HEAD"),
            )
            .arg(
                Arg::new("staged")
                    .long("staged")
                    .action(ArgAction::SetTrue)
                    .help("Diff the staged changes"),
            )
            .arg(
                Arg::new("threshold")
                    .long("threshold")
                    .value_name("RATIO")
                    .action(ArgAction::Set)
                    .help("Move/rename similarity threshold"),
            )
            .arg(
                Arg::new("color")
                    .short('c')
                    .long("color")
                    .value_name("BOOL")
                    .action(ArgAction::Set)
                    .value_parser(clap::builder::BoolishValueParser::new())
                    .help("Enable ANSI color"),
            )
            .arg(
                Arg::new("pattern")
                    .short('p')
                    .long("pattern")
                    .action(ArgAction::Set)
                    .help("Only diff paths matching this glob"),
            )
            .arg(
                Arg::new("summary")
                    .long("summary")
                    .value_name("STYLE")
                    .action(ArgAction::Set)
                    .default_value("brief")
                    .value_parser(["brief", "full", "none"])
                    .help("How much of each change to describe"),
            );

        match self.shape {
            Shape::Nested => root
                .subcommand(
                    Command::new("review")
                        .about("Review a range hunk by hunk")
                        .arg(
                            Arg::new("hunk")
                                .value_name("HUNK")
                                .help("Start at this hunk"),
                        ),
                )
                .subcommand(Command::new("stat").about("Summarize a range by file"))
                .subcommand(Command::new("export").about("Write the review to a file")),
            Shape::Flat => root.group(
                ArgGroup::new("target")
                    .args(["range", "staged"])
                    .required(true),
            ),
        }
    }
}

impl Default for Downstream {
    fn default() -> Self {
        Self::new()
    }
}

/// A built fixture: an [`App`] and the [`Command`] it was built for.
///
/// The pair is the point. `App::run` and `App::get_matches_from` both take the
/// command as a separate argument, so a test file that builds the two by hand
/// can — and did — let them drift apart. Here they come from one
/// [`Downstream`] and are handed out together.
pub struct Fixture {
    app: App,
    command: Command,
}

impl Fixture {
    /// The app, for the entry points that borrow it.
    pub fn app(&self) -> &App {
        &self.app
    }

    /// The command, cloned — the run entry points consume it, and a test
    /// commonly makes more than one call.
    pub fn command(&self) -> Command {
        self.command.clone()
    }
}

/// The app's own vocabulary, and none of the help template's.
///
/// A downstream app themes what *it* renders — its diff nodes, its added and
/// deleted lines — and has no reason to know that standout's help template
/// emits `[header]`, `[metavar]`, or `[values]`. That gap is the fixture's
/// point: an app theme has to overlay the default help theme, because a theme
/// that replaced it would leave every help tag unresolved.
pub fn incomplete_theme() -> Theme {
    Theme::new()
        .add("node", Style::new().cyan().bold())
        .add("added", Style::new().green())
        .add("deleted", Style::new().red())
}

#[cfg(test)]
mod tests {
    use super::*;
    use standout::cli::{default_help_theme, HelpResult};

    fn help_page(fixture: &Fixture, args: &[&str]) -> String {
        match fixture.app().get_matches_from(fixture.command(), args) {
            HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
            other => panic!("expected rendered help, got: {other:?}"),
        }
    }

    /// Every element the fixture promises, asserted on the declaration rather
    /// than on a rendered page — a later edit that hollows the shape out fails
    /// here, beside the list that says why each element is present, instead of
    /// silently weakening every suite that depends on it.
    #[test]
    fn the_shape_carries_every_element_it_promises() {
        let fixture = downstream().build();
        let cmd = fixture.command();

        let arg = |id: &str| {
            cmd.get_arguments()
                .find(|a| a.get_id() == id)
                .unwrap_or_else(|| panic!("no {id} argument"))
        };

        assert!(cmd.get_about().is_some() && cmd.get_long_about().is_some());
        assert!(
            cmd.get_subcommands().count() >= 3,
            "a COMMANDS section needs commands to list"
        );

        let range = arg("range");
        assert!(range.is_positional());
        assert_eq!(
            range.get_value_names().map(<[_]>::to_vec),
            Some(vec!["RANGE".into()])
        );

        assert!(
            matches!(arg("staged").get_action(), ArgAction::SetTrue),
            "the presence flag is what must not render a value"
        );

        assert_eq!(
            arg("threshold").get_value_names().map(<[_]>::to_vec),
            Some(vec!["RATIO".into()])
        );
        assert!(
            arg("pattern").get_value_names().is_none(),
            "the fallback-metavar case needs an option with no value name"
        );

        let summary = arg("summary");
        assert_eq!(summary.get_default_values(), ["brief"]);
        assert_eq!(summary.get_possible_values().len(), 3);
    }

    /// The fixture's theme is incomplete on purpose: it must not define the
    /// tags the help template emits, or the overlay it exists to exercise
    /// never happens.
    #[test]
    fn the_app_theme_defines_none_of_the_help_tags() {
        let app_styles = incomplete_theme().resolve_styles(None);
        assert!(
            !app_styles.is_empty(),
            "the app still themes its own output"
        );

        for tag in default_help_theme()
            .resolve_styles(None)
            .to_resolved_map()
            .keys()
        {
            assert!(
                !app_styles.has(tag),
                "{tag} is a help tag, so the theme is no longer incomplete"
            );
        }
    }

    /// Help handling is on and the topics are registered, so `help <topic>` is
    /// a real destination and the word has a reason to be listed.
    #[test]
    fn help_handling_and_topics_are_wired() {
        let fixture = downstream().build();

        let page = help_page(&fixture, &[NAME, "--help"]);
        assert!(page.contains("USAGE"), "{page}");

        let topic = help_page(&fixture, &[NAME, "help", "ranges"]);
        assert!(
            topic.contains("two revisions separated by two dots"),
            "{topic}"
        );

        let bare = downstream().without_topics().build();
        assert!(
            bare.app().registry().list_topics().is_empty(),
            "the topic-less shape is what leaves the word nowhere to go"
        );
    }

    /// The nested shape lists its commands; the flat one has none to list.
    #[test]
    fn the_shapes_differ_in_their_commands_not_their_arguments() {
        let nested = downstream().build();
        let flat = downstream().flat().build();

        assert!(nested.command().get_subcommands().count() >= 3);
        assert_eq!(flat.command().get_subcommands().count(), 0);

        let names = |cmd: Command| {
            cmd.get_arguments()
                .map(|a| a.get_id().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(nested.command()),
            names(flat.command()),
            "one argument declaration serves both shapes"
        );

        assert!(
            flat.command().get_groups().any(|g| g.get_id() == "target"),
            "the flat shape's requirements are what make the word unreachable"
        );
    }

    /// The word is an opt-in on a flat root with a positional, so the fixture
    /// has to be able to leave it off.
    #[test]
    fn the_help_word_opt_in_is_configurable() {
        let opted_in = downstream().flat().build();
        assert!(opted_in
            .app()
            .augment_command_with_help(opted_in.command())
            .find_subcommand("help")
            .is_some());

        let left_out = downstream().flat().without_help_word().build();
        assert!(left_out
            .app()
            .augment_command_with_help(left_out.command())
            .find_subcommand("help")
            .is_none());
    }
}
