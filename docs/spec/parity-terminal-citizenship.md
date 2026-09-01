# PAR03: Terminal Citizenship

Fourth epic of the capability-parity program, run in the order PAR02 (machine contract)
→ PAR01 (config layering) with PAR04 (corpus runner fixes) alongside → PAR03 (this) →
PAR05 (named configuration sets). PAR03 depends on both earlier epics: its four settings
are `[term]` keys that PAR01's ladder resolves, and its progress output must stay out of
the structured documents and NDJSON stream PAR02 defines. The exit criterion is
executable: the `tflike/progress` gap suite in `corpus/gap-suites` turns green, and a
blind `systemdlike` run lists zero color or pager workarounds in its questionnaire.

## Problem

**Color cannot be chosen separately from format.** `--output` accepts
`auto|term|text|term-debug|json|yaml|xml|csv` today (PAR02 removes `xml` and adds
`ndjson` before this epic starts), and `term` versus `text` is the only way to say
"color" or "no color". There is no `--color` flag and no application variable.
`ColorPolicy { Auto, Always, Never }` exists on `RenderRequest`
(`crates/standout-render/src/request.rs:66`), but `crates/standout/src/cli/dispatch.rs:94`
sets it to `Auto` on every production run. Detection is
`console::colors_supported()`, which reads `NO_COLOR` and `TERM=dumb` and ignores
`CLICOLOR` and `CLICOLOR_FORCE`; none of that is documented or tested as a ladder.
lookma switches color off through `set_color_capability_detector(|| false)`
(`crates/lookma/src/config.rs:70-98`), an API that 9.0 removed, so lookma cannot
upgrade without a replacement. rustloc's integration tests set `CLICOLOR_FORCE=1` to
get ANSI from a piped run (`tests/cli_integration.rs:587-600`).

**Paging exists only for help.** `display_with_pager` in
`crates/standout/src/topics.rs:340-380` runs `$PAGER`, then `less`, then `more`, with
no `LESS=FRX`, no `--no-pager`, no tool-specific variable, and no way for a `log` or
`list` handler to page its output. In the 9.0 re-run, the `systemdlike` agent's three
workarounds were all here: scanning argv for `--plain` and `--no-pager`, and rewriting
the final write to page command output (`corpus/rerun/scorecard.md`, systemdlike row).
`gitlike`'s re-run worked around the same paging seam.

**Progress does not exist.** A handler that wants a spinner adds `indicatif`, which
writes to stderr without knowing the output mode, so a `--output json` run interleaves
progress redraws with the document under capture, and `TestHarness` cannot assert on
progress at all. `tflike`'s progress milestone (3 expected-fail tests in
`corpus/gap-suites/tests/tflike_progress.rs`) requires total progress suppression under
`--output ndjson`.

**No `-q` or `-v`.** All eleven surveyed tools have them. Standout owns a warning
channel (`standout_render::warnings::flush_to_stderr`, called from
`crates/standout/src/cli/builder/execution.rs:397`) that nothing can silence, and
there is no info channel. padz prints its own `Warning:` lines by hand
(`crates/padz/src/cli/commands.rs:277-279`). `pnpmlike`'s quiet matrix
(`corpus/archetypes/pnpmlike/acceptance.toml:266-350`) asserts `-q` drops the warning
line and keeps progress steps, and that an explicit `--loglevel` beats `-q` in either
argument order.

## What the user gets

### The CLI user

```text
myapp list --color never                # tri-state flag, beats every env var
MYAPP_COLOR=always myapp list | cat     # app var, beats NO_COLOR and CLICOLOR_FORCE
NO_COLOR=1 myapp list                   # no ANSI on a TTY, unless a flag or app var says otherwise
CLICOLOR_FORCE=1 myapp list | cat       # ANSI into a pipe

myapp log                               # pages on a TTY, LESS=FRX applied when LESS is unset
myapp log --no-pager
MYAPP_PAGER="less -S" myapp log         # app var beats PAGER

myapp sync                              # spinner on a TTY, plain "step 2/5: ..." lines when piped
myapp sync --output json                # no progress output at all
myapp sync -q                           # warnings silenced
myapp sync -vv                          # info and debug channels on stderr
```

Ladder under `--color auto`: `MYAPP_COLOR`, then `NO_COLOR`, then `CLICOLOR_FORCE`, then
terminal detection. `--output term` means `--color always` and `--output text` means
`--color never`; giving `--color` and one of those with different meanings is a clap
usage error, exit 2. Human modes only: under `json`, `yaml`, `csv` and `ndjson` no ANSI
is written anywhere and no progress is written anywhere.

Config keys, resolved by PAR01's ladder:

```toml
[term]
color = "auto"          # auto | always | never
pager = "less -FRX"     # user layer and env only (PAR01 D15)
verbosity = 0           # -1 quiet, 0, 1, 2
```

### The app author

```rust
App::builder()
    .color_flag(true)            // installs --color (default true when an output flag is installed)
    .verbosity_flags(true)       // installs -q/--quiet and -v/--verbose (countable)
    .pager(PagerPolicy::opt_in().secure_when_elevated())
    ...

#[handler(pager)]                // this command's human output goes through the pager on a TTY
fn log(#[ctx] ctx: &CommandContext) -> Result<Output<LogView>, anyhow::Error> { ... }

#[handler]
fn sync(#[ctx] ctx: &CommandContext) -> Result<Output<Report>, anyhow::Error> {
    let progress = ctx.progress();
    let bar = progress.bar("syncing", items.len());
    for item in &items {
        bar.step(&item.name);            // rich on a stderr TTY, plain line when piped, silent in machine modes
    }
    ctx.info("resolved 5 remotes");      // shown at -v
    ctx.warn("remote b is stale");       // shown at default, silenced by -q
    Ok(Output::Render(report))
}
```

Error classes: a clap usage error for conflicting `--color` and `--output`; a
`SetupError` when `color_flag` is requested but `no_output_flag()` removed the output
axis it maps onto; pager spawn failure falls back to a plain write, never an error.

### The test author

```rust
let r = TestHarness::new(app).env("NO_COLOR", "1").tty(Stream::Stdout).run(&["list"]);
r.assert_no_ansi();

let r = TestHarness::new(app).run(&["sync", "-q"]);
assert_eq!(r.progress_steps(), ["a", "b", "c"]);     // captured as data, not bytes
assert!(r.warnings().is_empty());

let r = TestHarness::new(app).tty(Stream::Stdout).pager_capture().run(&["log"]);
assert!(r.paged());                                   // the pager decision, not the process
```

## Decisions

**D18. Color axis.** A global `--color auto|always|never` flag maps onto the existing
`ColorPolicy` and replaces the hardwired `Auto` in `dispatch.rs`. Under `auto` the
ladder is app var, `NO_COLOR`, `CLICOLOR_FORCE`, detection, resolved once into
`TargetProperties`. `--output term` and `text` keep their meaning as spellings of
`always` and `never`; an explicit flag beats every env var. Reason: every surveyed tool
ranks flag above env, and `systemdlike`'s case
`output-term-beats-everything-when-piped` already asserts `--output term` over
`NO_COLOR`. The earlier spec text wanted `NO_COLOR` to win everywhere; it now wins over
detection only. Cost: colorized JSON stays inexpressible, by choice.

**D19. Pager.** Paging is opt-in per command through `#[handler(pager)]`, with a global
`--no-pager`. The chain is `MYAPP_PAGER`, then `PAGER`, then `less`, then `more`, with
`LESS=FRX` exported when `LESS` is unset. The pager runs only when stdout is a TTY and
the output mode is human. When the process runs elevated (euid 0 on Unix) and
`secure_when_elevated` is set, config- and env-sourced pager commands are ignored and
`less` runs with `LESSSECURE=1`. Help's `--page` is reimplemented on the same decision.
Reason: git's `LESS=FRX` and systemd's `SYSTEMD_PAGERSECURE` are the two behaviours the
survey found users depend on. Cost: no auto-paging heuristic (systemctl-style); config
can flip a command's default but the framework never pages a command that did not opt
in.

**D20. Progress.** `ctx.progress()` returns a handle with `step`, `spinner` and `bar`.
Every byte goes to stderr. The backend is chosen once from resolved facts: rich when
stderr is a TTY and the mode is human, plain lines when stderr is not a TTY, silent
under `json`, `yaml`, `csv` and `ndjson`. `indicatif` is the implementation dependency
and is not re-exported. `TestHarness` records emitted steps as data. Reason: correct
suppression needs the resolved output mode, which only the framework has. Cost: no bar
styling options until a corpus app asks.

**D21. Verbosity.** `-q` and `-v` (countable) are global flags installed by
`verbosity_flags(true)`, resolved with `[term] verbosity` into one level: -1 quiet, 0
default, 1 info, 2 debug. Levels filter stderr channels only: quiet drops warnings,
info adds `ctx.info`, debug adds `ctx.debug`. Structured stdout never changes with the
level. Reason: a level that alters the document a script parses is a second content
axis. The earlier spec text mapped levels onto diagnostic detail; that mapping is
dropped. Cost: an app that wants `--loglevel warn` to outrank `-q` regardless of order
(pnpmlike) implements that precedence in its own clap definition; the framework
supplies the level, not the flag precedence.

**D22. Exit criteria.** `tflike`'s full suite green and a blind `systemdlike` run with
zero color or pager workarounds. Reason: `systemdlike` already passes 18 of 18 through
three workarounds, so its pass count cannot measure this epic; the questionnaire's
workaround list can.

## Workstreams

**WS01. Color axis, env ladder, `--output` mapping.** `--color` flag in
`crates/standout/src/cli/builder`, ladder resolution in
`crates/standout-render/src/environment.rs` writing `ColorPolicy` into
`TargetProperties`, the `term`/`text` mapping table in `docs/topics/output-modes.md`,
`[term] color` consumed. Done when the matrix test over (color flag × output mode × TTY
× env ladder row) passes and `systemdlike`'s color-precedence group passes in the
hermetic loop test. First, because every other workstream reads the resolved facts it
produces.

**WS02. Verbosity.** `-q`/`-v`, the level in `ResolvedConfig`, `ctx.info` and
`ctx.debug` channels, warning-channel filtering at the flush in `execution.rs`,
harness `warnings()` and `info()` accessors. Done when the channel table test passes
and `pnpmlike`'s `quiet-silences-the-log-channel-only` passes.

**WS03. Pager.** `PagerPolicy`, `#[handler(pager)]`, `--no-pager`, the chain and
`LESS=FRX`, secure mode, help `--page` rebased, `pager_capture` in the harness. Done
when `systemdlike`'s and `gitlike`'s pager groups pass in the hermetic loop test and a
`run_process` check shows `LESS=FRX` in the child's environment.

**WS04. Progress and harness capture.** `ctx.progress()`, the three backends, harness
`progress_steps()`. Done when `tflike_progress.rs` runs green with its `expect_gap`
wrappers removed and `gaps.toml`'s `tflike/progress` entry flipped to closed.

WS02, WS03 and WS04 run in parallel after WS01.

## Exit criteria

- `corpus/gap-suites/tests/tflike_progress.rs` green, wrappers removed, ledger entry
  closed (`armed = 0`).
- Hermetic loop tests: `systemdlike` color-precedence and pager groups, `gitlike`
  pager group, `pnpmlike` quiet matrix.
- One blind `systemdlike` run via `corpus-runner batch` whose questionnaire lists no
  workaround touching color, `--plain`, `--no-pager` or the final write.
- `docs/topics/terminal-behavior.md` states the full ladder for all four settings in
  one table each.

## Issues

- #329 is already fixed (ADR-0030) and is closed in the tail sweep, not here. WS01
  restates the behavior it documented (`--output term` forces ANSI) as the D18 mapping.
- lookma's `set_color_capability_detector` call and rustloc's `CLICOLOR_FORCE` test
  setup are adopter evidence, not issues; both port to `--color` after this epic.

## Out of scope

A TUI layer; theme or styling changes; log-file or tracing integration; progress bar
styling; auto-paging heuristics; `--loglevel` style named levels (apps define their own
on top of the numeric level).
