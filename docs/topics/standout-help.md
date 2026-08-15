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

1. Disables clap's default `help` subcommand and registers its own (with `--page` for pager support), subject to the [install policy](#the-help-word) below
2. **Keeps** clap's native `--help`/`-h` flag, on purpose: clap's flag short-circuits argument validation, so `myapp build --help` renders even when required arguments are missing
3. Intercepts all help requests and renders them through a MiniJinja template with style tags — the `help` word, which clap routes like any other subcommand, and clap's `DisplayHelp` (from `--help`/`-h`, at root and subcommand level)

Every form that is available renders the same help, through the same template and theme — with one exception, which is about the *form*, not the entry point: `--output` reaches the `help` word but not the flags. `myapp help --output text` renders in text mode; `myapp --help --output text` renders in `Auto` and the mode is ignored. The reason is where each form is answered: the word is a subcommand, so clap parses its line in full, globals included, while `--help` short-circuits inside clap before the parse completes — so there are no matches to read a mode from when its `DisplayHelp` is rendered.

Subcommand-level help (e.g. `myapp build --help`) also works, rendering that subcommand's help through standout.

**Required for features:** `command_groups` and topics require `help_handling(true)`. If you configure either without it, `build()` returns a `SetupError`.

### Whichever entry point you use

Help is answered the same way through both parse paths — `run()` / `run_to_string()` / `dispatch_from()` and `get_matches_from()` / `parse_from()`. Same install policy for the word, same interception of `--help` / `-h`, same rendering: an application's entry point is not a fact about what `myapp help` means.

The one thing the two paths cannot share is `--page`, because paging is a terminal side effect and only a *printing* entry point may perform it. `run()` and `parse_from()` hand the text to the pager; the capture APIs return it instead — `run_to_string()` marks it `SuccessKind::PagedHelp`, `get_matches_from()` returns `HelpResult::PagedHelp` — and leave the decision to you.

## The `help` Word

`--help` and `-h` are flags: they are always available and can never collide with your data. A bare `help` is different — at the root of a CLI with no subcommands, a bare word is *data*. `echo help`, `grep help`, and `ls help` all treat it as such, and a tool whose positional is a revision range or a file name would be wrong to swallow it.

So standout only installs the word where it knows nothing else can claim it:

| Root shape | `help` word |
| --- | --- |
| Has subcommands | Installed — a bare word there is already a command |
| Flat, no positionals | Installed — nothing to collide with |
| Flat, with positionals | **Opt-in only** — see below |

For the third shape, only your application knows whether its positional domain excludes the word. Opt in with `.help_word(true)`:

```rust
// `mytool <RANGE>` — a revision range is never the word "help".
App::builder()
    .help_handling(true)
    .help_word(true)
    .build()?;
```

Opting in accepts the cost: the literal word `help` can no longer reach the positional, and `--` becomes the escape for it — `mytool -- help` passes the string through. Without the opt-in, `--help` / `-h` remain the only spelling, and they still render themed help.

`help_word(true)` only ever *adds* the word; it is not a way to suppress `help` on a CLI that has subcommands. It requires `help_handling(true)` — the word is standout's own subcommand, so `build()` returns a `SetupError` without interception.

### Why the word is reachable at all

On a flat CLI whose root arguments are required, an injected `help` subcommand used to be advertised in help output and impossible to run: clap validates the root's requirements before routing, so `myapp help` failed with "the following required arguments were not provided" instead of printing help.

The fix is a declaration, not a parser of standout's own. Where standout installs the word, it also sets clap's `subcommand_negates_reqs`, which suspends the root's requirements once a command is named — so `myapp help` routes to the word, while `myapp` on its own still reports its missing arguments and `myapp <RANGE>` still parses as data. The word's arguments (`myapp help topics`, `myapp help --page`, `myapp help --output text`) are clap's to parse, like any other subcommand's.

The cost is worth naming: `subcommand_negates_reqs` applies to *your* subcommands too, so a root that declares required arguments stops requiring them once any command is named. That is why standout sets it only where it installs the word, and never on a CLI that did not get one. See [ADR-0018](../adr/0018-let-the-parser-classify-the-command-line.md).

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

The `help` word respects the `--output` flag, but only as far as *styling*. Help is always the rendered template; the mode decides what happens to its style tags — applied in `Term`, stripped in `Text`, left visible as `[header]…[/header]` in `TermDebug`:

```bash
myapp help --output text
```

`--help` / `-h` do not take the flag with them (see [above](#enabling-help-handling)): they render in `Auto`, which styles for the terminal it finds. Spell the mode with the word when you need it.

The structured modes (`json`, `yaml`, `xml`, `csv`) strip the tags exactly as `Text` does. None of them serializes `HelpData`, so help is themed prose in every mode, not a machine-readable document. If you need help as data, render it yourself: `HelpData` is what a [custom template](#custom-templates) receives, and a template that emits JSON is the seam for it.
