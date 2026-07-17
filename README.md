# Standout

[![Crates.io](https://img.shields.io/crates/v/standout.svg)](https://crates.io/crates/standout)
[![Documentation](https://img.shields.io/badge/docs-book-blue)](https://standout.magik.works/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Test your data. Render your view.**

Standout is a CLI framework for Rust built around one claim: a shell application's logic should be as testable as any other Rust code, and its full pipeline — argv in, rendered output out — should be testable in-process, not through brittle subprocess-and-regex dances. Handlers return structs, not strings. A dedicated test harness runs the whole app against a controlled environment (piped stdin, env vars, fixture files, clipboard, terminal width, color capability) in microseconds.

If you've been writing CLI integration tests by spawning the binary and grepping stdout, Standout is built to replace most of them. See **[Introduction to Testing](https://standout.magik.works/guides/intro-to-testing.html)**.

## The Problem

CLI code that mixes logic with `println!` statements is impossible to unit test:

```rust
fn list_command(show_all: bool) {
    let todos = storage::list().unwrap();
    println!("Your Todos:");
    for todo in todos.iter() {
        if show_all || todo.status == Status::Pending {
            println!("  {} {}", if todo.done { "[x]" } else { "[ ]" }, todo.title);
        }
    }
}
```

## The Solution

With Standout, a CLI-free library owns application behavior. Handlers are thin
adapters that return CLI-owned view data; the framework handles rendering:

```rust
#[handler]
fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all { TodoFilter::All } else { TodoFilter::Pending };
    let todos = store.list(filter).into_iter().map(TodoView::from).collect();
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}

#[test]
fn test_list_returns_pending_view() {
    let Output::Render(result) = list(false, &ctx).unwrap() else {
        panic!("expected rendered data");
    };
    assert!(result.todos.iter().all(|todo| !todo.done));
}
```

The core behavior is tested through the library interface. The direct handler
test checks only adapter and view-model behavior—no stdout capture, regex, or
template coupling. See the
**[production-shaped example](https://standout.magik.works/guides/production-shaped-example.html)**
for the complete two-package layout.

For full-pipeline tests — "run the CLI as if it were invoked from a shell, with *this* env, *this* piped stdin, *these* fixture files" — the `standout-test` crate runs the whole app in-process:

```rust
use standout_test::{serial, TestHarness};

#[test]
#[serial]
fn list_reads_from_env_configured_file() {
    let result = TestHarness::new()
        .env("TODO_FILE", "other.txt")
        .fixture("other.txt", "buy milk\nwrite tests\n")
        .no_color()
        .run(&app, cmd, ["myapp", "list"]);

    result.assert_success();
    result.assert_stdout_contains("buy milk");
}
```

No subprocess. No stdout plumbing. Env vars, cwd, stdin, clipboard, terminal width, and color capability are all controllable, and every override is restored on drop — even on panic. See **[Introduction to Testing](https://standout.magik.works/guides/intro-to-testing.html)** for the full tour.

## Features

- **Testable by design** — Handlers return data; `standout-test` runs the full pipeline in-process against a controlled environment
- **Multiple output modes** — Rich terminal, JSON, YAML, CSV from the same handler
- **MiniJinja templates** — Familiar syntax with partials, filters, and hot reload
- **CSS/YAML styling** — Semantic styles with light/dark mode support
- **Tabular layouts** — Declarative columns with alignment, truncation, wrapping
- **Clap integration** — Automatic dispatch via derive macros
- **Incremental adoption** — Migrate one command at a time

## Installation

```bash
cargo add standout
```

## Quick Example

```rust
use standout::cli::{App, Dispatch, CommandContext, HandlerResult, Output};
use standout::{embed_templates, embed_styles};

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    List,
}

let app = App::builder()
    .commands(Commands::dispatch_config())?
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .build()?;

app.run(Cli::command(), std::env::args());
```

```bash
myapp list                  # Rich terminal output
myapp list --output json    # JSON for scripting
```

## Documentation

You can find comprehensive documentation in our book: **[standout.magik.works](https://standout.magik.works/)**

- [Introduction to Testing](https://standout.magik.works/guides/intro-to-testing.html) — The primary value prop: why Standout CLIs are testable end-to-end, in-process, without subprocess spawning
- [Introduction to Standout](https://standout.magik.works/guides/intro-to-standout.html) — Adopting the framework in an existing CLI
- [Rendering System](https://standout.magik.works/topics/rendering-system.html) — Templates and styles
- [Tabular Layouts](https://standout.magik.works/topics/tabular.html) — Tables and alignment
- [All Topics](https://standout.magik.works/topics/index.html) — Complete reference

## Contributing

Contributions welcome. Use the [issue tracker](https://github.com/arthur-debert/standout/issues) for bugs and feature requests.

## License

MIT
