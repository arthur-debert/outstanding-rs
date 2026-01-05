//! Sample CLI Application for Testing Outstanding-Clap
//!
//! This is a TEST/SAMPLE application designed to demonstrate and test
//! the outstanding-clap integration features. It is NOT a real application.
//!
//! Modeled after a simplified version of `padz` CLI structure.

use clap::{Parser, Subcommand, Args, CommandFactory};
use console::Style;
use outstanding::{render_with_color, Theme, ThemeChoice};
use outstanding::topics::{Topic, TopicType};
use outstanding_clap::{TopicHelper, TopicHelpResult, display_with_pager};
use serde::Serialize;

const ECHO_TEMPLATE: &str = r#"{{ "Command Executed" | style("header") | nl }}
{{ "==================================================" | nl }}

{% if command %}{{ "Command:" | style("label") }}       {{ command | style("value") | nl }}{% endif %}
{% if subcommand %}{{ "Subcommand:" | style("label") }}    {{ subcommand | style("value") | nl }}{% endif %}
{% if args %}{{ "Arguments:" | style("label") }}    {{ args | style("value") | nl }}{% endif %}
{% if options %}{{ "Options:" | style("label") | nl }}{% for opt in options %}  {{ opt.name | style("opt_name") }}: {{ opt.value | style("opt_value") | nl }}{% endfor %}{% endif %}

{{ "--------------------------------------------------" | nl }}
{{ "This is a SAMPLE app for testing outstanding-clap" | style("note") | nl }}
"#;

fn echo_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold().cyan())
        .add("label", Style::new().yellow())
        .add("value", Style::new().green())
        .add("opt_name", Style::new().magenta())
        .add("opt_value", Style::new().white())
        .add("note", Style::new().dim().italic())
}

#[derive(Serialize)]
struct EchoData {
    command: String,
    subcommand: Option<String>,
    args: Option<String>,
    options: Vec<OptionPair>,
}

#[derive(Serialize)]
struct OptionPair {
    name: String,
    value: String,
}

fn echo_command(data: &EchoData) {
    let use_color = console::Term::stdout().features().colors_supported();
    match render_with_color(ECHO_TEMPLATE, data, ThemeChoice::from(&echo_theme()), use_color) {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Template error: {}", e),
    }
}

/// Sample CLI - A TEST application for outstanding-clap features
///
/// WARNING: This is NOT a real application. It exists solely to test
/// and demonstrate the outstanding-clap integration, including topics,
/// styled help, and pager support.
#[derive(Parser)]
#[command(name = "sample-cli")]
#[command(version = "0.0.1-test")]
#[command(about = "Sample CLI - TEST app for outstanding-clap (NOT a real app)")]
#[command(long_about = "Sample CLI Application\n\n\
    WARNING: This is a TEST/SAMPLE application.\n\
    It demonstrates and tests outstanding-clap features:\n\
    - Topic-based help system\n\
    - Styled terminal output\n\
    - Pager support (--page flag)\n\n\
    All commands simply echo back what was invoked.")]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Use global scope
    #[arg(short, long, global = true)]
    global: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new item
    Create(CreateArgs),

    /// List all items
    List(ListArgs),

    /// View one or more items
    View(ViewArgs),

    /// Edit an item
    Edit(EditArgs),

    /// Delete one or more items
    Delete(DeleteArgs),

    /// Manage configuration
    Config(ConfigArgs),
}

#[derive(Args)]
struct CreateArgs {
    /// Title words (joined with spaces)
    #[arg(value_name = "TITLE")]
    title: Vec<String>,

    /// Skip opening the editor
    #[arg(long)]
    no_editor: bool,

    /// Add tags to the item
    #[arg(short, long)]
    tags: Vec<String>,
}

#[derive(Args)]
struct ListArgs {
    /// Filter by tag
    #[arg(short, long)]
    tag: Option<String>,

    /// Limit number of results
    #[arg(short, long, default_value = "10")]
    limit: usize,

    /// Sort order (asc/desc)
    #[arg(short, long, default_value = "desc")]
    sort: String,
}

#[derive(Args)]
struct ViewArgs {
    /// Indexes of items to view (e.g., 1 2 3)
    #[arg(required = true)]
    indexes: Vec<String>,

    /// Peek at content without full view
    #[arg(long)]
    peek: bool,
}

#[derive(Args)]
struct EditArgs {
    /// Index of item to edit
    #[arg(required = true)]
    index: String,

    /// Editor to use
    #[arg(short, long)]
    editor: Option<String>,
}

#[derive(Args)]
struct DeleteArgs {
    /// Indexes of items to delete
    #[arg(required = true)]
    indexes: Vec<String>,

    /// Force deletion without confirmation
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    action: Option<ConfigAction>,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
    /// List all configuration
    List,
}

fn setup_topics() -> TopicHelper {
    TopicHelper::builder()
        .add_topic(Topic::new(
            "Syntax Guide",  // title (displayed)
            "Syntax Guide for Sample CLI\n\n\
            This sample app accepts various commands and options.\n\n\
            Index Syntax:\n\
            - Plain numbers: 1, 2, 3\n\
            - Prefixed: p1 (pinned), d1 (deleted)\n\n\
            Tag Syntax:\n\
            - Use -t or --tags to add tags\n\
            - Multiple tags: -t foo -t bar\n\n\
            NOTE: This is a SAMPLE app - commands only echo back input.",
            TopicType::Text,
            Some("syntax".to_string()),  // name (lookup key)
        ))
        .add_topic(Topic::new(
            "Usage Examples",  // title (displayed)
            "Examples for Sample CLI\n\n\
            Create an item:\n\
              sample-cli create \"My Title\" --tags important\n\n\
            List with filters:\n\
              sample-cli list --tag work --limit 5\n\n\
            View multiple items:\n\
              sample-cli view 1 2 3\n\n\
            Configure:\n\
              sample-cli config set editor vim\n\
              sample-cli config get editor\n\n\
            Using pager for help:\n\
              sample-cli help --page syntax\n\
              sample-cli help --page examples\n\n\
            NOTE: This is a SAMPLE app for testing outstanding-clap.",
            TopicType::Text,
            Some("examples".to_string()),  // name (lookup key)
        ))
        .add_topic(Topic::new(
            "Testing Guide",  // title (displayed)
            "Testing Outstanding-Clap Features\n\n\
            This sample app tests the following features:\n\n\
            1. Topic Help System\n\
               - Topics registered via TopicHelper\n\
               - Accessible via: help <topic>\n\
               - Topics: syntax, examples, testing\n\n\
            2. Styled Help Output\n\
               - Uses outstanding templates\n\
               - Theme-based styling\n\
               - Color support detection\n\n\
            3. Pager Support (NEW)\n\
               - Use --page flag with help\n\
               - Respects $PAGER environment variable\n\
               - Falls back to: less -> more\n\n\
            Test commands:\n\
              sample-cli help --page          # Root help via pager\n\
              sample-cli help --page syntax   # Topic via pager\n\
              sample-cli help --page create   # Command via pager\n\n\
            Compare with/without pager:\n\
              sample-cli help syntax          # Direct output\n\
              sample-cli help --page syntax   # Via pager",
            TopicType::Text,
            Some("testing".to_string()),  // name (lookup key)
        ))
        .build()
}

fn build_command() -> clap::Command {
    Cli::command()
}

fn main() {
    let helper = setup_topics();
    let cmd = build_command();

    match helper.get_matches(cmd) {
        TopicHelpResult::Help(h) => {
            println!("{}", h);
        }
        TopicHelpResult::PagedHelp(h) => {
            if let Err(e) = display_with_pager(&h) {
                eprintln!("Pager error: {}, falling back to stdout", e);
                println!("{}", h);
            }
        }
        TopicHelpResult::Error(e) => {
            e.exit();
        }
        TopicHelpResult::Matches(matches) => {
            handle_matches(&matches);
        }
    }
}

fn handle_matches(matches: &clap::ArgMatches) {
    let verbose = matches.get_flag("verbose");
    let global = matches.get_flag("global");

    let mut base_options = vec![];
    if verbose {
        base_options.push(OptionPair {
            name: "verbose".to_string(),
            value: "true".to_string(),
        });
    }
    if global {
        base_options.push(OptionPair {
            name: "global".to_string(),
            value: "true".to_string(),
        });
    }

    match matches.subcommand() {
        Some(("create", sub)) => {
            let title: Vec<_> = sub.get_many::<String>("title")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let no_editor = sub.get_flag("no_editor");
            let tags: Vec<_> = sub.get_many::<String>("tags")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();

            let mut options = base_options;
            if no_editor {
                options.push(OptionPair { name: "no-editor".to_string(), value: "true".to_string() });
            }
            if !tags.is_empty() {
                options.push(OptionPair { name: "tags".to_string(), value: tags.join(", ") });
            }

            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some("create".to_string()),
                args: if title.is_empty() { None } else { Some(title.join(" ")) },
                options,
            });
        }
        Some(("list", sub)) => {
            let tag = sub.get_one::<String>("tag").cloned();
            let limit = sub.get_one::<usize>("limit").copied().unwrap_or(10);
            let sort = sub.get_one::<String>("sort").cloned().unwrap_or_default();

            let mut options = base_options;
            if let Some(t) = tag {
                options.push(OptionPair { name: "tag".to_string(), value: t });
            }
            options.push(OptionPair { name: "limit".to_string(), value: limit.to_string() });
            options.push(OptionPair { name: "sort".to_string(), value: sort });

            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some("list".to_string()),
                args: None,
                options,
            });
        }
        Some(("view", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let peek = sub.get_flag("peek");

            let mut options = base_options;
            if peek {
                options.push(OptionPair { name: "peek".to_string(), value: "true".to_string() });
            }

            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some("view".to_string()),
                args: Some(indexes.join(" ")),
                options,
            });
        }
        Some(("edit", sub)) => {
            let index = sub.get_one::<String>("index").cloned().unwrap_or_default();
            let editor = sub.get_one::<String>("editor").cloned();

            let mut options = base_options;
            if let Some(e) = editor {
                options.push(OptionPair { name: "editor".to_string(), value: e });
            }

            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some("edit".to_string()),
                args: Some(index),
                options,
            });
        }
        Some(("delete", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let force = sub.get_flag("force");

            let mut options = base_options;
            if force {
                options.push(OptionPair { name: "force".to_string(), value: "true".to_string() });
            }

            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some("delete".to_string()),
                args: Some(indexes.join(" ")),
                options,
            });
        }
        Some(("config", sub)) => {
            match sub.subcommand() {
                Some(("get", get_sub)) => {
                    let key = get_sub.get_one::<String>("key").cloned().unwrap_or_default();
                    echo_command(&EchoData {
                        command: "sample-cli".to_string(),
                        subcommand: Some("config get".to_string()),
                        args: Some(key),
                        options: base_options,
                    });
                }
                Some(("set", set_sub)) => {
                    let key = set_sub.get_one::<String>("key").cloned().unwrap_or_default();
                    let value = set_sub.get_one::<String>("value").cloned().unwrap_or_default();
                    echo_command(&EchoData {
                        command: "sample-cli".to_string(),
                        subcommand: Some("config set".to_string()),
                        args: Some(format!("{} = {}", key, value)),
                        options: base_options,
                    });
                }
                Some(("list", _)) => {
                    echo_command(&EchoData {
                        command: "sample-cli".to_string(),
                        subcommand: Some("config list".to_string()),
                        args: None,
                        options: base_options,
                    });
                }
                _ => {
                    echo_command(&EchoData {
                        command: "sample-cli".to_string(),
                        subcommand: Some("config".to_string()),
                        args: None,
                        options: base_options,
                    });
                }
            }
        }
        Some((name, _)) => {
            echo_command(&EchoData {
                command: "sample-cli".to_string(),
                subcommand: Some(name.to_string()),
                args: None,
                options: base_options,
            });
        }
        None => {
            // No subcommand - show help
            println!("Sample CLI - TEST app for outstanding-clap");
            println!("Run 'sample-cli help' for usage information.");
        }
    }
}
