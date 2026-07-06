# Styled Help

Standout can replace clap's built-in help with themed, template-driven output. Instead of clap's fixed format, your `--help` renders through the same MiniJinja + style-tag pipeline as the rest of your CLI.

This gives you bold headers, consistent alignment, and a "Learn More" section linking to help topics. For CLIs with many commands, you can organize subcommands into named groups with section headers, help text, and visual separators.

## Enabling Help Handling

Help interception is **opt-in**. Enable it with `.help_handling(true)`:

```rust
App::builder()
    .help_handling(true)
    .build()?;
```

When enabled, standout:

1. Disables clap's default `help` subcommand and `--help`/`-h` flag
2. Registers its own `help` subcommand (with `--page` for pager support) and a custom `--help`/`-h` flag
3. Intercepts all help requests (`help`, `--help`, `-h` — at root and subcommand level) and renders them through a MiniJinja template with style tags

All three invocation forms produce identical output. Subcommand-level help (e.g. `myapp build --help`) also works, rendering that subcommand's help through standout.

**Required for features:** `command_groups` and topics require `help_handling(true)`. If you configure either without it, `build()` will panic.

## Styling User-Provided Strings

When help interception is enabled, your clap `about` and `help` strings are rendered through standout's BBCode parser, so they can use any tag defined in your stylesheet:

```rust
#[command(name = "myapp", about = "[bold]myapp[/bold] — a small CLI")]
struct Cli { /* ... */ }
```

To emit a literal `[` or `]` in help text, escape it with a backslash: `\[` and `\]`. Other backslashes (file paths, regex examples like `\d+`) pass through unchanged. To emit a literal `\[`, write `\\[`.

```rust
#[command(about = "Match pattern \\[regex: \\d+\\]")]
// renders as: Match pattern [regex: \d+]
```

## Default Behavior

Without any group configuration, all subcommands appear in a single "Commands" section:

```text
My application

USAGE
  myapp <COMMAND>

COMMANDS
  init:         Initialize the project
  list:         List all items
  delete:       Delete an item
  config:       Manage configuration

OPTIONS
  --output      Output format
```

## Command Groups

CLIs with many commands (20+) benefit from organized help. The `CommandGroup` struct lets you split subcommands into named sections:

```rust
use standout::cli::{App, CommandGroup};

App::builder()
    .help_handling(true)
    .command_groups(vec![
        CommandGroup {
            title: "Commands".into(),
            help: None,
            commands: vec![
                Some("init".into()),
                Some("create".into()),
                Some("list".into()),
                Some("search".into()),
            ],
        },
        CommandGroup {
            title: "Per Pad(s)".into(),
            help: Some(
                "These commands accept one or more pad ids: <id> or ranges <id>-<id>\n\
                 ex: $ padz view 3 5 7-9  # views pads 3, 5, 7, 8 and 9".into()
            ),
            commands: vec![
                Some("open".into()), Some("view".into()), Some("peek".into()),
                None, // blank line separator
                Some("pin".into()), Some("unpin".into()),
                None,
                Some("complete".into()), Some("reopen".into()),
            ],
        },
        CommandGroup {
            title: "Misc".into(),
            help: None,
            commands: vec![
                Some("completions".into()),
                Some("help".into()),
                Some("config".into()),
            ],
        },
    ])
    .build()?;
```

This produces:

```text
COMMANDS
  init:         Initialize the store
  create:       Create a new pad
  list:         List pads
  search:       Search pads

PER PAD(S)
  These commands accept one or more pad ids: <id> or ranges <id>-<id>
  ex: $ padz view 3 5 7-9  # views pads 3, 5, 7, 8 and 9

  open:         Open a pad in the editor
  view:         View one or more pads
  peek:         Peek at pad content previews

  pin:          Pin one or more pads
  unpin:        Unpin one or more pads

  complete:     Mark pads as done
  reopen:       Reopen pads

MISC
  completions:  Generate shell completions
  help:         Print this message
  config:       Get or set configuration
```

### Blank Line Separators

Use `None` entries in the `commands` vec to insert blank lines within a group. This creates visual sub-clusters without introducing nested group hierarchy:

```rust
commands: vec![
    Some("open".into()),
    Some("view".into()),
    None,               // blank line
    Some("pin".into()),
    Some("unpin".into()),
],
```

### Ungrouped Commands

Commands that exist in your clap definition but don't appear in any `CommandGroup` are automatically appended to an "Other" section. This is a safety net: if you add a new subcommand but forget to add it to the group config, it still shows up in help. Silently hiding commands would be worse than slightly messy help.

### Group Help Text

Each group can include optional help text displayed between the section header and the command list. Use this to explain shared arguments, conventions, or usage patterns that apply to all commands in the group.

## Standalone Rendering

You can render help without `App` using `render_help` directly:

```rust
use standout::cli::{render_help, CommandGroup, HelpConfig};
use standout::OutputMode;

let config = HelpConfig {
    output_mode: Some(OutputMode::Text),
    command_groups: Some(vec![
        CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("init".into()), Some("list".into())],
        },
    ]),
    ..Default::default()
};

let output = render_help(&cmd, Some(config))?;
println!("{}", output);
```

## Validation

The group config is static — it should be validated at test time, not when a user runs `--help`. Use `validate_command_groups` in a `#[test]`:

```rust
use standout::cli::{validate_command_groups, CommandGroup};
use clap::CommandFactory;

#[test]
fn test_help_groups_match_commands() {
    let cmd = Cli::command();
    let groups = my_command_groups();
    validate_command_groups(&cmd, &groups).unwrap();
}
```

**What it checks:**

- **Phantom reference** — a group names a command that doesn't exist in the clap definition (catches typos and stale configs)

**What it allows:**

- **Ungrouped commands** — commands not in any group are OK; they auto-append to "Other" at render time

This follows the same pattern as `app.verify_command(&cmd)` for handler/argument validation.

## Themes

Help rendering uses a theme to style output. The default theme applies bold to headers and command names:

```rust
pub fn default_help_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())   // COMMANDS, OPTIONS, etc.
        .add("item", Style::new().bold())     // Command/option names
        .add("desc", Style::new())            // Descriptions
        .add("usage", Style::new())           // Usage line
        .add("example", Style::new())         // Examples section
        .add("about", Style::new())           // About text
}
```

Override via `HelpConfig`:

```rust
let config = HelpConfig {
    theme: Some(
        Theme::new()
            .add("header", Style::new().bold().cyan())
            .add("item", Style::new().green())
            .add("desc", Style::new())
            .add("usage", Style::new())
            .add("example", Style::new().dim())
            .add("about", Style::new())
    ),
    ..Default::default()
};
```

Or when using `AppBuilder`, set the theme with `.theme()` — it applies to both help and command output.

## Custom Templates

The default template renders about, usage, grouped commands, options, examples, and learn-more topics. Override it via `HelpConfig::template`:

```rust
let config = HelpConfig {
    template: Some(my_custom_template.into()),
    ..Default::default()
};
```

### Template Variables

The template receives a `HelpData` struct with these fields:

| Variable | Type | Description |
| ---------- | ------ | ------------- |
| `about` | String | Command's about text |
| `usage` | String | Usage line (without "Usage: " prefix) |
| `subcommands` | Vec | Command groups (each with `title`, `help`, `commands`) |
| `options` | Vec | Option groups (each with `title`, `options`) |
| `examples` | String | Examples text |
| `learn_more` | Vec | Topic list items (each with `name`, `title`, `padding`) |

### Group Fields in Templates

Each subcommand group has:

- `group.title` — section header (rendered as `group.title | upper` in the default template)
- `group.help` — optional help text for the group
- `group.commands` — list of command entries

Each command entry has:

- `cmd.name` — command name
- `cmd.about` — command description
- `cmd.padding` — alignment spaces
- `cmd.separator` — true for blank-line separator entries

### Example Custom Template

```jinja
[about]{{ about }}[/about]

[header]USAGE[/header]
  [usage]{{ usage }}[/usage]
{%- for group in subcommands %}

[header]{{ group.title | upper }}[/header]
{%- if group.help %}
  [desc]{{ group.help }}[/desc]
{% endif %}
{%- for cmd in group.commands %}
{%- if cmd.separator %}

{%- else %}
  [item]{{ cmd.name }}[/item]:{{ cmd.padding }}[desc]{{ cmd.about }}[/desc]
{%- endif %}
{%- endfor %}
{%- endfor %}
```

Style tags like `[header]...[/header]` are resolved against the theme. Unknown tags pass through or show a `?` indicator depending on the output mode.

## Output Modes

Help respects the `--output` flag. In `Text` mode, style tags are stripped. In `Json` mode, the `HelpData` struct is serialized directly. This means help output is machine-readable when needed:

```bash
myapp help --output json
```
