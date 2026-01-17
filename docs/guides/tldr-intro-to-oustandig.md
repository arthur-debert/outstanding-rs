# Fast Paced intro to your First Outstanding Based Command

This is a terse and direct how to for more experienced developers or at least the ones in a hurry.
It skimps rationale , design and other useful bits you can read from the [longer form version](full-tutorial.md)

## Prerequisites

A cli app, that uses clap for arg parsing.
A command fucntion that is pure logic, that is , returns the result, and does not print to stdout or format output.

For this guide's purpose we'll use a ficticious "list" command of our todo list manager

## The Core: A pure function logic handler

The logic handler: receives parsed cli args, and returns a serializable data structure:

```rust
    pub fn list(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {}
```

## Making it oustanding

### 1. The File System

Create a templates/list.jinja and styles/default.css:

```text
    src/
        ├── handlers.rs         # where list it
        ├── templates/          # outstanding will match templates name against rel. paths from temp root, here
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
        .templates(embed_templates!("src/templates"))    //  Sets the root template path, hot relead for dev, embeded in release
        .styles(embed_styles!("src/styles"))                       //  Likewise the styles root
        .default_theme("default")                                       // Use styles/default.css or default.yaml
        .commands(Commands::dispatch_config())          // Register handlers from derive macro
    .build()?;
```

Connect your logic to a command name and template :

```rust
    #[dispatch(handlers = handlers)]
    pub enum Commands {
          ...
          list,
    }
```

And finally, run in main, the autodispatcher:

```rust
    match app.run(Cli::command(), std::env::args()) {
        // If you've got other commands on vanilla manual dispatch, call it for unported commands
        RunResult::NoMatch(matches) => legacy_dispatch(matches),  // Your existing handler
    }
```
