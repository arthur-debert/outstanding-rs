# Configuration Files

A command-line tool is used the same way many times, and users stop wanting to
retype the same flags. They want three kinds of persistence: a personal default
that follows them everywhere, a per-project setting that lives in the repository
and is shared with whoever clones it, and a one-off override for a script or a CI
job. They also want to see which value is in effect, change it without hand-editing
a file, and be told about a typo with a file and a line rather than a silent default.

Standout does not implement any of that. The sibling crate
[clapfig](https://docs.rs/clapfig) does: a typed struct with a derive, sparse
merging of compiled defaults under files under environment variables under
overrides, discovery of a project file by walking up from the current directory and
of a user file in the platform directory, `MYAPP__SECTION__KEY` environment mapping,
strict unknown-key errors, a template generator and a `config` command family.
Standout owns only the points where clapfig meets the framework: when resolution
runs, how the struct reaches a handler, how the `config` command renders, how a
config error becomes a diagnostic, and how tests set it up. This page is about
those seams. Everything about the files themselves is in clapfig's own guides.

## The ladder

From highest to lowest precedence, a value comes from:

1. a flag the user typed, resolved by the handler through an [`InputChain`](#a-flag-above-a-config-value);
2. an override pair from the app's override flag, when the app installs one;
3. the `MYAPP__SECTION__KEY` environment variable;
4. the project file, found by walking up from the current directory;
5. the user file in the platform configuration directory;
6. the compiled default on the struct field.

Levels 2 to 6 are clapfig's and are tested there. Standout adds level 1 and tests
that each level reaches a handler, not the order among them.

## Declaring configuration

An application declares its settings as one struct and hands clapfig's builder to
`App::builder()` unchanged. `tdoo`, the example application in this workspace,
declares a store path, a listing order and the framework's own section:

```rust
use clapfig::{Clapfig, SearchPath, TypedBuilder};
use serde::{Deserialize, Serialize};
use standout::TermSettings;

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
pub struct TdooConfig {
    /// Where the todo store lives.
    pub store: Option<String>,
    /// List newest first by default.
    #[clapfig(default = false)]
    pub reverse: bool,
    pub term: TermSettings,
}

pub fn builder(user_scope: SearchPath) -> TypedBuilder<TdooConfig> {
    Clapfig::typed::<TdooConfig>()
        .app_name("tdoo")
        .search_paths(vec![user_scope.clone(), SearchPath::Cwd])
        .persist_scope("local", SearchPath::Cwd)
        .persist_scope("global", user_scope)
}
```

The builder takes the user directory as a parameter so that the binary passes
`SearchPath::Platform` and tests pass a directory of their own; see
[Testing](#testing) for why.

```rust,ignore
App::builder()
    .config(config::builder(SearchPath::Platform))
    .term_settings(|config: &TdooConfig| &config.term)
    .commands(Commands::dispatch_config())?
    .build()?
```

`config(...)` accepts clapfig's `TypedBuilder<C>` for any `C` that derives
`clapfig::Schema`. `term_settings(...)` is optional; it tells the framework where
its own section lives inside the app's struct, because clapfig has no reserved
names and standout does not look for a field by name in someone else's type.

Two more knobs exist. `config_override_flag("set")` installs a global, repeatable
`--set key=value` whose pairs land on clapfig's override layer, level 2 above. It is
opt-in and app-named because `--config` conventionally means a file path.
`no_config_command()` removes the injected `config` command for an application that
mounts clapfig's `ConfigArgs` itself.

## When resolution runs

Configuration resolves once per run, after clap has parsed the command line and
only on the path that dispatches an application handler. `--help`, `--version`,
usage errors and the injected `config` command never read the app's configuration,
so a broken file cannot take `--help` down with it, and the override flag's values
exist before resolution needs them. The resolved struct is inserted into the
command context, and a handler reads it once:

```rust,ignore
#[handler]
fn list(#[ctx] ctx: &CommandContext) -> Result<Output<Listing>, anyhow::Error> {
    let config: &TdooConfig = ctx.config()?;
    let store = TodoStore::load(config.store_path())?;
    ...
}
```

`ctx.config::<C>()` returns `MissingConfig` when no configuration was registered or
the run did not resolve one, which is a typed error, not a panic. Handlers do not
load configuration themselves; loading is the framework's, so every handler sees the
same struct and pays for one resolution.

## A flag above a config value

Most applications have a flag that overrides a configured value: `tdoo list
--reverse` when the file says `reverse = false`. The flag is not routed through
clapfig. It is one more source in an `InputChain`, ahead of the value the handler
already read from its struct:

```rust,ignore
let reverse = InputChain::<bool>::new()
    .try_source(FlagSource::new("reverse"))
    .try_source(ConfigSource::new(Some(config.reverse)))
    .resolve_from(matches, ctx.input_sources())?;
```

`ConfigSource` carries a value, never a key: the handler holds the typed struct, so
there is nothing to look up. It reports `InputSourceKind::Config` so a caller that
asks where a value came from gets the same answer shape as for a flag, an
environment variable or a prompt.

A boolean flag has a subtlety here. A plain `--reverse` switch can only turn the
value on, so a user whose file says `true` has no way back from the command line.
`tdoo` declares the flag as `--reverse[=true|false]` (clap's `ArgAction::Set` with a
`bool` parser and a default missing value of `true`), which makes `--reverse=false`
count as the flag being present.

## The `[term]` section

`standout::TermSettings` is the framework's own section. In this release it holds
one key:

```toml
[term]
output = "json"     # the structured encoding a bare run produces
```

`output` names one of the four structured encodings — `json`, `yaml`, `csv` or
`ndjson` — and nothing else; a file naming a retired value (`auto`, `term`, `text`)
fails the way any unknown configuration value fails, and `term-debug` is a
diagnostic view with no configuration spelling.

The value fills the same slot as `output_mode_fallback` on the builder, and an
explicit `--output` still outranks it. It never applies to `--help` or a usage
error, which are emitted before configuration exists; for the same reason the
`[default: ...]` shown in `--help` for `--output` is the builder's static fallback
when that fallback is a structured encoding, and absent when it is the human
representation, which the flag cannot name. An application that used to read its own file into
`output_mode_fallback` at build time should stop doing so and let `[term] output`
carry the setting. Color, pager and verbosity
keys join this section with the terminal-citizenship work; the theme is not a key,
because the theme is resolved when the application is built.

## The `config` command

When `config(...)` is set, standout installs a top-level `config` command built from
clapfig's own `ConfigCommand`:

```text
tdoo config list                       # the merged settings, in the file's format
tdoo config list --output json         # {"reverse": false, "term.output": "json", ...}
tdoo config get reverse                # {"reverse": false} under --output json
tdoo config list --set reverse=true    # read actions see the override flag's pairs; writes never do
tdoo config set reverse true           # writes the first persist scope ("local")
tdoo config set reverse true --scope global
tdoo config unset reverse
tdoo config gen > tdoo.toml            # a commented template
tdoo config schema                     # JSON Schema for editors
```

clapfig executes every action and standout renders the result through the normal
output pipeline, which is why `--output json` applies and why an integer stays a
JSON number. A listing keeps clapfig's flat dotted keys rather than nesting them,
because a map key may itself contain a dot and the flat spelling is the only
lossless one. `gen` and `schema` are artifacts. One spelling differs from clapfig's
own adapter: writing a template or schema to a file is `--file PATH`, because
`--output` is standout's output-mode flag on every command.

The command name is reserved. An application that declares its own `config`
subcommand fails at `build()` with a `SetupError` naming the collision, and so does
a root-level global flag that the injected tree already takes (`--scope`, `--file`,
`--force`, `-o`). Either rename, or call `no_config_command()`.

`config list` does not show which layer a value came from; clapfig does not expose
that on a success path yet.

## Errors

A configuration error is clapfig's error, in clapfig's words: an unknown key with
its file, line and the nearest valid key, an unreadable or malformed file, a type
mismatch. Standout adds no wording. The process exits `1` with the message on
stderr; under a structured output mode the same failure is the diagnostic document,
with the file and line in its range when clapfig has a position.

## Testing

`TestHarness` needs nothing specific to configuration. A test writes the file as a
fixture, points the working directory at it and sets environment variables; the
harness restores everything afterwards:

```rust,ignore
let result = TestHarness::new(app)
    .fixture("proj/tdoo.toml", "reverse = true\n")
    .cwd("proj")                        // relative to the harness tempdir
    .env("TDOO__STORE", "todos.json")
    .run(&["list", "--output", "json"]);
```

Two rules keep these tests hermetic. First, the application's builder takes its user
directory as a parameter, and tests pass `SearchPath::Path` inside the tempdir:
`SearchPath::Platform` resolves against the real account and would read whatever
file the developer has there. Second, tests that run the binary as a subprocess
move `HOME`, `XDG_CONFIG_HOME` and their Windows equivalents under the tempdir for
the same reason.

What to test is the seam, not the ladder. That a file value reaches the handler,
that a flag beats the configured value, that `config set --scope global` writes the
user file, that `[term] output` changes what a bare run prints. Whether the
environment beats the project file is clapfig's test, and repeating it here would
only go stale when clapfig changes.
