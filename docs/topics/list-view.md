# List Views

Most CLI commands that print a collection share the same shape: an optional
intro line, the items, an optional list of messages (warnings, info), and an
optional summary of how many items were filtered out of a larger total. The
`list_view` builder in `standout::views` captures that shape once so handlers
stop reassembling it by hand, and `#[dispatch(list_view)]` wires the result to
the framework's built-in list template without a project-owned template file.

Reach for it whenever a handler returns "here are N things" — a `list`,
`search`, or `status` command — and the result needs more than a bare `Vec<T>`:
an intro line, a filter summary, or per-item messages.

## Where it lives

`list_view`, `ListViewBuilder`, and `ListViewResult` live in
`standout::views`, a module (`pub mod views`) rather than a crate-root
re-export:

```rust
use standout::views::{list_view, ListViewBuilder, ListViewResult};
```

## The builder

```rust
pub fn list_view<T>(items: impl IntoIterator<Item = T>) -> ListViewBuilder<T>;
```

`ListViewBuilder<T>` accepts items in any order and returns itself, so calls
chain:

| Method | Effect |
| --- | --- |
| `.intro(text)` | A line shown before the items |
| `.ending(text)` | A line shown after the items |
| `.message(level, text)` | Attaches a `Message` at the given `MessageLevel` |
| `.info(text)` | Shortcut for `.message(MessageLevel::Info, text)` |
| `.success(text)` | Shortcut for `.message(MessageLevel::Success, text)` |
| `.warning(text)` | Shortcut for `.message(MessageLevel::Warning, text)` |
| `.error(text)` | Shortcut for `.message(MessageLevel::Error, text)` |
| `.total_count(n)` | Records the unfiltered total, for a "showing X of Y" summary |
| `.filter_summary(text)` | A human-readable description of the active filter |
| `.tabular_spec(spec)` | Attaches a `TabularSpec` directly (usually left to `#[dispatch(list_view, item_type = "...")]` instead) |
| `.empty_exit_status(n)` | The exit status a successful run declares when `items` is empty; see [Empty lists](#empty-lists-and-the-exit-status) |
| `.build()` | Consumes the builder and returns a `ListViewResult<T>` |
| `.output()` | `.build().into_output()`: the `Output::Render` a handler returns, with the empty-list status applied |

## The result

```rust
pub struct ListViewResult<T> {
    pub items: Vec<T>,
    pub empty_exit_status: Option<ExitStatus>,   // not serialized
    pub intro: Option<String>,
    pub ending: Option<String>,
    pub messages: Vec<Message>,
    pub total_count: Option<usize>,
    pub filter_summary: Option<String>,
    pub tabular_spec: Option<TabularSpec>,
}
```

`ListViewResult<T>` implements `Serialize` (fields that are `None` or empty
are skipped, so `--output json` stays uncluttered), `Default` (an empty list
with every optional field unset), and carries `.is_empty()` and `.len()`
methods that read `items` directly.

It is a framework-owned document, so it serializes with a `schema_version`
key first, beside its own fields:

```json
{"schema_version": 1, "items": [{"id": 1, "name": "Implement auth"}], "intro": "Your tasks:"}
```

The key is the version of the document's shape (`1` today); a script that
parses `--output json` reads it to tell a shape change from a data change.
`ListViewResult<T>` implements `ContractSurface` with that version, and the
same key sits in a template's context. Adding it is the breaking change of
the 10.0 line for list-view consumers: a snapshot of the JSON document gains
one top-level key. See [What Is Contract](./stability.md#the-versioned-document).

Because it derives `Serialize` rather than requiring one, `ListViewResult<T>`
is itself a valid handler return type: a handler can return
`HandlerResult<ListViewResult<Task>>` and wrap it in `Output::Render` like any
other structured output. See
[Handler Contract](../crates/dispatch/topics/handler-contract.md) for the
`Output` enum and the render pipeline it feeds.

## Empty lists and the exit status

An empty list is a successful run. A command whose callers want to tell "found
nothing" from "found something" without parsing output declares the status it
exits with in that case; the framework names no code for it:

```rust,ignore
Ok(list_view(matches).empty_exit_status(3).output())
```

`ListViewResult::into_output` (which `.output()` calls) returns
`Output::Render(list)`, and when `items` is empty and a status was declared,
`Output::Render(list).with_exit_status(status)`. The list still renders — the
template's "No items found" branch, or `{"items":[]}` under `--output json` —
and the process exits with the declared status. See [Execution
Outcomes](./execution-outcomes.md#status-and-streams) for what a declared
status is and is not.

## Connecting to `#[dispatch(list_view)]`

The `Dispatch` derive has a `list_view` variant attribute that does two
things to a variant's handler:

- It sets the command's template to the framework-provided
  `standout/list-view` template, unless the variant also sets
  `#[dispatch(template_name = "...")]`, in which case that name wins.
- If `item_type = "..."` names a type implementing `Tabular`, the derive
  wraps the handler so that, on a successful `Output::Render(list_view_result)`,
  it stamps `list_view_result.tabular_spec` with `<ItemType as Tabular>::tabular_spec()`
  before rendering. The handler itself never has to know about the column
  layout.

Handlers under `#[dispatch(list_view)]` can be either the two-argument shape
(`fn(&ArgMatches, &CommandContext) -> HandlerResult<ListViewResult<T>>`) or,
with `#[dispatch(simple)]` added, a single-argument shape
(`fn(&ArgMatches) -> HandlerResult<ListViewResult<T>>`). Both are wrapped the
same way.

## Worked example

```rust
use clap::{ArgMatches, Subcommand};
use serde::Serialize;
use standout::cli::{CommandContext, Dispatch, HandlerResult, Output};
use standout::views::list_view;
use standout::{Tabular, TabularRow};

#[derive(Serialize, Tabular, TabularRow, Clone)]
struct Task {
    #[col(width = 5)]
    id: u32,
    #[col(width = 20)]
    name: String,
}

mod handlers {
    use super::*;

    pub fn list(
        _matches: &ArgMatches,
        _ctx: &CommandContext,
    ) -> HandlerResult<standout::views::ListViewResult<Task>> {
        let tasks = vec![Task {
            id: 1,
            name: "Write docs".to_string(),
        }];
        Ok(Output::Render(list_view(tasks).build()))
    }
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(list_view, item_type = "Task")]
    List,
}
```

Running `list` renders the framework's list template with `Task`'s column
widths already attached; `--output json` serializes the same
`ListViewResult<Task>`, `tabular_spec` included, with no template involved.
An empty `items` vec renders the template's "No items found" branch rather
than an empty table.

## Disabling the framework template

`App::builder().include_framework_templates(false)` refuses to build if a
command names `standout/list-view` (or any other framework template) without
supplying a replacement — a project can opt out of the built-in list layout,
but only by registering its own template under the same name.
