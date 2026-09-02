# Testing

Standout treats testability as a primary design constraint, not an afterthought. This page is the reference view: how Standout's layers compose to make a CLI testable, which seams the framework exposes, and where each testing technique fits.

For the tutorial introduction — how to *use* `TestHarness` starting from a small surface — see [Introduction to Testing](../guides/intro-to-testing.md).

## Why this section exists

Most CLI frameworks punt on testing. Users end up with one of two patterns: (a) a tangled handler they can't unit test, tested only via subprocess + regex on stdout; (b) a split architecture they enforce by convention, with scaffolding to match mocks to sources duplicated across every test file. Standout tries to make the clean path the easy path.

This is a mix of architectural choices (which move more testable code closer to the surface) and concrete tooling (`standout-test`, `TargetProperties` injection, `InputSources`).

## Four levels, four tools

A production-shaped Standout app has four testing layers, each appropriate to a different kind of change:

| Level | What it covers | Tool | Speed |
| --- | --- | --- | --- |
| Core | Library validation, filtering, transitions, persistence | Plain `#[test]` through the library interface | Microseconds |
| Adapter | CLI-to-core mapping and returned view DTOs | Direct typed handler call | Microseconds |
| Integration | Full dispatch pipeline in-process: argv → handler → render | `standout-test::TestHarness` | Microseconds to low milliseconds |
| End-to-end | Real process, real PTY, real signals, real subprocess fan-out | `assert_cmd`, `expectrl`, `rexpect` | Tens to hundreds of milliseconds per test |

Choose by what the change touches. A bug in a filter predicate belongs in the
core library. A bug mapping `--all` to that filter belongs in a direct handler
test. A bug in "does this command actually read piped stdin?" belongs in the
harness. A bug in raw-mode TUI redraw belongs in an end-to-end test.

## What each layer gives you for free

### The library owns behavior; handlers are adapters

Keep the reusable application library free of Clap, Standout, command contexts,
environment lookup, templates, and output. Test filtering, validation, state
transitions, and persistence through that library's interface.

The handler then maps CLI input to a library call and maps the result to a
CLI-owned serializable view model. It does not touch stdout or render. With
`#[handler]`, test this mapping by calling the preserved typed function and
asserting on `Output::Render` data.

For the canonical example, see the
[production-shaped application](../guides/production-shaped-example.md). The
key invariant is stronger than terminal independence: *nothing in the reusable
library depends on the CLI*.

### Clap is already tested

Argument parsing is clap's responsibility, and clap has an extensive test suite of its own. You don't need to rewrite those tests; you just need to trust the seam. If you have truly exotic arg-parsing logic, test it by calling `Command::try_get_matches_from(...)` directly — that's clap's in-process API.

### Rendering is already tested

`standout-render` has snapshot tests for MiniJinja template evaluation, CSS parsing, style resolution, tag transforms, tabular layouts, and every output mode. Again, you don't need to re-test it — you need to test that *your* templates render the shape of data you think they do. The harness covers that naturally by running the full pipeline.

## What the harness adds

`TestHarness` (in the `standout-test` crate) is the unified in-process runner. It wraps `App::run_with_sink` with fluent setup for every injectable piece of state:

Its `TestResult` also exposes `exit_status()`, `success_kind()`, and
`error_kind()`, with assertions for typed status and failure origin. `NoMatch`
returns no framework status because the fallback dispatcher still owns the
command. For an `AppFailure` or an `ExternalFailure`, `stdout()` is empty,
`error()` and `stderr()` are the verbatim diagnostic, `error_kind()` is
`RunErrorKind::App` or `RunErrorKind::External`, and `exit_status()` retains the
declared value (including values such as `128`).

- Env vars (real `std::env::set_var`, originals captured and restored on drop)
- Working directory (real `std::env::set_current_dir`, original restored on drop)
- Fixture files (written into a `tempfile::TempDir`)
- Destination facts on `TargetProperties`: width, color capability, color-scheme, icon mode, ambiguous-width (injected, never detected)
- Stdin, clipboard, and prompt responder as an [`InputSources`](https://docs.rs/standout/latest/standout/struct.InputSources.html) value passed into `App::run_with` (not process-global overrides)
- Interactive prompt responder on those sources, so wizard handlers that call `.prompt_from(ctx.input_sources())` are testable in process — see [Interactive Flows → Testing Wizards](../crates/input/topics/interactive-flows.md#testing-wizards)
- Forced `OutputMode` (injected as `--output=<mode>` into argv)
- Framework warnings captured from the run boundary, including accepted answer-sheet parse warnings queued by questionnaire commands

A `RestoreState` held inside the returned `TestResult` runs on drop — on both normal exit and panic unwind — and tears down every override, so a failing assertion never leaks state into sibling tests. Two nuances worth knowing:

- **Env vars and cwd** are restored to the values captured at `run()` time. This is a true "put it back the way you found it."
- **Destination facts** are injected on `TargetProperties` for that run; the harness does not install detector overrides. Stdin, clipboard, and the prompt responder are not process-global: they live on the `InputSources` value for that run.

The harness is `#[must_use]`: a `TestHarness::new()` without a `.run(...)` does nothing and gets flagged by the compiler.

See [Introduction to Testing](../guides/intro-to-testing.md) for the full builder tour.

### Captured warnings

`App::run` renders framework warnings to stderr after the primary command
output, styled from stderr color capability on `TargetProperties`. `TestHarness`
reads them from the run result: each `TestResult` owns the warnings produced by
that run and exposes them through `warnings()` plus assertion helpers such as
`assert_warning_contains(...)`. There is no thread-local warning collector.

## Environment seams exposed by the framework

The harness doesn't invent new mechanisms; it wires together seams that Standout exposes deliberately, all of which you can also use directly.

### `TargetProperties` (standout-render)

Detection is [`TargetProperties::detect()`](https://docs.rs/standout-render/latest/standout_render/struct.TargetProperties.html#method.detect) at the crate edge. Convenience wrappers and `App::run` call it there, then pass the result into `render_request`. Tests do not call `detect()`; they construct `TargetProperties` or inject facts through `TestHarness` (`terminal_width`, `with_color` / `no_color`, `color_scheme`, `icon_mode`, `ambiguous_width`). Unset facts take fixed defaults — `width: None`, `ColorMode::Dark`, `IconMode::Classic`, `AmbiguousWidth::Narrow` — so `$COLUMNS`, `$NERD_FONT`, and the OS appearance setting cannot change an in-process run.

The detector override APIs are removed: `set_terminal_width_detector`, `set_color_capability_detector`, `set_ambiguous_width_detector`, `set_theme_detector`, `set_icon_detector`, `DetectorGuard`, and the public `detect_*` cluster they served.

There is no TTY detector: one existed, nothing in production ever read it, and it was removed rather than left as a seam that answers only about stdout (`docs/adr/0022-delete-the-in-process-tty-seam.md`). Terminal-dependent behavior is tested against a real process via `TestHarness::run_process`.

### `standout-input` InputSources

Stdin, clipboard, and the prompt responder are arguments to input collection,
carried on [`InputSources`](https://docs.rs/standout-input/latest/standout_input/struct.InputSources.html).
Production `App::run` constructs them from the real process.
`TestHarness` constructs mocks and passes them into `App::run_with`.
`StdinSource::new()` / `ClipboardSource::new()` bind to those sources at
resolve time; handlers that resolve a chain themselves call
`InputChain::resolve_from(matches, ctx.input_sources())`.

```rust
use standout_input::{InputSources, MockStdin};

let sources = InputSources::from_process().with_stdin(MockStdin::piped("hello"));
let value = chain.resolve_from(&matches, &sources)?;
```

Handlers that need a source-local mock keep using
`StdinSource::with_reader(MockStdin::piped(...))` as before.

### Testing invocation-aware default commands

`default_command_with` reads the stdin terminal fact from `InputSources`, so `TestHarness` drives it with no extra wiring:

```rust
// Piped stdin resolves the naked invocation to the piped entry point.
TestHarness::new()
    .piped_stdin("ship the docs\n")
    .run(&app, cli::command(), ["tdoo"])
    .assert_stdout_contains("Added");

// A terminal resolves it to the interactive one.
TestHarness::new()
    .interactive_stdin()
    .run(&app, cli::command(), ["tdoo"])
    .assert_stdout_contains("Your Todos");
```

`piped_stdin("")` covers the **piped-but-empty** case: the resolver sees a pipe, not a terminal, because emptiness is only knowable by reading. Use it to assert that a receiving command's `InputChain` rejects empty input, rather than expecting resolution to route around it.

`interactive_stdin()` is required for the terminal branch — without it the harness inherits the real stdin, which is *not* a terminal under a test runner, so a naked invocation would take the piped branch and the test would pass or fail depending on how it was launched.

For the parse-only path, `get_matches_from` takes sources explicitly:

```rust
use standout_input::{InputSources, MockStdin};

let sources = InputSources::from_process().with_stdin(MockStdin::terminal());
match app.get_matches_from(cli::command(), ["tdoo"], &sources) {
    HelpResult::Matches(m) => assert_eq!(m.subcommand_name(), Some("list")),
    other => panic!("expected matches, got {other:?}"),
}
```

### `standout-input` prompt responder

The `.prompt_from(&sources)` shortcut on every interactive source (`InquireText`, `InquireSelect`, `TextPromptSource`, `EditorSource`, …) consults the [`PromptResponder`](https://docs.rs/standout-input/latest/standout_input/trait.PromptResponder.html) on [`InputSources`] before opening any real prompt. Put a [`ScriptedResponder`](https://docs.rs/standout-input/latest/standout_input/struct.ScriptedResponder.html) on those sources — or use `TestHarness::prompts(...)` — to make wizard handlers testable in-process:

```rust
use std::sync::Arc;
use standout_input::{InputSources, ScriptedResponder, PromptResponse};

let sources = InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
    PromptResponse::text("BadName!"),  // rejected by validator
    PromptResponse::text("good-name"), // accepted on re-ask
])));
let name = TextPromptSource::new("Pack name: ").prompt_from(&sources)?;
```

Most tests should reach for `TestHarness::prompts(...)` instead; handlers then call `.prompt_from(ctx.input_sources())`.

Open prompts (`Text`/`Password`/`Editor`) take a `Text(String)`; finite-choice prompts (`Confirm`/`Select`/`MultiSelect`) take a `Bool` / `Choice(usize)` / `Choices(Vec<usize>)`. Position-based responses are deliberate: a test that picked `Choice(2)` keeps working when you rename `"Production"` to `"Live"`. `ScriptedResponder` panics on kind mismatch so a wizard reorder fails loudly. `PromptResponse::Cancel` and `PromptResponse::Skip` are kind-agnostic and let tests cover the abort and re-ask paths without real signal handling. See [Interactive Flows](../crates/input/topics/interactive-flows.md) for the wizard-shape walkthrough and `TestHarness::prompts(...)` for the harness-level wiring.

### Env vars and cwd

These aren't proxied through a Standout abstraction — they're just real OS primitives. Use `std::env::set_var` / `std::env::set_current_dir` (directly or through the harness). The harness adds: (a) capture-and-restore around `.run()`, and (b) a tempdir per test for fixtures.

## Concurrency model

Env vars and cwd remain process-global. Parallel tests that mutate them will interfere with each other. Destination facts (width, color, color-scheme, icon mode) are injected on `TargetProperties` and no longer need `#[serial]` for detector reasons.

Use `#[serial]` from the `serial_test` crate (re-exported as `standout_test::serial`) on every in-process `TestHarness::run` test while those env/cwd overrides exist: `serial_test` only orders annotated tests against each other, so an unannotated `run` can race with one that mutates env or cwd. Input sources and warning capture no longer require `#[serial]`. Within a test binary, serial execution is automatic among annotated tests; across test binaries, cargo runs one test binary at a time by default, so there's no extra coordination needed.

## Recipes

### Snapshot testing with `insta`

Pin terminal state for determinism, run, snapshot the output:

```rust
use insta::assert_snapshot;

#[test]
#[serial]
fn list_snapshot() {
    let result = TestHarness::new()
        .fixture("todos.txt", "a\nb\nc\n")
        .terminal_width(80)
        .ambiguous_width(standout::AmbiguousWidth::Narrow)
        .no_color()
        .run(&app(), command(), ["todo", "list"]);

    assert_snapshot!(result.stdout());
}
```

Use `TestHarness::ambiguous_width(AmbiguousWidth::Narrow)` and
`AmbiguousWidth::Wide` to assert the same rendering fixture under both explicit
policies. The override crosses the same App/Renderer width seam as production
configuration and is restored when the `TestResult` drops.

### Asserting JSON shape

Force `OutputMode::Json` to bypass the template and serialize the handler's data directly:

```rust
let result = TestHarness::new()
    .output_mode(OutputMode::Json)
    .run(&app, cmd, ["myapp", "list"]);

let v: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
assert_eq!(v["todos"].as_array().unwrap().len(), 3);
```

### Testing a handler without going through dispatch

For pure logic tests, skip the harness entirely:

```rust
#[test]
fn filter_excludes_done_by_default() {
    let matches = Command::new("t")
        .arg(clap::Arg::new("all").long("all").action(clap::ArgAction::SetTrue))
        .try_get_matches_from(["t"])
        .unwrap();
    let ctx = CommandContext::default();

    let Output::Render(result) = list(&matches, &ctx).unwrap() else { panic!() };
    assert!(result.todos.iter().all(|t| matches!(t.status, Status::Pending)));
}
```

### Testing a delegated process failure

Use a direct typed-handler test for the adapter mapping, `TestHarness` for the
captured metadata, and one process test when exact OS status and stream bytes
are part of the application's contract:

```rust
let result = TestHarness::new().run(&app, command, ["myapp", "fetch"]);
result.assert_error_kind(RunErrorKind::External);
assert_eq!(result.exit_status().unwrap().code(), 128);
assert_eq!(result.error(), Some("fatal: repository not found\n"));
result.assert_stdout_eq("");
```

The harness does not perform final writes, so the process-level test remains
the proof that `run()` writes only the declared payload to stderr and exits with
the same status.

This path has no `#[serial]` requirement — nothing global is touched.

### Asserting a compound artifact

For `Output::Artifact`, the harness observes the whole framework-owned
transaction: the bytes, what the application suggested, where the framework
actually wrote, the rendered report, and the typed failure when no destination
can be selected.

```rust
let dir = tempfile::tempdir().unwrap();
let out = dir.path().join("todos.csv");

let result = TestHarness::new().run(
    &app,
    command,
    ["myapp", "export", "--output-file-path", out.to_str().unwrap()],
);

result.assert_success();
result.assert_artifact_bytes(b"id,title,done\n1,buy milk,false\n");
result.assert_artifact_suggested_destination("todos.csv"); // the app's suggestion
result.assert_artifact_written_to(&out);                   // where it actually went
result.assert_artifact_report_contains("Exported 1 todos");
```

`assert_artifact_to_stdout()` covers the `allow_stdout()` destination, and
`artifact_report()` returns the rendered (or, in structured mode, serialized)
report for deeper assertions. A write that cannot pick a destination — or that
fails — is a typed error the harness asserts like any other:

```rust
result.assert_error_kind(RunErrorKind::FinalWrite(OutputKind::Artifact));
assert!(result.artifact().is_none()); // a failed write reports nothing
```

### Mixing levels

A common layout for a CLI crate:

```text
tests/
├── handlers.rs       # level 1 — direct handler calls
├── harness.rs        # level 2 — TestHarness integration tests
└── e2e.rs            # level 3 — assert_cmd for the few things the harness can't cover
```

Run them together with `cargo test`. Level 1 is by far the largest file; level 3 is usually less than a dozen tests.

## Boundaries

`TestHarness` is an in-process runner. It cannot simulate:

- **Real PTY.** `isatty()` on the real stdin file descriptor, raw-mode terminals, progress bars that depend on cursor control. Use `expectrl` / `rexpect` with a spawned subprocess.
- **Signals.** SIGINT / SIGTERM handling needs a real process.
- **Shelling out from your handler.** If a handler invokes `git`, `rg`, `$EDITOR`, etc., those run as real subprocesses in the test too. A `ProcessRunner` abstraction to address this is in progress (Phase 3 of the test-tooling work); until it lands, structure shell-outs behind a local trait you can swap for a mock in handler tests.
- **Build / linker integration.** Testing that the compiled binary has the right embedded resources, dependencies, or `--version` output is fair game for a small `assert_cmd` suite.

The goal is to keep level-3 tests small and intentional — the cases where you really do need a real process — and put everything else at level 1 or 2.

## See also

- [Introduction to Testing](../guides/intro-to-testing.md) — the tutorial
- [Handler Contract](../crates/dispatch/topics/handler-contract.md) — typed handler adapter contract
- [Output Modes](./output-modes.md) — forcing deterministic output
- [Introduction to Input](../crates/input/guides/intro-to-input.md) — input sources and their mock variants
