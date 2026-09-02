# Testing Standout CLIs

This is the guide for testing CLIs built with Standout. It starts from a claim most people nod at but few act on — *"shell apps should be easy to test"* — and shows how Standout's architecture, combined with the `standout-test` crate, actually makes that true.

**See also:**

- [Handler Contract](../crates/dispatch/topics/handler-contract.md)
- [Testing (Topic)](../topics/testing.md) — reference for the `standout-test` API surface
- [Output Modes](../topics/output-modes.md)

## 1. The claim no one keeps

"Shell applications should be easy to test. Just keep logic separate from output."

Sure. And yet look at any CLI in the wild and count the tests that:

- Spawn the compiled binary as a subprocess
- Pipe some argv in
- Capture stdout
- Regex-match the output

That's not testing behavior. That's reverse-engineering the user interface on every run. When you test a function via its rendered output, every trivial copy change breaks the test. Every color tweak breaks the test. Every time you add an emoji, every time the column widths shift, every time a locale flips — broken tests.

The honest answer is that most CLI codebases *don't* keep logic and output cleanly separated, because there's no discipline enforcing it. `println!` is always one line away. The tests you end up writing reflect that: they're shell-out + regex, because the production code is too tangled to test any other way.

## 2. The free win: architecture

Standout's first contribution to testability has nothing to do with testing
tools. It is the architecture around the framework: a CLI-free library owns
behavior, and Standout handlers adapt shell input and library results.

The library exposes ordinary Rust behavior:

```rust
pub fn list(&self, filter: TodoFilter) -> Vec<Todo> {
    // filtering and persistence details stay behind this interface
}
```

The handler is a typed adapter returning CLI view data:

```rust
#[handler]
pub fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all { TodoFilter::All } else { TodoFilter::Pending };
    let todos = store.list(filter).into_iter().map(TodoView::from).collect();
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}
```

Test behavior through the library interface, then test only adapter mapping in
the handler:

```rust
#[test]
fn list_filters_completed_by_default() {
    let store = fixture_store();
    let todos = store.list(TodoFilter::Pending);
    assert!(todos.iter().all(|todo| !todo.done));
}

#[test]
fn list_handler_maps_pending_view() {
    let ctx = context_with_fixture_store();
    let Output::Render(result) = list(false, &ctx).unwrap() else {
        panic!("expected Render");
    };
    assert!(result.todos.iter().all(|todo| !todo.done));
}
```

No stdout capture. No regex. No subprocess. Just a function call and a struct assertion. The test reads like the behavior it describes.

This keeps the majority of real logic—filtering, aggregation, validation, and
business rules—independent of the shell. Standout keeps the adapter data-first
so the CLI seam is directly testable too.

> **Verify:** Pick a handler in your app. Write a test that calls it directly and asserts on the returned data. If you can't, the handler has logic tangled with side effects — that's the real bug.

### Intermezzo A: What the architecture already bought you

**What you got for free:**

- Core behavior is tested through a CLI-free library interface.
- Typed handlers are directly testable adapters returning `Output<T>`.
- Output data is a `Serialize` struct. You can also assert on it as JSON (useful for cross-language consumers).
- Argument parsing is clap's problem. Clap has its own extensive test suite — you don't need to re-test it.
- Template rendering is `standout-render`'s problem. Its test suite covers MiniJinja syntax, tag parsing, style resolution, output modes.

**What's left:**

- **Integration** — does the full pipeline (argv → dispatch → handler → render → stdout) actually work for this command?
- **Environment-dependent behavior** — does this command react correctly to piped stdin, a missing env var, a narrow terminal, no color support?
- **Filesystem-dependent behavior** — does the command find, read, and write files in the right places?

These three are where CLIs traditionally fall back to subprocess-based e2e tests. That's what the rest of this guide is about.

## 3. The remaining gap

Let's be precise about what the architecture *doesn't* solve and why subprocess tests are tempting.

**Integration.** Even with clean handlers, a bug can live at the seam: an argument you thought was global isn't, a hook mutates state the handler doesn't see, a template references a field that doesn't exist. You want to assert on the *rendered output* of a full invocation, not just on the handler's return value.

**The environment.** CLIs read from the environment in a dozen places: `$EDITOR`, `$HOME`, piped stdin, the clipboard, the terminal width, whether stdout is a TTY, whether the terminal supports color, the current working directory, files at specific paths. Any of these can change behavior. None of them are the handler's "input" in the argv sense.

**Filesystem state.** Your command may need to read a config file at `~/.myapp/config.toml`, write a lockfile, list entries in a working directory. Testing this with real paths pollutes the developer's machine; testing it by hand-rolling temp dirs in every test file duplicates code.

The default answer is:

```rust
#[test]
fn list_shows_todos() {
    let output = Command::cargo_bin("myapp")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("buy milk"));
}
```

This works. It's also:

- **Slow.** Spawning your binary is tens to hundreds of milliseconds, not microseconds.
- **Opaque.** If it fails, you get the stdout blob and a non-zero exit. You can't step into it, you can't inspect intermediate state.
- **Brittle.** The assertion is on rendered text; any presentation change breaks it.
- **Hostile to invariants.** Want to assert "the command set no env var as a side effect"? "The JSON payload had exactly these keys"? "A specific template was selected"? Good luck.

Subprocess tests have a place — and section 6 below names it — but they shouldn't be the default.

## 4. The `standout-test` harness

`standout-test` gives you a fluent builder that runs your app *in-process* with full control over the environment, then hands back a `TestResult` with typed accessors and assertion helpers.

```toml
# Cargo.toml
[dev-dependencies]
standout-test = "9"
```

The smallest possible test:

```rust
use serial_test::serial;
use standout_test::TestHarness;

#[test]
#[serial]
fn list_runs() {
    let app = build_app();              // your normal App::builder().build()?
    let cmd = build_cli_command();      // your clap Command

    let result = TestHarness::new().run(&app, cmd, ["myapp", "list"]);

    result.assert_success();
    result.assert_stdout_contains("buy milk");
}
```

That's it. `run()` drives the same dispatch path as production — same clap parsing, same handler lookup, same render pipeline — and returns the rendered text. No subprocess, no stdout capture gymnastics.

> **Why `#[serial]`?** The harness mutates process-global state (env vars, cwd). Destination facts (width, color, color-scheme, icon mode) are injected on `TargetProperties` and do not need `#[serial]` for detector reasons. All in-process `run` tests still need `#[serial]` while those env/cwd overrides exist: `serial_test` only orders annotated tests against each other, so an unannotated `run` can race with one that mutates env or cwd. The `serial_test::serial` attribute is re-exported from `standout_test` for convenience: `use standout_test::serial;`.
>
> **Verify:** Add a `TestHarness::new().run(...)` test to your app. It should run in under 10ms, not 100ms.

For width-sensitive fixtures, run the same app with
`.ambiguous_width(AmbiguousWidth::Narrow)` and
`.ambiguous_width(AmbiguousWidth::Wide)`. Narrow is the compatibility default;
the harness override makes either policy deterministic without locale guessing.

### 4.1 Env vars

Your command reads `$EDITOR`? Set it:

```rust
#[test]
#[serial]
fn respects_editor_env() {
    let result = TestHarness::new()
        .env("EDITOR", "vim")
        .run(&app, cmd, ["myapp", "note", "new"]);

    result.assert_stdout_contains("opening vim");
}
```

Need to *remove* an env var that exists on your dev machine?

```rust
.env_remove("HOME")
```

Both are backed by real `std::env::set_var` / `remove_var`. The originals are captured before the run and restored when the `TestResult` drops — including on panic unwind, so a failing assertion never leaks state into the next test.

### 4.2 Fixtures and working directory

For commands that read or write files:

```rust
#[test]
#[serial]
fn reads_config() {
    let result = TestHarness::new()
        .fixture("config.toml", r#"format = "short""#)
        .fixture("todos/today.md", "- buy milk\n- write tests\n")
        .run(&app, cmd, ["myapp", "show"]);

    result.assert_stdout_contains("buy milk");
}
```

Each `.fixture()` call writes a file into a freshly created `tempfile::TempDir`. The first fixture call also sets that tempdir as the working directory for the run, so handlers using relative paths just work.

You can access the tempdir directly if you need absolute paths as handler arguments:

```rust
let harness = TestHarness::new().fixture("input.txt", "hello\n");
let path = harness.tempdir().unwrap().join("input.txt");
let result = harness.run(&app, cmd, ["myapp", "cat", path.to_str().unwrap()]);
```

Fixture paths must be relative and stay inside the tempdir — absolute paths and `..` components are rejected so a stray fixture can't clobber your real home directory.

### 4.3 Piped stdin

Want to test the "CLI piped as input" path?

```rust
#[test]
#[serial]
fn reads_from_stdin() {
    let result = TestHarness::new()
        .piped_stdin("draft text\n")
        .run(&app, cmd, ["myapp", "publish"]);

    result.assert_stdout_contains("draft text");
}
```

Any handler built on `standout-input::StdinSource::new()` — or on `standout_input::read_if_piped()` — transparently sees the mock. It reports `is_terminal() == false` and reads the content you supplied.

The counterpart:

```rust
.interactive_stdin()    // StdinSource::new().is_terminal() reports true; nothing to read
```

### 4.4 Clipboard

Same story for the system clipboard:

```rust
.clipboard("https://example.com/pasted-url")
```

`ClipboardSource::new()` returns the mock content; no shelling out to `pbpaste` / `xclip`.

### 4.5 Interactive prompts (wizards)

Apps that drive their own interactive shell — wizards, setup helpers, REPLs — call `InquireText::new(...).prompt_from(ctx.input_sources())`, `InquireSelect::new(...).prompt_from(ctx.input_sources())`, etc. Without a seam those calls need a real TTY and become level-3 territory. With `.prompts(...)`, the harness places a responder on the run's `InputSources` so a wizard handler that reads `ctx.input_sources()` is fully testable in process:

```rust
use standout_input::{PromptResponse, ScriptedResponder};
use std::sync::Arc;

#[test]
fn setup_wizard_completes_with_scripted_answers() {
    let result = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([
            PromptResponse::text("foo"),     // pack name
            PromptResponse::Bool(true),      // confirm
            PromptResponse::Choice(2),       // env -> options[2]
        ])))
        .run(&app, cmd, ["mycli", "setup"]);

    result.assert_stdout_contains("created pack `foo`");
}
```

Open prompts (`Text`/`Password`/`Editor`) take `PromptResponse::Text(...)`; finite-choice prompts (`Confirm`/`Select`/`MultiSelect`) take a `Bool`/`Choice(usize)`/`Choices(Vec<usize>)`. Position-based responses make tests resilient to copy changes: `Choice(2)` keeps working when "Production" is renamed to "Live". `ScriptedResponder` panics on kind mismatch, so a wizard-step reorder fails loudly. See [Interactive Flows → Testing Wizards](../crates/input/topics/interactive-flows.md#testing-wizards) for the full pattern.

### 4.6 Terminal state

Two orthogonal knobs, injected on `TargetProperties` (never detected from the process):

```rust
.terminal_width(80)     // forces a fixed width for tabular layouts
.no_color()             // forces OutputMode::Auto to behave like Text
.with_color()           // forces Auto to behave like Term even when piped
```

Useful for snapshot testing: pin the width, turn off color, and the rendered string is deterministic across developer machines and CI.

`with_color()` is also what makes an *ANSI-positive* assertion possible in-process. Two switches stand between a styled template and escape bytes: Standout's own color decision, and `console`'s process-global color switch, which `Style::apply_to` consults and which is off in a non-TTY process — and a test binary is never a TTY. `with_color()` sets both (and restores the second on drop), so a `Term` render in a test emits the escapes a terminal user would see, with no `force_styling` needed in the theme:

```rust
let result = TestHarness::new()
    .with_color()
    .output_mode(OutputMode::Term)
    .run(&app(), command(), ["myapp", "list"]);

assert!(result.stdout().contains('\x1b'));    // really styled
assert_eq!(result.stdout_plain(), expected);  // and strippable
```

There is no TTY knob. Questions that genuinely depend on being (or not being) a terminal belong to a real process — see [`run_process`](#410-running-the-real-binary) and [ADR-0022](../adr/0022-delete-the-in-process-tty-seam.md).

### 4.7 Forcing an output mode

Sometimes you want to assert on structured output regardless of what the user's `--output` flag would have chosen. Instead of manually appending `--output=json` to argv:

```rust
#[test]
#[serial]
fn list_as_json_has_expected_shape() {
    let result = TestHarness::new()
        .output_mode(OutputMode::Json)
        .run(&app, cmd, ["myapp", "list"]);

    let value: serde_json::Value =
        serde_json::from_str(result.stdout()).unwrap();
    assert!(value["todos"].is_array());
    assert_eq!(value["todos"].as_array().unwrap().len(), 3);
}
```

If your app renamed the flag via `AppBuilder::output_flag(Some("format"))`, tell the harness:

```rust
.output_flag_name("format")
```

### 4.8 Invariant assertions

`assert_stdout_contains("default: auto")` is an *existential* claim: this string
is somewhere on the page. Most rendering defects are not that shape. They are
universals ("every value-taking option shows a metavar") and negatives ("no
presence flag lists possible values") — and a list of strings that should be
present says nothing about a wrong line rendered beside them.

`standout_test::invariants` holds those as reusable assertions, each naming the
offending element when it fails:

```rust
use standout_test::invariants::*;

let page = TestHarness::new().text_output().run(&app, cmd(), ["notes", "--help"]);

assert_every_tag_resolved(&page);                        // the theme defines every tag rendered
assert_no_unresolved_tag_markers(&page);                 // no `[tag?]` reached the page
assert_metavar_for_valued_args(&page, &cmd());           // clap's metadata is the oracle
assert_no_possible_values_for_valueless_args(&page, &cmd());
assert_descriptions_aligned(&page);                      // every section's column, not two rows by hand
```

`assert_every_tag_resolved` reads structured data — `TestResult::tag_resolutions()`,
what each style-tag pass could not resolve — so it holds in every output mode
and names the tag. `assert_no_unresolved_tag_markers` scans the rendered page
for a `[tag?]`-shaped marker — a guard against a regression that would
reintroduce the retired marker into output, which should never appear. The two
assertions catch the same defect from different directions and both are worth
running. Each *text* assertion also has an `*_in_page` form taking the rendered
page directly (`assert_styling_preserves_layout_in_pages` takes the two it
compares), for asserting on a page the harness did not produce.
`assert_every_tag_resolved` is the exception: what it reads is
`TestResult::tag_resolutions()`, which no page carries — that is exactly why it
can name a tag in a mode where the page shows nothing.

### 4.9 Clap-parity: the differential oracle

The invariants above state properties of the rows a page *does* render. They say
nothing about a row that was never rendered at all — and a field the help data
extractor forgot to copy has no row, no wrong line, and nothing for an
existential assertion to trip over. That is the shape of every defect in the
themed-help cluster: `long_about`, defaults, possible values and metavars were
all things clap knew and the page did not say.

`standout_test::clap_parity` closes that hole by asserting against an oracle
outside standout — clap's own metadata:

```rust
use standout::cli::HelpLength;
use standout_test::clap_parity::assert_states_clap_facts;

let page = TestHarness::new().text_output().run(&app, cmd(), ["notes", "--help"]);

// `--help` and the `help` word owe `long_about`; `-h` owes `about`.
assert_states_clap_facts(&page, &cmd(), HelpLength::Long);
```

It walks the command — subcommands, arguments, value names, help texts,
defaults and possible values with clap's own suppression rules, hidden metadata
respected — and requires each fact to appear in the row that owns it, naming
the argument and the value when one is missing. It asserts **presence of
facts, never layout**: themed help is meant to look different from clap's, so
`default: brief` and `[default: brief]` satisfy it equally.

Facts standout deliberately does not render live in one allowlist,
`clap_parity::DELIBERATE_OMISSIONS`, each with its reason — an unexplained
exemption is indistinguishable from a forgotten field, which is the failure
mode the differential exists to end. Pass your own list to
`assert_page_states_clap_facts_with` when a page is deliberately narrower;
`&[]` asserts full parity.

### 4.10 Running the real binary

`run()` calls into your app inside the test process, so the two text streams it reports are a faithful *reconstruction* of what `App::run`'s writer seam would have emitted — not a recording of what the OS carried. For the handful of facts only the real boundary settles, `run_process()` runs the compiled binary instead and returns what the kernel saw:

```rust
#[test]  // no #[serial]: nothing process-global is touched
fn a_usage_error_goes_to_stderr_and_leaves_stdout_clean() {
    let result = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_mycli"), ["bogus-command"]);

    result.assert_exit_code(2);
    result.assert_stdout_empty();               // real pipe, not a model of one
    result.assert_stderr_contains("unexpected argument");
}
```

`ProcessResult` carries `stdout()` / `stderr()` (and `stdout_bytes()` / `stderr_bytes()` when the output isn't text), the ANSI-stripping `stdout_plain()` / `stderr_plain()`, `status()` / `code()` / `success()`, and the assertion helpers above. `tempdir()` returns the fixture tempdir — the child's working directory too, unless you passed an explicit `cwd()`, which wins — so a command's effect on disk is assertable.

The builder settings that describe a *process* carry over — `env()` / `env_remove()`, `cwd()`, `fixture()` (whose tempdir becomes the child's working directory), and `output_mode()`, which is the same argv edit `run()` makes. The settings that describe an *in-process injection seam* cannot: a child resolves width, color, stdin, clipboard, and prompts from its own environment, so declaring one and then calling `run_process()` panics rather than quietly asking the CI machine's terminal instead. Express those through something the child can see — an environment variable, a fixture file, argv.

It costs a compile and a fork per call. Use it for evidence, not for coverage.

### Intermezzo B: A full-pipeline test, in-process

**What you achieved:** Your integration tests run in the same process, in microseconds, with complete environment control.

**What's now possible:**

- Assert on both the rendered output *and* the handler's return data in the same test (via `result.outcome()`).
- Test env-dependent branches without touching `std::env` from your test code directly.
- Pin terminal width and color for snapshot tests.
- Replace a subprocess-based integration suite with a harness-based one; watch the run time drop by an order of magnitude.

**What's next:** A worked example, and the boundaries — what the harness still can't do.

## 5. A worked example

Let's test a todo CLI end-to-end. The app reads todos from `$TODO_FILE` (or `todos.txt` in the cwd), supports adding via argument or piped stdin, and renders either as a styled list or as JSON.

```rust
use clap::Command;
use serial_test::serial;
use standout_test::TestHarness;
use standout_render::OutputMode;

fn app() -> standout::cli::App {
    // your real App::builder() -> build()
    todo!()
}

fn command() -> Command {
    // your real clap Command definition
    todo!()
}

#[test]
#[serial]
fn list_shows_todos_from_cwd_file() {
    let result = TestHarness::new()
        .fixture("todos.txt", "buy milk\nwrite tests\n")
        .run(&app(), command(), ["todo", "list"]);

    result.assert_success();
    result.assert_stdout_contains("buy milk");
    result.assert_stdout_contains("write tests");
}

#[test]
#[serial]
fn list_prefers_env_var_over_cwd_file() {
    let result = TestHarness::new()
        .fixture("todos.txt", "from-cwd\n")
        .fixture("other.txt", "from-env\n")
        .env("TODO_FILE", "other.txt")
        .run(&app(), command(), ["todo", "list"]);

    result.assert_stdout_contains("from-env");
    assert!(!result.stdout().contains("from-cwd"));
}

#[test]
#[serial]
fn add_reads_from_piped_stdin_when_no_arg() {
    // Capture the fixture tempdir path *before* .run() consumes the
    // builder, so we can read files back after the handler has written
    // to them. The tempdir itself lives inside the returned TestResult
    // and stays alive until that result drops at end of scope.
    let harness = TestHarness::new()
        .fixture("todos.txt", "")
        .piped_stdin("buy milk");
    let todos_path = harness.tempdir().unwrap().join("todos.txt");

    let result = harness.run(&app(), command(), ["todo", "add"]);
    result.assert_success();

    let contents = std::fs::read_to_string(todos_path).unwrap();
    assert!(contents.contains("buy milk"));
}

#[test]
#[serial]
fn list_as_json_is_valid_and_shaped() {
    let result = TestHarness::new()
        .fixture("todos.txt", "a\nb\nc\n")
        .output_mode(OutputMode::Json)
        .run(&app(), command(), ["todo", "list"]);

    let v: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    let items = v["todos"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["title"], "a");
}

#[test]
#[serial]
fn list_without_color_strips_ansi() {
    let result = TestHarness::new()
        .fixture("todos.txt", "one\n")
        .no_color()
        .run(&app(), command(), ["todo", "list"]);

    assert!(
        !result.stdout().contains('\x1b'),
        "expected no ANSI escapes in output, got: {:?}",
        result.stdout()
    );
}
```

Every test reads like a statement of behavior. Nothing runs in a subprocess. Nothing depends on the developer's real home directory or clipboard. Every test restores the environment on drop.

### Intermezzo C: Integration tests that don't suck

**What you achieved:** A full integration test suite that runs in under a second, covers env-dependent branches, and breaks only when the behavior actually changes — not when someone tweaks a template.

**What you traded:** Your tests are `#[serial]` (they mutate process globals). For a CLI binary that isn't a library dependency of a massive workspace, this is almost never a problem — CLI test suites are small enough that serial execution is fine.

## 6. What the harness still can't do

Be honest about the boundaries. There are things you shouldn't try to test in-process:

**Real PTY behavior.** If your CLI drives progress bars, raw-mode TUIs, or prompts that sniff `isatty()` on a PTY (not just on the `StdinReader` abstraction), the harness can't simulate that. Use `rexpect` or `expectrl` with a spawned subprocess.

**Signals.** SIGINT / SIGTERM handling only makes sense against a real process.

**Subprocess fan-out from your app.** If your handler shells out to `git`, `rg`, `$EDITOR`, or any other external program, the harness can't intercept that call. Structure handlers so the shell-out is a trait you can swap for a mock in the handler's tests directly.

**Binary-level concerns.** Linkage, the real exit code, which stream a byte actually went to, behavior that keys off stdout not being a terminal — that's integration-of-the-build. [`run_process()`](#410-running-the-real-binary) covers it from the same builder; reach for `assert_cmd` only if you want its matcher vocabulary.

The goal isn't to replace subprocess tests entirely. It's to reduce them to the small set of cases where they're actually earning their keep.

## 7. Cheat sheet

```rust
TestHarness::new()
    // environment variables (real OS env, restored on drop)
    .env("KEY", "value")
    .env_remove("KEY")

    // working directory and fixture files
    .cwd("/some/path")                       // explicit cwd
    .fixture("notes/todo.txt", "content")    // writes file, sets cwd to tempdir
    .fixture_bytes("data.bin", vec![1,2,3])

    // destination facts on TargetProperties (fixed defaults when unset:
    // width None, ColorMode::Dark, IconMode::Classic, AmbiguousWidth::Narrow)
    .terminal_width(80)
    .no_terminal_width()
    .with_color()                             // or .no_color(); fills per-stream capability

    // forced output mode (injects --output=<mode> into argv)
    .output_mode(OutputMode::Json)
    .text_output()                            // shortcut for OutputMode::Text
    .output_flag_name("format")               // if AppBuilder::output_flag was renamed

    // stdin (routed through standout-input's default reader)
    .piped_stdin("content")
    .interactive_stdin()

    // clipboard (same)
    .clipboard("content")

    // interactive prompts (routed through standout-input's PromptResponder)
    .prompts(Arc::new(ScriptedResponder::new([
        PromptResponse::text("answer"),
        PromptResponse::Bool(true),
        PromptResponse::Choice(2),     // -> options[2]
    ])))

    // execute in-process...
    .run(&app, cmd, ["binname", "subcommand", "--flag"])

    // ...or as the real binary (ProcessResult; rejects in-process
    // destination facts, stdin, clipboard, and prompt settings a child can't inherit)
    .run_process(env!("CARGO_BIN_EXE_binname"), ["subcommand", "--flag"])

// TestResult: choose the assertion group that matches the observed outcome.

// Success
result.assert_success();                // Handled / Silent / Binary
result.assert_exit_status(ExitStatus::SUCCESS);
result.assert_stdout_contains("hi");
result.assert_stdout_eq("hi\n");

// No match
result.assert_no_match();               // clap didn't match any subcommand
assert_eq!(result.exit_status(), None);  // fallback owns the eventual status

// Error
result.assert_error();
result.assert_exit_status(ExitStatus::FAILURE);
result.assert_error_kind(RunErrorKind::Handler);

// Accessors for any outcome
result.stdout();                        // &str
result.outcome();                       // &DispatchResult, for bespoke assertions
result.binary();                        // Option<(&[u8], &str)> for Binary
result.exit_status();                   // Option<ExitStatus>; None for NoMatch
result.success_kind();                  // command / Clap help / Clap version
result.error_kind();                    // typed failure origin
result.tag_resolutions();               // what each style-tag pass resolved
result.unresolved_tag_names();          // the tags the theme did not define
```

The harness captures the pipeline before the real stdout/stderr write. Use it
for typed parser, handler, hook, render, pipe, and output-file outcomes. Keep a
small `run_process()` suite for OS exit codes, stream routing, and broken final
writers.

## Appendix: common pitfalls

- **Tests leak state into each other.** Every in-process `run` test must be `#[serial]` while the harness still mutates env/cwd. `serial_test` only orders annotated tests against each other, so an unannotated `run` can race with one that mutates those globals. Parallel execution mixed with process-global mutations is unsupported.
- **A `TestHarness::new()` without `.run(...)` does nothing.** The harness is `#[must_use]` — inert until you call `.run`.
- **`output_mode(...)` injects `--output=<mode>` into argv.** If your app uses a different flag name (via `AppBuilder::output_flag(Some("format"))`), set `.output_flag_name("format")`.
- **Unset destination facts are fixed defaults, not detected.** `$COLUMNS`, `$NERD_FONT`, and the OS appearance setting cannot change an in-process run. Inject `terminal_width` / `color_scheme` / `icon_mode` when a test needs non-default facts.
- **Handlers that bypass `standout-input`.** If a handler reads stdin directly via `std::io::stdin()` instead of `StdinSource::new()` or `read_if_piped()`, the harness's `.piped_stdin()` won't reach it. Prefer the abstractions.
