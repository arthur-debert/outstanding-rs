# Fast Paced intro to your First Standout Based Command

This is a terse and direct how to for more experienced developers or at least the ones in a hurry.
It skimps rationale, design and other useful bits you can read from the [longer form version](intro-to-standout.md)

## Prerequisites

A cli app, that uses clap for arg parsing.
A CLI-free library function that owns the application behavior, plus a handler
that adapts parsed CLI input to that library and returns serializable view data.
The library must not depend on Clap, Standout, `CommandContext`, templates,
styles, environment lookup, or app construction.

For this guide's purpose we'll use a fictitious "list" command of our todo list manager

## The core and its handler adapter

The library owns filtering, validation, and state transitions. The handler is a
CLI adapter: it receives parsed arguments, calls the library, and returns a
serializable CLI view:

```rust
    #[handler]
    pub fn list(
        #[flag] all: bool,
        #[ctx] ctx: &CommandContext,
    ) -> Result<Output<TodoResult>, anyhow::Error> {
        let store = ctx.app_state.get_required::<TodoStore>()?;
        let filter = if all { TodoFilter::All } else { TodoFilter::Pending };
        Ok(Output::Render(TodoResult::from(store.list(filter))))
    }
```

## Making it outstanding

### 1. The File System

Create a templates/list.jinja and styles/default.css:

```text
    src/
        ├── handlers.rs         # where list it
        ├── templates/          # standout will match templates name against rel. paths from temp root, here
            ├── list.jinja      # the template to render list, name matched against the command name
        ├── styles/             #  likewise for themes, this sets a theme called "default"
            ├── default.css     # the default style for the command, filename will be the theme name
```

### 2. Define your styles

```css
    .done {
        text-decoration: line-through;
        color: gray;
    }
    .pending {
        font-weight: bold;
        color: white;
    }
    .index {
        color: yellow;
    }
```

### 3. Write your template

```Jinja
    {% if message %}
        [message]{{ message }} [/message]
    {% endif %}
    {% for todo in todos %}
        [index]{{ loop.index }}.[/index] [{{ todo.status }}]{{ todo.title }}[/{{ todo.status }}]
    {% endfor %}
```

### 4. Putting it all together

Configure the app:

```rust
    let app = App::builder()
        .app_state(Database::connect()?)                 // Optional: shared state for handlers
        .templates(embed_templates!("src/templates"))    // Sets the root template path
        .styles(embed_styles!("src/styles"))             // Likewise the styles root
        .default_theme("default")                        // Use styles/default.css
        .commands(Commands::dispatch_config())?          // Register handlers from derive macro
        .build()?;
```

> Handlers access shared state via `ctx.app_state.get_required::<Database>()?`. See [App State and Extensions](../crates/dispatch/topics/app-state.md) for details.

Connect your logic to a command name and template:

```rust
    #[derive(Subcommand, Dispatch)]
    #[dispatch(handlers = handlers)]
    pub enum Commands {
          // ...
          #[dispatch(pure)]
          List,
    }
```

And finally, run in main, the autodispatcher:

```rust
    if !app.run(Cli::command(), std::env::args()) {
        // If some commands still use manual dispatch, fall back here.
        legacy_dispatch();
    }
```

When the fallback needs the unmatched `ArgMatches`, call `run_with(cmd, args,
target, sources)` instead of `run`, and match `DispatchResult::NoMatch(matches)`
on the result's `into_outcome()`.
