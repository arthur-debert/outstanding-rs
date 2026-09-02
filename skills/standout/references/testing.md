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
use standout::OutputMode;
use standout_test::{serial, TestHarness};

#[test]
#[serial]
fn list_is_machine_readable() {
    let result = TestHarness::new()
        .fixture("todos.txt", "buy milk\n")
        .env("TODO_FILE", "todos.txt")
        .terminal_width(80)
        .output_mode(OutputMode::Json)
        .run(&app(), cli_command(), ["tdoo", "list"]);

    result.assert_success();
    let value: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(value["total"], 1);
}
```

The harness can control env vars, cwd and tempdir fixtures, terminal width, TTY/color detection, stdin, clipboard, scripted prompts, and output mode. It returns assertions and accessors for handled text, errors, no-match, and binary outcomes. There is no dedicated silent accessor: `run_to_string` currently exposes silent output as an empty handled string, so the harness cannot distinguish it from intentionally empty text.

Every test using `TestHarness` must use `#[serial]`: its seams mutate process-global state. Restoration occurs when the returned `TestResult` drops. Do not mix a harness run with manually installed detector or default-reader overrides in the same scope; those reset to library defaults, not prior custom overrides.

The harness cannot provide a real PTY, deliver signals, mock subprocesses launched by application code, or validate build/link integration. Keep those cases in a small end-to-end suite. Place subprocess calls behind an application-owned trait when unit tests need a fake.

Under a structured mode a failed run's stdout is the diagnostic document and
`stderr()` carries no framework prose for it (an `AppFailure` or
`ExternalFailure` keeps its verbatim bytes there; warnings stay on stderr
outside `ndjson`): read the failure with `result.diagnostic()` /
`expect_diagnostic()` (`kind`, `summary`, `detail`, `range`) and pair it with
`assert_error_kind` and `assert_exit_status`. Under `OutputMode::Ndjson`, `stdout()` is the whole stream,
one entry per line with its newline kept; parse lines with `serde_json` and read
the failure with `diagnostic()`, which finds the error entry in the stream. A
status a handler declared with `with_exit_status` is a success:
`assert_success()` holds, `exit_status()` reports the status, `diagnostic()` is
`None`. `assert_schema_snapshot("list.json")` pins a document's key names and
value types against `tests/schemas/list.json` and ignores values.

Use JSON to assert returned shape, text/no-color for rendered strings, and
terminal-debug for style tags. See `crates/standout-test/src/lib.rs`,
`docs/topics/testing.md`,
`crates/todo-example/todo-core/src/store.rs`, and
`crates/todo-example/tdoo/src/app.rs`.
