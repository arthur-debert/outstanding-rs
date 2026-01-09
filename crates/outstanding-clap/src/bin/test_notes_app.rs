//! Test Notes App - Sample CLI for Testing Outstanding-Clap
//!
//! **THIS IS A TEST/SAMPLE APPLICATION - NOT A REAL TOOL**
//!
//! This application demonstrates and tests outstanding-clap features:
//! - Styled help rendering
//! - Help topics via TopicHelper
//! - Output mode flag (--output=auto|term|text)
//! - Pager support
//!
//! Run with: cargo run --bin test-notes-app -- <command>

use clap::{Parser, Subcommand, Args, CommandFactory};
use console::Style;
use outstanding::{render_with_output, Theme, ThemeChoice, OutputMode};
use outstanding::topics::{Topic, TopicType};
use outstanding_clap::TopicHelper;
use serde::Serialize;

const ECHO_TEMPLATE: &str = r#"{{ "TEST NOTES APP - Command Echo" | style("header") | nl }}
{{ "(This is a sample app for testing outstanding-clap)" | style("muted") | nl }}
{{ "==================================================" | nl }}

{% if command %}{{ "Command:" | style("label") }}       {{ command | style("value") | nl }}{% endif %}
{% if subcommand %}{{ "Subcommand:" | style("label") }}    {{ subcommand | style("value") | nl }}{% endif %}
{% if args %}{{ "Arguments:" | style("label") }}    {{ args | style("value") | nl }}{% endif %}
{% if options %}{{ "Options:" | style("label") | nl }}{% for opt in options %}  {{ opt.name | style("opt_name") }}: {{ opt.value | style("opt_value") | nl }}{% endfor %}{% endif %}

{{ "--------------------------------------------------" | nl }}
"#;

fn echo_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold().cyan())
        .add("muted", Style::new().dim())
        .add("label", Style::new().yellow())
        .add("value", Style::new().green())
        .add("opt_name", Style::new().magenta())
        .add("opt_value", Style::new().white())
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

fn echo_command(data: &EchoData, mode: OutputMode) {
    match render_with_output(ECHO_TEMPLATE, data, ThemeChoice::from(&echo_theme()), mode) {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Template error: {}", e),
    }
}

/// Test Notes App - A TEST application for outstanding-clap features
///
/// WARNING: This is NOT a real application. It exists solely to test
/// and demonstrate the outstanding-clap integration.
#[derive(Parser)]
#[command(name = "test-notes-app")]
#[command(version = "0.0.1-test")]
#[command(about = "Test Notes App - Sample CLI for outstanding-clap (NOT a real app)")]
#[command(long_about = "Test Notes App\n\n\
    WARNING: This is a TEST/SAMPLE application.\n\n\
    It demonstrates and tests outstanding-clap features:\n\
    - Topic-based help system\n\
    - Styled terminal output\n\
    - Output mode flag (--output)\n\
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
    /// Create a new note
    Create(CreateArgs),

    /// List notes
    List(ListArgs),

    /// Search notes
    Search(SearchArgs),

    /// View one or more notes
    View(ViewArgs),

    /// Edit a note in the editor
    Edit(EditArgs),

    /// Open a note in the editor
    Open(OpenArgs),

    /// Delete one or more notes
    Delete(DeleteArgs),

    /// Restore deleted notes
    Restore(RestoreArgs),

    /// Pin notes (delete-protected)
    Pin(PinArgs),

    /// Unpin notes
    Unpin(UnpinArgs),

    /// Print file path to notes
    Path(PathArgs),

    /// Permanently delete notes
    Purge(PurgeArgs),

    /// Export notes to archive
    Export(ExportArgs),

    /// Import files as notes
    Import(ImportArgs),

    /// Check and fix data
    Doctor,

    /// Manage configuration
    Config(ConfigArgs),

    /// Initialize the store
    Init,
}

#[derive(Args)]
struct CreateArgs {
    /// Title words
    #[arg(value_name = "TITLE")]
    title: Vec<String>,
    /// Skip editor
    #[arg(long)]
    no_editor: bool,
}

#[derive(Args)]
struct ListArgs {
    /// Filter by tag
    #[arg(short, long)]
    tag: Option<String>,
    /// Limit results
    #[arg(short, long, default_value = "10")]
    limit: usize,
}

#[derive(Args)]
struct SearchArgs {
    /// Search term
    query: String,
    /// Case sensitive
    #[arg(short, long)]
    case_sensitive: bool,
}

#[derive(Args)]
struct ViewArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
    /// Raw output
    #[arg(long)]
    raw: bool,
}

#[derive(Args)]
struct EditArgs {
    /// Note index
    #[arg(required = true)]
    index: String,
    /// Editor to use
    #[arg(short, long)]
    editor: Option<String>,
}

#[derive(Args)]
struct OpenArgs {
    /// Note index
    #[arg(required = true)]
    index: String,
}

#[derive(Args)]
struct DeleteArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
    /// Force deletion
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
struct RestoreArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
}

#[derive(Args)]
struct PinArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
}

#[derive(Args)]
struct UnpinArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
}

#[derive(Args)]
struct PathArgs {
    /// Note indexes
    #[arg(required = true)]
    indexes: Vec<String>,
}

#[derive(Args)]
struct PurgeArgs {
    /// Note indexes
    indexes: Vec<String>,
    /// Purge all deleted
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
struct ExportArgs {
    /// Output file
    #[arg(short, long)]
    output: Option<String>,
    /// Single file mode
    #[arg(long)]
    single_file: bool,
}

#[derive(Args)]
struct ImportArgs {
    /// Files to import
    #[arg(required = true)]
    files: Vec<String>,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    action: Option<ConfigAction>,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Get a configuration value
    Get { key: String },
    /// Set a configuration value
    Set { key: String, value: String },
    /// List all configuration
    List,
}

fn setup_topics() -> TopicHelper {
    TopicHelper::builder()
        .output_flag(None) // Add --output flag
        .add_topic(Topic::new(
            "Scopes",
            "Projects and Global Notes\n\n\
            Notes can be scoped to projects or stored globally.\n\n\
            PROJECT SCOPE (default)\n\
            -----------------------\n\
            Notes are stored in the current directory under .notes/\n\
            This keeps notes close to the project they relate to.\n\n\
            GLOBAL SCOPE (-g, --global)\n\
            ---------------------------\n\
            Use the -g or --global flag to access global notes.\n\
            Global notes are stored in ~/.notes/ and are accessible\n\
            from anywhere.\n\n\
            EXAMPLES\n\
            --------\n\
              test-notes-app list              # List project notes\n\
              test-notes-app -g list           # List global notes\n\
              test-notes-app create \"Note\"     # Create project note\n\
              test-notes-app -g create \"Note\"  # Create global note\n\n\
            NOTE: This is a SAMPLE app for testing outstanding-clap.",
            TopicType::Text,
            Some("scopes".to_string()),
        ))
        .add_topic(Topic::new(
            "Output Modes",
            "Controlling Terminal Output\n\n\
            Use the --output flag to control how output is rendered.\n\n\
            MODES\n\
            -----\n\
              auto    Detect terminal capabilities (default)\n\
              term    Always use ANSI colors/styles\n\
              text    Plain text, no ANSI codes\n\n\
            EXAMPLES\n\
            --------\n\
              test-notes-app --output=auto list\n\
              test-notes-app --output=term help\n\
              test-notes-app --output=text list > notes.txt\n\n\
            The 'text' mode is useful for piping output to files\n\
            or other programs that don't understand ANSI codes.\n\n\
            NOTE: This is a SAMPLE app for testing outstanding-clap.",
            TopicType::Text,
            Some("output".to_string()),
        ))
        .add_topic(Topic::new(
            "Syntax Guide",
            "Syntax Guide for Test Notes App\n\n\
            INDEX SYNTAX\n\
            ------------\n\
            - Plain numbers: 1, 2, 3\n\
            - Prefixed: p1 (pinned), d1 (deleted)\n\n\
            EXAMPLES\n\
            --------\n\
              test-notes-app view 1 2 3        # View notes 1, 2, 3\n\
              test-notes-app view p1           # View pinned note 1\n\
              test-notes-app restore d1        # Restore deleted note 1\n\n\
            NOTE: This is a SAMPLE app - commands only echo back input.",
            TopicType::Text,
            Some("syntax".to_string()),
        ))
        .build()
}

fn build_command() -> clap::Command {
    Cli::command()
}

fn main() {
    let helper = setup_topics();
    let matches = helper.run(build_command());
    handle_matches(&matches);
}

fn get_output_mode(matches: &clap::ArgMatches) -> OutputMode {
    match matches.get_one::<String>("_output_mode").map(|s| s.as_str()) {
        Some("term") => OutputMode::Term,
        Some("text") => OutputMode::Text,
        Some("term-debug") => OutputMode::TermDebug,
        _ => OutputMode::Auto,
    }
}

fn handle_matches(matches: &clap::ArgMatches) {
    let verbose = matches.get_flag("verbose");
    let global = matches.get_flag("global");
    let mode = get_output_mode(matches);

    let mut base_options = vec![];
    if verbose {
        base_options.push(OptionPair { name: "verbose".to_string(), value: "true".to_string() });
    }
    if global {
        base_options.push(OptionPair { name: "global".to_string(), value: "true".to_string() });
    }

    match matches.subcommand() {
        Some(("create", sub)) => {
            let title: Vec<_> = sub.get_many::<String>("title").map(|v| v.cloned().collect()).unwrap_or_default();
            let no_editor = sub.get_flag("no_editor");
            let mut options = base_options;
            if no_editor { options.push(OptionPair { name: "no-editor".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("create".to_string()),
                args: if title.is_empty() { None } else { Some(title.join(" ")) },
                options,
            }, mode);
        }
        Some(("list", sub)) => {
            let tag = sub.get_one::<String>("tag").cloned();
            let limit = sub.get_one::<usize>("limit").copied().unwrap_or(10);
            let mut options = base_options;
            if let Some(t) = tag { options.push(OptionPair { name: "tag".to_string(), value: t }); }
            options.push(OptionPair { name: "limit".to_string(), value: limit.to_string() });
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("list".to_string()),
                args: None,
                options,
            }, mode);
        }
        Some(("search", sub)) => {
            let query = sub.get_one::<String>("query").cloned().unwrap_or_default();
            let case_sensitive = sub.get_flag("case_sensitive");
            let mut options = base_options;
            if case_sensitive { options.push(OptionPair { name: "case-sensitive".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("search".to_string()),
                args: Some(query),
                options,
            }, mode);
        }
        Some(("view", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            let raw = sub.get_flag("raw");
            let mut options = base_options;
            if raw { options.push(OptionPair { name: "raw".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("view".to_string()),
                args: Some(indexes.join(" ")),
                options,
            }, mode);
        }
        Some(("edit", sub)) => {
            let index = sub.get_one::<String>("index").cloned().unwrap_or_default();
            let editor = sub.get_one::<String>("editor").cloned();
            let mut options = base_options;
            if let Some(e) = editor { options.push(OptionPair { name: "editor".to_string(), value: e }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("edit".to_string()),
                args: Some(index),
                options,
            }, mode);
        }
        Some(("open", sub)) => {
            let index = sub.get_one::<String>("index").cloned().unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("open".to_string()),
                args: Some(index),
                options: base_options,
            }, mode);
        }
        Some(("delete", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            let force = sub.get_flag("force");
            let mut options = base_options;
            if force { options.push(OptionPair { name: "force".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("delete".to_string()),
                args: Some(indexes.join(" ")),
                options,
            }, mode);
        }
        Some(("restore", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("restore".to_string()),
                args: Some(indexes.join(" ")),
                options: base_options,
            }, mode);
        }
        Some(("pin", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("pin".to_string()),
                args: Some(indexes.join(" ")),
                options: base_options,
            }, mode);
        }
        Some(("unpin", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("unpin".to_string()),
                args: Some(indexes.join(" ")),
                options: base_options,
            }, mode);
        }
        Some(("path", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("path".to_string()),
                args: Some(indexes.join(" ")),
                options: base_options,
            }, mode);
        }
        Some(("purge", sub)) => {
            let indexes: Vec<_> = sub.get_many::<String>("indexes").map(|v| v.cloned().collect()).unwrap_or_default();
            let all = sub.get_flag("all");
            let mut options = base_options;
            if all { options.push(OptionPair { name: "all".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("purge".to_string()),
                args: if indexes.is_empty() { None } else { Some(indexes.join(" ")) },
                options,
            }, mode);
        }
        Some(("export", sub)) => {
            let output = sub.get_one::<String>("output").cloned();
            let single_file = sub.get_flag("single_file");
            let mut options = base_options;
            if let Some(o) = output { options.push(OptionPair { name: "output".to_string(), value: o }); }
            if single_file { options.push(OptionPair { name: "single-file".to_string(), value: "true".to_string() }); }
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("export".to_string()),
                args: None,
                options,
            }, mode);
        }
        Some(("import", sub)) => {
            let files: Vec<_> = sub.get_many::<String>("files").map(|v| v.cloned().collect()).unwrap_or_default();
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("import".to_string()),
                args: Some(files.join(" ")),
                options: base_options,
            }, mode);
        }
        Some(("doctor", _)) => {
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("doctor".to_string()),
                args: None,
                options: base_options,
            }, mode);
        }
        Some(("config", sub)) => {
            match sub.subcommand() {
                Some(("get", get_sub)) => {
                    let key = get_sub.get_one::<String>("key").cloned().unwrap_or_default();
                    echo_command(&EchoData {
                        command: "test-notes-app".to_string(),
                        subcommand: Some("config get".to_string()),
                        args: Some(key),
                        options: base_options,
                    }, mode);
                }
                Some(("set", set_sub)) => {
                    let key = set_sub.get_one::<String>("key").cloned().unwrap_or_default();
                    let value = set_sub.get_one::<String>("value").cloned().unwrap_or_default();
                    echo_command(&EchoData {
                        command: "test-notes-app".to_string(),
                        subcommand: Some("config set".to_string()),
                        args: Some(format!("{} = {}", key, value)),
                        options: base_options,
                    }, mode);
                }
                Some(("list", _)) => {
                    echo_command(&EchoData {
                        command: "test-notes-app".to_string(),
                        subcommand: Some("config list".to_string()),
                        args: None,
                        options: base_options,
                    }, mode);
                }
                _ => {
                    echo_command(&EchoData {
                        command: "test-notes-app".to_string(),
                        subcommand: Some("config".to_string()),
                        args: None,
                        options: base_options,
                    }, mode);
                }
            }
        }
        Some(("init", _)) => {
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some("init".to_string()),
                args: None,
                options: base_options,
            }, mode);
        }
        Some((name, _)) => {
            echo_command(&EchoData {
                command: "test-notes-app".to_string(),
                subcommand: Some(name.to_string()),
                args: None,
                options: base_options,
            }, mode);
        }
        None => {
            println!("Test Notes App - Sample CLI for testing outstanding-clap");
            println!("Run 'test-notes-app help' for usage information.");
        }
    }
}
