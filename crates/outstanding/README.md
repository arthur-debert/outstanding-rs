# Outstanding

**Create Outstanding Shell Applications in Rust.**

Outstanding is a library for building finely crafted, non-interactive command line applications. It enforces a clean separation between your application logic and its presentation, ensuring your CLI remains testable and maintainable as it grows.

## The Problem

If you're building a CLI in Rust, your time should be spent on core logic, not fiddling with `print!` statements and ANSI escape codes.

As applications grow, mixing logic with output formatting leads to:
- **Untestable Code**: You can't unit test logic that writes directly to stdout.
- **Fragile Integration Tests**: Parsing text output to verify correctness is brittle.
- **Inconsistent UX**: Styling inconsistencies creep in over time.

## The Solution

Outstanding handles the boilerplate between your `clap` definition and your terminal output.

1.  **Define Logic**: Write pure functions that receive arguments and return data.
2.  **Define Presentation**: Use templates (MiniJinja) and styles (YAML) to control appearance.
3.  **Let Framework Handle the Rest**: Outstanding runs the pipeline, applying themes, formatting tables, or serializing to JSON/YAML based on flags.

## Features

- **Application Life Cicle**:
    -   **Formal Logic/Presentation Split**: Decouples your Rust code from terminal formatting.
    - **End to end handling**: from clap arg parsing, to running the logic handlers and finally rendering it's results with rich output.
    - **Declarative API** for annotating your functions.
    - **Auto Dispatch** from cli input to the execution life cicle
- **Rendering Layer**:
    -   **File-Based Templates**: Uses [MiniJinja](https://github.com/mitsuhiko/minijinja) for powerful templating, including partials for reuse. See [Rendering System](docs/guides/rendering-system.md).
    -   **Rich Styling**: Integrates stylesheets with semantic tagging (e.g., `[title]{{ post.title }}[/title]`). Define styles using **YAML** or standard **CSS** syntax.
    -   **Adaptive Themes**: Supports [light/dark modes](docs/guides/rendering-system.md#adaptive-styles) and switchable themes automatically.
    -   **Live Reloading**: Edit templates and styles while your app runs [during development](docs/guides/rendering-system.md#hot-reloading) for rapid iteration.
    -   **Smart Output**: Delivers [rich terminal output](docs/guides/output-modes.md) that gracefully degrades to plain text based on capabilities.
    -   **Automatic Structured Data**: Get JSON, CSV, and YAML output for free by leveraging your pure data structures. See [Structured Modes](docs/guides/output-modes.md#structured-modes).

## Quick Start

### 1. The Logic
Write a handler that takes `ArgMatches` and returns serializable data.

```rust
#[derive(Serialize)]
struct TodoResult {
    todos: Vec<Todo>,
}

fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
    let todos = storage::list()?;
    Ok(Output::Render(TodoResult { todos }))
}
```

### 2. The Presentation
Write a template (`list.jinja`) with semantic style tags.

```jinja
[title]My Todos[/title]
{% for todo in todos %}
  - {{ todo.title }} ([status]{{ todo.status }}[/status])
{% endfor %}
```

### 3. The Setup
Wire it up in your `main.rs`.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .command("list", list_handler, "list.jinja")
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .build()?;

    app.run(Cli::command(), std::env::args());
    Ok(())
}
```

## Installation

Add `outstanding` to your `Cargo.toml`:

```bash
cargo add outstanding
```

Ensure you have `outstanding-macros` if you want to use the embedding features.

## Documentation

Learn more about building with Outstanding:

-   **Guides**
    -   [App Configuration](docs/guides/app-configuration.md)
    -   [Execution Model](docs/guides/execution-model.md)
    -   [Handler Contract](docs/guides/handler-contract.md)
    -   [Rendering System](docs/guides/rendering-system.md)
    -   [Output Modes](docs/guides/output-modes.md)
    -   [Topics System](docs/guides/topics-system.md)

-   **How-Tos**
    -   [Partial Adoption](docs/howtos/partial-adoption.md)
    -   [Format Tables](docs/howtos/tables.md)
    -   [Render Only](docs/howtos/render-only.md)

## Contributing

Contributions are very welcome , be it a feature request, a question and even feedback.
Use the issue tracker to report bugs and feature requests.

For code contributions, the standar practices apply : tests for changed code, passing test suite, Pull Request with code and motivation.

## License

MIT 
