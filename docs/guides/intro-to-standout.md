# Standout How To

This is a small, focused guide for adopting Standout in a working shell application. Each step is self-sufficient, takes a positive step towards a sane CLI design, and can be incrementally merged. This can be done for one command (probably a good idea), then replicated to as many as you'd like.

Note that only 2 out of 8 steps are Standout related. The others are generally good practices and clear designs for maintainable shell programs. This is not an accident, as Standout's goal is to allow your app to keep a great structure effortlessly, while providing testability, rich and fast output design, and more.

For explanation's sake, we will show a hypothetical list command for tdoo, a todo list manager.

**See Also:**

- [Handler Contract](../crates/dispatch/topics/handler-contract.md) - detailed handler API
- [App State and Extensions](../crates/dispatch/topics/app-state.md) - dependency injection patterns
- [Styling System](../crates/render/topics/styling-system.md) - themes and styles in depth
- [Output Modes](../topics/output-modes.md) - all output format options
- [Partial Adoption](../crates/dispatch/topics/partial-adoption.md) - migrating incrementally

## 1. Start: The Argument Parsing

Arg parsing is insanely intricate and deceptively simple. In case you are not already: define your application's interface with clap. Nothing else is worth doing until you have a sane starting point.

If you don't have clap set up yet, here's a minimal starting point:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tdoo")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all todos
    List {
        #[arg(short, long)]
        all: bool,
    },
    /// Add a new todo
    Add {
        title: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::List { all } => list_command(all),
        Commands::Add { title } => add_command(&title),
    }
}
```

(If you are using a non-clap-compatible crate, for now, you'd have to write an adapter for clap.)

> **Verify:** Run `cargo build` - it should compile without errors.

## 2. Hard Split Logic and Formatting

Now, your command should be split into two functions: the logic handler and its rendering. Don't worry about the specifics, do the straightest path from your current code.

This is the one key step, the key design rule. And that's not because Standout requires it, rather the other way around: Standout is designed on top of it, and keeping it separate and easy to iterate on both logic and presentation under this design is Standout's key value.

If your CLI is in good shape this will be a small task, otherwise you may find yourself patching together print statements everywhere, tidying up the data model and centralizing the processing. The silver lining here being: if it takes considerable work, there will be considerable gain in doing so.

**Before** (tangled logic and output):

```rust
fn list_command(show_all: bool) {
    let todos = storage::list().unwrap();
    println!("Your Todos:");
    println!("-----------");
    for (i, todo) in todos.iter().enumerate() {
        if show_all || todo.status == Status::Pending {
            let marker = if todo.status == Status::Done { "[x]" } else { "[ ]" };
            println!("{}. {} {}", i + 1, marker, todo.title);
        }
    }
    if todos.is_empty() {
        println!("No todos yet!");
    }
}
```

**After** (clean separation):

```rust
use clap::ArgMatches;

// Data types for your domain
#[derive(Clone)]
pub enum Status { Pending, Done }

#[derive(Clone)]
pub struct Todo {
    pub title: String,
    pub status: Status,
}

pub struct TodoResult {
    pub message: Option<String>,
    pub todos: Vec<Todo>,
}

// This is your core logic handler, receiving parsed clap args
// and returning a pure Rust data type.
//
// Note: This example uses immutable references. If your handler needs
// mutable state (&mut self), see the "Mutable Handlers" section below.
pub fn list(matches: &ArgMatches) -> TodoResult {
    let show_done = matches.get_flag("all");
    let todos = storage::list().unwrap();

    let filtered: Vec<Todo> = if show_done {
        todos
    } else {
        todos.into_iter()
            .filter(|t| matches!(t.status, Status::Pending))
            .collect()
    };

    TodoResult {
        message: None,
        todos: filtered,
    }
}

// This will take the Rust data type and print the result to stdout
pub fn render_list(result: TodoResult) {
    if let Some(msg) = result.message {
        println!("{}", msg);
    }
    for (i, todo) in result.todos.iter().enumerate() {
        let status = match todo.status {
            Status::Done => "[x]",
            Status::Pending => "[ ]",
        };
        println!("{}. {} {}", i + 1, status, todo.title);
    }
}

// And the orchestrator:
pub fn list_command(matches: &ArgMatches) {
    render_list(list(matches))
}
```

> **Verify:** Run `cargo build` and then `tdoo list` - output should look identical to before.

### Intermezzo A: Milestone - Logic and Presentation Split

**What you achieved:** Your command logic is now a pure function that returns data.
**What's now possible:**

- All of your app's logic can be unit tested as any code, from the logic inwards.
- You can test by feeding input strings and verifying your logic handler gets called with the right parameters.
- The rendering can also be tested by feeding data inputs and matching outputs (though this is brittle).

**What's next:** Making the return type serializable for automatic JSON/YAML output.
**Your files now:**

```text
src/
├── main.rs          # clap setup + orchestrators
├── handlers.rs      # list(), add() - pure logic
└── render.rs        # render_list(), render_add() - output formatting
```

## 3. Fine Tune the Logic Handler's Return Type

While any data type works, Standout's renderer takes a generic type that must implement `Serialize`. This enables automatic JSON/YAML output modes and template rendering through MiniJinja's context system. This is likely a small change, and beneficial as a baseline for logic results that will simplify writing renderers later.

Add serde to your `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

Update your types:

```rust
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pending, Done }

#[derive(Clone, Serialize)]
pub struct Todo {
    pub title: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct TodoResult {
    pub message: Option<String>,
    pub todos: Vec<Todo>,
}
```

> **Verify:** Run `cargo build` - it should compile without errors.

## 4. Replace Imperative Print Statements With a Template

Reading a template of an output next to the substituting variables is much easier to reason about than scattered prints, string concats and the like.

This step is optional - if your current output is simple, you can skip to step 5. If you want an intermediate checkpoint, use Rust's format strings:

```rust
pub fn render_list(result: TodoResult) {
    let output = format!(
        "{header}\n{todos}",
        header = result.message.unwrap_or_default(),
        todos = result.todos.iter().enumerate()
            .map(|(i, t)| format!("{}. [{}] {}", i + 1, t.status, t.title))
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("{}", output);
}
```

> **Verify:** Run `tdoo list` - output should still work.

## 5. Use a MiniJinja Template String

Rewrite your `std::fmt` or imperative prints into a MiniJinja template string, and add minijinja to your crate. If you're not familiar with it, it's a Rust implementation of Jinja, pretty much a de-facto standard for more complex templates.

**Resources:**

- [MiniJinja docs](https://docs.rs/minijinja)
- [Jinja syntax reference](https://jinja.palletsprojects.com/en/3.1.x/templates/)

Add minijinja to your `Cargo.toml`:

```toml
[dependencies]
minijinja = "2"
```

And then you call render in MiniJinja, passing the template string and the data to use. So now your rendering function looks like this:

```rust
pub fn render_list(result: TodoResult) {
    let output_tmpl = r#"
{% if message %}
    {{ message }}
{% endif %}
{% for todo in todos %}
    {{ loop.index }}. [{{ todo.status }}] {{ todo.title }}
{% endfor %}
"#;

    let env = minijinja::Environment::new();
    let tmpl = env.template_from_str(output_tmpl).unwrap();
    let output = tmpl.render(&result).unwrap();
    println!("{}", output);
}
```

> **Verify:** Run `tdoo list` - output should match (formatting may differ slightly).

## 6. Use a Dedicated Template File

Now, move the template content into a file (say `src/templates/list.jinja`), and load it in the rendering module. Dedicated files have several advantages: triggering editor/IDE support for the file type, more descriptive diffs, less risk of breaking the code/build and, in the event that you have less technical people helping out with the UI, a much cleaner and simpler way for them to contribute.

Create `src/templates/list.jinja`:

```jinja
{% if message %}{{ message }} {% endif %}
{% for todo in todos %}
    {{ loop.index }}. [{{ todo.status }}] {{ todo.title }}
{% endfor %}
```

Update your render function to load from file:

```rust
pub fn render_list(result: TodoResult) {
    let template_content = include_str!("templates/list.jinja");
    let env = minijinja::Environment::new();
    let tmpl = env.template_from_str(template_content).unwrap();
    let output = tmpl.render(&result).unwrap();
    println!("{}", output);
}
```

> **Verify:** Run `tdoo list` - output should be identical.

### Intermezzo B: Declarative Output Definition

**What you achieved:** Output is now defined declaratively in a template file, separate from Rust code.
**What's now possible:**

- Edit templates without recompiling (with minor changes to loading)
- Non-Rust developers can contribute to UI
- Clear separation in code reviews: "is this a logic change or display change?"
- Use partials, filters, and macros for complex outputs (see [Templating](../crates/render/topics/templating.md))

**What's next:** Hooking up Standout for automatic dispatch and rich output.
Also, notice we've yet to do anything Standout-specific. This is not a coincidence—the framework is designed around this pattern, making testability, fast iteration, and rich features natural outcomes of the architecture.
**Your files now:**

```text
src/
├── main.rs
├── handlers.rs
├── render.rs
└── templates/
    └── list.jinja
```

## 7. Standout: Offload the Handler Orchestration

And now the Standout-specific bits finally show up.

### 7.1 Add Standout to your Cargo.toml

```toml
[dependencies]
standout = "2"
```

> **Verify:** Run `cargo build` - dependencies should download and compile.

### 7.2 Create Handlers with the `#[handler]` Macro

The `#[handler]` macro transforms pure Rust functions into Standout-compatible handlers. Write your logic as a simple function, annotate parameters, and the macro generates the wrapper that extracts CLI arguments.

See [Handler Contract](../crates/dispatch/topics/handler-contract.md) for full handler API details.

```rust
use standout_macros::handler;

mod handlers {
    use super::*;

    // Pure function - easy to test, no boilerplate
    #[handler]
    pub fn list(#[flag] all: bool) -> Result<TodoResult, anyhow::Error> {
        let todos = storage::list()?;
        let filtered = if all {
            todos
        } else {
            todos.into_iter().filter(|t| !t.done).collect()
        };
        Ok(TodoResult { message: None, todos: filtered })
    }

    #[handler]
    pub fn add(#[arg] title: String) -> Result<TodoResult, anyhow::Error> {
        let todo = storage::add(&title)?;
        Ok(TodoResult {
            message: Some(format!("Added: {}", title)),
            todos: vec![todo],
        })
    }
}
```

The `#[handler]` macro:
- Extracts CLI arguments automatically from `ArgMatches`
- Converts `#[flag]` to `bool` flags, `#[arg]` to required/optional args
- Auto-wraps `Result<T, E>` in `Output::Render` for you

**Parameter Annotations:**

| Annotation | Type | What it extracts |
|------------|------|------------------|
| `#[flag]` | `bool` | Boolean flag (`--verbose`) |
| `#[arg]` | `T` | Required argument |
| `#[arg]` | `Option<T>` | Optional argument |
| `#[arg]` | `Vec<T>` | Multiple values |
| `#[ctx]` | `&CommandContext` | Access to context (when needed) |

### 7.2.1 Connect Commands to Handlers

```rust
use clap::Subcommand;
use standout::cli::Dispatch;

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    List,
    Add,
}
```

The derive macro matches command variants to handler functions by name (`List` → `handlers::list`).

> **Verify:** Run `cargo build` - it should compile without errors.

### 7.2.2 Accessing App State (Optional)

When your handler needs shared resources like databases, use the `#[ctx]` annotation:

```rust
#[handler]
pub fn list(#[flag] all: bool, #[ctx] ctx: &CommandContext) -> Result<TodoResult, anyhow::Error> {
    let db = ctx.app_state.get_required::<Database>()?;
    let todos = db.list()?;
    Ok(TodoResult { message: None, todos })
}
```

> **Note:** For the full handler signature (without macros) and advanced patterns, see [Handler Contract](../crates/dispatch/topics/handler-contract.md).

### 7.3 Configure AppBuilder

Use AppBuilder to configure your app. Instantiate the builder, add the path for your templates. See [App Configuration](../topics/app-configuration.md) for all configuration options.

```rust
use standout::cli::App;
use standout::{embed_templates, embed_styles};

let app = App::builder()
    .templates(embed_templates!("src/templates"))   // Embeds all .jinja/.j2 files
    .commands(Commands::dispatch_config())          // Register handlers from derive macro
    .build()?;
```

> **Verify:** Run `cargo build` - it should compile without errors.

### 7.3.1 Injecting Shared State (Optional)

If your handlers need access to shared resources like database connections or configuration, use `app_state`:

```rust
let app = App::builder()
    .app_state(Database::connect()?)    // Shared across all handlers
    .app_state(Config::load()?)
    .templates(embed_templates!("src/templates"))
    .commands(Commands::dispatch_config())
    .build()?;
```

Handlers retrieve app state via `#[ctx]`:

```rust
#[handler]
pub fn list(#[flag] all: bool, #[ctx] ctx: &CommandContext) -> Result<TodoResult, anyhow::Error> {
    let db = ctx.app_state.get_required::<Database>()?;
    let todos = db.list()?;
    Ok(TodoResult { message: None, todos })
}
```

> **Note:** For per-request state (user sessions, request IDs), use pre-dispatch hooks with `ctx.extensions`. See [App State and Extensions](../crates/dispatch/topics/app-state.md) for the full story.

### 7.4 Wire up main()

The final bit: handling the dispatching off to Standout:

```rust
use standout::cli::App;
use standout::embed_templates;

fn main() -> anyhow::Result<()> {
    let app = App::builder()
        .templates(embed_templates!("src/templates"))
        .commands(Commands::dispatch_config())
        .build()?;

    // Run with auto dispatch - handles parsing and execution
    app.run(Cli::command(), std::env::args());
    Ok(())
}
```

If your app has other clap commands that are not managed by Standout, check for unhandled commands. See [Partial Adoption](../crates/dispatch/topics/partial-adoption.md) for details on incremental migration.

```rust
if let Some(matches) = app.run(Cli::command(), std::env::args()) {
    // Standout didn't handle this command, fall back to legacy
    legacy_dispatch(matches);
}
```

> **Verify:** Run `tdoo list` - it should work as before.
> **Verify:** Run `tdoo list --output json` - you should get JSON output for free!

And now you can remove the boilerplate: the orchestrator (`list_command`) and the rendering (`render_list`). You're pretty much at global optima: a single line of derive macro links your app logic to a command name, a few lines configure Standout, and auto dispatch handles all the boilerplate.

For the next commands you'd wish to migrate, this is even simpler. Say you have a "create" logic handler: add a "create.jinja" to that template dir, add the derive macro for the create function and that is it. By default the macro will match the command's name to the handlers and to the template files, but you can change these and map explicitly to your heart's content.

### Intermezzo C: Welcome to Standout

**What you achieved:** Full dispatch pipeline with zero boilerplate.

**What's now possible:**

- Alter the template and re-run your CLI, without compilation, and the new template will be used
- Your CLI just got multiple output modes via `--output` (see [Output Modes](../topics/output-modes.md)):
  - **term**: rich shell formatting (more about this on the next step)
  - **term-debug**: print formatting info for testing/debugging
  - **text**: plain text, no styling
  - **auto**: the default, rich term that degrades gracefully
  - **json, csv, yaml**: automatic serialization of your data

**What's next:** Adding rich styling to make the output beautiful.

**Your files now:**

```text
src/
├── main.rs              # App::builder() setup
├── commands.rs          # Commands enum with #[derive(Dispatch)]
├── handlers.rs          # list(), add() with #[handler] - pure functions returning Result<T, E>
└── templates/
    ├── list.jinja
    └── add.jinja
```

## 8. Make the Output Awesome

Let's transform that mono-typed, monochrome string into a richer and more useful UI. Borrowing from web apps setup, we keep the content in a template file, and we define styles in a stylesheet file.

See [Styling System](../crates/render/topics/styling-system.md) for full styling documentation.

### 8.1 Create the stylesheet

Create `src/styles/default.css`:

```css
/* Styles for completed todos */
.done {
    text-decoration: line-through;
    color: gray;
}

/* Style for todo index numbers */
.index {
    color: yellow;
}

/* Style for pending todos */
.pending {
    font-weight: bold;
    color: white;
}

/* Adaptive style for messages */
.message {
    color: cyan;
}

@media (prefers-color-scheme: light) {
    .pending { color: black; }
}

@media (prefers-color-scheme: dark) {
    .pending { color: white; }
}
```

Or if you prefer YAML (`src/styles/default.yaml`):

```yaml
done: strikethrough, gray
index: yellow
pending:
  bold: true
  fg: white
  light:
    fg: black
  dark:
    fg: white
message: cyan
```

> **Verify:** The file exists at `src/styles/default.css` or `src/styles/default.yaml`.

### 8.2 Add style tags to your template

Update `src/templates/list.jinja` with style tags:

```jinja
{% if message %}[message]{{ message }}[/message]
{% endif %}
{% for todo in todos %}
[index]{{ loop.index }}.[/index] [{{ todo.status }}]{{ todo.title }}[/{{ todo.status }}]
{% endfor %}
```

The style tags use BBCode-like syntax: `[style-name]content[/style-name]`

Notice how we use `[{{ todo.status }}]` dynamically - if `todo.status` is "done", it applies the `.done` style; if it's "pending", it applies the `.pending` style.

> **Verify:** The template file is updated.

### 8.3 Wire up styles in AppBuilder

Add the styles to your app builder:

```rust
let app = App::builder()
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))       // Load stylesheets
    .default_theme("default")                  // Use styles/default.css or default.yaml
    .commands(Commands::dispatch_config())
    .build()?;
```

> **Verify:** Run `cargo build` - it should compile without errors.
> **Verify:** Run `tdoo list` - you should see colored, styled output!
> **Verify:** Run `tdoo list --output text` - plain text, no colors.

Now you're leveraging the core rendering design of Standout:

- File-based templates for content, and stylesheets for styles
- Custom template syntax with BBCode for markup styles `[style][/style]`
- Live reload: iterate through content and styling without recompiling

### Intermezzo D: The Full Setup Is Done

**What you achieved:** A fully styled, testable, multi-format CLI.

**What's now possible:**

- Rich terminal output with colors, bold, strikethrough
- Automatic light/dark mode adaptation
- JSON/YAML/CSV output for scripting and testing
- Hot reload of templates and styles during development
- Unit testable logic handlers

**Your final files:**

```text
src/
├── main.rs              # App::builder() setup
├── commands.rs          # Commands enum with #[derive(Dispatch)]
├── handlers.rs          # list(), add() with #[handler] - pure functions
├── templates/
│   ├── list.jinja       # with [style] tags
│   └── add.jinja
└── styles/
    └── default.css      # or default.yaml
```

For brevity's sake, we've ignored a bunch of finer and relevant points:

- The derive macros can set name mapping explicitly: `#[dispatch(handler = custom_fn, template = "custom.jinja")]`
- There are pre-dispatch, post-dispatch and post-render hooks (see [Execution Model](../crates/dispatch/topics/execution-model.md))
- Standout exposes its primitives as standalone crates (see [standout-render](../crates/render/guides/intro-to-rendering.md), [standout-dispatch](../crates/dispatch/guides/intro-to-dispatch.md))
- Powerful tabular layouts via the `col` filter (see [Tabular Layout](../crates/render/guides/intro-to-tabular.md))
- A help topics system for rich documentation (see [Topics System](../topics/topics-system.md))

Aside from exposing the library primitives, Standout leverages best-in-breed crates like MiniJinja and console::Style under the hood. The lock-in is really negligible: you can use Standout's BB parser or swap it, manually dispatch handlers, and use the renderers directly in your clap dispatch.

## Mutable Handlers (LocalApp)

If your application logic uses `&mut self` methods—common with database connections, file caches, or in-memory indices—you can use `LocalApp` instead of `App`:

```rust
use standout::cli::{LocalApp, Output};
use standout::embed_templates;

struct PadStore {
    index: HashMap<Uuid, Metadata>,
}

impl PadStore {
    fn complete(&mut self, id: Uuid) -> Result<()> {
        // This needs &mut self
        self.index.get_mut(&id).unwrap().completed = true;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut store = PadStore::load()?;

    LocalApp::builder()
        .app_state(Config::load()?)  // app_state works with LocalApp too
        .templates(embed_templates!("src/templates"))
        .command("complete", |m, ctx| {
            let id = m.get_one::<Uuid>("id").unwrap();
            store.complete(*id)?;  // &mut store works!
            Ok(Output::Silent)
        }, "")
        .command("list", |m, ctx| {
            // Even read-only handlers work with LocalApp
            Ok(Output::Render(store.list()))
        }, "{{ items }}")
        .build()?
        .run(Cli::command(), std::env::args());
    Ok(())
}
```

**Key differences from `App`:**

- `LocalApp::builder()` accepts `FnMut` closures (not just `Fn`)
- Handlers can capture `&mut` references to state
- No `Send + Sync` requirement on handlers
- `app.run()` takes `&mut self` instead of `&self`

Use `LocalApp` when:
- Your API has `&mut self` methods
- You want to avoid `Arc<Mutex<_>>` wrappers
- Your CLI is single-threaded (the common case)

See [Handler Contract](../crates/dispatch/topics/handler-contract.md) for the full comparison.

## Appendix: Common Errors and Troubleshooting

- Template not found
  - **Error:** `template 'list' not found`
  - **Cause:** The template path in `embed_templates!` doesn't match your file structure.
  - **Fix:** Ensure the path is relative to your `Cargo.toml`, e.g., `embed_templates!("src/templates")` and that the file is named `list.jinja`, `list.j2`, or `list.txt`.
- Style not applied
  - **Symptom:** Text appears but without colors/formatting.
  - **Cause:** Style name in template doesn't match stylesheet.
  - **Fix:** Check that `[mystyle]` in your template matches `.mystyle` in CSS or `mystyle:` in YAML. Run with `--output term-debug` to see style tag names.
- Handler not called
  - **Symptom:** Command runs but nothing happens or wrong handler runs.
  - **Cause:** Command name mismatch between clap enum variant and handler function.
  - **Fix:** Ensure enum variant `List` maps to function `handlers::list` (snake_case conversion). Or use explicit mapping: `#[dispatch(handler = my_custom_handler)]`
- JSON output is empty or wrong
  - **Symptom:** `--output json` produces unexpected results.
  - **Cause:** `Serialize` derive is missing or field names don't match template expectations.
  - **Fix:** Ensure all types in your result implement `Serialize`. Use `#[serde(rename_all = "lowercase")]` for consistent naming.
- Styles not loading
  - **Error:** `theme not found: default`
  - **Cause:** Stylesheet file missing or wrong path.
  - **Fix:** Ensure `src/styles/default.css` or `default.yaml` exists. Check `embed_styles!` path matches your file structure.
