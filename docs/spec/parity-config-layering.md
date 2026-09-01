# PAR01: Config Layering via clapfig

Second epic of the capability-parity program, run in the order PAR02 (machine contract)
→ PAR01 (this) with PAR04 (corpus runner fixes) alongside → PAR03 (terminal citizenship)
→ PAR05 (named configuration sets). PAR01 depends on the composition contracts
(`App::run_with(cmd, args, target, sources)` and `ResolvedConfig`, ADRs 0025 to 0031)
and on two PAR04 items landing first: post-run file assertions in the corpus runner
(#455) and the per-command `equal_across_modes` flag (#465). It does not depend on
PAR02. The exit criterion is executable: the config cases of `gitlike` (4) and
`cargolike` (22) pass in a blind corpus run whose produced applications depend on the
`clapfig` crate, and the harness precedence table in `crates/standout-test` is green.

## Problem

A standout application has no configuration files. The framework reads exactly five
environment variables (`VISUAL`, `EDITOR`, `PAGER`, `COLUMNS`, `NERD_FONT`) and nothing
else: no project file discovered by walking up from the current directory, no user file
in the platform config directory, no `MYAPP__SECTION__KEY` environment mapping, no
`config` subcommand, no `--config key=value` override. All eleven CLIs in the parity
survey (gh, git, jj, cargo, docker, kubectl, gcloud, terraform, systemctl, brew, pnpm)
have all of that.

Every adopter that needs the layer builds it by hand, outside `TestHarness`, so its
precedence is untested. lookma turns color off through a process-global detector call
in `crates/lookma/src/config.rs:70-98` because no config key exists for it. The corpus
measured the same thing: in the completion run, the blind agent implementing
`cargolike` parsed the command line twice before `App::build()` to find config files,
and the `gcloudlike` agent rewrote argv. Those two hand-rolled layers turned 45 of 46
expected-fail config cases into passes that the scorecard had to disown ("that is not
the framework closing them", `corpus/completion/scorecard.md`).

The framework also carries a second precedence system. `standout-input`'s `InputChain`
resolves one value across arg, env, stdin, prompt and default sources. A config layer
answers the same question ("where does this value come from, in what order") for a
different set of sources. Built separately, the two will disagree on precedence the
first time an app uses both for one value.

The sibling crate clapfig (0.24.0, same maintainer, no confique dependency since 0.23)
already implements the layer: `#[derive(clapfig::Schema)]` on a struct, sparse merge of
defaults < files < env < overrides, ancestor walk with boundary control, strict
unknown-key errors with file and line, `MYAPP__SECTION__KEY` env mapping, template and
JSON Schema generation, and a clap adapter that contributes
`config gen|list|get|set|unset|schema` with `--scope`. PAR01 integrates it; it builds no
configuration machinery in standout.

## What the user gets

### The CLI user

```text
myapp list --config term.color=never     # one-shot override, highest precedence
MYAPP__TERM__COLOR=never myapp list       # env, below the flag
myapp config list                         # every resolved key with the layer it came from
myapp config get term.color
myapp config set term.color never         # writes the user file (--scope local writes the project file)
myapp config gen > myapp.toml             # commented template from the struct's doc comments
```

Files are discovered by walking up from the current directory to the nearest
`myapp.toml` (project layer), then the platform config directory (user layer). A typo in
a key is a `ConfigError` naming file, line and the nearest valid key, exit 1.

The reserved section holds the framework's own settings:

```toml
[term]
output = "json"        # default output mode when --output is absent (not for --help or usage errors, D13)
color = "auto"         # auto | always | never (PAR03 reads it)
pager = "less -FRX"    # PAR03 reads it; honored from the user layer and env only
theme = "dark"
verbosity = 0          # PAR03 reads it
```

### The app author

```rust
#[derive(clapfig::Schema, Serialize, Deserialize)]
struct MyConfig {
    /// Where the index lives.
    #[clapfig(default = "~/.myapp/index")]
    index_dir: String,
    remote: RemoteConfig,
}

App::builder()
    .config(
        clapfig::Clapfig::typed::<MyConfig>()
            .app_name("myapp")
            .add_search_path(clapfig::SearchPath::Ancestors { boundary: ".git" })
            .persist_scope("local", clapfig::SearchPath::Cwd)
            .persist_scope("global", clapfig::SearchPath::Platform),
    )
    .config_subcommand(true)          // default true: injects `config gen|list|get|set|unset|schema`
    .reserved_section("term")         // default "term"
    .commands(Commands::dispatch_config())?
    .build()?
    .run(Cli::command(), std::env::args());
```

`App::builder().config(...)` takes clapfig's own builder unchanged. A handler reads the
typed value through `CommandContext`:

```rust
#[handler]
fn list(#[ctx] ctx: &CommandContext) -> Result<Output<Listing>, anyhow::Error> {
    let cfg: &MyConfig = ctx.config()?;        // ConfigError if resolution failed
    ...
}
```

An `InputChain` names a config key as one source, in the same chain vocabulary as env
and stdin:

```rust
InputChain::new()
    .source(FlagSource::new("remote"))
    .source(ConfigSource::new("remote.url"))
    .source(EnvSource::new("MYAPP_REMOTE"))
```

Error classes: `SetupError::ConfigCollision` when the app already registers a `config`
command and injection is on; `ConfigError` (clapfig's error, re-exported, rendered by
standout's error path) for unknown keys, unreadable files, type mismatches and
unresolvable required fields.

### The test author

```rust
let result = TestHarness::new(app)
    .config_file(ConfigLayer::User, "[term]\ncolor = \"never\"\n")
    .config_file(ConfigLayer::Project, "index_dir = \"./idx\"\n")
    .cwd("proj/sub")                  // anchors the ancestor walk inside the harness tempdir
    .env("MYAPP__INDEX_DIR", "/env")
    .run(&["list"]);
result.assert_config_value("index_dir", "/env");
result.assert_config_layer("index_dir", ConfigLayer::Env);
```

## Decisions

**D11. Builder surface.** `App::builder().config(clapfig_builder)` accepts
`clapfig::Clapfig<T>` as built by the app; standout adds three knobs only:
`config_subcommand(bool)`, `reserved_section(&str)`, and the harness fixtures. Reason:
clapfig's builder already expresses search paths, formats, strictness and persistence;
a standout wrapper would restate every option and lag every clapfig release. Cost:
adopters learn clapfig's builder, not a standout one, which is the intent.

**D12. Reserved section name.** The framework's settings live under `[term]`,
renameable per app through `reserved_section`. Reason: cargo's `[term]` already holds
`color`, `verbose`, `quiet` and `progress`, so users of cargo read the section without
explanation. The earlier spec text left the name open.

**D13. Order of argv and config.** `App::run` parses argv with clap first, then resolves
config with every `--config k=v` applied as a `cli_override`, then feeds the resolved
`[term]` values into the output-mode fallback (`output_mode_fallback`) and
`TargetProperties`, then dispatches. Config never changes how argv is parsed. Reason:
`--config` and `--scope` are argv, so config cannot exist before parsing; any key that
would need to precede parsing is rejected at `build()` with a `SetupError`. Cost: a
config-driven default subcommand or alias table is impossible, and stays out of scope;
and a pre-parse outcome (`--help`, a usage error) is emitted before config exists, so
`term.output` never reaches it: those paths take the argv-scanned `--output` (PAR02 D6)
or the builder's static `output_mode_fallback`.

**D14. One ladder.** Precedence from highest to lowest: flag, then `--config k=v`, then
`MYAPP__SECTION__KEY` env, then project files nearest-first, then the user file, then
struct defaults. `InputChain` gains one collector, `ConfigSource(key)`, that reads the
resolved map; nothing else in `standout-input` changes. Reason: WIZ03 just stabilized
input collection, and a chain's order is already explicit at the call site. Cost: an app
that puts `ConfigSource` above `FlagSource` in its chain contradicts the documented
ladder; the docs state the recommended order and the harness test checks the
framework's own chains.

**D15. Execution-adjacent keys from project scope.** `term.pager`, `term.editor` and any
key the app marks `#[clapfig(scope = "user")]` are honored from the user layer, env and
flags only. A project file that sets one is ignored for that key, and standout emits a
warning through the warning channel naming file and line. Reason: a cloned repository
is untrusted input, and a project file that names a pager command would otherwise run
arbitrary code on the first `myapp log`. A hard error was the alternative; the warning
wins because the user did nothing wrong by cloning.

**D16. Named configuration sets are out of PAR01.** clapfig 0.24.0 has no named-set
feature (a directory of set files plus an active-set pointer, gcloud's shape). The 24
`gcloudlike` cases that need it carry `gap = "PAR05"` and are owned by
`parity-named-config-sets.md`. PAR01's corpus oracle is `gitlike` and `cargolike`.
Reason: named sets are a clapfig feature first, and coupling PAR01 to it would hold four
standout workstreams on one clapfig feature.

**D17. The corpus exit criterion checks for the crate, not only the bytes.** Each config
archetype's manifest declares `evidence = "uses-crate:clapfig"` under its `[gaps]`
entry. The runner reads the produced workspace's `Cargo.toml` and records, per gap case,
whether the evidence crate is present. A passing gap case without the evidence is
reported as `hand-rolled-pass`, a distinct outcome from `unexpected-pass`, and counts as
open. Reason: a black-box case cannot tell a framework-supplied layer from an in-app one,
which is how the completion run produced 45 passes with the capability at zero. The
runner change belongs to PAR04 (WS03) and lands before PAR01's blind run.

## Workstreams

**WS01. Seam, builder and walking skeleton.** `App::builder().config(...)`,
`ctx.config::<T>()`, resolution placed between parse and dispatch in
`crates/standout/src/cli/builder/execution.rs`, `--config k=v` as a global flag mapped to
`cli_override`. Done when one struct field reaches a handler from a file, from env and
from the flag, in that precedence, under `TestHarness`.

**WS02. Injected `config` subcommands and collision rules.** The clap adapter's
`ConfigArgs` installed the way ADR-0017 installs questionnaire subcommands;
`config list` and `config get` render through standout's pipeline so `--output json`
applies; `SetupError::ConfigCollision` when the app registers its own `config`. Done
when `gitlike`'s `config-get-unset-key` and `cargolike`'s `config list` cases pass in
the hermetic loop test.

**WS03. The ladder, `ConfigSource`, the `[term]` section and the scope policy.**
`ConfigSource` in `crates/standout-input`; `[term]` read into `output_mode_fallback`,
`ColorPolicy` (consumed fully by PAR03) and theme selection; D15's per-key scope
enforcement with the warning. Done when the precedence table test (one row per adjacent
layer pair, plus the project-scope pager case) passes.

**WS04. Harness fixtures, docs and wizard.** `TestHarness::config_file`, `cwd`,
`assert_config_value`, `assert_config_layer`; a `docs/topics/configuration.md` topic
stating the ladder once; the wizard offers a config struct and `[term]` scaffold. Done
when the topic's examples run as doc tests and the wizard's generated app passes its own
config test.

**clapfig-side PRs**, opened in `arthur-debert/clapfig` as they are found, each released and its version
set in `Cargo.toml` here: first-file-wins list semantics for `gitlike`'s walk-up (`SearchMode::FirstMatch`
already exists; the per-type merge `cargolike` needs is checked in WS01), the
`#[clapfig(scope = "user")]` attribute for D15, and a `Cargo.lock`-visible crate name
that D17 can check.

## Exit criteria

- `cargo test -p standout-test` precedence table green: flag > `--config` > env >
  project > user > default, one test per adjacent pair, plus the D15 project-scope case.
- The hermetic loop tests for `gitlike` and `cargolike` pass their config groups.
- One blind run each of `gitlike` and `cargolike` via `corpus-runner batch` (PAR04
  WS04) where every former gap case reads `pass` with the clapfig evidence present, and
  no `hand-rolled-pass` outcome appears.
- The blind runs' questionnaires list no config workaround.
- `corpus/gap-suites/gaps.toml` has no PAR01 entry to flip (the config archetypes use
  `acceptance.toml` gap cases, not the gap-suite crate); the `[gaps]` entries in the two
  manifests are removed in the closing PR.

## Issues

- #455 (post-run file assertions) and #465 (`equal_across_modes`) are PAR04 items this
  epic waits on.
- No open framework issue is closed by PAR01 directly; the adopter evidence is dodot's
  bespoke loader and lookma's color detector call, which port after PAR03.

## Out of scope

Configuration machinery in standout; secrets or sensitive-key handling; migrating
dodot, padz, rustloc or lookma (they port on their own schedule); named configuration
sets (PAR05); structured emission of config errors (PAR02 owns the diagnostic shape,
and this epic routes config errors through whatever `emit_run_result` does at the
time).
