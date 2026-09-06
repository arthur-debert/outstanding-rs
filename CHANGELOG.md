<!-- generated - do not edit; fragments live in CHANGELOG/ (`shipit changelog render` regenerates this file) -->

# Changelog

## Unreleased

- **Breaking:** `InputCollector::bind_sources` has no default implementation. Every collector states what it does with the run's `InputSources`: one that reads stdin, the clipboard or a prompt rebuilds itself over them, and one that reads an argument, an environment variable, a config value or a default returns `None`. A collector written against 12.0.1 that omits the method no longer compiles, where it previously inherited a `None` default and silently kept reading the process's own streams. See [Input Backends](crates/standout-input/docs/topics/backends.md) (issue #550).
- **Breaking:** an unbound `StdinSource` fails instead of reading the process's own stdin. `StdinSource::new()` carries no reader until the chain binds one through `bind_sources`; a source that never reaches that call — used outside a chain, or held by a wrapping collector whose `bind_sources` returns `None` — collects `InputError::StdinNotBound`, a new variant whose message names `bind_sources`, and reports `is_available() == true` so the chain raises that error rather than skipping the source and moving on. `StdinSource::with_reader` and `with_shared_reader` are unchanged, and the chain still leaves a source built either way alone. Those two fallbacks together are what let a wrapper around `StdinSource` compile, pass every test that did not pipe stdin, and block on the test binary's stdin under a `TestHarness` (closes #550).
- **Breaking:** `ProcessOutcome` gains a third field, `final_write_failure: Option<RunError>`, and is no longer `Copy`, `PartialEq` or `Eq`. `run_emitted` computed the failure that turned a successful run into exit `1` and dropped it, so a caller could not tell a failed final write from any other status-1 exit, nor report the `io::Error` beneath it — the case where a caller's trace is the only remaining witness, because stdout is the stream that could not be written. Read `outcome.final_write_failure` for the cause; its `kind()` says which write failed and its `source()` is the `std::io::Error` the write returned. Callers that constructed `ProcessOutcome` with a struct literal add the field, and callers that compared two outcomes or relied on copying one compare `handled` and `status` instead (closes #564).
- `AppBuilder::usage_exit_status(u8)` names the status a run exits with when clap rejects the command line. An application that names none exits `2`, as before. An application with a published contract that already spends `2` on a deliberate refusal can now keep `1` for a rejected command line without owning the process edge itself. The error is still `RunErrorKind::ClapUsage` and its diagnostic document is still kind `clap-usage`; only the status moves. `usage_exit_status(0)` fails `build()`, because a rejected command line must not report shell success (closes #545).
- `AppFailure::framed()` keeps the failure's application-chosen exit status and gives up the verbatim stderr bytes. A framed failure takes the ordinary diagnostic path: `Error: <message>` on stderr under the human representation, and a stdout diagnostic document with stderr silent under `json`, `yaml`, `csv` and `ndjson`. A plain `AppFailure` is unchanged and still writes its bytes to stderr verbatim under every representation, and `ExternalFailure` has no framed form, since relaying another operation's own bytes is what that name is for. Use it for a refusal that is otherwise an ordinary error but must exit with a status your specification pins (closes #546).
- **Breaking:** the truncation the framework performs on a rendered value — `{{ value | col(10) }}`, `{{ value | truncate_at(10) }}`, a table column narrower than its cell, and `truncate_to_width` — now emits the reset at a cut that falls between a value's ANSI opener and its closer. All four previously kept the opener and dropped the closer, so the color ran on into every line printed afterwards — furthest when the surrounding style tag was unknown and stripped, because then nothing downstream reset either. A snapshot that pinned the truncated bytes of a pre-styled value gains a `\u{1b}[0m` before the ellipsis. Untruncated text is untouched: a value the framework passes through whole keeps the escape sequences the application wrote, unbalanced ones included. `standout-render`'s standalone `truncate_end` / `truncate_middle` helpers cut through a different path and still leave the opener unclosed; #570 tracks that (closes #566, for #568).
- The `config` command escapes its output line with the same ANSI-aware function the template filters use, rather than a private copy that turned every `[` into `\[` including the ones inside an escape sequence. Config keys and file paths carry no escape sequences, so what `config` prints today is unchanged; the copy that would have mangled the first value carrying one is gone. `escape_style_tags` is now exported from `standout-render`, where the surviving definition lives; `standout` does not re-export it, so it stays internal rather than contract (closes #565).
- **Breaking:** the text standout composes around application- and argv-supplied strings — a `RunError`'s stderr prose and the `Diagnostic` it produces, `ctx.warn` text, and the clap usage message standout captures — now carries control characters escaped to their Rust codepoint spelling (an `ESC` reads `\u{1b}`): everything `char::is_control()` matches except `\n` and `\t`, so an untrusted path or archive entry cannot paint the terminal through them. An error or warning message that carried its own ANSI now shows it escaped, and styled failure output belongs in `AppFailure` instead; `WarningBuffer::push` stores the escaped text, so a stderr or `TestResult::warnings()` snapshot that pinned a control character changes (for #552).
- Unchanged: the bytes `AppFailure` and `ExternalFailure` write to stderr stay verbatim, as does the diagnostic derived from them, and handler-rendered template output is untouched — an application that prints untrusted text through its own templates still escapes it itself (closes #552).
- `TestHarness::rendering(Representation, ColorPolicy)` names a test's rendering in one call where a suite was writing `.output_mode(...)` and `.color(...)` at every site, and takes the pair as two arguments so a test can parameterize over an array of them. Additive: `output_mode`, `color` and `text_output()` are unchanged, so existing tests need no edit (closes #553).
- **Breaking:** the help-page oracle modules `standout_test::clap_parity` (`clap_facts`, `assert_states_clap_facts`, `assert_page_states_clap_facts`, `Fact`, `Subject`, `Omission`, `DELIBERATE_OMISSIONS`) and `standout_test::invariants` (`assert_every_tag_resolved`, `assert_no_unresolved_tag_markers`, `assert_styling_preserves_layout`, `assert_no_possible_values_for_valueless_args`, `assert_metavar_for_valued_args`, `assert_descriptions_aligned` and their `_in_page(s)` forms) are gone; `TestResult::tag_resolutions()` and `unresolved_tag_names()` remain the way to check style-tag accounting (closes #539).
- **Breaking:** a `test-support` cargo feature on `standout` and `standout-render`, off by default and enabled by `standout-test`, now gates the harness-only seams: `standout_render::diagnostics::take_captured`, `standout::cli::warnings_delivered_on_stdout`, and the `STANDOUT_QUESTIONNAIRE_TERMINAL` scripted-terminal seam, which an adopter's debug build no longer reads. `CompletedRun::from_dispatch` and `with_entries` are crate-private, as are `diagnostics::is_capturing`, `record` and `drain`. `standout-test` no longer exports `assert_page_snapshot!`, `SnapshotCase`, `matrix`, `MatrixCell` or the `pty` module (closes #540).
- **Breaking:** `style_as` escapes the style-tag brackets in its value before wrapping it, so `{{ value | style_as('error') }}` on `missing [severity_map] table` renders those brackets literally inside the style; it previously built `[error]...[/error]` from the raw value, so the value opened a `[severity_map]` tag the caller never wrote — stripped with a warning, failing the run under `STANDOUT_STRICT_STYLE_TAGS=1`, or styled if the theme happened to define that name. The same rule now applies to a value that carries style tags on purpose: `{{ styled | col(10) | style_as('row') }}` renders the inner `[bold]...[/bold]` as literal text. Write the outer style as tag syntax around the value to keep the inner tags live. ANSI escape sequences already in the value are the exception — they pass through whole, so a pre-styled value keeps its color and still measures and truncates at its visible width (for #551).
- A `verbatim` filter escapes the same brackets on demand, so a command that prints a generated file, a JSON Schema, a regex or a TOML snippet through a template writes `{{ body | verbatim }}` and gets rendered bytes equal to what it handed the filter: a `[severity_map]` table header is text rather than a tag, and no longer fails the run under `STANDOUT_STRICT_STYLE_TAGS=1` or degrades to unstyled text plus a warning (closes #551).
- `verify_command` no longer reports a handler that reads an argument an ancestor command declares `global(true)` as mismatched. It checks handler arguments against a copy of the clap `Command` with those globals copied down the subcommand tree, which is what clap does during `build()` and the reason the argument was invisible before. Verification does not call `build()`, so it neither gains clap's generated `help` subcommand and `--help`/`--version` arguments nor answers differently for a `Command` the caller built first. Reachability and the flag and command collisions read the `Command` as declared, the tree the run path gets (closes #547).
- **Breaking:** a questionnaire command now reports a `--yes` or `--answers` an ancestor declares `global(true)` as a reserved-name conflict, because that global lands on the questionnaire command alongside the flag standout injects there. Two arguments then claim the same long name and clap's debug assertions reject the parsed tree, so the registration was never runnable; verification names it instead of leaving it to the first invocation (closes #547).

## 12.0.1 - 2026-09-05

- The corpus ADRs, specs and cross-references are gone from the documentation; the blind-adopter pilot findings (#349–#361) stay cited by issue number.
- The corpus runner, the archetype roster, the gap suites and the committed run evidence moved verbatim to [arthur-debert/standout-corpus](https://github.com/arthur-debert/standout-corpus); the per-PR `corpus` workflow is gone from this repository.

## 12.0.0 - 2026-09-04

- **Breaking:** `--output` names a structured encoding only: `json`, `yaml`, `csv`, `ndjson`, plus the diagnostic `term-debug`. `term`, `text` and `auto` are gone from the flag and from `[term] output`; passing one is a clap usage error, exit `2`. With no `--output` the run renders the human template, which has no flag value. `OutputMode` is replaced by `Representation` (what the run produces) and `StyleMode` (whether human text carries ANSI), so no public type combines the two. See [Output Modes](docs/topics/output-modes.md) (for #511, #513).
- `--color auto|always|never` decides whether human text carries escape sequences, `auto` the default, on every command like `--output`, renamed or removed through `color_flag(...)` / `no_color_flag()`. Resolution: a typed `--color`, then the caller's policy, then `NO_COLOR` against a configured `always`, then `[term] color` (`MYAPP__TERM__COLOR` from the environment), then whether the destination is a terminal, where a named output file never is. A structured encoding never carries ANSI. `TermSettings` gains `color`. See [Output Modes](docs/topics/output-modes.md) (for #511, #516).
- Incremental commands: a handler emits typed events through a `Results<E>` parameter and returns the summary as a `Summary<S>` — `Render` or `Silent`, with an exit status, so a payload from a command that declares events does not compile; **breaking:** `Handler` gains a third associated type, `Outcome`, which a hand-written implementation must name (`Output<T>` for a batch command, `Summary<T>` for one that declares events); `#[handler]` recognizes the parameter and `EventsFnHandler` is the closure adapter. The human representation renders one line per event as it happens from the command's `<name>.event` template, then the summary; `ndjson` writes each event as its own record and the summary as the `result` record; `json` and `yaml` write the array of exactly those records when the command succeeds; `csv` writes the events as rows and does not encode the summary. A failure after events keeps the events and appends the diagnostic; a reader that leaves (`| head -1`) never interrupts the handler. The contract is [ADR-0041](docs/adr/0041-hand-the-handler-a-typed-results-sink.md); see [Incremental Commands](docs/topics/incremental-commands.md) (for #511, #512, #514, #515).
- **Breaking:** `ctx.stream()`, `EntryStream` and `is_live()` are deleted; a handler cannot branch on the representation. `CompletedRun::entries()` is replaced by `results()`; `TestResult` gains `result()` and `delivery()`, and the harness takes the color policy and each destination's terminal facts as separate settings, replacing `with_color`, `no_color` and `text_output` (for #511, #513, #514).
- A command marked `pageable()` (or `#[dispatch(pageable)]`) delivers its complete human output through the user's pager when stdout is a terminal, no output file is named and `--no-pager` was not typed; structured encodings, incremental output and pipes never page. The pager command comes from `<APP>_PAGER` (the name given to `AppBuilder::name`) then `PAGER`, runs through `sh -c` with `LESS=FRX` and `LV=-c` set when unset, and an unset or empty value means no pager: the help pager follows the same rule and loses its `less`/`more` fallback and the `--page` flag. A pager that cannot start falls back to stdout with the status unchanged; one that stops reading ends delivery without failing the run. `Delivery::Pager` reports the decision to tests without spawning. See [Output Modes](docs/topics/output-modes.md#paging) (for #511, #517).
- Nothing beyond results and the machine contract's diagnostics reaches stdout or stderr under any representation; the `tflike` progress gap suite is closed. A transient progress line derived from events is a later feature (for #511, #518).

## 11.0.0 - 2026-09-03

- Configuration files, environment overrides and a `config` command, through clapfig (epic #476). `App::builder().config(clapfig_builder)` hands standout a `clapfig::TypedBuilder<C>`; a handler reads the struct with `ctx.config::<C>()`. Resolution runs once per run, after clap parses and only on the path that dispatches an app handler, so `--help`, `--version`, a usage error and the `config` tree itself never load a config file and a broken file cannot take `--help` down with it. Standout builds no configuration machinery of its own: discovery, formats, merge order, strictness and persistence are clapfig's, and are tested there. See [Configuration Files](docs/topics/config-files.md).
- `standout::TermSettings` is the framework's `[term]` section, opted in with `term_settings(accessor)` on the builder; the app embeds it under any field of its own struct. In this release it holds `output`, the output mode used when `--output` is absent, which fills the same slot as `output_mode_fallback` and is outranked by an explicit `--output`. An app that read its own file into `output_mode_fallback` at build time should let `[term] output` carry the setting instead.
- `config_override_flag("set")` installs an opt-in, app-named, repeatable global `--set key=value` that lands on clapfig's override layer; values parse with clapfig's scalar rule, so `port=8080` is an integer.
- When `config(...)` is set, an injected top-level `config list|get|set|unset|gen|schema` tree is executed by clapfig and rendered by standout, so `--output json` applies and a listing is a typed object with flat dotted keys; `gen` and `schema` are artifacts, written to a file with `--file PATH`. The `config` name and the tree's root-level flags (`--scope`, `--file`, `--force`, `-o`) are reserved: an app that declares them fails at `build()` with a `SetupError`, or calls `no_config_command()`.
- A `clapfig::ClapfigError` at run time is a `RunError` of kind `Config`, exit `1`, in clapfig's own words; under a structured output mode it is the diagnostic document, with the file and line in its `range` when clapfig has a position.
- `ConfigSource<T>::new(Option<T>)` in `standout-input` puts a value the handler already read from its typed config into an `InputChain`, behind a flag source, reporting the new `InputSourceKind::Config`. A `match` over `InputSourceKind` that was exhaustive gains an arm.
- `TestHarness::cwd(path)` sets the working directory of a run; a relative path resolves inside the harness tempdir, so a test writes a config fixture there and resolution finds it.
- **Breaking:** the workspace toolchain is Rust 1.96.1 (`rust-toolchain.toml`), clapfig 0.26's minimum. `tdoo` adopts configuration end to end and the `new-project` wizard generates the same shape.

## 10.0.0 - 2026-09-02

- `--output json` (and `yaml`) now emit object keys in the order the handler's struct or `json!` literal declared them, where they were previously sorted alphabetically. Standout builds serde_json's `Value` with the `preserve_order` feature on, so a view declared `name, zone, machine_type, status` serializes in that order instead of `machine_type, name, status, zone`. The feature is process-wide: a handler field typed as a raw `HashMap` emits its keys in `HashMap` iteration order (non-deterministic across runs) rather than sorted — use a struct or an ordered map (e.g. `indexmap::IndexMap`) when key order must be stable. `--output csv` columns follow the same declared order (PAR02-WS04). See [Output Modes](docs/topics/output-modes.md) (issue #464).
- `AppBuilder::strict_style_tags(true)` escalates an unresolved style tag from a graceful degradation into a hard failure. Off by default, an unresolved tag renders as unstyled text plus a stderr warning, unchanged. On, a render that leaves any style tag unresolved fails the run with a non-zero exit and an error naming the offending tags, turning a typo'd or theme-missing tag into a deterministic tripwire for dev, CI, and tests. The `STANDOUT_STRICT_STYLE_TAGS` environment variable forces it on regardless of the builder setting. The gate runs post-render over the diagnostics already captured for the run, so the default graceful path is untouched. See the "Strict mode" section of [Styling System](crates/standout-render/docs/topics/styling-system.md) (issue #464).
- **Breaking:** under `--output json`, `yaml` or `csv` a failed run writes a diagnostic document to stdout and no error prose to stderr, where it wrote prose to stderr and left stdout empty. A script that parsed stderr for the error text now reads the document; the exit status is unchanged (`1` for a handler, hook, render or write failure, `2` for a usage error). The document is `standout::cli::Diagnostic`, serialized flat — `type` (always `"diagnostic"`), `schema_version`, `severity`, `kind`, `summary`, `detail`, and an optional `range` of `filename` and `start.line` / `start.column` — and `kind` is the `DiagnosticKind` wire vocabulary: `clap-usage`, `default-command`, `handler`, `hook-pre-dispatch`, `hook-post-dispatch`, `hook-post-output`, `render`, `final-write`, `external`, `app`. In `csv` the document is one row whose last three columns are `range_filename`, `range_line` and `range_column`. An `AppFailure` or `ExternalFailure` keeps its verbatim stderr bytes and adds the document, kind `app` or `external`, with the bytes as `detail`. Human modes are unchanged. The rule is [ADR-0037](docs/adr/0037-emit-the-failure-as-a-stdout-document-in-structured-modes.md), spec D1 and D2; see [Execution Outcomes](docs/topics/execution-outcomes.md#failures-under-a-structured-mode) (issue #430, for #475).
- A handler returns a `Diagnostic` as its error to fill `detail` and `range`: `Err(Diagnostic::error("config line 2 does not parse").detail("expected `resource <name> <state>`").range("main.tfl", 2, 1).into())`. In a human mode the same value is prose, `Error: main.tfl:2:1: config line 2 does not parse`, the detail on the next line. Any other error type reaches the document with its `Display` text as `summary` and an empty `detail`. See [Error Handling](docs/topics/error-handling.md#the-diagnostic-document) (for #475).
- `--help` and `-h` honour `--output` the way the `help` word does: the mode comes from a scan of the raw argv for the output flag, last occurrence wins, `--output text` and `--output=text` alike, nothing after `--` counts. A usage error clap reports before a parse completes takes its mode from the same scan, so `myapp --bogus --output json` is a diagnostic document, exit `2`. A value that is not a mode (`--output jsn`) stays a prose usage error, exit `2`, since there is nothing to serialize the diagnostic in (issue #295, for #475).
- `standout::cli::emit_run_result(result, output_mode, stdout, stderr)` is public: it is the one function that writes a run's two streams, `run_emitted` calls it, and `standout-test`'s harness writes through it too, so a test observes what a process writes. Its former `render_stdout` / `render_stderr` copies in `standout-test` are deleted. `TestResult` gains `diagnostic()`, `expect_diagnostic()`, `output_mode()` and `stdout_bytes()`; `ProcessResult` gains `diagnostic(mode)` (for #475).
- `--output ndjson` is a new structured mode, and the one stream mode: stdout is one compact JSON object per line rather than a single document. `Output::Render(data)` becomes the line `{"type":"result","data":<data>}`; a failure is the diagnostic document as one line at the point in the stream where the run failed, and stderr carries nothing the framework wrote for it (an `AppFailure` or `ExternalFailure` keeps its verbatim stderr bytes, as in the other structured modes); each framework warning is a `severity: warning` diagnostic entry of kind `framework` after the result or the failure, where the single-document modes keep warnings as stderr prose. Exit statuses are unchanged. `--output-file-path` under `ndjson` receives the whole stream and stdout nothing. The rule is [ADR-0038](docs/adr/0038-stream-entries-under-ndjson.md), spec D2 and D3; see [Output Modes](docs/topics/output-modes.md#ndjson-mode) (for #475).
- `ctx.stream()` returns the run's `EntryStream` (`standout::cli::EntryStream`): `emit(&value)` writes the value as one line and flushes under `ndjson`, and neither serializes nor writes in every other mode, so a handler emits its entries unconditionally. `is_live()` is true only under `ndjson`; a handler whose entries are its whole result returns `Output::Silent` when it is and `Output::Render` otherwise. A value that does not serialize, or a line that cannot be written, is a `StreamError` the handler propagates with `?`. `App::run_with_sink` is `run_with` with the stream destination injected; `run_with`, `dispatch` and the harness capture the lines, which `CompletedRun::entries()` returns (for #475).
- **Breaking:** `Output::Binary` and `Output::Artifact` are render errors under `ndjson`, exit `1`, decided before anything is written: a stream of JSON lines has no room for a payload. The other structured modes are untouched (for #475).
- `DiagnosticKind::Framework` (`framework` on the wire) is the kind of a warning entry; no run failure produces it. `standout::cli::emit_warning_entries` writes the entries, and `standout::cli::parse_diagnostic` under `ndjson` reads the first error-severity entry out of the whole stream, which is what `TestResult::diagnostic()` returns (for #475).
- The `tflike` gap-suite fixture is built in this repository (`corpus/gap-suites/src/bin/tflike.rs`); the workspace's `.cargo/config.toml` points `CORPUS_TFLIKE_BIN` at it, so `corpus/gap-suites/tests/tflike_diagnostic.rs` runs its seven assertions as plain requirements under `pixi run test`, and `gaps.toml` reads the `tflike/diagnostic` gate as `closed` (for #475).
- **Breaking:** `ListViewResult` serializes with a `schema_version` key first (`{"schema_version":1,"items":[…]}`), so a script that parses `--output json` from a list-view command sees one new top-level key, and a snapshot of that document changes. Every framework-owned document now carries its version the same way: the diagnostic's `schema_version` comes from the `ContractSurface` trait instead of a plain constant, and the new help document carries it too. The rule is [ADR-0039](docs/adr/0039-version-the-document-and-mark-the-surface-with-a-trait.md), spec D4 (for #475).
- `ContractSurface` and `Envelope`: a view marked `#[derive(ContractSurface)]` with `#[contract(schema_version = N)]` returns `Output::Render(view.envelope())` and its `--output json` document becomes `{"schema_version":N,"data":<view>}`. In the human modes the template receives the same shape, so it reads the view's fields under `data`. Both items are available from the `standout` crate root and `standout::cli`. See [What Is Contract](docs/topics/stability.md) (for #475).
- **Breaking:** `--help`, `-h` and the `help` word under `--output json` or `--output yaml` answer with a versioned help document (`schema_version`, `name`, `path`, `usage`, `about`, `args`, `subcommands`) instead of the themed page; `render_help` and `render_help_with_topics` do the same for a `HelpConfig` whose `output_mode` is one of the two. `--help --output csv` is a render error, emitted as a diagnostic of kind `render`; `--help --output ndjson` renders the page. The document is `standout::cli::HelpDocument` (with `HelpArg` and `HelpSubcommand`), readable back through serde. ADR-0029's hold on structured help is retired by [ADR-0039](docs/adr/0039-version-the-document-and-mark-the-surface-with-a-trait.md), spec D9. See [Themed Help](docs/topics/standout-help.md) (for #475).
- A nested subcommand's help usage line names the path the reader types — `app nest inner leaf [OPTIONS]` rather than `leaf [OPTIONS]` — on the human page and in the help document alike, because the help model is now built from the root tree after clap's `build()` (issue #453).
- `TestResult::assert_schema_snapshot("list.json")` in `standout-test` compares the key names and JSON value types of a run's stdout document against `tests/schemas/list.json` in the crate under test and ignores values, so a renamed field fails while a changed value does not. A missing file is recorded and the assertion fails; `STANDOUT_UPDATE_SNAPSHOTS=1` accepts a changed schema. See [What Is Contract](docs/topics/stability.md) (for #475).
- **Breaking:** the `xml` output mode is deleted (spec D7, [ADR-0040](docs/adr/0040-declare-success-statuses-delete-xml-and-take-flat-csv-records.md)). `OutputMode::Xml`, `serialize_to_xml` and the `quick-xml` dependency are gone; `--output xml` is now a clap usage error, exit `2`, like any other value the flag does not accept. No client project used the mode, and its serializer dropped rows whose keys collided after element-name sanitization (issues #107, #408, #409).
- **Breaking:** `--output csv` takes flat records only (spec D8, issue #108). A flat record is a map whose values are scalars; the handler data must be one flat record or an array of them, each a row, columns in declared order. A nested value — an array or object inside a record, or a document that is not a record — is a render error whose message names the value and points at `CsvProjection`, instead of the silent `items.0.name` flattening and JSON-blob cells CSV produced before. `standout_render::flatten_json_for_csv` is replaced by `csv_records` and `write_csv`. A `CsvProjection` row source may now be `.` (the document itself), and a record at the row source is one row, which is how the framework's diagnostic becomes its CSV row.
- A successful handler can declare the exit status its run ends with (spec D5): `Output::Render(data).with_exit_status(ExitStatus::from(2))` emits the document as any success does and exits `2`; `list_view(items).empty_exit_status(3).output()` does the same for an empty list. The framework names no new code — the meaning of a declared status is the application's — and a declared status is never an error: nothing becomes a diagnostic. `ExitStatus` implements `From<u8>`, `RunOutput::exit_status()` reports the status, and `Output` gains the `WithStatus` variant with `with_exit_status`, `split_exit_status`, `exit_status` and `map_render`. See [Execution Outcomes](docs/topics/execution-outcomes.md).
- **Breaking:** hook failures are framed once. A hook diagnostic was `Hook error: hook error (pre-dispatch): …`, doubling the prefix because `HookError`'s own `Display` already names the phase; it is now `Error: hook error (pre-dispatch): …`, the same handler diagnostic framing an ordinary handler error gets. A test matching the old prefix asserts the new one instead. The framing is stated in [Error Handling](docs/topics/error-handling.md) (issue #353).
- **Breaking:** pre-dispatch and post-output hooks receive the deepest subcommand's `ArgMatches`, the same the handler gets, instead of the root's. A hook that compensated by walking down itself needs no change (the walk is idempotent); a hook that read a root-level argument by id now reads the subcommand's. Hook order relative to `.questionnaire::<T>()` is registration order, and [Derived Questionnaires](docs/guides/derived-questionnaires.md) states the rule (issue #352).
- `AppBuilder::output_mode_fallback(mode)` sets the output mode used when `--output` is absent from the command line, which was hard-coded to `Auto`. An explicit `--output` still outranks it, and every no-flag path uses the fallback: both help spellings, the flag's advertised default, and pre-parse error diagnostics. See [App Configuration](docs/topics/app-configuration.md) (issue #356).
- `AppFailure` is the handler-returnable domain error carrying any nonzero exit status and a verbatim stderr payload, with `HookError::pre_dispatch_app` for a pre-dispatch guard and `RunErrorKind::App` for capture callers. Construction rejects status `0`. `ExternalFailure` keeps its meaning — another operation owns the status and diagnostic. The line between this seam and the parity program's machine-readable error envelope is [ADR-0035](docs/adr/0035-name-the-app-owned-status-and-diagnostic.md) (issue #357).
- Let the application own the answer-sheet format behind `--answers` (issue #351). `AnswerSheetFormat` is the seam: `CommandConfig::answer_sheet_format(format)` reads submitted sheets through it, and the framework's preamble/fingerprint sheet is the default value `StandoutAnswerSheet` rather than a hard-coded call. A format that keeps the rendered sheet's tags calls `Questionnaire::parse_answer_sheet_body(text)`, which reads the tagged body without the preamble; a format that shares nothing with it fills a `RawAnswers` through `set`, `set_occurrence_count` and `push_warning` and returns its own diagnostics. Parsing is all a format owns — decoding, defaults, validators, whole-form rules, review and confirmation are unchanged. Source-incompatible: `Questionnaire::read_answer_sheet_file` takes the format as a second parameter, and `read_answer_sheet_stdin_with(reader)` is now `read_answer_sheet_stdin(reader, format)`; pass `&StandoutAnswerSheet` to keep today's behavior. See [Derived Questionnaires](../docs/guides/derived-questionnaires.md).

- Make the questionnaire confirmation gate the application's decision (issue #354). `CommandConfig::confirmation(Confirmation)` sets the acceptance rule — `ConfirmationAcceptance::Word(w)` (the default, with `yes`), `YesOrY` for `y` or `yes` in any case, or `Disabled` to run without asking as `--yes` does — the prompt wording, and the stream the review dump is written to. **The review dump now goes to stderr by default** rather than stdout, so a run's stdout stays the data channel; `Confirmation::review_stream(ReviewStream::Stdout)` restores the old destination. The prompt itself still goes to the controlling terminal. The ids of the injected arguments are the public constants `standout::cli::QUESTIONNAIRE_ANSWERS_ARG` and `QUESTIONNAIRE_YES_ARG`, so a hook reading `ArgMatches` no longer has to scan `ids()` for a suffix.
- Size a whole table from a template (issue #359). `tabular()` and `table()` take a `rows=` argument — the row arrays you are about to render — and resolve every `{"min": n}` / `{"min": n, "max": m}` column against the widest cell those rows hold, which is the measurement a formatter that receives one row at a time cannot make for itself. `table()` measures its `header=` row alongside the data, so a header wider than its column is no longer truncated. Without `rows=` nothing changes: a bounded column still lands on its `min`, and leftover width still goes to the rightmost bounded column when the table has no `"fill"`. A column carrying `sub_columns` is not measured from `rows=` — its sub-columns are resolved per row against the parent's width. A row (or `header=`) shorter than the column list measures the missing columns at their `null_repr`, which is what the formatter renders there; `rows=` that is not an array of arrays is an error rather than a row measured from a debug rendering of it. See [Introduction to Tabular](docs/crates/render/guides/intro-to-tabular.md), Step 4.
- List `-h`/`--help` and `-V`/`--version` on themed help pages (issue #334). The help extractor reads the `clap::Command` *after* `build()`, which is when clap creates its own arguments; before, it read the declared command and no page could tell a reader those flags existed. Every themed help page gains a `-h, --help` row, and an application that sets a version gains `-V, --version`, both after the application's own options — a rendered-byte change for any downstream pinning a help page. Two consequences of reading the built command are handled in the same pass: a valueless flag (`SetTrue`, `Count`) no longer states the `false`/`0` default clap materialises and hides from its own page, and clap's generated `help` subcommand is identified the way clap identifies it internally — the word `help` on a command that has not called `disable_help_subcommand`, a pairing clap rejects for an application's own `help` — so an application's own `help` subcommand keeps its COMMANDS row, and a caller who already called `build()` gets the same page. In `standout_test::clap_parity`, the `DELIBERATE_OMISSIONS` entry exempting clap-generated subjects is gone; `Omission::ClapGeneratedSubcommands` replaces it, covering the generated help word alone and no argument.

## 9.0.0 - 2026-08-31

The robustness program's one deliberate compatibility break, shipped as a single major
version. Standout now offers one blessed way to do each thing — register a command, provide
a template, provide a theme, enter the framework, observe a run in a test — and the
near-equivalent paths beside each of them are deleted rather than deprecated. There is no
shim and no fallback: every removal below names the surviving item that replaces it, and
porting is a source edit.

Three of the changes are visible to people who never recompile: `#[derive(Dispatch)]` now
registers multi-word commands under their kebab-case names, themed help is on by default,
and `--output=term` through a pipe emits ANSI. Those are the ones to read first.

`docs/topics/stability.md` states which surfaces are contract from this release on: the
blessed idioms, the structural shape of each `--output` mode's bytes, exit statuses, the two
command-line name mappings, availability of an item through the `standout` crate root, and
`standout-test`'s assertion API. Everything else is internal.

Surface census, 8.1.1 → 9.0.0: `AppBuilder` 35 → 30 public methods, `App` 27 → 14,
`CommandConfig` 21 → 20, `GroupBuilder` 12 → 8; root re-exports 101 → 83 items, `cli`
re-exports 46 → 42.

### Registering commands

- **Command names are kebab-case.** `ListUnits` now registers as `list-units`; scripts and
  tests typing the old spelling break. Keep an old name with
  `#[dispatch(name = "list_units")]`. Mapping rules: `docs/topics/dispatch-attributes.md`.
- **`#[handler]` reads clap ids with hyphens**: a parameter `no_legend` reads the id
  `no-legend`, where clap-derive ids an argument by the field name, `no_legend`. Match one
  side to the other — `#[arg(id = "no-legend")]` on the clap field, or
  `#[flag(name = "no_legend")]` on the parameter; renaming the parameter alone cannot reach
  an id with an underscore. `app.verify_command(&cmd)` reports the mismatch at build time:
  `docs/topics/dispatch-attributes.md`.
- **A registration no clap subcommand can reach now fails up front**, where `App::run`
  returned `false` silently. Register the canonical clap name, not an alias:
  `crates/standout-dispatch/docs/topics/partial-adoption.md`.
- **`#[dispatch(pure)]` requires `-> Result<Output<T>, E>` or `-> Result<(), E>`**; a plain
  `Result<T, E>` no longer compiles. Shapes per registration path:
  `crates/standout-dispatch/docs/topics/handler-contract.md`.
- **The `#[command]` attribute macro is gone.** Put `#[handler]` on the function and declare
  the `Command` with clap-derive. `#[dispatch(pure, simple)]`,
  `#[dispatch(pure, handler = …)]` and a `name` containing `.` are now rejected at
  expansion: keep one attribute, and spell a nested command with its registration path.
- **One `standout` dependency now carries the macros.** Delete a direct `standout-dispatch`
  or `standout-input` dependency added only to resolve `#[handler]` or `Questionnaire`.
- **New `#[dispatch(inputs = path)]`** names a `fn(CommandConfig) -> CommandConfig`, so a
  multi-source value no longer needs `command_with`.
- **Builder renames:** `AppBuilder::default()` → `App::builder()`; `group(name, f)` →
  `.commands(|g| g.group(name, f))`; `command`, `command_handler` and `command_handler_with`
  → `command_with` (now taking `impl Handler`) plus a registry entry for the template;
  `GroupBuilder::handler` and `handler_with` → `AppBuilder::command_with` with a dotted
  path. `GroupBuilder::len` and `is_empty` are removed with no replacement.

### Templates

- **Inline template source is gone** — `CommandConfig::template(source)`, the
  `template = "…"` key, and the recipe types' inline source. Register the template and name
  it with `template_name`: `crates/standout-render/docs/topics/templating.md`.
- **Also renamed or removed:** `dispatch!`'s `template:` key is now `template_name:`;
  `App::run_command`'s fourth parameter is a `TemplateRef`, not a `&str`; and
  `AppBuilder::template_ext(ext)` is gone — rename the file to `.jinja`, `.jinja2`, `.j2`,
  `.stpl` or `.txt`.

### Themes

- **`Theme::from_file`, `from_css_file`, `from_variants`, `source_path` and `refresh` are
  gone**, and `Theme::named(name)` becomes `Theme::new().with_name(name)`. Use
  `.styles(embed_styles!(…))` or `.styles_dir(path)`:
  `crates/standout-render/docs/topics/styling-system.md`.
- **`.styles(…)` together with `.theme(…)` is now a `SetupError`** naming both calls, where
  it used to discard the stylesheet registry silently. Drop one of the two.
- **A stylesheet registry with no `.default_theme(name)` resolves to no application theme.**
  Add `.default_theme("…")`; the `default`/`theme`/`base` name search is gone.
- **`App::get_default_theme` returns `&Theme`**, not `Option<&Theme>`. Delete the unwrap.

### Help

- **Themed help is on by default**, so an application that never called
  `.help_handling(true)` now serves standout's page and **a test pinning help output goes
  red**. Re-record it, or call `.help_handling(false)` to hand help back to clap unchanged.
  Configuration: `docs/topics/standout-help.md`.
- **Every existing `.help_handling(true)` call can be deleted.**
- **An application registering its own `help` command stops building** with a
  `DuplicateCommand` error — the flip's only build-breaking case. Rename the command, or
  call `.help_handling(false)` and keep the name; `command_groups`, topics and
  `.help_word(true)` are then unavailable.
- **A named help or topic template override that cannot be read is now a render error**
  rather than a silent fallback. Fix the name or drop the override.

### Entry points and parsing

Mechanical renames; the outcome model is `docs/topics/execution-outcomes.md`.

- **On `App`:** `run_to_string` and `dispatch_from` → `run_with(cmd, args, target, sources)`
  or `TestHarness::run`; `render`, `render_inline` and `render_inline_with` → `render_with`,
  whose first parameter is a `TemplateRef`; `parse`, `parse_with`, `parse_from` and
  `get_matches` → `get_matches_from`, which now takes `&InputSources` and carries what was
  `get_matches_from_with_sources`. The free `cli::parse` and `cli::parse_from` are removed
  with no replacement.
- **Result types:** `standout_dispatch::RunResult` → `DispatchResult`;
  `standout::cli::RunResult` → `standout::cli::CompletedRun`.

### Input

- **`set_default_stdin_reader` and the clipboard and responder override APIs are gone.**
  Build an `InputSources` and pass it to `App::run_with`, or use `TestHarness::prompts(…)`:
  `crates/standout-input/docs/topics/input-sources.md`. The warning thread-local goes with
  them — read warnings off the run result or the harness.
- **A failed file read is now `InputError::FileFailed`**, naming the path; a test matching
  the old parse-failure text against the argument id needs re-recording.
- **`CommandConfig::input` resolves a value or fails**, so an optional value with a second
  source has no blessed spelling. Keep a single-source optional value as a typed
  `#[handler]` parameter.
- **New `InputSourceKind::File`** lets `ctx.input_source("name")` tell a file apart from an
  argument.

### Output, color and rendering

- **`--output=term` through a pipe now emits ANSI**, matching the documented contract. Strip
  escapes downstream, or ask for `--output=text`: `docs/topics/output-modes.md`.
- **An explicit `--output` overrides the environment conventions:** `term` emits ANSI under
  `NO_COLOR=1` or `TERM=dumb`, `text` stays plain under `CLICOLOR_FORCE=1`. `auto` still
  follows the destination's color capability, which `NO_COLOR` and `TERM=dumb` suppress and
  `CLICOLOR_FORCE` does not create.
- **The detector override APIs are gone** — `set_terminal_width_detector`,
  `set_color_capability_detector`, `set_ambiguous_width_detector`, `set_theme_detector`,
  `set_icon_detector`, `set_tty_detector`, `detect_is_tty`, `DetectorGuard` and the whole
  public `detect_*` cluster. `TargetProperties::detect()` runs the same probes in one call:
  construct a `TargetProperties` and pass it to `App::run_with`, or inject one through
  `TestHarness`.
- **`Presentation` is deleted** (call `render_request`; `render` and `render_with_output`
  are unchanged), **`AppBuilder::output_mode()` is removed** (it always returned `Auto`),
  and **`MiniJinjaEngine` is `!Send`/`!Sync`** — build one engine per thread.
- **`standout` no longer depends on `serde_yaml`, `csv` or `quick-xml`.** Depend on
  `standout-render` for structured serialization.
- **Warning styling follows stderr's color capability**, `--output=text` opts the warning
  block out of ANSI, and standalone `render_auto` of a named struct emits XML under
  `<data>`. Re-record tests pinning any of these; CLI XML bytes are unchanged.

### Testing with `standout-test`

The assertion API is contract from this release on: `docs/topics/testing.md`.

- **`TestHarness::is_tty()` and `no_tty()` are removed**, with the in-process TTY seam
  behind them. Use `run_process()` or `run_pty()` when a real terminal must answer.
- **`TestHarness` injects destination facts rather than detecting them**, defaulting to
  `width: None`, `ColorMode::Dark`, `IconMode::Classic` and `AmbiguousWidth::Narrow`. A test
  relying on `$COLUMNS`, `$NERD_FONT` or the OS appearance must set the field instead.
- **`TestHarness::with_color()` now emits the escapes a terminal user sees.** Drop
  `force_styling` from the theme.
- Additions needing no edit: `TestResult::stdout()`/`stderr()` and their `_plain` forms,
  `tag_resolutions()`, `assert_page_snapshot!`, the `matrix`, `invariants` and `clap_parity`
  modules, and `run_process()`/`run_pty()`.

### Items that are no longer public

**Re-exports `standout` dropped** — still public in the defining crate, so add that crate as
a direct dependency to keep calling them.

- From `cli`, the `standout-dispatch` navigation helpers `extract_command_path`,
  `get_deepest_matches`, `has_subcommand` and `insert_default_command`.
- From the crate root, the `standout-render` items `build_embedded_registry`,
  `extension_priority`, `strip_extension`, `walk_dir`, `walk_template_dir`,
  `validate_template`, `render_auto_with_spec`, `rgb_to_ansi256`, `rgb_to_truecolor`,
  `flatten_json_for_csv`, `serialize_to_xml`, the `file_loader` module, and `FileRegistry`,
  `FileRegistryConfig`, `LoadError`, `LoadedEntry` and `LoadedFile`.

**Deleted outright** — no crate provides these, so a direct dependency does not bring them
back.

- `App::registry_mut`, `App::get_hooks` and `App::get_theme`.
- `standout-dispatch`'s unused render-callback API: `RenderFn`, `from_fn`, and the
  `RenderError` that belonged to it. (`standout_render::RenderError` is a different type and
  `standout` still re-exports it.)

### Failures that used to be silent

- **One hook phase registered for a path through both `CommandConfig` and
  `AppBuilder::hooks` is now a configuration error** instead of one set silently replacing
  the other. Register it in one place; different phases still combine.
- **Missing templates and themes fail with a message naming the remedy.** No edit needed
  unless a test pins the old text.

### Documentation

- New owners for what this file no longer restates: `docs/topics/stability.md`,
  `docs/topics/dispatch-attributes.md` (every `#[dispatch(…)]` key and both name mappings),
  and `crates/standout-render/docs/topics/templating.md` (the trailing-newline contract).
- `docs/SUMMARY.md` mounts every page, the guides pin `standout = "9"`, and two checks keep
  the book true: `standout-docs` compiles its Rust examples in a required `docs` CI lane and
  walks it as a graph in the test lane.

## 8.1.1 - 2026-08-16

- Overlay the application theme on the default help theme instead of replacing it (issue #303). An app that set its own theme (`.theme(..)` / `.default_theme(..)`) lost all help styling: the theme carries the app's output vocabulary, not help's, so every help tag went unresolved and a terminal reader saw literal `[about?]…[/about?]` markup across the whole page. Help and topic renders now start from `default_help_theme()` / `default_topic_theme()` and merge the configured theme over it — a tag the app names takes the app's style, and every tag it leaves alone keeps its default — so deliberate restyling keeps working and an app theme never has to duplicate the framework's help vocabulary.
- Restore clap-compatible value cues in themed help (issues #301 and #302): value-taking options now render their metavars, including short+long spellings, and presence-only bool flags no longer advertise `true, false` as accepted command-line values.

## 8.1.0 - 2026-08-16

- Let the builder declare the application's version: `App::builder().version(env!("CARGO_PKG_VERSION"))` (issue #304). Standout applies the value to the root clap command in the shared augmentation both parse paths go through, so `myapp --version` is answered identically by `run`, `run_to_string`, `dispatch_from`, `get_matches_from`, and `TestHarness` — clap's own display, on stdout, exit status 0, typed `SuccessKind::ClapVersion`. Clap still owns the spelling, the formatting, and the display short-circuit; leaving `.version()` unset leaves the supplied `clap::Command` exactly as configured, version included. The `tdoo` worked example adopts it, so `tdoo --version` now works.

## 8.0.2 - 2026-08-15

- Make themed-help section layout Unicode display-width-aware and size each section from its own contents, preserving long option, command, topic, and argument names without truncation or collisions with their descriptions (issue #297, PR #300). Grouped commands remain aligned across the page, and new semantic theme styles cover argument metavars, defaults, and possible values; command and topic rows no longer add a literal colon.
- Restore information that themed help previously dropped: `-h` renders `about`, while `--help` and the `help` word render `long_about` with an `about` fallback; option rows show defaults and non-hidden possible values; and positionals appear in a separate ARGUMENTS section before OPTIONS, with independently sized columns (issue #298, PR #300).
- Tailor standout's installed `help` word to flat CLIs: it says "Print this message" and no longer creates a self-referential COMMANDS section when it is the only command. Registering help topics keeps that section so `help <topic>` remains discoverable (issue #299, PR #300).

## 8.0.1 - 2026-08-15

- Refuse an application `help` that collides with standout's own, instead of panicking (issue #294). With `.help_handling(true)`, standout installs a `help` word wherever the install policy allows; an application that claimed the same name met clap's duplicate-subcommand debug assertion — `Command app: command name 'help' is duplicated` — a runtime panic on a configuration. Both spellings are now `SetupError::DuplicateCommand`, the error two commands claiming one name already had, each caught the moment it becomes visible: a registration whose first path segment is `help` (`.command("help", …)`, `.command("help.topic", …)`, a `.group("help", …)`) fails `build()`, while a `help` declared on the application's clap `Command` (by name or alias) is refused when that `Command` reaches `get_matches_from` / `dispatch_from`, before anything is parsed. The message names the registration or declaration that collided, the setting that installs the word, and the two ways out — rename the command, or drop `.help_handling(true)`. Not every spelling used to panic: a registration *nested* under the root `help` (`.command("help.topic", …)`, a `.group("help", …)`) built successfully and then ran unreachable, shadowed by the installed word, and now fails `build()` with the collision error — so an application that starts today can stop starting after this upgrade, with the message naming the registration to rename. The install policy is unchanged, and a CLI that never gets the word (help handling off, or flat-with-positionals without `.help_word(true)`) is unaffected, as is a `help` deeper in the tree (`db.help` is the application's own).

## 8.0.0 - 2026-08-15

- Give flat CLIs a working `help` word, and render help on every entry point (issue #292, PR #293, ADR-0018). A single-command app with required root arguments could not have one: clap validates the root before routing, so an injected `help` subcommand was advertised in help output and answered `error: the following required arguments were not provided`. Where standout installs the word it now also sets clap's `subcommand_negates_reqs`, which suspends the root's requirements once a command is named — so `myapp help` prints help while `myapp` alone still reports its missing arguments and `myapp <RANGE>` still parses as data. That setting is scoped to the install: it relaxes requirements for an application's own subcommands too, so a CLI that did not get the word never gets the semantics either. The word is installed for CLIs with subcommands and for flat CLIs with no positionals, and behind the new `.help_word(true)` for a flat CLI with positionals, where a bare word is data and only the application knows whether its domain excludes it (`--` is the escape).
- Answer help identically whichever entry point an application uses (issue #292, PR #293). `dispatch_from` / `run` / `run_to_string` previously had no help interception at all — no `help` word, and clap's `DisplayHelp` handed back as clap's own unthemed text — so an app entering through `run()` got none of standout's help. Both parse paths now share one interception and one rendering, and a `help --page` request comes back as the new `SuccessKind::PagedHelp`, which `run()` honours by paging while the capture APIs return the text and leave the decision to the caller. `--output` reaches the `help` word but not `--help` / `-h`, which short-circuit inside clap before anything is parsed; the asymmetry is documented.
- Report a help render that fails as the application bug it is (issue #292, PR #293). A broken template or theme — what a malformed runtime override stylesheet looks like — used to fall through each rendering step and surface as "the subcommand or topic '…' wasn't recognized", blaming a line that was fine; on the dispatch path it now carries `RunErrorKind::Render` and the failure exit status rather than the usage one.
- **Breaking:** `standout-dispatch` no longer privileges the name `help` in `extract_command_path`, `get_deepest_matches`, or `has_subcommand`. The word is special only where standout answers it before dispatch, and such a word never reaches those matches; an application's own `help` command now dispatches like any other.

## 7.10.2 - 2026-08-14

- Render booleans and none as `true`/`false`/`none` on every template surface, whatever `minijinja` version resolves, and uncap the dependency back to `2` (issue #247, PR #249). minijinja 2.22 switched to Jinja2's Python spellings, so 7.10.1 capped it below that; standout now normalizes for itself. A single sanctioned `Environment` constructor installs a formatter for interpolation and replaces the `string` and `join` filters, and a shared stringify helper covers the places a formatter cannot reach — the `nl` filter, the tabular filters, table cells and headers, and sequence and map literals. `register_filters` installs the same, so an environment you build yourself gets it too. The `~` concatenation operator formats inside minijinja and stays a documented, tested exception.
- Add questionnaire answer sheets (epic #258, PR #264): `standout-input` gains a `questionnaire` module that renders an application-defined questionnaire as a prose answer sheet, collects one submission from interactive prompts, a named answer file, or explicitly requested stdin, and decodes every source through one shared validation pipeline. Questionnaires are trees of scalar fields and nested or repeatable groups; identity rides stable line-terminal `<id:...>` tags with occurrence paths (`a.b[1].c`) for copy-the-block repetition, so wording, numbering, and indentation stay freely editable. A semantic fingerprint pins sheet compatibility — copy edits keep old sheets valid, semantic changes reliably invalidate them. Conditions (`active_when`), static and dynamic defaults, revisioned validators, and warning-level diagnostics apply identically however the answers arrived.
- Derive questionnaires from plain structs (epic #270, PR #288): `#[derive(Questionnaire)]` lowers a struct onto the runtime model — doc comments as prompts, nested structs as groups, `Vec<Struct>` as repeatable groups, enums as choice vocabularies via `#[derive(QuestionnaireChoices)]` with explicit `rename`, plus `active_when` conditions, defaults, dynamic defaults, and revisioned validators — and fills itself from validated answers without a serde boundary. Commands that declare a questionnaire on the CLI builder get an injected answer-sheet surface: `--answers <file>` (or `-` for stdin), a `questions` subcommand printing the blank sheet, and a confirmation gate with `--yes`. The `standout new-project` wizard is rewritten as the reference client, replacing its hand-rolled prompt loop. A post-epic review trim (issue #289, PR #290) then removed ~550 lines of zero-client surface — scalar `Vec` comma-lists, cross-struct `active_when` inheritance, kebab-case choice-name inference (explicit `rename` is now required), and over-granular diagnostic enums (collapsed with rendered messages unchanged).

## 7.10.1 - 2026-08-08

- Publish `standout-test` to crates.io (issue #244, PR #245). It was `publish = false` and stranded at 7.5.1, so every project generated by `standout new-project` failed dependency resolution — its generated dev-dependency pins the current version. A regression test now fails if the wizard emits a dependency on a workspace crate that is not published to crates.io.
- Track `Cargo.lock` and cap `minijinja` below 2.22 (issue #246, PR #248): the lockfile was gitignored while `minijinja` was declared as an open `2` range, so every clone, CI run, and `cargo install` re-resolved to the newest release. minijinja 2.22.0 changed boolean and none rendering to the Python spellings `True`/`False`/`None`, turning 14 tests red on an unchanged `main`. Standout keeps rendering `true`/`false`/`none`.

## 7.10.0 - 2026-07-30

- Add the `standout new-project` bootstrap wizard (epic #228), which interactively generates a runnable, production-shaped two-crate workspace with bounded string, boolean, and path inputs; ordered argument, file-content, and piped-stdin resolution where supported; MiniJinja and CSS presentation; and core, typed-handler, and `TestHarness` pipeline tests.

## 7.9.2 - 2026-07-19

- Resolve terminal width from a valid positive `$COLUMNS` value before probing the terminal, while preserving explicit detector overrides and the existing 80-column tabular fallback (issue #220, PR #221).
- Preserve semantic styles through visible-width measurement, truncation, and wrapping (issue #220, PR #222): style tags no longer get stripped as a layout shortcut, retained text keeps balanced nested tags, and property tests cover every truncation position across Unicode and generated style trees.

## 7.9.1 - 2026-07-19

- Cascade detected or test-injected terminal width through the render/template seam into `tabular()` and `table()` defaults (issue #215, PR #217), while explicit helper widths still take precedence and 80 columns remains the deterministic fallback only when width is unavailable; the framework list-view path and `TestHarness` cover the behavior.

## 7.9.0 - 2026-07-17

- Add invocation-aware default command resolution (issue #211, PR #213): `.default_command_with(resolver)` selects a naked invocation's command per invocation from a narrow `DefaultCommandContext` (root matches, read-only app state, and the existing non-consuming stdin terminal fact), so a CLI can pick a piped entry point when stdin is redirected — piped-empty included — and an interactive one at a terminal. Resolution is centralized, so `dispatch_from` / `run_to_string` and `get_matches_from` / `parse_from` agree; it runs only after a successful naked parse, leaving explicit and nested commands, help, version, and usage errors unchanged. Static `.default_command(...)` users are unaffected and can combine both, with the resolver declining via `None`.
- Add compound artifacts with post-write semantic reports (issue #212, PR #214): `Output::Artifact` carries owned bytes, an opt-in suggested destination, and an application-owned report, while Standout owns the shell transaction — it selects the destination (override, suggestion, or opted-in stdout), performs the final write, and only then renders the report enriched with a receipt naming the completed destination. Write failures are typed as `FinalWrite(Artifact)` and emit no success report; `Output::Binary` behavior is unchanged and a suggested filename still authorizes no write. `TestHarness` gains artifact byte, destination, receipt, report, and typed-failure assertions, and the `todo-core`/`tdoo` example demonstrates the ownership boundary with a CSV export.

## 7.8.0 - 2026-07-17

- Add an explicit narrow/wide East Asian Ambiguous character-width policy (issue #207, PR #210): App and direct Renderer configuration, tabular layout, MiniJinja filters, and TestHarness now share one authoritative measurement seam while narrow remains the compatibility default.
- Preserve authoritative external-command failures through Standout-owned output (issue #206, PR #208): applications can declare an exact nonzero status and verbatim stderr payload from handlers or pre-dispatch, while capture APIs and `TestHarness` expose the typed external origin without changing ordinary 0/1/2 semantics.

## 7.7.0 - 2026-07-17

- Add per-command structured-output projections (issue #200, PR #204): commands can declaratively select CSV rows, derive row- and root-level cells, and emit synthetic or conditional rows while canonical JSON, YAML, and XML serialization remains unchanged.
- Carry typed exit status and error origin through framework-owned final writes (issue #201, PR #205), preserving correct 0/1/2 exit codes and stdout/stderr routing and exposing typed `TestHarness` assertions.

## 7.6.5 - 2026-07-17

- Add a compact implementation-quality guide and skill checklist for leveraging Standout's logic/presentation boundary, output modes, templates and CSS, tabular layout, declarative dispatch, and layered test seams.

## 7.6.4 - 2026-07-17

- Fix the standout-docs skill: relocate to standout-docs/SKILL.md and add required frontmatter so it's discoverable
Migrated changelog to fragment-directory system from arthur-debert/release.
- ci: migrate release reusable-workflow callers from @v2 to @v3
- expand RenderError tests to cover all Display branches, From impls, and source() chaining
- Remove dead readthedocs/mkdocs config (.readthedocs.yaml, docs/requirements.txt); move palette_compare.sh to app-bin/

## [7.6.3] - 2026-05-23

### Changed

- First release through the canonical `arthur-debert/release/.github/workflows/rust-lib.yml@v1` reusable workflow (replacing the hand-rolled `publish.yml` tag-push trigger). No source changes; this dispatch validates the canonical rust-lib pipeline end-to-end for the standout workspace's 8 published crates.

## [7.6.2] - 2026-04-30

### Added

- **`RunResult::Error(String)` variant + `is_error()`/`error()` accessors** in `standout-dispatch`. Handler errors, hook errors, and output-write errors that previously surfaced as `RunResult::Handled(...)` (a *success* variant) now surface as `RunResult::Error(...)`. Closes [#141](https://github.com/arthur-debert/standout/issues/141).
- **`assert_error()` / `assert_error_contains()` / `is_error()` / `error()` on `standout-test::TestResult`** so test code can assert on error outcomes without relying on stdout pattern-matching.
- **Backslash escape syntax in `standout-bbparser`.** Text now supports `\[` → `[` and `\]` → `]` so user-provided strings (e.g. clap `about` / `help` text rendered through standout's help interception) can contain literal brackets without being mistaken for tag delimiters. A backslash that is not followed by `[` or `]` is left alone, so file paths (`C:\foo\bar`), regex examples (`\d+`), and other content containing `\` pass through unchanged. To emit a literal `\[` write `\\[` (the first `\` is preserved because `\\` is not a recognized escape, then `\[` is consumed and emits `[`). Escapes are honored in all transform modes (`Apply`, `Remove`, `Keep`) and by `strip_tags`; they don't generate validation errors.

### Fixed

- **CLI `run()` now sets a non-zero exit code when a handler, hook, output-write step, *or* clap argument-parse fails.** Previously, all of these were routed through `RunResult::Handled(...)` (the *success* variant), printed to stdout, and the process exited 0 — so `tdoo cmd && other-cmd` and `if ! tdoo cmd; then …` saw success even on `tdoo --bogus-flag`. Errors now go to stderr and `run()` calls `std::process::exit(1)`. Clap's `--help` / `--version` output is unchanged (still stdout, exit 0). Callers needing fine-grained exit-code control should use `run_to_string`/`dispatch_from` and match `RunResult` themselves. Closes [#141](https://github.com/arthur-debert/standout/issues/141).
- **`standout_render::tabular::visible_width` no longer mismeasures strings that contain both BBCode tags and raw ANSI escapes.** Previously the function ran `strip_tags` first, and the `[` / `]` bytes inside CSI sequences (`\x1b[31m...`) confused the tag stripper into treating the surrounding region as one malformed tag, leaving the inner BBCode intact. ANSI codes are now stripped before BBCode, matching the function's documented contract.

### Changed

- **`RunResult` is now `#[non_exhaustive]`.** Consumers matching exhaustively must add a wildcard `_` arm. This pairs with the new `Error` variant and lets future error-handling work add variants without breaking the API. Note: adding `Error` is itself a hard break for exhaustive matchers because the enum was not previously `#[non_exhaustive]`. Migration is a single `RunResult::Error(msg) => { eprintln!("{}", msg); std::process::exit(1); }` arm.

## [7.5.1] - 2026-04-25

### Added

- **`standout-input` is now wired into the `App` builder.** Commands can attach declarative input chains alongside templates, hooks, and piping with the new `CommandConfig::input(name, chain)` method. Chains run in pre-dispatch — before the handler — and the resolved values land in a name-keyed `Inputs` bag on `ctx.extensions`. Handlers retrieve them via the new `CommandContextInput` extension trait:

  ```rust
  use standout::cli::{App, CommandContextInput, Output};
  use standout::input::{ArgSource, EditorSource, InputChain, StdinSource};

  App::builder()
      .command_with("create", create, |cfg| {
          cfg.template("create.jinja")
              .input("body", InputChain::<String>::new()
                  .try_source(ArgSource::new("body"))
                  .try_source(StdinSource::new())
                  .try_source(EditorSource::new()))
      })?
      .build()?;

  fn create(_m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Value> {
      let body: &String = ctx.input("body")?;
      /* ... */
  }
  ```

  Multiple `.input(...)` calls accumulate; a command can declare any number of named inputs of any types (including multiple inputs of the same type, which the `TypeId`-keyed `ctx.extensions` cannot disambiguate on its own).

  Standalone `chain.resolve(&matches)?` still works for cases where input shape depends on already-resolved values.

- **New `Inputs` storage type in `standout-input`** (`standout::input::Inputs`) — a name-keyed, type-safe container for resolved inputs, plus the `MissingInput` error type for `(name, T)` lookup failures.

- **`standout::input` re-export** of the `standout-input` crate, so users do not need a separate dependency to access `InputChain` and the source types.

- **Book navigation now lists the input crate.** `docs/SUMMARY.md` gains an "Input (standout-input)" section linking to the introduction, sources, backends, and a new framework-integration topic. The pages were already present on disk but were not reachable from the rendered mdBook.

- **Heavier input backends are opt-in via standout features.** `standout` depends on `standout-input` with `default-features = false` and only enables `simple-prompts` (free, no extra deps) by default. Users who want the editor backend or inquire's TUI prompts add `features = ["input-editor"]` or `features = ["input-inquire"]` to their `standout` dependency. This preserves the "minimal by default" promise of `standout-input` — a default `standout` install no longer pulls `tempfile`, `which`, or `shell-words` transitively.

- **`.prompt()` shortcut on every interactive input source.** `InquireText`, `InquireConfirm`, `InquireSelect<T>`, `InquireMultiSelect<T>`, `InquirePassword`, `InquireEditor`, `TextPromptSource`, `ConfirmPromptSource`, and `EditorSource` now expose an inherent `prompt() -> Result<T, InputError>` method that bypasses the chain machinery and the `&clap::ArgMatches` parameter the `InputCollector` trait requires. Intended for wizard / setup helper / REPL flows that drive standout themselves and have no clap parser involved:

  ```rust
  use standout::input::{InquireSelect, InquireText};

  let pack = InquireText::new("Pack name:").help("a-z0-9-").prompt()?;
  let env  = InquireSelect::new("Environment:", vec!["dev", "staging", "prod"]).prompt()?;
  ```

  `Ok(None)` from the underlying `collect` (typically empty submission) maps to `InputError::NoInput`; cancellation maps to `InputError::PromptCancelled`. The `InputCollector` impls and chain behavior are unchanged.

- **New "Interactive Flows" topic** (`docs/crates/input/topics/interactive-flows.md`) walks through composing the new `.prompt()` API with a user-owned step graph and standout's `Renderer` / `Theme` to build wizards. The introduction guide gains a "Standalone Prompts" section that links into the new topic.

- **Wizard handlers are testable in process via `PromptResponder`.** Every interactive source's `.prompt()` shortcut now consults a process-global responder before doing any TTY work. Tests install a `ScriptedResponder` with a typed queue of answers; production wizard code is unchanged:

  ```rust
  use standout_input::{PromptResponse, ScriptedResponder};
  use std::sync::Arc;

  let result = TestHarness::new()
      .prompts(Arc::new(ScriptedResponder::new([
          PromptResponse::text("buy milk"),  // first text prompt
          PromptResponse::Bool(true),        // first confirm
          PromptResponse::Choice(2),         // first select -> options[2]
      ])))
      .run(&app, cmd, ["mycli", "setup"]);
  ```

  Open prompts (`Text`/`Password`/`Editor`) take a `Text(String)`; finite-choice prompts (`Confirm`/`Select`/`MultiSelect`) take a `Bool` / `Choice(usize)` / `Choices(Vec<usize>)`. Position-based responses are deliberate: a `Choice(2)` test keeps working when "Production" gets renamed to "Live". `ScriptedResponder` panics on kind mismatch so a wizard-step reorder fails loudly. `Cancel` and `Skip` cover the abort and re-ask paths.

  New public API: `PromptResponder` trait, `ScriptedResponder`, `PromptKind`, `PromptContext`, `PromptResponse` (in `standout_input`); `set_default_prompt_responder` / `reset_default_prompt_responder`; `TestHarness::prompts(...)` (in `standout-test`). The "Testing Wizards" section in the Interactive Flows topic documents the pattern; the Testing guide and topic cross-link it.

  This closes the testability gap that the `.prompt()` shortcut alone left open — the inquire adapters were previously untestable in CI without a real PTY.

## [7.5.0] - 2026-04-17

### Added

- **Framework warnings now render as a styled banner after the command output.** Non-fatal problems that standout detects during setup — stylesheet hot-reload failures, template walk errors, etc. — used to emit `Warning:` lines via `eprintln!` *before* the command ran, jammed on top of the real output as plain text. They now flow through a new thread-local collector (`standout::warnings`) and are flushed to stderr at the end of `App::run`, under a `Standout :: Warnings` banner with each entry on its own tab-indented line.

  Two new styles in `Theme::default()` — `standout_warning_banner` (black on orange #208, bold) and `standout_warning_item` — control the look; user themes can override either. Styling is applied only when stderr supports color; piped/redirected stderr still gets plain text. `OutputMode::Text` forces plain output.

  Public API: `standout::warnings::{push_warning, drain_warnings, has_warnings, flush_to_stderr}`.

## [7.4.0] - 2026-04-17

### Fixed

- **CSS stylesheets now hot-reload correctly in debug builds.** `EmbeddedStyles::into::<StylesheetRegistry>()` was parsing every on-disk stylesheet as YAML, so `.css` files failed with a YAML error and the registry silently fell back to the compile-time embedded content — making edits appear to be "cached" until the next rebuild. The hot-reload path and `StylesheetRegistry::add_inline` now use the same auto-detecting CSS/YAML parser as the release embedded path.

### Changed

- `StylesheetRegistry::add_inline` parameter renamed `yaml` → `content`; it accepts either CSS or YAML (format auto-detected). The method's behavior is strictly wider than before, so no caller changes are required.
- Docs and doc-examples across `standout-render`, `standout-macros`, and top-level docs updated to describe stylesheets as CSS (preferred) with legacy YAML, instead of YAML-only.

## [7.3.0] - 2026-04-16

### Added

- **Opt-in help handling** (Issue #116) — New `.help_handling(true)` builder method enables standout's themed help rendering. When enabled, all help invocations (`help`, `--help`, `-h`) — at both root and subcommand level — produce identical standout-rendered output. Previously, only the `help` subcommand went through standout while `--help`/`-h` fell through to clap's default ungrouped rendering.

### Changed

- **Help interception is now opt-in** — `App` no longer intercepts help by default. Call `.help_handling(true)` to enable standout's help rendering. This is required when using `command_groups` or topics — `build()` will panic if either is configured without it.

## [7.2.0] - 2026-04-15

### Added

- **Theme-relative colorspace module** (`standout_render::colorspace`) — A pure-computation module for generating perceptually uniform palettes from base16 themes via trilinear interpolation in CIE LAB space. Based on [jake-stewart's proposal](https://gist.github.com/jake-stewart/0a8ea46159a7da2c808e5be2177e1783).

  **New types:** `CubeCoord`, `Rgb`, `ThemePalette`

- **Cube color syntax** (`cube(60%, 20%, 0%)`) — Colors can now be specified as theme-relative coordinates in a color cube whose corners are the 8 base ANSI colors. Instead of absolute RGB, designers express intent as a position in the theme's color space, and the framework resolves the actual color via LAB interpolation.

  Supported in YAML stylesheets, CSS stylesheets, and shorthand strings:

  ```yaml
  accent:
    fg: "cube(60%, 20%, 0%)"
    bold: true
  ```

  ```css
  .accent { color: cube(60%, 20%, 0%); font-weight: bold; }
  ```

- **Theme palette support** — `Theme::with_palette(ThemePalette)` lets you attach a palette of 8 anchor colors to a theme. Cube colors in stylesheets are resolved against this palette (or a default xterm palette if none is set).

- **Alternating table row styles** — `Table::row_styles(even, odd)` wraps each data row in style tags that alternate between even and odd style names. The row counter auto-increments on every `row()` / `row_cells()` / `row_from()` / `row_from_trait()` call.

  In templates, pass `row_styles` to the `table()` function:

  ```jinja
  {% set t = table(columns, row_styles=true) %}          {# default gray tint #}
  {% set t = table(columns, row_styles="blue") %}         {# blue tint #}
  {% set t = table(columns, row_styles=["a", "b"]) %}     {# custom style names #}
  ```

- **Built-in table row tint styles** — `Theme::default()` now ships with adaptive alternating-row styles in five tints: gray (default), blue, red, green, and purple. Each tint provides a subtle background color shift for odd rows, with dark- and light-mode variants.

  | Tint | Dark bg (odd) | Light bg (odd) |
  | -------- | --------------- | ---------------- |
  | gray | 236 `#303030` | 254 `#e4e4e4` |
  | blue | 17 `#00005f` | 189 `#d7d7ff` |
  | red | 52 `#5f0000` | 224 `#ffd7d7` |
  | green | 22 `#005f00` | 194 `#d7ffd7` |
  | purple | 53 `#5f005f` | 225 `#ffd7ff` |

  Style names follow the pattern `table_row_{even,odd}[_{tint}]`.

### Changed

- `parse_stylesheet` and `parse_css` now accept an `Option<&ThemePalette>` parameter for resolving cube colors during style building.
- `ColorDef::to_console_color` now accepts an `Option<&ThemePalette>` parameter.
- `StyleAttributes::to_style` now accepts an `Option<&ThemePalette>` parameter.

## [7.1.0] - 2026-04-15

### Added

- **Passthrough commands** — Commands that bypass the rendering pipeline but still participate in help, completions, and dispatch. Use `command_passthrough` on `App` or `passthrough` on `GroupBuilder` for commands that manage their own output (e.g., shell init scripts, config delegation).

  ```rust
  App::builder()
      .command_passthrough("init-sh", |_m, _ctx| {
          print!("export PATH=\"$HOME/.myapp/bin:$PATH\"");
          Ok(())
      })?
      .build()?;
  ```

- **`Theme::from_css()` and `Theme::from_css_file()`** — New constructors for loading themes directly from CSS content or CSS files, matching the existing `from_yaml()` / `from_file()` pattern.

- **CSS stylesheet support in `embed_styles!`** — The `embed_styles!` macro and `StylesheetRegistry` now recognize `.css` files in addition to `.yaml` / `.yml`. CSS files are auto-detected and parsed with the CSS parser. `.css` has highest priority when multiple formats exist with the same base name.

- **Complete working example** — New `docs/guides/complete-example.md` with a self-contained, copy-paste project (Cargo.toml, main.rs, template, CSS stylesheet).

### Changed

- **`console` crate bumped to 0.16** — The `console` dependency has been updated from 0.15 to 0.16 across all crates (`standout`, `standout-render`, `standout-bbparser`). Users who construct `console::Style` values programmatically must ensure they depend on `console = "0.16"`. No API changes were needed — all existing console APIs remain compatible.

- **CSS is now the primary styling format** — All documentation now presents CSS as the recommended and primary format for defining styles. YAML themes are still supported but are documented as a legacy alternative. Specific changes:
  - `docs/index.md`, `docs/intro.md`: References to "CSS or YAML" replaced with "CSS"
  - `docs/guides/intro-to-standout.md`: Removed YAML styling alternative section
  - `docs/guides/tldr-intro-to-standout.md`: Updated comments to reference CSS only
  - `docs/topics/app-configuration.md`: Examples and file listings now show `.css` files
  - `crates/standout-render/docs/topics/styling-system.md`: Restructured to lead with CSS, YAML moved to legacy note
  - `crates/standout-render/docs/guides/intro-to-rendering.md`: Complete example now uses `Theme::from_css()`
  - `crates/standout-render/src/lib.rs`: Rustdoc example changed from `Theme::from_yaml()` to `Theme::from_css()`

## [7.0.0] - 2026-02-17

## [6.2.0] - 2026-02-15

### Added

- **Sub-column support for tabular layout** — Columns can now contain inner sub-columns for per-row layout distribution. One sub-column is the "grower" (`Fill`); the rest are `Fixed` or `Bounded`. Widths are resolved per-row from actual content, enabling patterns like `title + padding + tag` where the space between them varies per row.

  ```rust
  use standout_render::tabular::{Col, SubCol, SubColumns, CellValue};

  let col = Col::fill().sub_columns(
      SubColumns::new(
          vec![SubCol::fill(), SubCol::bounded(0, 30).right()],
          " ",
      ).unwrap(),
  );

  // In templates:
  // {{ fmt.row(["1.", ["Title", "[tag]"], "4d"]) }}
  ```

  **New types:** `SubColumn`, `SubColumns`, `SubCol`, `CellValue`
  **New methods:** `TabularFormatter::format_row_cells()`, `Table::row_cells()`

- **`visible_width` utility function** — New `visible_width(s)` that strips both BBCode tags and ANSI escape codes before measuring display width. Use this for any text that may contain markup.

### Fixed

- **BBCode tags miscounted in formatter width calculations** (Issue #104) — `format_value`, `format_cell_lines`, `CellOutput::line`, and `resolve_sub_widths` now correctly strip BBCode tags before measuring display width. Previously, tags like `[bold]...[/bold]` were counted toward column width, causing incorrect truncation and padding.

## [6.1.0] - 2026-02-11

### Added

- **Subcommand group support in help rendering** (Issue #102) — CLIs with many commands can now organize them into titled sections in help output instead of a single flat "COMMANDS" list.

  ```rust
  use standout::cli::{App, CommandGroup, HelpConfig};

  let groups = vec![
      CommandGroup {
          title: "Project".into(),
          help: Some("Commands for managing projects".into()),
          commands: vec![Some("init".into()), Some("build".into()), None, Some("clean".into())],
      },
      CommandGroup {
          title: "Config".into(),
          help: None,
          commands: vec![Some("get".into()), Some("set".into())],
      },
  ];

  App::builder()
      .command_groups(groups)
      .command("init", handler, template)?
      // ...
      .build()?
  ```

  **Features:**
  - `CommandGroup` struct with `title`, optional `help` text, and ordered command list
  - `None` entries in `commands` produce blank-line separators between commands
  - Ungrouped commands auto-append to an "Other" group
  - Each group renders its own uppercased header (e.g., `PROJECT`, `CONFIG`)
  - `validate_command_groups()` for test-time validation of group references
  - Full `AppBuilder` integration via `.command_groups()` method
  - Backward-compatible: no groups configured produces identical "COMMANDS" output

  See [Help System](docs/topics/standout-help.md) for full documentation.

## [6.0.2] - 2026-02-09

### Fixed

- **Extension-agnostic registry resolution for templates and styles** - Registry lookups now fall back to the base name when a recognized extension doesn't match exactly. For example, looking up `"list.j2"` now finds a template registered from `list.jinja`, because the registry strips the known `.j2` extension and retries with the base name `"list"`. This fix applies uniformly to `FileRegistry`, `TemplateRegistry`, and `StylesheetRegistry` — all lookup tiers (inline, file-based, directory-based, framework) support the fallback.
  - Added `resolve_in_map()` helper in `file_loader` for extension-agnostic HashMap lookups.
  - Updated `FileRegistry::get()` and `get_entry()` to try base name when exact lookup fails.
  - Updated `TemplateRegistry::get()` to use fallback for inline, files, and framework tiers.
  - Updated `StylesheetRegistry::get()` and `contains()` to use fallback for inline tier.

## [6.0.1] - 2026-02-09

### Fixed

- **Fixed `col` filter truncating content with BBCode markup tags** (Issue #98) - The `col` filter (and related filters `display_width`, `pad_left`, `pad_right`, `pad_center`, `truncate_at`) now correctly treats BBCode-style tags as zero-width when measuring and formatting text. Previously, tags like `[additions]+32[/additions]` were counted as visible characters, causing premature truncation.
  - Added `strip_tags()` convenience function to `standout-bbparser` for stripping all BBCode tags from text.
  - Width measurement uses `strip_tags()` to compute visible width before formatting.
  - Padding preserves BBCode tags in output; truncation operates on stripped text.

## [6.0.0] - 2026-02-03

### Fixed

- **Fixed command sequencing sensitivity (Late Binding)** - Refactored command dispatch to resolve dependencies (like `Theme`) at runtime rather than build time. This fixes an issue where configuring the theme after registering commands resulted in commands using the default theme (Issue #89).
  - Updated internal `DispatchFn` signature to accept `&Theme` at runtime.
  - Commands now correctly use the final configured theme regardless of registration order.
  - Works with all registration methods: `.command()`, `.commands()` (dispatch! macro), and nested `.group()` calls.

## [5.0.0] - 2026-02-03

### Added

- **New `standout-input` crate** - Declarative input collection from multiple sources with automatic fallback chains.

  ```rust
  use standout_input::{InputChain, ArgSource, StdinSource, EditorSource};

  let message = InputChain::<String>::new()
      .try_source(ArgSource::new("message"))
      .try_source(StdinSource::new())
      .try_source(EditorSource::new())
      .resolve(&matches)?;
  ```

  **Core sources (always available):**
  - `ArgSource`, `FlagSource` - CLI arguments and flags
  - `StdinSource` - Piped stdin (skipped when terminal)
  - `EnvSource` - Environment variables
  - `ClipboardSource` - System clipboard (macOS/Linux)
  - `DefaultSource<T>` - Fallback values

  **Feature-gated sources:**

  | Feature | Dependencies | Provides |
  | --------- | -------------- | ---------- |
  | `editor` (default) | tempfile, which | `EditorSource` - Opens $VISUAL/$EDITOR |
  | `simple-prompts` (default) | none | `TextPromptSource`, `ConfirmPromptSource` |
  | `inquire` | inquire (~29 deps) | Rich TUI: `InquireText`, `InquireConfirm`, `InquireSelect`, `InquireMultiSelect`, `InquirePassword`, `InquireEditor` |

  **Features:**
  - Chain-level validation with retry support for interactive sources
  - Mock implementations for all sources (testable without real terminal/env)
  - `resolve_with_source()` returns which source provided the input

  See [Introduction to Input](crates/standout-input/docs/guides/intro-to-input.md) for the full guide.

## [4.0.0] - 2026-02-02

### Changed

- **BREAKING: Unified `App` and `LocalApp` into single-threaded `App`** - The dual architecture has been removed in favor of a simpler, single-threaded design. CLI applications are fundamentally single-threaded (parse → run one handler → output → exit), so thread-safety bounds were unnecessary complexity.

  **Removed types:**
  - `LocalApp`, `LocalAppBuilder` (merged into `App`, `AppBuilder`)
  - `LocalHandler` (merged into `Handler`)
  - `Local`, `ThreadSafe` marker types
  - `HandlerMode` trait

  **Key changes:**
  - `App` now uses `Rc<RefCell<...>>` instead of `Arc<...>`
  - `Handler::handle()` takes `&mut self` instead of `&self`
  - Handler functions use `FnMut` instead of `Fn`
  - `App::builder()` no longer requires generic type parameter
  - Removed all `Send + Sync` bounds from handler system

  **Migration:**

  ```rust
  // Before
  use standout::cli::{App, ThreadSafe, LocalApp, LocalHandler};
  App::<ThreadSafe>::builder()
      .command("list", handler, template)?
      .build()?

  // After
  use standout::cli::{App, Handler};
  App::builder()
      .command("list", handler, template)?
      .build()?

  // Handler trait: &self → &mut self
  impl Handler for MyHandler {
      fn handle(&mut self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
          // ...
      }
  }
  ```

  This simplifies the API for the common case (single-threaded CLI apps) while supporting mutable handler state directly without `Arc<Mutex<_>>` wrappers.

## [3.8.0] - 2026-02-02

### Changed

- **Piped content is now automatically plain text** - When using `pipe_to()`, `pipe_through()`, `pipe_to_clipboard()`, or custom `PipeTarget` implementations, ANSI escape codes are automatically stripped from the piped content. This matches standard shell semantics where `command | other_command` receives unformatted output.

  ```rust
  // Template with styled output
  cfg.template("[bold]{{ title }}[/bold]: [green]{{ count }}[/green]")
     .pipe_through("jq .")

  // Terminal sees formatted output with colors
  // jq receives plain text: "Report: 42"
  ```

  **Implementation details:**
  - `TextOutput` struct now has both `formatted` (ANSI codes for terminal) and `raw` (plain text for piping) fields
  - All piping methods use `raw` for external commands while returning `formatted` for terminal display
  - Uses existing `OutputMode::Text` rendering path to strip style tags cleanly

## [3.7.0] - 2026-01-31

## [3.6.1] - 2026-01-31

### Added

- **Auto-wrap `Result<T>` in `Output::Render`** - Handlers can now return `Result<T, E>` directly instead of wrapping in `Ok(Output::Render(...))`. The framework automatically wraps successful results.

  ```rust
  // Before: explicit wrapping required
  fn list(m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
      let items = storage::list()?;
      Ok(Output::Render(items))  // Framework ceremony
  }

  // After: auto-wrap Result<T>
  fn list(m: &ArgMatches, ctx: &CommandContext) -> Result<Vec<Item>, Error> {
      storage::list()  // Clean and natural
  }
  ```

  **New types:**
  - `IntoHandlerResult<T>` trait - Converts `Result<T, E>` or `HandlerResult<T>` into handler results

  **Behavior:**
  - `Result<T, E>` → automatically wrapped in `Output::Render`
  - `HandlerResult<T>` → passed through unchanged (for `Output::Silent` or `Output::Binary`)

- **Optional `CommandContext` in handler signatures** - Handlers that don't need context can now omit the parameter entirely.

  ```rust
  // Before: context required even when unused
  fn list(_m: &ArgMatches, _ctx: &CommandContext) -> Result<Vec<Item>, Error> {
      storage::list()
  }

  // After: context can be omitted
  fn list(m: &ArgMatches) -> Result<Vec<Item>, Error> {
      storage::list()
  }
  ```

  **New types:**
  - `SimpleFnHandler<F, T>` - Thread-safe handler wrapper for functions without context
  - `LocalSimpleFnHandler<F, T>` - Local (non-Send) variant

  **Dispatch derive support:**

  ```rust
  #[derive(Subcommand, Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      #[dispatch(simple)]  // Handler only takes &ArgMatches
      List,
  }
  ```

- **`#[handler]` proc macro for pure function handlers** - Transform pure Rust functions into Standout-compatible handlers with automatic CLI argument extraction.

  ```rust
  // Before: Standout-specific boilerplate
  fn list(m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
      let all = m.get_flag("all");
      let limit = m.get_one::<usize>("limit").copied();
      let items = storage::list(all, limit)?;
      Ok(Output::Render(items))
  }

  // After: pure function, easy to test
  #[handler]
  fn list(#[flag] all: bool, #[arg] limit: Option<usize>) -> Result<Vec<Item>, Error> {
      storage::list(all, limit)
  }
  // Generates: fn list__handler(m: &ArgMatches) -> Result<Vec<Item>, Error>
  ```

  **Supported annotations:**

  | Annotation | Type | Description |
  | ------------ | ------ | ------------- |
  | `#[flag]` | `bool` | Boolean CLI flag |
  | `#[flag(name = "x")]` | `bool` | Flag with custom CLI name |
  | `#[arg]` | `T` | Required CLI argument |
  | `#[arg]` | `Option<T>` | Optional CLI argument |
  | `#[arg]` | `Vec<T>` | Multiple CLI arguments |
  | `#[arg(name = "x")]` | `T` | Argument with custom CLI name |
  | `#[ctx]` | `&CommandContext` | Access to command context |
  | `#[matches]` | `&ArgMatches` | Raw matches (escape hatch) |

  **Return type handling:**
  - `Result<T, E>` → passed through (dispatch auto-wraps via `IntoHandlerResult`)
  - `Result<(), E>` → wrapped in `HandlerResult<()>` with `Output::Silent`

  **Benefits:**
  - Pure functions with no Standout dependencies
  - Direct testing: call `list(true, None)` in tests
  - Self-documenting: annotations show what comes from CLI
  - Familiar pattern: similar to Axum/Actix extractors

- **Output piping to external commands** - New `standout-pipe` crate enables sending rendered output to shell commands for filtering, logging, or clipboard operations.

  ```rust
  // Via derive macro
  #[derive(Subcommand, Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      #[dispatch(pipe_through = "jq '.items'")]  // Filter with jq
      List,

      #[dispatch(pipe_to_clipboard)]  // Copy to clipboard
      Export,

      #[dispatch(pipe_to = "tee /tmp/log.txt")]  // Log while displaying
      Debug,
  }

  // Via builder API
  App::builder()
      .commands(|g| {
          g.command_with("list", handlers::list, |cfg| {
              cfg.template("list.jinja")
                 .pipe_through("jq '.data'")
          })
      })
  ```

  **Three piping modes:**

  | Mode | Method | Behavior |
  | ------ | -------- | ---------- |
  | Passthrough | `pipe_to()` | Run command, return original output |
  | Capture | `pipe_through()` | Use command's stdout as new output |
  | Consume | `pipe_to_clipboard()` | Send to clipboard, return empty |

  **Features:**
  - Platform-aware clipboard (pbcopy on macOS, xclip on Linux)
  - Configurable timeouts via `pipe_to_with_timeout()`, `pipe_through_with_timeout()`
  - Chainable: multiple pipes execute in sequence
  - Custom implementations via `PipeTarget` trait
  - Error messages include command name for debugging

  See [Output Piping](crates/standout-pipe/docs/topics/piping.md) for full documentation.

## [3.6.0] - 2026-01-30

### Added

- **SimpleEngine for lightweight templates** - New `SimpleEngine` using format-string style `{variable}` syntax as an alternative to MiniJinja. Ideal for simple templates that only need variable substitution, with minimal binary overhead (~5KB vs ~248KB for MiniJinja).

  **Syntax:**
  - `{name}` - Simple variable substitution
  - `{user.profile.email}` - Nested property access via dot notation
  - `{items.0}` - Array index access
  - `{{` and `}}` - Escaped braces (render as `{` and `}`)

  **Does NOT support** (by design):
  - Loops, conditionals, filters, includes, macros

  **Usage:**

  ```rust
  use standout_render::{Renderer, Theme, OutputMode};
  use standout_render::template::SimpleEngine;

  let engine = Box::new(SimpleEngine::new());
  let mut renderer = Renderer::with_output_and_engine(
      Theme::new(),
      OutputMode::Auto,
      engine,
  )?;

  renderer.add_template("status", "Hello, {name}!")?;
  ```

  **New file extension:** `.stpl` for SimpleEngine templates. Extension priority: `.jinja` > `.jinja2` > `.j2` > `.stpl` > `.txt`

  See the [Template Engines](crates/standout-render/docs/topics/template-engines.md) topic for full documentation.

## [3.5.0] - 2026-01-30

### Changed

- **Pluggable template engine architecture** - The template rendering system now uses a `TemplateEngine` trait, decoupling the public API from the MiniJinja implementation. This enables future alternative backends (e.g., a lighter "simple-templates" engine for users who don't need full template features).

  **New types:**
  - `TemplateEngine` trait - Abstraction for template backends with methods for rendering, named templates, and context injection
  - `MiniJinjaEngine` - Default implementation wrapping MiniJinja (existing behavior)
  - `RenderError` - New error type that doesn't expose MiniJinja internals

  **New APIs:**
  - `Renderer::with_output_and_engine()` - Create a renderer with a custom template engine
  - `render_auto_with_engine()` - Render with a custom engine and auto-dispatch

  **Migration:** Replace `minijinja::Error` with `RenderError` in error handling. The default behavior is unchanged - `MiniJinjaEngine` is used automatically.

  ```rust
  // Custom engine injection (optional)
  let engine = Box::new(MyCustomEngine::new());
  let renderer = Renderer::with_output_and_engine(theme, mode, engine)?;

  // Default usage unchanged
  let renderer = Renderer::new(theme)?;
  ```

## [3.4.0] - 2026-01-30

### Added

- **App State for shared, immutable dependencies** - New `app_state` field in `CommandContext` for injecting app-level resources (database connections, configuration, API clients) that are shared across all command dispatches.

  ```rust
  // Configure at build time
  App::builder()
      .app_state(Database::connect()?)  // Shared via Arc
      .app_state(Config::load()?)
      .command("list", list_handler, template)
      .build()?

  // Access in handlers
  fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
      let db = ctx.app_state.get_required::<Database>()?;
      let config = ctx.app_state.get_required::<Config>()?;
      Ok(Output::Render(db.list(&config.api_url)?))
  }
  ```

  **Two-state model:**

  | Aspect | `ctx.app_state` | `ctx.extensions` |
  | -------- | ----------------- | ------------------ |
  | Mutability | Immutable (`&`) | Mutable (`&mut`) |
  | Lifetime | App lifetime | Per-request |
  | Set by | `AppBuilder::app_state()` | Pre-dispatch hooks |
  | Use for | Database, Config, API clients | User sessions, request IDs |

  Pre-dispatch hooks can read `app_state` to set up per-request `extensions`:

  ```rust
  Hooks::new().pre_dispatch(|matches, ctx| {
      let db = ctx.app_state.get_required::<Database>()?;
      let user = db.authenticate(matches)?;
      ctx.extensions.insert(UserScope { user });
      Ok(())
  })
  ```

### Changed

- **BREAKING: `CommandContext` now includes `app_state` field** - The struct now has three fields: `command_path`, `app_state`, and `extensions`. Code that constructs `CommandContext` manually needs to include `app_state: Rc::new(Extensions::new())` or use `..Default::default()`.

## [3.3.0] - 2026-01-30

### Added

- **Context Extensions for dependency injection** - Pre-dispatch hooks can now inject state that handlers retrieve, enabling dependency injection without modifying handler signatures.

  ```rust
  // Pre-dispatch hook injects dependencies
  Hooks::new().pre_dispatch(|_m, ctx| {
      ctx.extensions.insert(Database::connect()?);
      ctx.extensions.insert(Config::load()?);
      Ok(())
  })

  // Handler retrieves them - works with #[derive(Dispatch)]!
  fn list_handler(m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Items> {
      let db = ctx.extensions.get_required::<Database>()?;
      Ok(Output::Render(db.query()?))
  }
  ```

  **Extensions API:**
  - `insert<T>(value)` - Insert a value, returns previous if any
  - `get<T>()` - Get reference (`Option<&T>`)
  - `get_required<T>()` - Get reference or error (`Result<&T, Error>`)
  - `get_mut<T>()` / `get_mut_required<T>()` - Mutable variants
  - `remove<T>()`, `contains<T>()`, `len()`, `is_empty()`, `clear()`

### Changed

- **Pre-dispatch hooks now receive `&mut CommandContext`** - This enables state injection via `ctx.extensions`. Existing hooks that don't use extensions continue to work unchanged.

## [3.2.0] - 2026-01-30

### Added

- **ListView macro support** - New attributes for `#[derive(Dispatch)]` to streamline list/table command output:

  ```rust
  #[derive(Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      #[dispatch(list_view, item_type = "Task")]
      List(ListArgs),
  }
  ```

  Features:
  - `list_view` attribute marks a command as returning tabular data
  - `item_type` specifies the struct type for column inference
  - Automatically injects `tabular_spec` into dispatch handlers
  - Framework assets infrastructure with built-in `list-view.jinja` template

### Fixed

- **Pinned Rust 1.93.0** - Added `rust-toolchain.toml` to ensure local and CI environments use the same Rust version
- **Improved CI caching** - Switched to `Swatinem/rust-cache` for faster builds

## [3.1.0] - 2026-01-30

### Added

- **New Seeker module** - A query/filtering system for collections with three layers of API:

  **Imperative API** - Build queries programmatically:

  ```rust
  use standout::seeker::{Query, Filter, Op};

  let query = Query::new()
      .filter(Filter::new("status", Op::Eq, "active"))
      .filter(Filter::new("priority", Op::Gte, 5))
      .order_by("created_at", Descending)
      .limit(10);

  let results = query.apply(&items);
  ```

  **Derive macro** - Add querying to any struct:

  ```rust
  #[derive(Seekable)]
  struct Task {
      #[seekable]
      status: Status,
      #[seekable]
      priority: u8,
      #[seekable(rename = "created")]
      created_at: DateTime,
  }
  ```

  **String parsing** - Parse CLI arguments or query strings:

  ```rust
  // "status-eq=active" "priority-gte=5" "order=created_at:desc"
  let query = parse_query::<Task>(&args)?;
  ```

  Supported operators: `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `contains`, `startswith`, `endswith`, `regex`, `before`, `after`, `in`, `is`

## [3.0.0] - 2026-01-30

### Changed

- **BREAKING: Removed `clap` feature flag** - The `cli` module and clap integration are now always available. The `clap` feature has been removed.

  ```diff
  [dependencies]
  - standout = { version = "2", features = ["clap", "macros"] }
  + standout = "3"
  ```

  **Migration:** Remove `features = ["clap"]` from your `Cargo.toml`. If you only used `features = ["macros"]`, note that macros are also now always available.

- **`macros` feature is now a no-op** - The `macros` feature still exists for backwards compatibility but does nothing. All macros (`embed_templates!`, `embed_styles!`, `Dispatch`, `Tabular`, `TabularRow`) are now always available.

### Added

- **New `standout-dispatch` crate** - Extracted command dispatch/routing into a standalone crate for users who need routing without the full framework.

  The new crate provides:
  - Command registration and path-based dispatch
  - Handler and hook type definitions
  - Clean separation from rendering concerns

  **Usage:**

  ```rust
  // For dispatch-only use cases
  use standout_dispatch::{Dispatcher, Handler, Output};

  // Full framework users continue using standout (unchanged API)
  use standout::{cli::App, Handler, Output};
  ```

  The main `standout` crate re-exports everything from `standout-dispatch`, so existing code continues to work without changes.

- **Documentation rewrite** - Standalone-first documentation for the split crate architecture (`standout`, `standout-render`, `standout-dispatch`).

## [2.1.0] - 2026-01-18

### Added

- **New `standout-render` crate** - Extracted the rendering layer into a standalone crate for users who need rich terminal output without CLI framework features.

  The new crate provides:
  - Two-pass template rendering (MiniJinja + BBCode-style styling)
  - Adaptive themes with light/dark mode support
  - Output modes (Auto, Term, Text, JSON, YAML, CSV, XML)
  - Tabular formatting with Unicode support
  - File-based resources with hot-reload in dev, embedded in release

  **Usage:**

  ```rust
  // For render-only use cases (no CLI framework)
  use standout_render::{render, Theme};

  // Full framework users continue using standout (unchanged API)
  use standout::{render, Theme, cli::App};
  ```

  The main `standout` crate re-exports everything from `standout-render`, so existing code continues to work without changes.

### Changed

- **BREAKING: `App` is now generic over `HandlerMode`** - `App` and `LocalApp` have been unified into a single generic type `App<M: HandlerMode>`. `LocalApp` is now a type alias for `App<Local>`.

  ```diff
  - use standout::cli::App;
  - App::builder()
  + use standout::cli::{App, ThreadSafe};
  + App::<ThreadSafe>::builder()
  ```

  Note: `App::builder()` still works and defaults to `ThreadSafe`, but explicit type annotation is recommended for clarity.

- **BREAKING: Builder methods now return `Result`** - All `AppBuilder` command registration methods now return `Result<Self, SetupError>` instead of `Self`. This catches configuration errors at build time rather than runtime.

  ```diff
  App::builder()
  -     .command("list", handler, template)
  +     .command("list", handler, template)?
      .build()?
  ```

  **Migration:** Add `?` or `.unwrap()` after each `.command()`, `.command_with()`, `.command_handler()`, and `.group()` call.

- **Internal: Shared AppCore architecture** - Extracted common functionality from `App` and `LocalApp` into a shared `AppCore` struct. This ensures feature parity between both app types and eliminates code duplication.

### Added

- **Duplicate command detection** - Registering the same command path twice now returns `SetupError::DuplicateCommand` instead of silently overwriting. This catches configuration bugs early.

  ```rust
  App::builder()
      .command("list", handler1, template)?
      .command("list", handler2, template)?  // Error: duplicate command "list"
  ```

- **Design guidelines documentation** - Added `docs/dev/design-guidelines.md` codifying configuration safety principles, structural unification requirements, and testing requirements for contributors.

- **Comprehensive property-based testing** - Added `proptest` tests that verify rendering invariants across all configuration combinations:
  - 8 output modes (Auto, Term, Text, TermDebug, Json, Yaml, Xml, Csv)
  - 2 handler modes (ThreadSafe, Local)
  - Theme variations (none, empty, populated)
  - Template variations (simple, styled, nested)

### Fixed

- **LocalApp now supports `{% include %}` in templates** - LocalApp templates can now use `{% include %}` directives to include other templates from the registry, matching the behavior of `App`.

## [1.1.0] - 2026-01-18

### Added

- **LocalApp for mutable handlers** - New `LocalApp` and `LocalAppBuilder` types for CLI applications that need `FnMut` handlers with `&mut self` access to state, without requiring `Send + Sync` bounds or interior mutability wrappers.

  **When to use:**
  - Your handlers need `&mut self` access to state
  - You want to avoid `Arc<Mutex<_>>` wrappers
  - Your CLI is single-threaded (the common case)

  **New types:**
  - `LocalApp` - Single-threaded CLI application with mutable dispatch
  - `LocalAppBuilder` - Builder accepting `FnMut` handlers
  - `LocalHandler` trait - For struct-based handlers with `&mut self`

  **Example:**

  ```rust
  use standout::cli::{LocalApp, Output};

  let mut counter = 0u32;

  LocalApp::builder()
      .command("increment", |_m, _ctx| {
          counter += 1;  // FnMut allows direct mutation!
          Ok(Output::Render(json!({"count": counter})))
      }, "Count: {{ count }}")
      .build()?
      .run(cmd, args);
  ```

  **Comparison with App:**

  | Aspect | `App` | `LocalApp` |
  | -------- | ------- | ------------ |
  | Handler type | `Fn + Send + Sync` | `FnMut` |
  | State mutation | Via `Arc<Mutex<_>>` | Direct |
  | Thread safety | Yes | No |
  | Use case | Libraries, async | Simple CLIs |

- **Comprehensive tabular layout system** - New `standout::tabular` module for creating aligned, column-based terminal output with full Unicode support.

  **Template filters:**
  - `col(width, align=?, truncate=?, ellipsis=?)` - Format value to fit column width
  - `pad_left(width)`, `pad_right(width)`, `pad_center(width)` - Padding helpers
  - `truncate_at(width, position?, ellipsis?)` - Truncation with start/middle/end positions
  - `display_width` - Get visual width of Unicode strings
  - `style_as(style)` - Wrap value in style tags

  **Template functions:**
  - `tabular(columns, separator=?, width=?)` - Create a TabularFormatter for row-by-row output
  - `table(columns, border=?, header=?, header_style=?, row_separator=?, width=?)` - Create decorated tables with borders

  **Rust API:**
  - `TabularSpec` - Column layout specification with builder pattern
  - `TabularFormatter` - Row formatter with field extraction support
  - `Table` - Decorated table with borders, headers, and separators
  - `Col` - Shorthand column constructors (`Col::fixed()`, `Col::fill()`, `Col::min()`, etc.)

  **Features:**
  - Multiple width strategies: fixed, bounded (min/max), fill, fractional
  - Column anchoring (left/right edge positioning)
  - Overflow handling: truncate (start/middle/end), wrap, clip, expand
  - Automatic field extraction from structs via `row_from()`
  - Column styles with `style_from_value` for dynamic styling
  - Six border styles: none, ascii, light, heavy, double, rounded
  - Row separators between data rows
  - Headers from column specs via `header_from_columns()`
  - Full Unicode support (CJK characters, combining marks, ANSI codes)

### Changed

- **BREAKING: Renamed `table` module to `tabular`** - The module is now accessed as `standout::tabular` instead of `standout::table`. This better reflects its purpose of providing tabular layout functionality.
  - `use standout::table::*` → `use standout::tabular::*`

- **BREAKING: Renamed types for consistency:**
  - `TableFormatter` → `TabularFormatter`
  - `register_table_filters()` → `register_tabular_filters()`
  - Removed backward compatibility aliases (`TableSpec`, `TableSpecBuilder`)

## [2.2.0] - 2026-01-15

## [2.1.2] - 2026-01-15

### Added

- **Default command support** - Configure a command to run when no subcommand is specified
  - `AppBuilder::default_command("name")` - Set the default command imperatively
  - `#[dispatch(default)]` variant attribute - Mark a command as default in `#[derive(Dispatch)]`
  - When CLI is invoked without a subcommand (e.g., `myapp` or `myapp --verbose`), the default command is automatically used
  - Only one command can be marked as default per dispatch group

  ```rust
  // Imperative API
  App::builder()
      .default_command("list")
      .command("list", list_handler, "list.j2")
      .command("add", add_handler, "add.j2")

  // Macro API
  #[derive(Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      #[dispatch(default)]
      List,
      Add,
  }
  ```

## [2.1.1] - 2026-01-15

### Fixed

- **Fixed broken `clap` feature** - The `clap` feature was completely broken due to incorrect internal imports introduced during the rendering module reorganization:
  - `crate::render::TemplateRegistry` → `crate::TemplateRegistry`
  - `crate::stylesheet::StylesheetRegistry` → `crate::StylesheetRegistry`
  - `crate::render::filters::register_filters` → `crate::rendering::template::filters::register_filters`
  - `DispatchRenderedOutput` → `DispatchOutput`
  - `crate::cli::hooks::Output` → `crate::cli::hooks::RenderedOutput`

### Added

- **Pre-commit hook for feature validation** - Added `.githooks/pre-commit` to check all feature combinations compile before commit
- **CI feature matrix testing** - CI now tests all feature combinations (`default`, `macros`, `clap`, `all-features`) plus formatting and clippy checks

## [2.1.0] - 2026-01-15

### Changed

- **BREAKING: Reorganized rendering modules into `src/rendering/`** - All rendering-related code is now consolidated under the `rendering` module for clearer organization and potential future extraction to a standalone crate.
  - `render/` → `rendering/template/`
  - `theme/` → `rendering/theme/`
  - `style/` → `rendering/style/`
  - `stylesheet/` → merged into `rendering/style/`
  - `table/` → `rendering/table/`
  - `output.rs` → `rendering/output.rs`
  - `context.rs` → `rendering/context.rs`

- **BREAKING: Merged `stylesheet` module into `style`** - The `stylesheet` module has been absorbed into `style`. All YAML parsing functionality is now accessed through the `style` module.
  - `use standout::stylesheet::*` → `use standout::style::*`
  - Types like `StylesheetRegistry`, `parse_stylesheet`, `ThemeVariants` are now in `style`

### Added

- **`rendering::prelude` module** - Convenient imports for standalone rendering:

  ```rust
  use standout::rendering::prelude::*;
  ```

  Includes: `render`, `render_auto`, `render_with_output`, `render_with_mode`, `render_with_vars`, `Theme`, `ColorMode`, `OutputMode`, `Renderer`, `Style`

- **`render_with_vars()` function** - Simplified context injection for adding key-value pairs to templates without the full `ContextRegistry` system:

  ```rust
  let mut vars = HashMap::new();
  vars.insert("version", "1.0.0");
  let output = render_with_vars(template, &data, &theme, mode, vars)?;
  ```

## [2.0.0] - 2026-01-14

## [1.0.0] - 2026-01-14

## [1.0.0] - 2026-01-13

### 🚀 First Stable Release

Standout reaches 1.0 with a cleaner, more ergonomic template syntax.

### ⚠️ BREAKING CHANGE: Tag-Based Styling

**The MiniJinja `style` filter has been replaced with BBCode-style tags.**

```diff
- {{ title | style("heading") }}
+ [heading]{{ title }}[/heading]

- {{ "Error:" | style("error") }} {{ message }}
+ [error]Error:[/error] {{ message }}
```

**Migration is straightforward:** wrap your content with `[name]...[/name]` tags instead of piping through the `style` filter.

### Added

- **Tag-based style syntax** - Ergonomic `[name]content[/name]` syntax for applying styles
  - Two-pass rendering: MiniJinja first, then BBParser style tag processing
  - Output mode support: tags become ANSI codes (Term), stripped (Text), or preserved (TermDebug)
  - Unknown tags show `[tag?]` marker for easy debugging
- **Template validation** - `validate_template()` function to catch unknown style tags
  - Returns detailed error info with tag name and position
  - Re-exported `UnknownTagError`, `UnknownTagErrors`, `UnknownTagKind` types
- **New `standout-bbparser` crate** - Standalone BBCode-style tag parser for terminal styling
  - `BBParser` with configurable `TagTransform` (Apply/Remove/Keep)
  - `UnknownTagBehavior` (Passthrough with `?` marker, or Strip)
  - Strict validation for unbalanced/unexpected close tags
  - Optimized nested style application (reduced ANSI bloat)
  - CSS identifier rules for tag names
- **`#[derive(Dispatch)]` macro** - Convention-based command dispatch for clap `Subcommand` enums
  - Generates `dispatch_config()` method that maps variants to handlers automatically
  - PascalCase variants map to snake_case handlers (e.g., `AddTask` → `handlers::add_task`)
  - Container attribute: `#[dispatch(handlers = path)]` specifies handler module
  - Variant attributes: `handler`, `template`, `nested`, `skip`
  - Hook support: `pre_dispatch`, `post_dispatch`, `post_output` per variant

### Removed

- **`style` filter** - Use tag syntax `[name]{{ value }}[/name]` instead

### Example

```rust
use standout::{render_with_output, Theme, OutputMode};
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold())
    .add("count", Style::new().cyan());

// Tag syntax for all styled content
let template = r#"[title]Report[/title]: [count]{{ count }}[/count] items"#;

let output = render_with_output(template, &data, &theme, OutputMode::Term)?;
```

## [0.14.0] - 2026-01-12

- **Added**:
  - **Declarative command dispatch** - New `dispatch!` macro for defining command hierarchies with clean, Python-dict-like syntax
    - Simple command syntax: `name => handler`
    - Config block syntax: `name => { handler: ..., template: ..., pre_dispatch: ... }`
    - Nested groups: `group_name: { ... }`
    - Hook support inline: `pre_dispatch`, `post_dispatch`, `post_output`
  - **Nested builder API** - `.group()` method for programmatic command organization
    - `GroupBuilder` for building nested command groups
    - `CommandConfig` for inline handler configuration
    - `.command_with()` for inline template and hook configuration
  - **Convention-based template resolution** - Templates resolved automatically from command path
    - `.template_dir("templates")` sets base directory
    - `.template_ext(".j2")` sets extension (default: `.j2`)
    - Command `db.migrate` resolves to `templates/db/migrate.j2`
  - **`.commands()` method** - Accepts closure from `dispatch!` macro for bulk command registration

- **Example**:

  ```rust
  use standout_clap::{dispatch, Standout, CommandResult};
  use serde_json::json;

  Standout::builder()
      .template_dir("templates")
      .commands(dispatch! {
          db: {
              migrate => db::migrate,
              backup => {
                  handler: db::backup,
                  template: "backup.j2",
                  pre_dispatch: validate_auth,
              },
          },
          app: {
              start => app::start,
              config: {
                  get => config::get,
                  set => config::set,
              },
          },
          version => |_m, _ctx| CommandResult::Ok(json!({"v": "1.0"})),
      })
      .run_and_print(cmd, args);
  ```

## [0.13.0] - 2026-01-12

## [0.12.0] - 2026-01-12

- **Added**:
  - **Compile-time resource embedding macros** - Embed templates and stylesheets into binaries at compile time
    - `embed_templates!("./templates")` - Walks directory and embeds all template files
    - `embed_styles!("./styles")` - Walks directory and embeds all stylesheet files
    - Same resolution API as runtime loading (access by base name or with extension)
    - Extension priority preserved (e.g., `.jinja` > `.jinja2` > `.j2` > `.txt`)
  - **EmbeddedSource with debug hot-reload** - Macros return `EmbeddedSource<R>` type that supports automatic hot-reload
    - In debug mode: if source path exists, files are read from disk (hot-reload)
    - In release mode: embedded content is used (zero file I/O)
    - `EmbeddedTemplates` and `EmbeddedStyles` type aliases for convenience
    - `From` implementations for converting to `TemplateRegistry` and `StylesheetRegistry`
  - **RenderSetup builder** - Unified setup API for templates, styles, and themes
    - `RenderSetup::new().templates(...).styles(...).default_theme(...).build()`
    - `StandoutApp` for ready-to-use rendering with pre-loaded templates
  - **standout-clap integration** - `.styles()` and `.default_theme()` methods on `StandoutBuilder`

- **Changed**:
  - **Simplified embed macro architecture** - Macros are now "dumb" collectors that only walk directories
    - All smart logic (extension priority, name stripping, collision detection) moved to `from_embedded_entries()` methods
    - `TemplateRegistry::from_embedded_entries()` for compile-time template embedding
    - `StylesheetRegistry::from_embedded_entries()` for compile-time stylesheet embedding
  - **Consolidated file loader helpers** - Shared functions in `file_loader` module
    - `extension_priority()` - Returns priority index for filename extension
    - `strip_extension()` - Removes recognized extension from filename
    - `build_embedded_registry()` - Generic helper for building registries from embedded entries
  - **Updated template extensions** - Changed from `.tmpl` to `.jinja` as primary extension
    - New priority order: `.jinja`, `.jinja2`, `.j2`, `.txt`

- **Fixed**:
  - **Hot-reload mode now works correctly with `names()` iteration** - Previously, converting `EmbeddedSource` to registries in debug mode used lazy loading, causing `names()` to return empty. Now uses immediate loading for both templates and stylesheets.

## [0.11.1] - 2026-01-11

- **Added**:
  - **File-based stylesheet loading** - Load themes and styles from YAML files at runtime
    - `StylesheetRegistry` for managing file-based themes
    - YAML stylesheet parsing with full spec compliance
    - Adaptive themes that respond to terminal capabilities
  - **Auto output to file** - Automatically save command output to files
    - Configurable output path patterns
    - Support for all output formats (text, JSON, YAML, XML, CSV)

- **Changed**:
  - **Renamed `TableSpec` to `FlatDataSpec`** - Better reflects its purpose for flat data extraction across multiple formats (tables, CSV)
  - Improved data extraction for CSV export

## [0.10.1] - 2026-01-11

- **Added**:
  - **File-based template loading** - Load templates from `.txt` or `.jinja` files at runtime
    - `TemplateRegistry` for managing file-based templates
    - Hot reload support in debug mode for rapid iteration
    - Template caching in release mode for performance
  - **Multiple output format support**:
    - **YAML output** - Serialize data to YAML format
    - **XML output** - Serialize data to XML format
    - **CSV output** - Automatic flattening of nested data structures for tabular export
  - **Generic file loader infrastructure** - Reusable file loading utilities for templates, stylesheets, and other resources

- **Changed**:
  - Template caching is now enabled by default in release builds

## [0.9.0] - 2026-01-10

## [0.7.2] - 2026-01-10

- **Added**:
  - **Post-dispatch hooks** - New hook phase that runs after handler execution but before rendering
    - `post_dispatch` hooks receive raw handler data as `serde_json::Value`
    - Can inspect, modify, or replace data before it's rendered
    - Useful for data enrichment, validation, filtering, and normalization
    - Full access to `ArgMatches` and `CommandContext` in hook functions
  - `HookError::post_dispatch()` factory method for creating post-dispatch errors
  - `HookPhase::PostDispatch` variant for error phase tracking
  - `serde_json` dependency added to `standout-clap` (previously dev-only)

- **Example**:

  ```rust
  use standout_clap::{Standout, Hooks, HookError};
  use serde_json::json;

  Standout::builder()
      .command("list", handler, template)
      .hooks("list", Hooks::new()
          .pre_dispatch(|_m, ctx| {
              println!("Running: {}", ctx.command_path.join(" "));
              Ok(())
          })
          .post_dispatch(|_m, _ctx, mut data| {
              // Add metadata before rendering
              if let Some(obj) = data.as_object_mut() {
                  obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
              }
              Ok(data)
          })
          .post_output(|_m, _ctx, output| {
              // Transform or inspect output
              Ok(output)
          }))
      .run_and_print(cmd, args);
  ```

## [0.7.1] - 2026-01-10

## [0.7.0] - 2026-01-10

- **Added**:
  - **Hook system for pre/post command execution** - Register custom callbacks that run before and after command handlers execute
    - `pre_dispatch` hooks: Run before command handler, can abort execution
    - `post_output` hooks: Run after output is generated, can transform output or abort
    - Multiple hooks can be chained at each phase
    - Full access to `ArgMatches` and `CommandContext` in hook functions
  - New `Output` enum for hook output handling:
    - `Output::Text(String)` - Text output from templates
    - `Output::Binary(Vec<u8>, String)` - Binary output with filename
    - `Output::Silent` - No output
  - `HookError` type with phase information (`PreDispatch` / `PostOutput`)
  - `Hooks::new()` builder with fluent `.pre_dispatch()` and `.post_output()` methods

- **Example**:

  ```rust
  use standout_clap::{Standout, Hooks, Output, HookError};

  Standout::builder()
      .command("list", handler, template)
      .hooks("list", Hooks::new()
          .pre_dispatch(|matches, ctx| {
              println!("Running: {}", ctx.command_path.join(" "));
              Ok(())
          })
          .post_output(|matches, ctx, output| {
              // Transform or inspect output
              Ok(output)
          }))
      .run_and_print(cmd, args);
  ```

## [0.6.2] - 2025-01-10

- **Changed**:
  - Code reorganization: split `lib.rs` into focused modules for both `standout` and `standout-clap` crates

## [0.6.1] - 2025-01-09

- **Changed**:
  - Switched to cargo-release for publishing

## [0.6.0] - 2025-01-09

- **Added**:
  - Tabular output support with `TableFormatter` and MiniJinja filters
  - Width resolution algorithm for responsive column layouts
  - ANSI-aware text manipulation utilities
  - `OutputMode::Json` for structured output
  - `render_or_serialize()` method for conditional rendering/serialization
  - Command handler system with `dispatch_from` convenience method
  - Archive variant support in clap integration

[7.6.2]: https://github.com/arthur-debert/standout/compare/standout-v7.6.1...standout-v7.6.2
[7.5.1]: https://github.com/arthur-debert/standout/compare/standout-v7.5.0...standout-v7.5.1
[7.5.0]: https://github.com/arthur-debert/standout/compare/standout-v7.4.0...standout-v7.5.0
[7.4.0]: https://github.com/arthur-debert/standout/compare/standout-v7.3.0...standout-v7.4.0
[7.3.0]: https://github.com/arthur-debert/standout/compare/standout-v7.2.0...standout-v7.3.0
[7.2.0]: https://github.com/arthur-debert/standout/compare/standout-v7.1.0...standout-v7.2.0
[7.1.0]: https://github.com/arthur-debert/standout/compare/standout-v7.0.0...standout-v7.1.0
[7.0.0]: https://github.com/arthur-debert/standout/compare/standout-v6.2.0...standout-v7.0.0
[6.2.0]: https://github.com/arthur-debert/standout/compare/standout-v6.1.0...standout-v6.2.0
[6.1.0]: https://github.com/arthur-debert/standout/compare/standout-v6.0.2...standout-v6.1.0
[6.0.2]: https://github.com/arthur-debert/standout/compare/standout-v6.0.1...standout-v6.0.2
[6.0.1]: https://github.com/arthur-debert/standout/compare/standout-v6.0.0...standout-v6.0.1
[6.0.0]: https://github.com/arthur-debert/standout/compare/standout-v5.0.0...standout-v6.0.0
[5.0.0]: https://github.com/arthur-debert/standout/compare/standout-v4.0.0...standout-v5.0.0
[4.0.0]: https://github.com/arthur-debert/standout/compare/standout-v3.8.0...standout-v4.0.0
[3.8.0]: https://github.com/arthur-debert/standout/compare/standout-v3.7.0...standout-v3.8.0
[3.7.0]: https://github.com/arthur-debert/standout/compare/standout-v3.6.1...standout-v3.7.0
[3.6.1]: https://github.com/arthur-debert/standout/compare/standout-v3.6.0...standout-v3.6.1
[3.6.0]: https://github.com/arthur-debert/standout/compare/standout-v3.5.0...standout-v3.6.0
[3.5.0]: https://github.com/arthur-debert/standout/compare/standout-v3.4.0...standout-v3.5.0
[3.4.0]: https://github.com/arthur-debert/standout/compare/standout-v3.3.0...standout-v3.4.0
[3.3.0]: https://github.com/arthur-debert/standout/compare/standout-v3.2.0...standout-v3.3.0
[3.2.0]: https://github.com/arthur-debert/standout/compare/standout-v3.1.0...standout-v3.2.0
[3.1.0]: https://github.com/arthur-debert/standout/compare/standout-v3.0.0...standout-v3.1.0
[3.0.0]: https://github.com/arthur-debert/standout/compare/standout-v2.1.0...standout-v3.0.0
[2.1.0]: https://github.com/arthur-debert/standout/compare/standout-v2.0.0...standout-v2.1.0
[1.1.0]: https://github.com/arthur-debert/standout/compare/standout-v1.0.0...standout-v1.1.0
[2.2.0]: https://github.com/arthur-debert/standout/compare/v2.1.2...v2.2.0
[2.1.2]: https://github.com/arthur-debert/standout/compare/v2.1.1...v2.1.2
[2.1.1]: https://github.com/arthur-debert/standout/compare/v2.1.0...v2.1.1
[2.0.0]: https://github.com/arthur-debert/standout/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/arthur-debert/standout/compare/v0.15.0...v1.0.0
[0.14.0]: https://github.com/arthur-debert/standout/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/arthur-debert/standout/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/arthur-debert/standout/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/arthur-debert/standout/compare/v0.10.1...v0.11.1
[0.10.1]: https://github.com/arthur-debert/standout/compare/v0.9.0...v0.10.1
[0.9.0]: https://github.com/arthur-debert/standout/compare/v0.7.2...v0.9.0
[0.7.2]: https://github.com/arthur-debert/standout/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/arthur-debert/standout/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/arthur-debert/standout/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/arthur-debert/standout/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/arthur-debert/standout/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/arthur-debert/standout/releases/tag/v0.6.0
