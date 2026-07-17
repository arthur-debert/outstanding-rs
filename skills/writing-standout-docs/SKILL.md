---
name: writing-standout-docs
description: Write or review documentation for the Standout Rust CLI framework. Use when authoring Standout guides or topics, choosing guide versus topic placement, maintaining canonical examples and the shared `tdoo` domain, or checking documentation structure, cross-links, and tone. Use the `standout` skill instead for application implementation.
---

# Writing Standout documentation

Write practical documentation that leads with Standout's testable separation of command logic from shell presentation. Verify examples against current public Rust signatures and integration tests before publishing them.

For production-shaped examples, state the stronger ownership invariant: the
reusable library is fully CLI-free, while the binary package owns Clap,
Standout, handlers, view DTOs, assets, environment lookup, app construction,
and final output. Describe handlers as adapters, not as pure application logic.

## Place the content

- Keep the project entry point in `README.md`; `crates/standout/README.md` should link rather than duplicate it.
- Put progressive, task-led walkthroughs in `docs/guides/`. Each step should explain its value and the commitment it adds.
- Put focused framework reference and rationale in `docs/topics/`.
- Put crate-owned guides and topics under `crates/<crate>/docs/`; core documentation should live with the code it explains.
- Update `docs/SUMMARY.md` when adding or moving published documentation.

Use a guide for a reader journey through a need or subsystem. Use a topic for a focused system, its use cases, design, and detailed behavior. Both explain why and how; neither should be an API inventory that rustdoc or an IDE already supplies.

## Show canonical forms

Prefer one recommended path in the main example:

| Concern | Canonical form |
| --- | --- |
| Command setup | `#[derive(Dispatch)]` when convention fits |
| Handler arguments | `#[handler]` typed functions |
| Templates and styles | File-based MiniJinja plus CSS |
| Asset loading | `embed_templates!` and `embed_styles!` |
| Execution | `app.run(...)`; `run_to_string(...)` only for capture or explicit result handling |
| Testing | Library tests, then direct handler tests, then `TestHarness` for the pipeline |

Mention alternatives briefly unless the page specifically teaches them. Keep examples compilable, include necessary imports, and use convention-based names where possible.

Current execution returns a boolean:

```rust
if !app.run(Cli::command(), std::env::args()) {
    run_legacy_path();
}
```

When fallback code needs the unmatched `ArgMatches`, show `run_to_string(...)` and match `RunResult::NoMatch(matches)` instead.

## Use the `tdoo` example domain

Use the shared todo application unless a feature requires a different domain:

- `crates/todo-example/todo-core/` is the canonical reusable library.
- `crates/todo-example/tdoo/` is the canonical binary-only CLI.
- `docs/guides/minimal-single-crate.md` is only the compact introductory form;
  do not use it to teach production package ownership.

```rust
// todo-core domain and persistence type
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

// tdoo CLI-owned structured output types
#[derive(serde::Serialize)]
pub struct TodoView {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

#[derive(serde::Serialize)]
pub struct TodoListView {
    pub todos: Vec<TodoView>,
    pub total: usize,
}

#[derive(serde::Serialize)]
pub struct TodoActionView {
    pub message: String,
    pub todo: TodoView,
}
```

```jinja
[title]Your Todos[/title] [muted]({{ total }})[/muted]
{% if total == 0 %}
[muted]Nothing here yet. Add one with `tdoo add --title "<title>"`.[/muted]
{%- else %}
{% for todo in todos -%}
{%- set status = "done" if todo.done else "pending" -%}
[index]#{{ todo.id }}[/index]  [{{ status }}]{{ todo.title }}[/{{ status }}]
{% endfor -%}
{%- endif %}
```

```css
.title { color: cyan; font-weight: bold; }
.index { color: yellow; }
.done { color: gray; text-decoration: line-through; }
.pending { font-weight: bold; }
.muted { color: gray; }
```

Cross-link from guides to deeper topics. Use relative links that resolve from the source file and check them after moves.

## Review bar

- Lead with testability and the logic/presentation boundary, not visual polish alone.
- Keep application behavior in the CLI-free library and show handlers mapping
  to CLI-owned view DTOs.
- Make partial adoption prominent where a reader may already have a CLI.
- Show structured output for automation and direct handler assertions for logic.
- Explain runtime override trade-offs alongside compile-time embedding.
- Use screenshots or recordings only when they prove terminal behavior prose cannot.
- Keep the tone direct, contextual, and free of marketing superlatives.
