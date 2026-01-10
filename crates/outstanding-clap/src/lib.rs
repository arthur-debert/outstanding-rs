//! # Outstanding Clap - Clap Integration
//!
//! Batteries-included integration of `outstanding` with `clap`. This crate handles
//! the boilerplate of connecting outstanding's styled output to your clap-based CLI:
//!
//! - Styled help output using outstanding templates
//! - Help topics system (`help <topic>`, `help topics`)
//! - `--output` flag for user output control (enabled by default)
//! - Pager support for long help content
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! // Simplest usage - styled help with --output flag
//! let matches = Outstanding::run(Command::new("my-app"));
//! ```
//!
//! ## With Help Topics
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! let matches = Outstanding::builder()
//!     .topics_dir("docs/topics")  // Load topics from directory
//!     .run(Command::new("my-app"));
//!
//! // Users can now run:
//! //   my-app help topics     - list all topics
//! //   my-app help <topic>    - view specific topic
//! ```
//!
//! ## Configuration Options
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding::Theme;
//! use outstanding_clap::Outstanding;
//!
//! let my_theme = Theme::new();  // Customize as needed
//!
//! let matches = Outstanding::builder()
//!     .topics_dir("docs/topics")    // Load topics from directory
//!     .theme(my_theme)              // Custom theme (optional)
//!     .output_flag(Some("format"))  // Custom flag name (default: "output")
//!     .no_output_flag()             // Or disable the flag entirely
//!     .run(Command::new("my-app"));
//! ```
//!
//! ## What This Crate Does
//!
//! The `outstanding` crate provides the core rendering framework (themes, templates,
//! output modes, topic system). This crate provides the **clap integration**:
//!
//! - Intercepts `help`, `help <topic>`, `help topics` subcommands
//! - Injects `--output` flag to all commands
//! - Renders clap command help using outstanding templates
//! - Calls outstanding's topic rendering for topic help
//!
//! For non-clap applications, use `outstanding` directly and write your own
//! argument parsing glue.
//!
//! ## Command Handler System
//!
//! For declarative command handling, use the builder's `command()` method:
//!
//! ```rust,ignore
//! use outstanding_clap::{Outstanding, CommandResult};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct ListOutput { items: Vec<String> }
//!
//! Outstanding::builder()
//!     .command("list", |_m, _ctx| {
//!         CommandResult::Ok(ListOutput { items: vec!["one".into()] })
//!     }, "{% for item in items %}{{ item }}\n{% endfor %}")
//!     .run_and_print(cmd, std::env::args());
//! ```
//!
//! ## Handler Hooks
//!
//! Hooks allow running custom code before and after command handlers execute.
//! Use cases include logging, clipboard operations, output transformation, and validation.
//!
//! ```rust,ignore
//! use outstanding_clap::{Outstanding, Hooks, Output, HookError};
//!
//! Outstanding::builder()
//!     .command("export", handler, template)
//!     .hooks("export", Hooks::new()
//!         .pre_dispatch(|ctx| {
//!             println!("Running: {:?}", ctx.command_path);
//!             Ok(())
//!         })
//!         .post_output(|_ctx, output| {
//!             // Copy text output to clipboard
//!             if let Output::Text(ref text) = output {
//!                 // clipboard::copy(text)?;
//!             }
//!             Ok(output)
//!         }))
//!     .run_and_print(cmd, args);
//! ```
//!
//! Hooks are per-command and support chaining (multiple hooks at the same phase
//! run in order, with post-output hooks able to transform output).
//!
//! For the regular API (manual dispatch), use `Outstanding::run_command()`:
//!
//! ```rust,ignore
//! let outstanding = Outstanding::builder()
//!     .hooks("list", Hooks::new().post_output(copy_to_clipboard))
//!     .build();
//!
//! let matches = outstanding.run_with(cmd);
//! if let Some(("list", sub_m)) = matches.subcommand() {
//!     let output = outstanding.run_command("list", sub_m, handler, template)?;
//!     println!("{}", output);
//! }
//! ```
//!
//! See the [`hooks`] module for full documentation.
//!
//! ## Module Structure
//!
//! - [`handler`]: Command handler types (`CommandContext`, `CommandResult`, `Handler`)
//! - [`hooks`]: Hook system for pre/post command execution
//! - [`help`]: Help rendering functions and configuration
//! - Internal: `dispatch`, `result`, `outstanding` modules

// Internal modules
mod dispatch;
mod outstanding;
mod result;

// Public modules
pub mod handler;
pub mod help;
pub mod hooks;

// Re-export main types from outstanding module
pub use outstanding::{Outstanding, OutstandingBuilder};

// Re-export result type
pub use result::HelpResult;

// Re-export help types
pub use help::{default_help_theme, render_help, render_help_with_topics, HelpConfig};

// Re-export handler types
pub use handler::{CommandContext, CommandResult, FnHandler, Handler, RunResult};

// Re-export hook types
pub use hooks::{HookError, HookPhase, Hooks, Output};

// Re-export core types from outstanding crate for convenience
pub use ::outstanding::topics::{
    display_with_pager, render_topic as render_topic_core,
    render_topics_list as render_topics_list_core, Topic as TopicDef,
    TopicRegistry as TopicRegistryDef, TopicType,
};

// ============================================================================
// BACKWARDS COMPATIBILITY (deprecated)
// ============================================================================

/// Alias for Outstanding (deprecated, use Outstanding instead)
#[deprecated(since = "0.4.0", note = "Use Outstanding instead")]
pub type TopicHelper = Outstanding;

/// Alias for OutstandingBuilder (deprecated, use OutstandingBuilder instead)
#[deprecated(since = "0.4.0", note = "Use OutstandingBuilder instead")]
pub type TopicHelperBuilder = OutstandingBuilder;

/// Alias for HelpResult (deprecated, use HelpResult instead)
#[deprecated(since = "0.4.0", note = "Use HelpResult instead")]
pub type TopicHelpResult = HelpResult;

/// Alias for HelpConfig (deprecated, use HelpConfig instead)
#[deprecated(since = "0.4.0", note = "Use HelpConfig instead")]
pub type Config = HelpConfig;

/// Runs a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
pub fn run(cmd: clap::Command) -> clap::ArgMatches {
    Outstanding::run(cmd)
}

/// Like `run`, but takes arguments from an iterator.
pub fn run_from<I, T>(cmd: clap::Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Outstanding::new().run_from(cmd, itr)
}
