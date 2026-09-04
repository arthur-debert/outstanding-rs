# Rendering and output

Standout renders human output in two passes:

```text
serializable data -> MiniJinja -> semantic [style] tags -> terminal style transform
```

Keep templates in files and style semantic tags with CSS:

```jinja
[title]Todos[/title]
{% for item in items %}
[{{ item.status }}]{{ item.title }}[/{{ item.status }}]
{% endfor %}
```

```css
.title { color: cyan; font-weight: bold; }
.done { color: green; }
.pending { color: yellow; }
```

`embed_templates!` accepts `.jinja`, `.jinja2`, `.j2`, `.stpl`, and `.txt`. `embed_styles!` accepts CSS plus legacy YAML. A stylesheet filename supplies its theme name. Prefer CSS and MiniJinja for new application code.

## Output modes

The global `--output` flag names a structured encoding; the human representation has no `--output` name and is what a bare invocation renders. Neither changes the handler's data:

| Representation | Result | Agent use |
| --- | --- | --- |
| none (bare invocation) | Template; ANSI per `--color` and the destination | Normal user output |
| `term-debug` | Template with tags preserved | Inspect style placement |
| `json`, `yaml`, `csv` | Direct serialization; template skipped | Parse or assert on data |
| `ndjson` | One compact JSON object per line: handler events, then `{"type":"result","data":…}` | Consume a stream while the command runs |

`--color auto|always|never` is the separate decision about escape sequences in human text; a structured encoding never carries them. `--no-pager` turns paging off for the run.

An unknown style tag degrades to unstyled text in the human representation and is recorded as a warning; term-debug keeps it literal. Structured encodings also skip injected template context.

`csv` takes flat records only: one map of scalars, or an array of them, one row each in declared column order. A nested value is a render error naming `CsvProjection`; declare the columns with a `CsvProjection` on the command when the handler data is not flat. There is no XML mode.

Under a structured mode a failure is a diagnostic document on stdout and the exit status is unchanged: `docs/topics/execution-outcomes.md` owns where each stream's bytes go per mode, `docs/topics/error-handling.md` the `Diagnostic` a handler returns, and `docs/topics/stability.md` the versioned document (`ContractSurface`, `Envelope`, `schema_version`). A successful run declares a nonzero status with `Output::with_exit_status` or `list_view(items).empty_exit_status(n)`.

A command whose result accrues while it runs takes `results: &mut Results<E>` as a third handler parameter and calls `results.emit(event)`; register it with `EventsFnHandler::new` or let `#[handler]` read the parameter. The human representation renders each event from `<name>.event` beside the command's template, one flushed write each; `ndjson` writes each event raw on its own line and the summary as the `result` record; `json`, `yaml` and `csv` build one document when the command ends — the events, the `result` record and the warning entries as an array, or the events as CSV rows with the summary not encoded. A handler never branches on the representation (`docs/topics/incremental-commands.md`).

Prefer `--output json` plus a parser whenever an agent needs facts rather than presentation. Use `--color never` on a bare invocation when the rendered wording matters. Do not scrape ANSI output.

`--output-file-path=PATH` writes output to the file and suppresses duplicate stdout, and wins over the pager. Applications rename or disable each of the four flags through `AppBuilder`, and name themselves with `AppBuilder::name` so `<NAME>_PAGER` is read before `PAGER`.

Read `crates/standout-render/src/output.rs`, `crates/standout-render/docs/topics/templating.md`, `crates/standout-render/docs/topics/styling-system.md`, and `docs/topics/output-modes.md` for the detailed surface.
