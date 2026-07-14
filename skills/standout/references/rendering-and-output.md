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
| `json`, `yaml`, `xml`, `csv` | Direct serialization; template skipped | Parse or assert on data |

Unknown style tags gain a `?` marker in terminal mode, disappear in text mode, and remain literal in terminal-debug mode. Structured modes also skip injected template context.

Prefer `--output json` plus a parser whenever an agent needs facts rather than presentation. Use `--output text` when the rendered wording matters. Do not scrape ANSI output.

`--output-file-path=PATH` writes output to the file and suppresses duplicate stdout. Applications can rename or disable both output flags through `AppBuilder`.

Read `crates/standout-render/src/output.rs`, `crates/standout-render/docs/topics/templating.md`, `crates/standout-render/docs/topics/styling-system.md`, and `docs/topics/output-modes.md` for the detailed surface. If prose says output mode is in `CommandContext`, follow the current Rust type instead: it is a render-layer concern.
