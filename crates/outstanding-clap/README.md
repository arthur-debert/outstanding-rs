# outstanding-clap

Batteries-included integration of `outstanding` with `clap`.

This crate handles the boilerplate of connecting outstanding's styled output to your clap-based CLI:

- Styled help output using outstanding templates
- Help topics system (`help <topic>`, `help topics`)
- `--output` flag for user output control (enabled by default)
- Pager support for long help content

## Quick Start

```rust
use clap::Command;
use outstanding_clap::Outstanding;

// Simplest usage - styled help with --output flag
let matches = Outstanding::run(Command::new("my-app"));
```

That's it. Your CLI now has:

- Styled help output
- `--output=<auto|term|text|term-debug>` flag on all commands
- `help` subcommand with topic support

## Adding Help Topics

```rust
use clap::Command;
use outstanding_clap::Outstanding;

let matches = Outstanding::builder()
    .topics_dir("docs/topics")  // Load .txt and .md files as topics
    .run(Command::new("my-app"));

// Users can now run:
//   my-app help topics     - list all topics
//   my-app help <topic>    - view specific topic
```

Topic files should have the title on the first line, followed by content:

```text
Storage Guide
=============

Notes are stored in ~/.notes/

Each note is a separate file with a UUID-based filename.
```

## Configuration Options

```rust
use clap::Command;
use outstanding::Theme;
use outstanding_clap::Outstanding;

let my_theme = Theme::new();  // Customize as needed

let matches = Outstanding::builder()
    .topics_dir("docs/topics")    // Load topics from directory
    .theme(my_theme)              // Custom theme
    .output_flag(Some("format"))  // Custom flag name (default: "output")
    .no_output_flag()             // Or disable the flag entirely
    .run(Command::new("my-app"));
```

## What This Crate Does

The `outstanding` crate provides the core framework:

- Template rendering with MiniJinja
- Themes and styles
- Output mode control
- Topic system (data structures, rendering, pager)

This crate provides the **clap integration**:

- Intercepts `help`, `help <topic>`, `help topics` subcommands
- Injects `--output` flag to all commands
- Renders clap command help using outstanding templates
- Calls outstanding's topic rendering for topic help

For non-clap applications, use `outstanding` directly and write your own argument parsing glue.

## API Overview

### Outstanding

Main entry point. Use `Outstanding::run()` for the simplest case or `Outstanding::builder()` for configuration.

```rust
// Static method - quick setup
let matches = Outstanding::run(cmd);

// Builder pattern - full control
let matches = Outstanding::builder()
    .topics_dir("docs/topics")
    .theme(my_theme)
    .build()
    .run_with(cmd);
```

### OutstandingBuilder

Builder for configuring Outstanding:

| Method | Description |
| -------- | ------------- |
| `topics_dir(path)` | Load topics from a directory |
| `add_topic(topic)` | Add a single topic |
| `theme(theme)` | Set custom theme |
| `output_flag(Some("name"))` | Custom output flag name |
| `no_output_flag()` | Disable output flag |
| `build()` | Build the Outstanding instance |
| `run(cmd)` | Build and run in one step |

### HelpResult

Result of `get_matches()`:

| Variant | Description |
   | --------- | ------------- |
| `Matches(ArgMatches)` | Normal command execution |
| `Help(String)` | Help content to print |
| `PagedHelp(String)` | Help content for pager |
| `Error(clap::Error)` | Parse or lookup error |

## Defaults

By default:

- `--output` flag is **enabled** (use `no_output_flag()` to disable)
- Help topics are **enabled** but empty (add with `topics_dir()` or `add_topic()`)
- Pager support via `help --page <topic>`

## License

MIT
