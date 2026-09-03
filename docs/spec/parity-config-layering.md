# PAR01: Config Layering via clapfig

A standout application gets its settings from files, environment variables and
flags through the sibling crate clapfig. standout builds no configuration machinery
and tests none of clapfig's behavior; it owns the points where clapfig meets the run
pipeline, the handler, the `config` subcommand, the output modes and the test harness.

## Problem

Every standout adopter already depends on clapfig. What each one hand-builds is the
glue between clapfig and standout, and each builds it differently:

- **Where resolution runs and how the result reaches a handler.** lookma loads in a
  pre-dispatch hook and stores the struct in `ctx.extensions`
  (`crates/lookma/src/app.rs:36-58`). rustloc reloads the whole configuration inside
  post-dispatch hooks, once per render (`crates/rustloc/src/main.rs:514-535`). padz
  constructs its clapfig builder in three places (`padz/src/cli/commands.rs:340`,
  `padzapp/src/init.rs:392`, `padzapp/src/commands/transfer.rs:214`), so its search
  paths can drift apart.
- **The `config` subcommand.** padz declares its own clap enum, maps it by hand onto
  clapfig's `ConfigAction`, and calls `handle_and_print`, which writes straight to
  stdout. `padz config list --output json` cannot exist.
- **Flags as the top layer.** lookma builds a `ConfigOverrides` struct by hand and
  threads each field through `cli_override`; rustloc ORs flags with config values in
  the hook. Neither is visible to standout's output-mode logic.
- **Framework settings have no key.** There is nowhere to write "output json by
  default" or "no color" in a file, so lookma switches color off through a
  process-global detector call (`crates/lookma/src/config.rs:70-98`), an API 9.0 removed.
- **Testing happens outside `TestHarness`.** lookma's config tests mutate the process
  environment under `serial`; rustloc tests its loader against temp dirs but never
  through the CLI. The harness can write fixture files and set env, and nothing checks
  that a resolved value reaches a handler or changes what the app prints.

dodot is the exception: it uses clapfig's `Resolver` for per-pack domain configuration
(`dodot-lib/src/config/mod.rs:793-810`), which is not CLI settings and is not this
epic's concern.

The framework reads six environment variables (`VISUAL`, `EDITOR`, `PAGER`, `COLUMNS`,
`NERD_FONT`, `STANDOUT_STRICT_STYLE_TAGS`) and no files. `standout-input`'s
`InputChain` already answers "which of flag, env, stdin, prompt, default supplies this
value"; a config value is one more answer to the same question, so it belongs in the
same chain vocabulary rather than in a second precedence system.

## Client shapes the design must express

The adopters above are design input, not part of the deliverable. Each shape they
exhibit has one answer in this epic, and the in-repo client `tdoo` (WS04) exercises
every one of them:

- **A flag above a config value** (lookma's `--color false`, rustloc's `--shows-ratio`
  ORed with the file): an `InputChain` with `FlagSource` before `ConfigSource` (D18).
  The app never touches clapfig's override layer for its own flags.
- **Project file beside a user file, with `set` writing either** (padz's `.padz/padz.toml`
  and its `-g`): two `persist_scope`s on the clapfig builder and `--scope` on the
  injected command. A differently spelled scope flag is not offered; `-g` becomes
  `--scope global`.
- **Load once, read everywhere** (rustloc reloading per hook, padz building the
  loader three times): one resolution per run, one struct in the context (D12).
- **A framework setting in the file** (lookma's color, any app's default output
  mode): `[term]` through the accessor (D14).
- **Per-entity configuration** (dodot's per-pack resolver): not a CLI-settings shape
  and not expressed here.

## What clapfig provides, verified at 0.24.0

Everything below is clapfig's, and this epic neither reimplements nor tests it:

- `Clapfig::typed::<C>()` and its builder: `app_name`, `file_name`/`file_stem`,
  `formats`, `search_paths`/`add_search_path`, `SearchPath::{Platform, Home, Cwd, Path,
  Ancestors(Boundary)}`, `SearchMode::{Merge, FirstMatch}`, `persist_scope`,
  `env_prefix`/`no_env`, `strict`/`strict_at`/`on_unknown_key`, `layer_order`,
  `post_validate`, `cli_override`, `cli_overrides_from`.
- Precedence: compiled defaults, then files in search-path order, then
  `MYAPP__SECTION__KEY` env, then overrides. Every layer is sparse.
- `#[derive(clapfig::Schema)]` with `default`, `env`, `optional`, `rename`, `allowed`,
  `min`/`max`, `value`; doc comments become template comments.
- Strict unknown-key errors carrying file, line and the nearest valid key
  (`ClapfigError`, `#[non_exhaustive]`, with `render_plain` for prose).
- `TypedResolver<C>::resolve_at(dir)`: resolution anchored on a caller-supplied
  directory. `load()` is `resolve_at(current_dir())`.
- The `config` verb set as data: `ConfigAction::{List, Gen, Schema, Get, Set, Unset}`
  with `--scope`; `ConfigCommand::new().as_command(name)` builds the clap tree at
  runtime and `ConfigCommand::parse(matches)` reads the action back;
  `builder.handle(&action) -> ConfigResult`, an enum carrying structured fields plus a
  `rendered` string in the active file format. Only `handle_and_print` touches stdout.

What clapfig does not have, and this epic does not build around: per-key origin on a
success path (`Origin` is crate-private; `docs/spec/provenance.md` lists a public form
as a follow-on), per-key layer restrictions, named sets, injectable environment
variables, a reserved-section concept.

## What the user gets

### The CLI user

```text
myapp list                                # settings from myapp.toml (project), then the platform dir (user), then env
MYAPP__INDEX_DIR=/tmp/idx myapp list      # env beats files
myapp list --set index_dir=/tmp/idx       # one-shot override, beats env (only when the app installs the flag)
myapp config list                         # merged settings in the app's file format
myapp config list --output json           # the same, as one JSON object with typed values
myapp config get index_dir
myapp config set index_dir /srv/idx       # writes the default persist scope; --scope global writes the user file
myapp config gen > myapp.toml             # commented template
myapp config schema                       # JSON Schema for editors
```

A typo in a key exits 1 with clapfig's own message, file and line. Under `--output
json` the same failure is a diagnostic document with the position filled in.

`[term]` is the section standout reads for itself when the app opts in:

```toml
[term]
output = "json"     # the output mode when --output is absent; never applies to --help or usage errors
```

### The app author

```rust
#[derive(clapfig::Schema, Serialize, Deserialize)]
struct MyConfig {
    /// Where the index lives.
    #[clapfig(default = "~/.myapp/index")]
    index_dir: String,
    term: standout::TermSettings,
}

App::builder()
    .config(
        clapfig::Clapfig::typed::<MyConfig>()
            .app_name("myapp")
            .add_search_path(clapfig::SearchPath::Cwd)
            .persist_scope("local", clapfig::SearchPath::Cwd)
            .persist_scope("global", clapfig::SearchPath::Platform),
    )
    .term_settings(|c: &MyConfig| &c.term)   // opt-in: lets [term] reach the framework
    .config_override_flag("set")             // opt-in: installs --set key=value
    .commands(Commands::dispatch_config())?
    .build()?
    .run(Cli::command(), std::env::args());
```

`.config(...)` takes clapfig's builder unchanged. `config` is injected as a top-level
subcommand by default; `.no_config_command()` removes it for an app that mounts
clapfig's `ConfigArgs` itself. A handler reads the struct once, resolved before
dispatch:

```rust
#[handler]
fn list(#[ctx] ctx: &CommandContext) -> Result<Output<Listing>, anyhow::Error> {
    let cfg: &MyConfig = ctx.config()?;
    ...
}
```

A value that a chain should take from config enters the chain as a source, in the same
vocabulary as a flag or a prompt:

```rust
InputChain::new()
    .try_source(FlagSource::new("remote"))
    .try_source(ConfigSource::new(cfg.remote.clone()))   // reports InputSourceKind::Config
    .try_source(TextPromptSource::new("Remote?"))
```

Error classes: `SetupError::Config` at `build()` when the app registers its own
`config` command while injection is on, or sets `config_override_flag` on a name the
app already uses; `RunErrorKind::Config` (exit 1) when resolution fails at run time.

### The test author

The harness needs nothing new to write config files and set env; it gains one fix
(`cwd` relative to the harness tempdir) and one accessor:

```rust
let r = TestHarness::new(app)
    .fixture("proj/myapp.toml", "index_dir = \"./idx\"\n")
    .cwd("proj")                                   // relative to the harness tempdir
    .env("MYAPP__INDEX_DIR", "/env")
    .run(&["list"]);
r.assert_stdout_contains("/env");
```

A test app points its search paths at `SearchPath::Cwd` or `SearchPath::Path`;
`SearchPath::Platform` resolves against the real user and is clapfig's to test.

## Decisions

**D11. Builder surface.** `App::builder().config(clapfig::TypedBuilder<C>)` is the
only entry; standout adds `term_settings(accessor)`, `config_override_flag(name)`,
`no_config_command()`. Reason: clapfig's builder already expresses discovery,
formats, strictness and persistence, and a standout wrapper would restate and lag it.
Cost: the app author learns clapfig's builder, which is the intent.

**D12. One resolver per run, built after parse.** At `build()` standout stores the
builder. On each run, after clap parses and only on the path that dispatches an app
handler, standout clones the builder, applies the override flag's pairs, calls
`build_resolver()` then `resolve_at(current_dir())`, and inserts the struct into
`ctx.extensions`. Reason: clapfig snapshots the process environment inside
`build_resolver`, so a resolver built at `App::build()` would ignore env set by
`TestHarness` after `TestHarness::new(app)`; and the override flag's values exist only
after parse. Cost: `TypedBuilder` must be `Clone`, a clapfig change (WS00).

**D13. Config never precedes parsing.** `--help`, `--version`, usage errors, the help
word, the questionnaire `questions` interception and the injected `config` tree run
without resolving the app's config. Reason: the override flag is argv, so config
cannot exist before clap; and a broken config file must not take `--help` down with
it. Cost: `term.output` never applies to pre-parse outcomes, which take the
argv-scanned `--output` (PAR02 D6) or the builder's static `output_mode_fallback`.

**D14. `[term]` is an opt-in accessor, not a reserved name.** `standout::TermSettings`
derives `clapfig::Schema`; the app embeds it under any field and hands standout an
accessor. In this epic it holds `output: Option<OutputMode>`, consumed by
`extract_output_mode`'s fallback arm when the flag was not typed. Reason: clapfig has
no reserved-section concept, and standout has no business finding a field by name in
someone else's struct. Theme is not a key: the theme is resolved at `build()`
(ADR-0020) and there is no post-parse seam to change it. Cost: an app that forgets the
accessor gets no `[term]`, which is explicit.

**D15. Override flag is opt-in and app-named.** `config_override_flag("set")`
installs a global `--set key=value` (repeatable) that lands on clapfig's override
layer. Reason: `--config` conventionally means a config file path, no adopter needs
the flag, and a flag every app pays for contradicts ROB05. Values parse with
clapfig's env scalar rule so `port=8080` is an integer (WS00).

**D16. `config` renders through standout, clapfig executes.** The injected tree is
`clapfig::ConfigCommand::as_command("config")`; a post-parse interception beside the
help word turns the matches into a `ConfigAction` with `ConfigCommand::parse` and calls
`builder.handle`. The `ConfigResult` is rendered by a framework handler: `Listing` and
`KeyValue` as a key/value document (clapfig's `rendered` text in human modes, a typed
object in structured modes), `ValueSet`/`ValueUnset`/`TemplateWritten`/`SchemaWritten`
as one-line confirmations, `Template` and `Schema` as artifacts (PAR02's artifact
output). Reason: `handle_and_print` is a two-line wrapper over `handle`; nothing in
clapfig requires stdout. Cost: structured `list` needs typed entries, which is a clapfig
change (WS00); origin per key is not shown in this epic.

**D17. Config errors keep clapfig's words.** A `ClapfigError` at run time becomes a
`RunError` of kind `Config`, exit 1, whose prose is `clapfig::render::render_plain`
and whose `Diagnostic` position is copied from the error's path and line (`UnknownKeyInfo`,
`ParseError`). standout adds no wording of its own. Reason: clapfig already renders
snippets and carets; a second rendering would drift.

**D18. `ConfigSource` carries a value, not a key.** `ConfigSource<T>::new(Option<T>)`
reports `InputSourceKind::Config` and yields the value the handler read from its typed
config. Reason: `InputCollector::collect` sees only `&ArgMatches`, and a stringly key
lookup into a map would bypass the typed struct the app already holds. Cost: one enum
variant on `InputSourceKind`.

**D19. The test boundary.** clapfig's own suite covers discovery, merge order,
precedence among files, env and overrides, strictness, env-name mapping, persistence
and templates. standout's tests exercise only the seams standout owns, listed under
WS01 to WS03. No precedence table is written in this repository. Reason: a table
asserting env beats file tests clapfig with standout in the loop; when it fails the
finding belongs to clapfig, and when clapfig changes, standout's copy goes stale.

## Workstreams

**WS00. clapfig 0.25.** In `arthur-debert/clapfig`: `Builder` and `TypedBuilder`
implement `Clone` (hooks move from `Box` to `Arc`); `ConfigResult::Listing` entries and
`KeyValue` carry `Value` beside `rendered`; `cli_override_str(key, &str)` parses with
the env scalar rule; docs say 0.25. Released to crates.io and pinned here. Blocks
everything below.

**WS01. Seam and handler access.** `.config(...)`, `.term_settings(...)`,
`.config_override_flag(...)`, `.no_config_command()` on `AppBuilder`; per-run
resolution inserted at `execution.rs:377` on the app-handler path only;
`ctx.config::<C>()` on `CommandContextInput` beside `questionnaire::<T>()`;
`RunErrorKind::Config` with D17's diagnostic; `TermSettings` and its arm in
`extract_output_mode`. Tests, each through `TestHarness` with a fixture app whose
search path is `SearchPath::Cwd`:

- a file value reaches the handler; an env value reaches the handler;
- `--set k=v` reaches the handler and beats env (one test, the flag is standout's);
- a bad key exits 1 with clapfig's message; under `--output json` the diagnostic
  carries the file and line;
- `--help` and a usage error succeed with the same bad file present;
- `term.output = "json"` makes a bare run emit JSON, `--output term` beats it,
  `--help` ignores it;
- `ctx.config()` without `.config(...)` is a typed error, not a panic.

**WS02. The injected `config` command.** Installed in `augment_framework_surface`,
collision reported beside `help_word_collision`, intercepted beside `intercept_help_word`,
rendered per D16. Tests: the tree is present and `no_config_command()` removes it; an
app-declared `config` fails `build()`; `config list` renders the same entries in
`term` and as a typed object in `json`; `config get`, `set`, `unset` and `--scope`
reach `handle` with the right `ConfigAction` (assert on the written file and the
confirmation, not on merge semantics); `config gen` and `config schema` are artifacts.

**WS03. Chain source, harness, docs, wizard.** `ConfigSource<T>` and
`InputSourceKind::Config` in `standout-input`; `TestHarness::cwd` relative to the
tempdir; a `docs/topics/config-files.md` topic that states the ladder once, names
clapfig as the owner of everything under it and shows the harness pattern; the
`new-project` wizard offers a config struct with `TermSettings` and the `.config(...)`
call, with `generated_manifests_only_depend_on_publishable_workspace_crates` allowing
clapfig. Tests: the topic's examples are doc tests; the generated project builds and
its own config test passes.

**WS04. `tdoo` adopts config end to end.** `crates/todo-example/tdoo` gains a config
struct (store location, default ordering, `TermSettings`), `.config(...)` with a
project and a user persist scope, the injected `config` command, one chain that puts a
flag above a config value, and `TestHarness` tests for each client shape listed above.
Done when those tests pass and `tdoo config list --output json` returns typed values.
This is the epic's proof, inside this repository; downstream ports stay in #480.

## Exit criteria

- WS01 to WS04 tests green in this repository; no precedence table exists.
- clapfig 0.25 on crates.io and pinned; no `path =` dependency.
- `docs/topics/config-files.md` present, indexed, examples as doc tests.
- `tdoo` runs on the feature with every client shape covered by a harness test.

## Issues

- Epic #476. Downstream ports (padz, rustloc, lookma) are #480 and are not this
  epic's exit.
- lookma's color detector call belongs to ANSI presentation
  (`docs/spec/typed-command-output.md`); PAR01 only gives `[term]` a home.

## Out of scope

Configuration machinery or a precedence suite in standout; per-key origin in `config
list` (a clapfig follow-on); per-key layer or scope restrictions, including the
project-file pager rule (the pager is a delivery decision of
`docs/spec/typed-command-output.md`); named
configuration sets; theme selection from config; dodot's per-pack resolver; secrets.
