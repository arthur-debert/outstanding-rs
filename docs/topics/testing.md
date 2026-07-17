# Testing

Standout treats testability as a primary design constraint, not an afterthought. This page is the reference view: how Standout's layers compose to make a CLI testable, which seams the framework exposes, and where each testing technique fits.

For the tutorial introduction — how to *use* `TestHarness` starting from a small surface — see [Introduction to Testing](../guides/intro-to-testing.md).

## Why this section exists

Most CLI frameworks punt on testing. Users end up with one of two patterns: (a) a tangled handler they can't unit test, tested only via subprocess + regex on stdout; (b) a split architecture they enforce by convention, with scaffolding to match mocks to sources duplicated across every test file. Standout tries to make the clean path the easy path.

This is a mix of architectural choices (which move more testable code closer to the surface) and concrete tooling (`standout-test`, environment detectors, default reader shims).

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

`TestHarness` (in the `standout-test` crate) is the unified in-process runner. It wraps `App::run_to_string` with fluent setup for every injectable piece of state:

Its `TestResult` also exposes `exit_status()`, `success_kind()`, and
`error_kind()`, with assertions for typed status and failure origin. `NoMatch`
returns no framework status because the fallback dispatcher still owns the
command.

- Env vars (real `std::env::set_var`, originals captured and restored on drop)
- Working directory (real `std::env::set_current_dir`, original restored on drop)
- Fixture files (written into a `tempfile::TempDir`)
- Terminal detectors: width, TTY, color capability
- Stdin reader (process-global override consulted by `StdinSource::new()`)
- Clipboard reader (same mechanism for `ClipboardSource::new()`)
- Interactive prompt responder (process-global override consulted by every interactive source's `.prompt()` shortcut, so wizard handlers are testable in process — see [Interactive Flows → Testing Wizards](../crates/input/topics/interactive-flows.md#testing-wizards))
- Forced `OutputMode` (injected as `--output=<mode>` into argv)

A `RestoreState` held inside the returned `TestResult` runs on drop — on both normal exit and panic unwind — and tears down every override, so a failing assertion never leaks state into sibling tests. Two nuances worth knowing:

- **Env vars and cwd** are restored to the values captured at `run()` time. This is a true "put it back the way you found it."
- **Terminal detectors and default stdin/clipboard readers** are reset to the library defaults, not to whatever was installed before `run()`. If you mix `TestHarness` with a manually installed `set_*_detector` / `set_default_*_reader` on the same thread, the harness's drop will wipe your override. Keep them separate, or scope the manual override entirely outside the harness.

The harness is `#[must_use]`: a `TestHarness::new()` without a `.run(...)` does nothing and gets flagged by the compiler.

See [Introduction to Testing](../guides/intro-to-testing.md) for the full builder tour.

## Environment seams exposed by the framework

The harness doesn't invent new mechanisms; it wires together seams that Standout exposes deliberately, all of which you can also use directly.

### `standout-render::environment`

The render crate exposes three overridable detectors:

```rust
use standout_render::{
    set_terminal_width_detector, set_tty_detector, set_color_capability_detector,
    reset_environment_detectors, DetectorGuard,
};
```

Each takes a `fn() -> T` (function pointer or non-capturing closure). `DetectorGuard` is a RAII helper that resets all three on drop.

These drive `OutputMode::Auto`'s color decision and the render context's terminal width. Install an override in any test that snapshots rendered output, and the result becomes deterministic across machines.

### `standout-input` default readers

`StdinSource::new()` and `ClipboardSource::new()` resolve their reader through the `DefaultStdin` / `DefaultClipboard` shims. Each shim first consults a process-global override; if none is installed, it falls back to the real OS-backed reader.

```rust
use std::sync::Arc;
use standout_input::{
    set_default_stdin_reader, reset_default_stdin_reader,
    set_default_clipboard_reader, reset_default_clipboard_reader,
};
use standout_input::env::{MockStdin, MockClipboard};

set_default_stdin_reader(Arc::new(MockStdin::piped("hello")));
// ... run test ...
reset_default_stdin_reader();
```

Handlers that use `StdinSource::new()` / `ClipboardSource::new()` / `read_if_piped()` pick up the mock transparently — no handler refactor needed.

Handlers that need per-instance control keep using `StdinSource::with_reader(MockStdin::piped(...))` as before.

### `standout-input` prompt responder

The `.prompt()` shortcut on every interactive source (`InquireText`, `InquireSelect`, `TextPromptSource`, `EditorSource`, …) consults a process-global [`PromptResponder`](https://docs.rs/standout-input/latest/standout_input/trait.PromptResponder.html) before opening any real prompt. Install one to make wizard handlers testable in-process:

The override is process-global, so tests installing it directly must (a) carry a `#[serial(prompt_responder)]` attribute and (b) reset on every exit path including panics — either via an RAII guard or by preferring the harness, which handles both:

```rust
use std::sync::Arc;
use serial_test::serial;
use standout_input::{
    set_default_prompt_responder, reset_default_prompt_responder,
    ScriptedResponder, PromptResponse,
};

struct ResponderGuard;
impl Drop for ResponderGuard {
    fn drop(&mut self) { reset_default_prompt_responder(); }
}

#[test]
#[serial(prompt_responder)]
fn pack_name_re_asks_on_invalid() {
    set_default_prompt_responder(Arc::new(ScriptedResponder::new([
        PromptResponse::text("BadName!"),  // rejected by validator
        PromptResponse::text("good-name"), // accepted on re-ask
    ])));
    let _guard = ResponderGuard; // resets even if assertions panic

    // ... call the wizard step under test, assert, etc ...
}
```

Most tests should reach for `TestHarness::prompts(...)` instead — the harness's `RestoreState` runs `reset_default_prompt_responder()` on drop just like it does for stdin and clipboard, so the boilerplate disappears.

Open prompts (`Text`/`Password`/`Editor`) take a `Text(String)`; finite-choice prompts (`Confirm`/`Select`/`MultiSelect`) take a `Bool` / `Choice(usize)` / `Choices(Vec<usize>)`. Position-based responses are deliberate: a test that picked `Choice(2)` keeps working when you rename `"Production"` to `"Live"`. `ScriptedResponder` panics on kind mismatch so a wizard reorder fails loudly. `PromptResponse::Cancel` and `PromptResponse::Skip` are kind-agnostic and let tests cover the abort and re-ask paths without real signal handling. See [Interactive Flows](../crates/input/topics/interactive-flows.md) for the wizard-shape walkthrough and `TestHarness::prompts(...)` for the harness-level wiring.

### Env vars and cwd

These aren't proxied through a Standout abstraction — they're just real OS primitives. Use `std::env::set_var` / `std::env::set_current_dir` (directly or through the harness). The harness adds: (a) capture-and-restore around `.run()`, and (b) a tempdir per test for fixtures.

## Concurrency model

Every seam above is process-global. Parallel tests that mutate them will interfere with each other.

Use `#[serial]` from the `serial_test` crate (re-exported as `standout_test::serial`) on every test that uses `TestHarness` or any of the lower-level detectors. Within a test binary, serial execution is automatic; across test binaries, cargo runs one test binary at a time by default, so there's no extra coordination needed.

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
        .no_color()
        .run(&app(), command(), ["todo", "list"]);

    assert_snapshot!(result.stdout());
}
```

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

This path has no `#[serial]` requirement — nothing global is touched.

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
