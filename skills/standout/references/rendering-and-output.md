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

`embed_templates!` accepts `.jinja`, `.jinja2`, `.j2`, and `.txt`; runtime template loading additionally accepts `.stpl`. `embed_styles!` accepts CSS plus legacy YAML. A stylesheet filename supplies its theme name. Prefer CSS and MiniJinja for new application code.

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

Under every structured mode a failure is a **diagnostic document** on stdout (`{"type":"diagnostic","schema_version":1,"severity":"error","kind":"handler","summary":…,"detail":…}`, optional `range`) and the framework writes no error prose to stderr; an `AppFailure` or `ExternalFailure` still writes its verbatim bytes to stderr and adds the document with those bytes as `detail`, and `json`/`yaml`/`csv` keep warnings as stderr prose. The exit status is unchanged (`1` for a failure, `2` for a usage error). Human modes keep `Error: …` prose on stderr. A handler returns `Diagnostic::error(summary).detail(…).range(file, line, col).into()` to fill `detail` and `range`. A `ContractSurface` view (`#[derive(ContractSurface)]`, `#[contract(schema_version = N)]`) returned as `Output::Render(view.envelope())` serializes as `{"schema_version":N,"data":…}`; framework documents (`ListViewResult`, help under `json`/`yaml`, the diagnostic) carry `schema_version` as a top-level key. A successful run declares a nonzero status with `Output::Render(data).with_exit_status(ExitStatus::from(n))` or `list_view(items).empty_exit_status(n).output()`; nothing becomes a diagnostic.

Under `ndjson`, `ctx.stream().emit(&entry)` writes one line and flushes; in every other mode it does nothing, so emit unconditionally. `ctx.stream().is_live()` is the one mode predicate a handler takes: return `Output::Silent` when the entries are the whole result. A failure is one diagnostic line where the run failed; warnings are `severity: warning` entries of kind `framework`; binary and artifact output are render errors.

Prefer `--output json` plus a parser whenever an agent needs facts rather than presentation. Use `--output text` when the rendered wording matters. Do not scrape ANSI output.

`--output-file-path=PATH` writes output to the file and suppresses duplicate stdout. Applications can rename or disable both output flags through `AppBuilder`.

Read `crates/standout-render/src/output.rs`, `crates/standout-render/docs/topics/templating.md`, `crates/standout-render/docs/topics/styling-system.md`, and `docs/topics/output-modes.md` for the detailed surface. If prose says output mode is in `CommandContext`, follow the current Rust type instead: it is a render-layer concern.
