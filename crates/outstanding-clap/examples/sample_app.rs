//! # Sample CLI Application
//!
//! **THIS IS A TEST/SAMPLE APPLICATION - NOT A REAL TOOL**
//!
//! This example demonstrates outstanding-clap features:
//! - Styled help rendering via `render_help`
//! - Help topics via `Outstanding`
//! - Command/subcommand/option/argument handling
//!
//! Run with: cargo run --example sample_app -- <command>
//!
//! Try:
//!   cargo run --example sample_app -- help
//!   cargo run --example sample_app -- help create
//!   cargo run --example sample_app -- help storage
//!   cargo run --example sample_app -- create "My Note"
//!   cargo run --example sample_app -- list --all
//!   cargo run --example sample_app -- config get editor

use clap::{Arg, ArgAction, Command};
use outstanding::topics::{Topic, TopicType};
use outstanding::{render_with_output, OutputMode, Theme};
use outstanding_clap::{display_with_pager, HelpResult, Outstanding};
use serde::Serialize;
use std::process::ExitCode;

// ============================================================================
// TEMPLATES
// ============================================================================

/// Template for echoing back command execution details
const ECHO_TEMPLATE: &str = r#"
{{ "SAMPLE APP - Command Echo" | style("header") | nl }}
{{ "(This is a test app that echoes what you called)" | style("muted") | nl }}

{{ "Command:" | style("label") }} {{ command | style("value") }}
{%- if subcommand %}
{{ "Subcommand:" | style("label") }} {{ subcommand | style("value") }}
{%- endif %}
{%- if args %}
{{ "Arguments:" | style("label") }}
{%- for arg in args %}
  {{ arg.name | style("arg_name") }}: {{ arg.value | style("arg_value") }}
{%- endfor %}
{%- endif %}
{%- if options %}
{{ "Options:" | style("label") }}
{%- for opt in options %}
  {{ opt.name | style("opt_name") }}: {{ opt.value | style("opt_value") }}
{%- endfor %}
{%- endif %}
"#;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Serialize)]
struct EchoData {
    command: String,
    subcommand: Option<String>,
    args: Vec<ArgData>,
    options: Vec<OptData>,
}

#[derive(Serialize)]
struct ArgData {
    name: String,
    value: String,
}

#[derive(Serialize)]
struct OptData {
    name: String,
    value: String,
}

// ============================================================================
// THEME
// ============================================================================

fn sample_theme() -> Theme {
    use console::Style;
    Theme::new()
        .add("header", Style::new().bold().cyan())
        .add("muted", Style::new().dim())
        .add("label", Style::new().bold())
        .add("value", Style::new().green())
        .add("arg_name", Style::new().yellow())
        .add("arg_value", Style::new().white())
        .add("opt_name", Style::new().magenta())
        .add("opt_value", Style::new().white())
}

// ============================================================================
// CLI DEFINITION
// ============================================================================

fn build_cli() -> Command {
    Command::new("sample-app")
        .version("0.1.0-demo")
        .about("SAMPLE/TEST APP - Demonstrates outstanding-clap features (NOT A REAL TOOL)")
        .long_about(
            "This is a SAMPLE APPLICATION for testing outstanding-clap.\n\n\
             It demonstrates:\n\
             - Styled help output via outstanding templates\n\
             - Help topics (try: help storage)\n\
             - Command/subcommand structure\n\
             - Option and argument handling\n\n\
             Commands echo back what was called using outstanding templates.",
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Enable verbose output"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Suppress non-essential output"),
        )
        // Core commands
        .subcommand(
            Command::new("create")
                .about("Create a new note")
                .arg(
                    Arg::new("title")
                        .help("Title for the new note")
                        .required(false),
                )
                .arg(
                    Arg::new("no-editor")
                        .long("no-editor")
                        .action(ArgAction::SetTrue)
                        .help("Skip opening the editor"),
                ),
        )
        .subcommand(
            Command::new("list")
                .alias("ls")
                .about("List all notes")
                .arg(
                    Arg::new("all")
                        .short('a')
                        .long("all")
                        .action(ArgAction::SetTrue)
                        .help("Show all notes including deleted"),
                )
                .arg(
                    Arg::new("search")
                        .short('s')
                        .long("search")
                        .value_name("TERM")
                        .help("Filter notes by search term"),
                ),
        )
        .subcommand(
            Command::new("view")
                .alias("v")
                .about("View one or more notes")
                .arg(
                    Arg::new("indexes")
                        .help("Note indexes to view (e.g., 1 2 3)")
                        .num_args(1..)
                        .required(true),
                )
                .arg(
                    Arg::new("raw")
                        .long("raw")
                        .action(ArgAction::SetTrue)
                        .help("Show raw content without formatting"),
                ),
        )
        .subcommand(
            Command::new("delete")
                .alias("rm")
                .about("Delete one or more notes")
                .arg(
                    Arg::new("indexes")
                        .help("Note indexes to delete")
                        .num_args(1..)
                        .required(true),
                )
                .arg(
                    Arg::new("force")
                        .short('f')
                        .long("force")
                        .action(ArgAction::SetTrue)
                        .help("Skip confirmation"),
                ),
        )
        // Nested subcommand example
        .subcommand(
            Command::new("config")
                .about("Manage configuration")
                .subcommand(
                    Command::new("get")
                        .about("Get a configuration value")
                        .arg(Arg::new("key").help("Configuration key").required(true)),
                )
                .subcommand(
                    Command::new("set")
                        .about("Set a configuration value")
                        .arg(Arg::new("key").help("Configuration key").required(true))
                        .arg(Arg::new("value").help("Value to set").required(true)),
                )
                .subcommand(Command::new("list").about("List all configuration")),
        )
}

// ============================================================================
// TOPICS
// ============================================================================

fn build_outstanding() -> Outstanding {
    Outstanding::builder()
        .add_topic(Topic::new(
            "Storage",
            r#"Where Notes are Stored
======================

This sample app demonstrates the concept of storage locations.

LOCAL STORAGE
-------------
Notes are stored in the current directory under .notes/

Each note is a separate file with:
  - UUID-based filename
  - JSON metadata sidecar

CONFIGURATION
-------------
Storage location can be configured via:

  sample-app config set storage-path /path/to/notes

Or by setting the SAMPLE_APP_STORAGE environment variable.

BACKUP
------
To backup your notes, simply copy the .notes/ directory.
"#,
            TopicType::Text,
            Some("storage".to_string()),
        ))
        .add_topic(Topic::new(
            "Syntax",
            r#"Note Syntax Reference
=====================

Notes in this sample app use a simple format:

STRUCTURE
---------
Line 1:    Title (required)
Line 2:    Blank separator
Line 3+:   Body content (optional)

EXAMPLE
-------
  My Shopping List

  - Milk
  - Eggs
  - Bread

The title is extracted from the first non-empty line.
"#,
            TopicType::Text,
            Some("syntax".to_string()),
        ))
        .build()
}

// ============================================================================
// COMMAND HANDLERS
// ============================================================================

fn echo_command(data: EchoData) {
    let theme = sample_theme();
    match render_with_output(ECHO_TEMPLATE, &data, &theme, OutputMode::Auto) {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Render error: {}", e),
    }
}

fn handle_create(matches: &clap::ArgMatches, verbose: bool) {
    let title = matches
        .get_one::<String>("title")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(no title)".to_string());
    let no_editor = matches.get_flag("no-editor");

    let mut opts = vec![];
    if no_editor {
        opts.push(OptData {
            name: "--no-editor".to_string(),
            value: "true".to_string(),
        });
    }
    if verbose {
        opts.push(OptData {
            name: "--verbose".to_string(),
            value: "true".to_string(),
        });
    }

    echo_command(EchoData {
        command: "create".to_string(),
        subcommand: None,
        args: vec![ArgData {
            name: "title".to_string(),
            value: title,
        }],
        options: opts,
    });
}

fn handle_list(matches: &clap::ArgMatches, verbose: bool) {
    let all = matches.get_flag("all");
    let search = matches.get_one::<String>("search").map(|s| s.to_string());

    let mut opts = vec![];
    if all {
        opts.push(OptData {
            name: "--all".to_string(),
            value: "true".to_string(),
        });
    }
    if let Some(term) = search {
        opts.push(OptData {
            name: "--search".to_string(),
            value: term,
        });
    }
    if verbose {
        opts.push(OptData {
            name: "--verbose".to_string(),
            value: "true".to_string(),
        });
    }

    echo_command(EchoData {
        command: "list".to_string(),
        subcommand: None,
        args: vec![],
        options: opts,
    });
}

fn handle_view(matches: &clap::ArgMatches, verbose: bool) {
    let indexes: Vec<String> = matches
        .get_many::<String>("indexes")
        .unwrap_or_default()
        .map(|s| s.to_string())
        .collect();
    let raw = matches.get_flag("raw");

    let mut opts = vec![];
    if raw {
        opts.push(OptData {
            name: "--raw".to_string(),
            value: "true".to_string(),
        });
    }
    if verbose {
        opts.push(OptData {
            name: "--verbose".to_string(),
            value: "true".to_string(),
        });
    }

    echo_command(EchoData {
        command: "view".to_string(),
        subcommand: None,
        args: vec![ArgData {
            name: "indexes".to_string(),
            value: indexes.join(", "),
        }],
        options: opts,
    });
}

fn handle_delete(matches: &clap::ArgMatches, verbose: bool) {
    let indexes: Vec<String> = matches
        .get_many::<String>("indexes")
        .unwrap_or_default()
        .map(|s| s.to_string())
        .collect();
    let force = matches.get_flag("force");

    let mut opts = vec![];
    if force {
        opts.push(OptData {
            name: "--force".to_string(),
            value: "true".to_string(),
        });
    }
    if verbose {
        opts.push(OptData {
            name: "--verbose".to_string(),
            value: "true".to_string(),
        });
    }

    echo_command(EchoData {
        command: "delete".to_string(),
        subcommand: None,
        args: vec![ArgData {
            name: "indexes".to_string(),
            value: indexes.join(", "),
        }],
        options: opts,
    });
}

fn handle_config(matches: &clap::ArgMatches, verbose: bool) {
    let (subcommand, sub_matches) = matches.subcommand().unwrap_or(("list", matches));

    let mut args = vec![];
    let mut opts = vec![];

    if verbose {
        opts.push(OptData {
            name: "--verbose".to_string(),
            value: "true".to_string(),
        });
    }

    match subcommand {
        "get" => {
            let key = sub_matches
                .get_one::<String>("key")
                .map(|s| s.to_string())
                .unwrap_or_default();
            args.push(ArgData {
                name: "key".to_string(),
                value: key,
            });
        }
        "set" => {
            let key = sub_matches
                .get_one::<String>("key")
                .map(|s| s.to_string())
                .unwrap_or_default();
            let value = sub_matches
                .get_one::<String>("value")
                .map(|s| s.to_string())
                .unwrap_or_default();
            args.push(ArgData {
                name: "key".to_string(),
                value: key,
            });
            args.push(ArgData {
                name: "value".to_string(),
                value,
            });
        }
        _ => {}
    }

    echo_command(EchoData {
        command: "config".to_string(),
        subcommand: Some(subcommand.to_string()),
        args,
        options: opts,
    });
}

// ============================================================================
// MAIN
// ============================================================================

fn main() -> ExitCode {
    let cmd = build_cli();
    let outstanding = build_outstanding();

    match outstanding.get_matches(cmd) {
        HelpResult::Help(help) => {
            print!("{}", help);
            ExitCode::SUCCESS
        }
        HelpResult::PagedHelp(help) => {
            if let Err(e) = display_with_pager(&help) {
                eprintln!("Pager error: {}", e);
                print!("{}", help);
            }
            ExitCode::SUCCESS
        }
        HelpResult::Error(e) => {
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
        HelpResult::Matches(matches) => {
            let verbose = matches.get_flag("verbose");

            match matches.subcommand() {
                Some(("create", sub_m)) => handle_create(sub_m, verbose),
                Some(("list", sub_m)) => handle_list(sub_m, verbose),
                Some(("view", sub_m)) => handle_view(sub_m, verbose),
                Some(("delete", sub_m)) => handle_delete(sub_m, verbose),
                Some(("config", sub_m)) => handle_config(sub_m, verbose),
                Some((cmd, _)) => {
                    eprintln!("Unknown command: {}", cmd);
                    return ExitCode::FAILURE;
                }
                None => {
                    // No command - show help
                    println!("SAMPLE APP - No command specified. Try: help");
                    println!();
                    println!("This is a TEST APPLICATION demonstrating outstanding-clap.");
                    println!("Run with --help or 'help' for usage information.");
                }
            }
            ExitCode::SUCCESS
        }
    }
}
