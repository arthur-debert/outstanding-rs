//! A downstream-shaped fixture the help and rendering suites assert against.
//!
//! [`Fixture::app`] and [`Fixture::command`] are built together from one
//! [`Downstream`] configuration, so app and command can't drift apart the
//! way two hand-written test doubles could.
//!
//! The shape carries the properties that co-occur in a real CLI and must be
//! distinguished from each other: subcommands (so COMMANDS renders), a
//! presence flag next to a valued positional, a `Count` flag (valueless but
//! not boolean), a valued option with and without a metavar, an enumerated
//! option with a default, a long option name, an app theme that overlays
//! rather than replaces the help theme, and `help_handling` with topics
//! registered. [`Downstream::flat`] swaps the subcommands for a required
//! `ArgGroup` on the same arguments, so nested and flat shapes never disagree
//! about what an option is called.
//!
//! ```
//! use standout_fixtures::downstream;
//!
//! let fixture = downstream().build();
//! assert_eq!(fixture.command().get_name(), "lookma");
//! ```

pub mod derive_surface;

use clap::{Arg, ArgAction, ArgGroup, Command};
use console::Style;
use serde_json::json;
use standout::cli::{App, Output};
use standout::topics::{Topic, TopicType};
use standout::Theme;

pub const NAME: &str = "lookma";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Nested,
    Flat,
}

pub fn downstream() -> Downstream {
    Downstream::new()
}

#[derive(Debug, Clone)]
#[must_use = "a Downstream is a configuration; call build() for the fixture"]
pub struct Downstream {
    shape: Shape,
    help_word: bool,
    theme: bool,
    topics: bool,
}

impl Downstream {
    pub fn new() -> Self {
        Self {
            shape: Shape::Nested,
            help_word: true,
            theme: true,
            topics: true,
        }
    }

    pub fn flat(mut self) -> Self {
        self.shape = Shape::Flat;
        self
    }

    pub fn without_help_word(mut self) -> Self {
        self.help_word = false;
        self
    }

    pub fn without_theme(mut self) -> Self {
        self.theme = false;
        self
    }

    pub fn without_topics(mut self) -> Self {
        self.topics = false;
        self
    }

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
                Arg::new("verbose")
                    .short('v')
                    .action(ArgAction::Count)
                    .help("Raise the detail level"),
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

pub struct Fixture {
    app: App,
    command: Command,
}

impl Fixture {
    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn command(&self) -> Command {
        self.command.clone()
    }
}

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
        assert!(
            matches!(arg("verbose").get_action(), ArgAction::Count),
            "the counted flag is the valueless argument that is not SetTrue"
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
