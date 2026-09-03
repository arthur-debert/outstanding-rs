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

The global `--output` flag chooses the view without changing handler data:

| Mode | Result | Agent use |
| --- | --- | --- |
| `auto` | Template; ANSI only when supported | Normal user output |
| `term` | Template with forced ANSI | Explicit colored output |
| `text` | Template with tags stripped | Stable rendered assertions |
| `term-debug` | Template with tags preserved | Inspect style placement |
| `json`, `yaml`, `csv` | Direct serialization; template skipped | Parse or assert on data |
| `ndjson` | One compact JSON object per line: handler entries, then `{"type":"result","data":…}` | Consume a stream while the command runs |

An unknown style tag degrades to unstyled text in terminal and text modes and is recorded as a warning; terminal-debug keeps it literal. Structured modes also skip injected template context.

`csv` takes flat records only: one map of scalars, or an array of them, one row each in declared column order. A nested value is a render error naming `CsvProjection`; declare the columns with a `CsvProjection` on the command when the handler data is not flat. There is no XML mode.

Under a structured mode a failure is a diagnostic document on stdout and the exit status is unchanged: `docs/topics/execution-outcomes.md` owns where each stream's bytes go per mode, `docs/topics/error-handling.md` the `Diagnostic` a handler returns, and `docs/topics/stability.md` the versioned document (`ContractSurface`, `Envelope`, `schema_version`). A successful run declares a nonzero status with `Output::with_exit_status` or `list_view(items).empty_exit_status(n)`.

A command whose result accrues while it runs takes `results: &mut Results<E>` as a third handler parameter and calls `results.emit(event)`; register it with `EventsFnHandler::new` or let `#[handler]` read the parameter. The human representation renders each event from `<name>.event` beside the command's template, one flushed write each; `ndjson` writes each event raw on its own line and the summary as the `result` record; `json`, `yaml` and `csv` carry no events yet and are a render error. A handler never branches on the representation (`docs/topics/output-modes.md`, "Incremental Commands").

Prefer `--output json` plus a parser whenever an agent needs facts rather than presentation. Use `--output text` when the rendered wording matters. Do not scrape ANSI output.

`--output-file-path=PATH` writes output to the file and suppresses duplicate stdout. Applications can rename or disable both output flags through `AppBuilder`.

Read `crates/standout-render/src/output.rs`, `crates/standout-render/docs/topics/templating.md`, `crates/standout-render/docs/topics/styling-system.md`, and `docs/topics/output-modes.md` for the detailed surface.
