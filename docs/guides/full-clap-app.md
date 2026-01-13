# Building a CLI with Outstanding and Clap

This guide walks through building a task manager CLI. The goal: keep your handlers focused on logic, let Outstanding handle presentation.

## What We're Building

`taskr` - a simple task manager with four commands: `add`, `list`, and `complete`.

## Project Setup

```bash
cargo new taskr
cd taskr
```

**Cargo.toml:**

```toml
[package]
name = "taskr"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
outstanding = "0.14"
outstanding-clap = "0.14"
serde = { version = "1", features = ["derive"] }
anyhow = "1"
console = "0.15"
```

---

## The Data Model

One result type for all commands: an optional message plus the current todos.

```rust
// src/data.rs
use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Done,
}

#[derive(Serialize, Clone)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct TodoResult {
    pub message: Option<String>,
    pub todos: Vec<Todo>,
}
```

---

## Define the CLI

Doc comments become help text automatically.

```rust
// src/cli.rs
use clap::{Parser, Subcommand};

/// A simple task manager
#[derive(Parser)]
#[command(name = "taskr")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a new task
    Add {
        /// Task description
        text: String,
    },

    /// List all tasks
    List,

    /// Mark tasks as complete
    Complete {
        /// Task IDs to complete
        ids: Vec<u32>,
    },

}
```

---

## The Handlers

Each handler does its job and returns `TodoResult`. No formatting, no colors - just data.

```rust
// src/handlers.rs
use clap::ArgMatches;
use outstanding_clap::{CommandContext, CommandResult};

use crate::data::{Todo, TodoResult};
use crate::storage;

pub fn add(matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TodoResult> {
    let text = matches.get_one::<String>("text").unwrap().clone();

    let todo = storage::add(&text)?;
    let todos = storage::list()?;

    CommandResult::Ok(TodoResult {
        message: Some(format!("Added: {}", todo.text)),
        todos,
    })
}

pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TodoResult> {
    let todos = storage::list()?;

    CommandResult::Ok(TodoResult {
        message: None,
        todos,
    })
}

pub fn complete(matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<TodoResult> {
    let ids: Vec<u32> = matches .get_many::<u32>("ids") .unwrap() .copied() .collect();

    let completed = storage::complete(&ids)?;
    let todos = storage::list()?;

    let msg = match completed.len() {
        1 => format!("Completed: {}", completed[0].text),
        n => format!("Completed {} tasks", n),
    };

    CommandResult::Ok(TodoResult {
        message: Some(msg),
        todos,
    })
}

```

---

## The Template

One template handles all commands. It shows the message (if any) then lists todos.

**templates/todos.j2:**

```jinja
{% if message %}
    {{ message | style("success") }}
{% endif %}

{% if todos %}
    {% for todo in todos %}
        {{ todo.id | style("id") }}  {{ todo.text | style(todo.status) }}
    {% endfor %}
{% else %}
    {{ "No tasks yet." | style("muted") }}
{% endif %}
```

---

## The Theme

**theme.yaml:**

```yaml
success: green
id: yellow bold
muted: dim
pending: white
done: dim strikethrough
```

---

## Wire It Up

```rust
// src/main.rs
mod cli;
mod data;
mod handlers;
mod storage;

use clap::CommandFactory;
use outstanding::Theme;
use outstanding_clap::{dispatch, Outstanding};

use crate::cli::Cli;

fn main() {
    let theme = Theme::from_yaml(include_str!("../theme.yaml")).unwrap();

    Outstanding::builder()
        .theme(theme)
        .commands(dispatch! {
            add => handlers::add,
            list => handlers::list,
            complete => handlers::complete,
        })
        .run_and_print(Cli::command(), std::env::args());
}
```

---

## What You Get

```bash
$ taskr add "Buy milk"
Added: Buy milk

1  [ ]  Buy milk

$ taskr add "Walk dog"
Added: Walk dog

1  [ ]  Buy milk
2  [ ]  Walk dog

$ taskr complete 1
Completed: Buy milk

1  ~~Buy milk~~
2  Walk dog

$ taskr list --output=json
{
  "message": null,
  "todos": [
    {"id": 1, "text": "Buy milk", "status": "done"},
    {"id": 2, "text": "Walk dog", "status": "pending"}
  ]
}
```

1. Isolated Application Logic that is easy to test.
2. Term Output
    1. Zero code output: just define the content (template) and formatting (through styles)
    1. Rich term output with graceful degradation to plain text depening on client capabilities.
    1. Auto dark/light mode suport
    1. Hot reloading of template and styles for quick iteration
    1. Reusable templates and partials for consistent and duplication free code.
3. Structured data output:
    1. YAML, JSON, CSV formats
    1. Useful for integration testing , or piping / feeding into more tools, data exports, etc.
4. Boilerplate free dispatch
5. Keep Clap benefits: rich and declarative cli parser declaration, help text and help.

---

## The Pattern

1. **One result type** - `TodoResult` with message + data
2. **Handlers return data** - No formatting, no IO
3. **One template** - Handles the message + list pattern
4. **Framework dispatches** - `dispatch!` macro routes commands

That's it. Logic in handlers, presentation in templates, routing handled by Outstanding.
