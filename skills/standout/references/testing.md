# Testing Standout applications

Choose the smallest boundary that covers the behavior:

| Level | Covers | Tool |
| --- | --- | --- |
| Core | Validation, filtering, state transitions, persistence | Library interface |
| Adapter | CLI-to-core mapping and returned view DTOs | Direct typed handler call |
| Integration | clap through handler, hooks, and rendering | `standout_test::TestHarness` |
| End to end | Real process, PTY, signals, build/link behavior | `assert_cmd`, `expectrl`, or `rexpect` |

Test filtering, validation, state transitions, and persistence directly through
the CLI-free library interface. With `#[handler]`, call the preserved typed
function to test flag/argument mapping and CLI-owned returned data rather than
constructing `ArgMatches` for the generated wrapper.

Use `TestHarness` when command registration, input/environment seams, templates, or output modes matter:

```rust
use standout::Representation;
use standout_test::{serial, TestHarness};

#[test]
#[serial]
fn list_is_machine_readable() {
    let result = TestHarness::new()
        .fixture("todos.txt", "buy milk\n")
        .env("TODO_FILE", "todos.txt")
        .terminal_width(80)
        .output_mode(Representation::Json)
        .run(&app(), cli_command(), ["tdoo", "list"]);

    result.assert_success();
    let value: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(value["total"], 1);
}
```

The harness can control env vars, cwd and tempdir fixtures, terminal width, the four per-stream terminal and color-capability facts, the run's `ColorPolicy` as a separate setting, stdin, clipboard, scripted prompts, and the representation. It returns assertions and accessors for handled text, errors, no-match, binary, artifact and silent outcomes (`result.outcome()`).

Every test using `TestHarness` must use `#[serial]`: env and cwd are process-global. Restoration occurs when the returned `TestResult` drops.

The harness cannot provide a real PTY, deliver signals, mock subprocesses launched by application code, or validate build/link integration. Keep those cases in a small end-to-end suite. Place subprocess calls behind an application-owned trait when unit tests need a fake.

Under a structured mode read a failure with `result.diagnostic()` (`kind`,
`summary`, `detail`, `range`), not from `stderr()`; under `Representation::Ndjson`,
`stdout()` is the whole stream and `diagnostic()` finds the error entry in it.
A status a handler declared with `with_exit_status` is a success:
`assert_success()` holds and `diagnostic()` is `None`.
`assert_schema_snapshot("list.json")` pins a document's key names and value
types (`docs/topics/stability.md`).

Use JSON to assert returned shape, `color(ColorPolicy::Never)` for rendered
strings, and term-debug for style tags. See `crates/standout-test/src/lib.rs`,
`docs/topics/testing.md`,
`crates/todo-example/todo-core/src/store.rs`, and
`crates/todo-example/tdoo/src/app.rs`.
