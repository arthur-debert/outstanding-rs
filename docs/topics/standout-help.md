# Styled Help

Standout can replace clap's built-in help with themed, template-driven output. Instead of clap's fixed format, your `--help` renders through the same MiniJinja + style-tag pipeline as the rest of your CLI.

This gives you bold headers, consistent alignment, and a "Learn More" section linking to help topics. For CLIs with many commands, you can organize subcommands into named groups with section headers, help text, and visual separators.

## Help Handling

Help interception is **on by default**. An app that configures nothing renders
themed help; `.help_handling(false)` gives clap's own help back:

```rust
// Themed help, no call needed.
App::builder().build()?;

// Clap's own help instead.
App::builder().help_handling(false).build()?;
```

With help handling on, standout:

1. Disables clap's default `help` subcommand and registers its own (with `--page` for pager support), subject to the [install policy](#the-help-word) below
2. **Keeps** clap's native `--help`/`-h` flag, on purpose: clap's flag short-circuits argument validation, so `myapp build --help` renders even when required arguments are missing
3. Intercepts all help requests and renders them through a MiniJinja template with style tags — the `help` word, which clap routes like any other subcommand, and clap's `DisplayHelp` (from `--help`/`-h`, at root and subcommand level)

Every form that is available renders the same help, through the same template and theme, and `--output` reaches every form alike: `myapp help --output text` and `myapp --help --output text` both render in text mode. The two are answered differently — the word is a subcommand, so clap parses its line in full, globals included, while `--help` short-circuits inside clap before the parse completes — so the flags take their mode from a scan of the raw argv for the output flag: the last occurrence wins, `--output text` and `--output=text` alike, and nothing after `--` counts. A value that is not a mode (`--output txt`) is a clap usage error, exit 2, whichever form asked.

Subcommand-level help (e.g. `myapp build --help`) also works, rendering that subcommand's help through standout.

**Incompatible with the opt-out:** `command_groups` and topics need help interception to render. Configuring either alongside `.help_handling(false)` returns a `SetupError` from `build()`.

### Whichever entry point you use

Help is answered the same way through both parse paths — `run()` / `run_with()` and `get_matches_from()`. Same install policy for the word, same interception of `--help` / `-h`, same rendering: an application's entry point is not a fact about what `myapp help` means.

The one thing the two paths cannot share is `--page`, because paging is a terminal side effect and only a *printing* entry point may perform it. `run()` hands the text to the pager; the capture APIs return it instead — `run_with()` marks it `SuccessKind::PagedHelp`, `get_matches_from()` returns `HelpResult::PagedHelp` — and leave the decision to you.

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
App::builder().help_word(true).build()?;
```

Opting in accepts the cost: the literal word `help` can no longer reach the positional, and `--` becomes the escape for it — `mytool -- help` passes the string through. Without the opt-in, `--help` / `-h` remain the only spelling, and they still render themed help.

`help_word(true)` only ever *adds* the word; it is not a way to suppress `help` on a CLI that has subcommands. It cannot be combined with `.help_handling(false)` — the word is standout's own subcommand, so `build()` returns a `SetupError` without interception.

### On a flat CLI, the word describes a flat CLI

Clap's `help` is worded for a CLI with subcommands — "Print this message or the
help of the given subcommand(s)". On a flat CLI that sentence points at a
namespace that cannot exist, and the flat shape is exactly the one
`help_word(true)` serves. So the word describes the shape it is installed on:

| Root shape | `help` about |
| --- | --- |
| Has subcommands | Print this message or the help of the given subcommand(s) |
| Flat | Print this message |

A flat CLI also drops the COMMANDS section entirely when `help` would be its
only entry. The word is machinery standout installs, not part of your surface,
and a section listing nothing but the command that printed it is noise:

```text
COMMANDS
  help          Print this message
```

Registered topics earn the section back, because `help <topic>` is then a real
destination and the word is how a reader reaches it. A root with commands of its
own always keeps its section, `help` included.

### If your CLI already has a `help`

Where standout installs the word, the name is standout's. An application that claims it too — a clap subcommand called `help` (or aliased to it), or a registration whose first path segment is `help` (`.command_with("help", …)`, `.command_with("help.topic", …)`, a `.commands(|g| g.group("help", …))`) — is a configuration standout refuses rather than serves:

```text
duplicate command: help — this application's clap `Command` declares `help` (as a
subcommand name or alias), and standout installs a `help` word of its own, since
help handling is on by default. Rename the application's command, or call
.help_handling(false) to keep the name (help is then clap's own, and
command_groups and topics become unavailable)
```

Each spelling is caught the moment it becomes visible: a registration under the root `help` fails `build()`, while a clap-declared one is only visible when your `Command` reaches a parse entry point, so it comes back as `HelpResult::Error` / `DispatchResult::Error` before anything is parsed. Neither reaches clap, whose answer to two subcommands of one name is a debug assertion — a panic on a configuration, which is what `SetupError` exists to prevent.

Standing down — letting your `help` win and rendering nothing itself — is deliberately not offered. `myapp help` would then run your handler while `myapp --help` rendered standout's themed help: one CLI answering the same question two ways.

Unaffected: a `help` deeper in the tree (`myapp db help` is yours, at a path the word is never installed on), and any root that never gets the word — a flat CLI with positionals and no `.help_word(true)`, or any CLI built with `.help_handling(false)`.

### Why the word is reachable at all

On a flat CLI whose root arguments are required, an injected `help` subcommand used to be advertised in help output and impossible to run: clap validates the root's requirements before routing, so `myapp help` failed with "the following required arguments were not provided" instead of printing help.

The fix is a declaration, not a parser of standout's own. Where standout installs the word, it also sets clap's `subcommand_negates_reqs`, which suspends the root's requirements once a command is named — so `myapp help` routes to the word, while `myapp` on its own still reports its missing arguments and `myapp <RANGE>` still parses as data. The word's arguments (`myapp help topics`, `myapp help --page`, `myapp help --output text`) are clap's to parse, like any other subcommand's.

The cost is worth naming: `subcommand_negates_reqs` applies to *your* subcommands too, so a root that declares required arguments stops requiring them once any command is named. That is why standout sets it only where it installs the word, and never on a CLI that did not get one. See [ADR-0018](../adr/0018-let-the-parser-classify-the-command-line.md).

## Short and Long Help

Clap gives a command a terse `about` and an optional full `long_about`, and its
convention is that `-h` shows the first while `--help` shows the second. Themed
help keeps that distinction:

| Invocation | Renders |
| --- | --- |
| `-h` | `about` |
| `--help` | `long_about`, falling back to `about` |
| `help` | `long_about`, falling back to `about` — the spelled-out request reads like `--help` |

Standout has to recover the spelling itself: `--help` short-circuits inside clap
and arrives as a `DisplayHelp` error that names neither the flag that raised it
nor the command it was raised for. Both are recovered from a *parse*, not from a
scan of the argument list — the two flags are re-declared as ordinary global
arguments on a throwaway clone whose own help flag is disabled, and clap
answers. That keeps `--` termination, `--flag=value`, short-option clusters, and
option-value consumption (`-o h` is not a help request) the parser's business,
per [ADR-0018](../adr/0018-let-the-parser-classify-the-command-line.md).

Rendering help yourself with [`render_help`](#standalone-rendering) has no
invocation to classify, so it defaults to `HelpLength::Short`. Ask for the full
text with `length`:

```rust
let config = HelpConfig {
    length: HelpLength::Long,
    ..Default::default()
};
```

## What an Option Row Shows

Option rows carry the information clap surfaces about an argument — its
value syntax, description, default, and accepted values:

```text
OPTIONS
  --staged            Diff the staged changes
  --threshold <RATIO> Move/rename similarity threshold
  -c, --color <BOOL>  Enable ANSI color
  --output            Output format
                      default: auto
                      possible values: auto, term, text, term-debug, json, yaml, csv
  --output-file-path  Write output to file instead of stdout
```

The default and possible-value lines hang under the description column and
carry their own `[default]` and `[values]` tags, so a stylesheet can dim or
recolor them independently. Standout writes them as words rather than clap's
`[default: auto]` brackets: literal `[` would have to be escaped through the
style-tag parser, and the emphasis belongs to the theme. Hidden possible values
(`PossibleValue::hide`) are left out.

Clap's own flags are rows like any other. The extractor reads the command
*after* clap builds it, which is when `-h`/`--help` and — for an application
that sets a version — `-V`/`--version` come into existence, so the page names
the flags it accepts instead of listing only what the application declared.
They sort last, after the application's own options.

Options that take values render their metavar alongside the spelling, using an
explicit `value_name` when present and clap's fallback display otherwise. Pure
presence flags such as `ArgAction::SetTrue` and `SetFalse` do not render a
metavar and do not show parser-derived `true, false` values, because those words
are not valid command-line values for the flag.

### Positionals get their own section

Positionals render in an ARGUMENTS section ahead of OPTIONS, the way clap orders
them, rather than filed among the flags:

```text
ARGUMENTS
  RANGE         Git range to diff, e.g. main..HEAD

OPTIONS
  --staged      Diff the staged changes
```

A positional is listed under its `value_name` when it declares one, else its
argument id, and is tagged `[metavar]`. The two sections size their columns
independently, so a long flag name does not push the ARGUMENTS column out with
it.

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
  init          Initialize the project
  list          List all items
  delete        Delete an item
  config        Manage configuration

OPTIONS
  --output      Output format
                default: auto
                possible values: auto, term, text, term-debug, json, yaml, csv
  -h, --help    Print help
```

## Command Groups

CLIs with many commands (20+) benefit from organized help. The `CommandGroup` struct lets you split subcommands into named sections:

```rust
use standout::cli::{App, CommandGroup};

App::builder()
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
  init          Initialize the store
  create        Create a new pad
  list          List pads
  search        Search pads

PER PAD(S)
  These commands accept one or more pad ids: <id> or ranges <id>-<id>
  ex: $ padz view 3 5 7-9  # views pads 3, 5, 7, 8 and 9

  open          Open a pad in the editor
  view          View one or more pads
  peek          Peek at pad content previews

  pin           Pin one or more pads
  unpin         Unpin one or more pads

  complete      Mark pads as done
  reopen        Reopen pads

MISC
  completions   Generate shell completions
  help          Print this message
  config        Get or set configuration
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
        .add("metavar", Style::new().bold())  // Argument names in ARGUMENTS
        .add("desc", Style::new())            // Descriptions
        .add("default", Style::new().dim())   // "default: auto"
        .add("values", Style::new().dim())    // "possible values: ..."
        .add("usage", Style::new())           // Usage line
        .add("example", Style::new())         // Examples section
        .add("about", Style::new())           // About text
}
```

A configured theme *overlays* this default rather than replacing it: per style name, an entry the configured theme defines wins, and every tag it leaves out keeps its default styling. Restyle only what you mean to change:

```rust
let config = HelpConfig {
    theme: Some(
        Theme::new()
            .add("header", Style::new().bold().cyan())
            .add("item", Style::new().green())
    ),
    ..Default::default()
};
```

Or when using `AppBuilder`, set the theme with `.theme()` — it applies to both help and command output. Because of the overlay, an application theme does not need to define the help vocabulary at all: a theme that only declares the app's own output styles leaves help rendered entirely by the default help theme.

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
| `about` | String | The command's `about`, or its `long_about` — see [Short and long help](#short-and-long-help) |
| `usage` | String | Usage line (without "Usage: " prefix), naming the full command path — `app nest leaf [OPTIONS]` for a nested leaf |
| `subcommands` | Vec | Command groups (each with `title`, `help`, `items`) |
| `subcommands_width` | usize | Width of the COMMANDS name column |
| `arguments` | Vec | Positional groups (each with `title`, `help`, `items`) |
| `arguments_width` | usize | Width of the ARGUMENTS name column |
| `options` | Vec | Flag groups (each with `title`, `help`, `items`) |
| `options_width` | usize | Width of the OPTIONS name column |
| `examples` | String | Examples text |
| `learn_more` | Vec | Topic list items (each with `name`, `title`) |
| `learn_more_width` | usize | Width of the LEARN MORE name column |

#### Alignment is the template's job

There are no `padding` fields. A row aligns itself by padding its name to its
section's width with `pad_right`, one of [standout-render's tabular
filters](../crates/render/guides/intro-to-tabular.md):

```jinja
{%- set opt_label = "[item]" ~ opt.name ~ "[/item]" -%}
{%- if opt.value_name %}
{%- set opt_label = opt_label ~ " [metavar]" ~ opt.value_name ~ "[/metavar]" -%}
{%- endif %}
{{ opt_label | pad_right(options_width) }}  [desc]{{ opt.help }}[/desc]
```

Two properties of that filter are the point of doing it this way. It measures
*display* width, so a CJK name counts the terminal columns it really occupies
and the style tags around it count for nothing — byte length gets both wrong.
And it never truncates: a name wider than the column keeps its full text and
its separator, which is the failure the fixed-width column used to produce.

The widths themselves are resolved from the data, as a `Width::Bounded` column
that is at least 12 columns wide and otherwise as wide as the section's longest
name. One width per *section*, not per group, is what keeps a grouped command
list aligned down the whole page instead of realigning at each header.

### Group Fields in Templates

Each subcommand group has:

- `group.title` — section header (rendered as `group.title | upper` in the default template)
- `group.help` — optional help text for the group
- `group.items` — list of command entries

Each command entry has:

- `cmd.name` — command name
- `cmd.about` — command description
- `cmd.separator` — true for blank-line separator entries

Each argument and option entry has:

- `opt.name` — `range` / `RANGE` for a positional, `-o, --output` for a flag
- `opt.value_name` — rendered value syntax for a value-taking flag, such as `<MODE>` or `[<PATH>]`; empty for positionals and presence flags
- `opt.help` — description
- `opt.short` / `opt.long` — the flag's spellings (both empty for a positional)
- `opt.default` — the declared default, or nothing
- `opt.possible_values` — the selectable values, hidden ones left out

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
{%- for cmd in group.items %}
{%- if cmd.separator %}

{%- else %}
  {{ ("[item]" ~ cmd.name ~ "[/item]") | pad_right(subcommands_width) }}  [desc]{{ cmd.about }}[/desc]
{%- endif %}
{%- endfor %}
{%- endfor %}
```

Style tags like `[header]...[/header]` are resolved against the theme. A tag the resolved theme does not define degrades to unstyled text and raises a stderr warning; it is never shown with a `?` marker.

## Output Modes

The `help` word, `--help` and `-h` all respect the `--output` flag. In the human modes help is the rendered template, and the mode decides what happens to its style tags — applied in `Term`, stripped in `Text`, left visible as `[header]…[/header]` in `TermDebug`:

```bash
myapp help --output text
myapp --help --output text
```

Without the flag, every form renders in the app's [output-mode fallback](./app-configuration.md#output-mode-fallback), which is `Auto` — styling for the terminal it finds — unless the app set another one.

### Help as data

Under `json` and `yaml`, every form answers with the **help document** instead of the page: one versioned document a script can read the way it reads any other `--output json` result.

```bash
$ myapp deps --help --output json
{
  "schema_version": 1,
  "name": "deps",
  "path": ["myapp", "deps"],
  "usage": "myapp deps [OPTIONS] <FORMULA>",
  "about": "The transitive dependency closure of a formula.",
  "args": [
    {"name": "formula", "short": null, "long": null, "value_name": "FORMULA", "required": true,
     "help": "The formula to resolve.", "default": null, "possible_values": []},
    {"name": "tree", "short": null, "long": "--tree", "value_name": null, "required": false,
     "help": "Show the closure as a tree instead of a set.", "default": null, "possible_values": []},
    {"name": "help", "short": "-h", "long": "--help", "value_name": null, "required": false,
     "help": "Print help", "default": null, "possible_values": []}
  ],
  "subcommands": []
}
```

| Field | Meaning |
| ------- | --------- |
| `schema_version` | The version of this document's shape, `1` today — see [What Is Contract](./stability.md#the-versioned-document) |
| `name` | The command's own name |
| `path` | The words from the root to the command, `["myapp", "deps"]` |
| `usage` | The usage line, naming the full path |
| `about` | `about` for `-h`, `long_about` (falling back to `about`) for `--help` and the `help` word |
| `args` | Every visible argument, positionals first, in display order: `name` is the clap id, `short` and `long` are the tokens as typed (`-t`, `--tree`), `value_name` is the metavar when the argument takes a value, `required`, `help`, `default` and `possible_values` are clap's |
| `subcommands` | Every visible subcommand's `name` and `about` |

The document lists clap's own `-h`/`--help` and `-V`/`--version` and the framework's `--output` flag, because a script reading it wants every argument the command accepts. It is `standout::cli::HelpDocument` (with `HelpArg` and `HelpSubcommand`), so a test can read it back with serde. `render_help` and `render_help_with_topics` produce the same document for a `HelpConfig` whose `output_mode` is `Json` or `Yaml`.

`csv` has no help projection: `--help --output csv` is a render error, emitted as the diagnostic document of kind `render` that every structured-mode failure produces (see [Error Handling](./error-handling.md)). Topic pages — `help topics` and `help <topic>` — are prose in every mode and stay so.
