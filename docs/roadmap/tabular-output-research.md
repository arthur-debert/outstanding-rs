# Rust columnar terminal output: ecosystem survey and templating patterns

**No single Rust crate handles all columnar output requirements—interleaved output, middle truncation, and template-declarative layouts are notably absent.** The `comfy-table` and `tabled` crates provide the strongest foundations, with excellent ANSI awareness and width controls, but both render tables as complete units rather than supporting non-contiguous output. For a template-based CLI approach, building custom MiniJinja filters on top of lower-level primitives (`console`, `unicode-width`, `textwrap`) appears most practical, drawing design inspiration from Python Rich's column specification API and Starship's declarative format strings.

## Rust table crates ranked by requirements coverage

The table ecosystem splits into **full table renderers** (comfy-table, tabled, prettytable-rs), **grid layouters** (term_grid), and **columnar text formatters** (colonnade). Each addresses different use cases but shares common gaps.

| Feature | comfy-table | tabled | prettytable-rs | term_grid | colonnade |
|---------|:-----------:|:------:|:--------------:|:---------:|:---------:|
| **Maintained (2025)** | ✅ Active | ✅ Active | ⚠️ Abandoned | ⚠️ Stable | ⚠️ Stable |
| **ANSI-aware width** | ✅ via ansi-str | ⚠️ feature flag | ⚠️ Partial/broken | ❌ Manual | ✅ macerate() |
| **Fixed column widths** | ✅ | ✅ | ❌ | ⚠️ Different model | ✅ |
| **Min/max constraints** | ✅ | ✅ | ❌ | ❌ | ✅ |
| **Terminal auto-expand** | ✅ ContentArrangement | ✅ | ❌ | ✅ fit_into_width | ❌ |
| **End truncation** | ⚠️ Row height only | ✅ + custom suffix | ❌ | ❌ | ⚠️ Wrap only |
| **Start/middle truncation** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Interleaved output** | ❌ | ❌ | ❌ | ❌ | ⚠️ Returns lines |
| **Unicode/CJK handling** | ✅ grapheme-aware | ✅ | ⚠️ Old unicode-width | ⚠️ Manual | ✅ |
| **API ergonomics** | ✅ Builder pattern | ✅ Derive + builder | ⚠️ Macro-heavy | ❌ Imperative | ⚠️ Imperative |

**comfy-table** emerges as the most complete option with **53M+ downloads**, active maintenance, and proper grapheme handling as of v7.2. Its `custom_styling` feature flag enables external ANSI code handling with ~30-50% performance overhead. However, it wraps rather than truncates content, and tables render as atomic units.

**tabled** offers the most **truncation control** via `Width::truncate(10).suffix("...")` and derive macros for struct-based tables (`#[derive(Tabled)]`). Its `color` feature must be explicitly enabled for ANSI awareness—a common footgun.

**colonnade** uniquely returns `Vec<String>` lines rather than printing directly, making it the **closest option for interleaved output**—you could intersperse other content between rendered lines.

## Lower-level primitives form the real building blocks

For custom columnar output, these utility crates provide the essential operations:

| Crate | Purpose | Key functions |
|-------|---------|---------------|
| `console` | ANSI-aware measurement | `measure_text_width()`, `truncate_str()`, `pad_str()`, `strip_ansi_codes()` |
| `unicode-width` | Character width | `UnicodeWidthStr::width()`, `width_cjk()` |
| `textwrap` | Word wrapping | `wrap()`, `fill()`, SMAWK optimal-fit algorithm |
| `terminal_size` | Terminal dimensions | `terminal_size()` returns `(Width, Height)` |
| `ansi-width` | ANSI-aware width | `ansi_width()` skips CSI/OSC sequences |

The **console crate** (by Armin Ronacher, 8.1M monthly downloads) deserves special attention. Its `measure_text_width()` correctly handles ANSI escape sequences, and `pad_str()` respects display width for alignment:

```rust
use console::{measure_text_width, pad_str, Alignment};
let styled = "\x1b[31mred\x1b[0m";
let width = measure_text_width(styled);  // => 3, not 12
let padded = pad_str("text", 10, Alignment::Left, None);
```

**textwrap** provides sophisticated wrapping but is **not ANSI-aware by design**—strip codes first or accept mangled output.

## Critical gaps: what must be built

Three requirements have **no existing solution** in the Rust ecosystem:

**Interleaved/non-contiguous output**: Every table crate renders complete table units. For output like "header row → log message → data row → log message → footer," you must either build custom line-by-line rendering or use colonnade's line vector output and manually interleave.

**Start/middle truncation**: All truncation is end-only. Implementing `"...filename.txt"` or `"start...end"` requires custom code:

```rust
fn truncate_middle(s: &str, max_width: usize, ellipsis: &str) -> String {
    let width = console::measure_text_width(s);
    if width <= max_width { return s.to_string(); }
    let ellipsis_width = console::measure_text_width(ellipsis);
    let available = max_width.saturating_sub(ellipsis_width);
    let left = available / 2;
    let right = available - left;
    // Take `left` chars from start, `right` from end, join with ellipsis
}
```

**Template-declarative column layouts**: No crate separates column specification from rendering in a config-driven way. Columns are defined programmatically, not in TOML/YAML schemas or template syntax.

## Rich-like libraries exist but are nascent

**fast-rich** and **richrs** port Python Rich concepts to Rust with table rendering, styled text, and progress bars. fast-rich shows particular promise:

```rust
let mut table = Table::new();
table.add_column("Feature");
table.add_column("Status");
table.add_row_strs(&["Rich Text", "✅ Ready"]);
```

However, both remain alpha-quality with unstable APIs. **termimad** offers markdown-to-terminal rendering with table templates but follows a different paradigm than Rich's programmatic column control.

## Template engines lack columnar awareness

MiniJinja, Tera, and Handlebars-rust provide text templating without built-in column/table support. **MiniJinja** (also by Armin Ronacher) offers the best foundation with minimal dependencies and Jinja2 compatibility.

Built-in filters relevant to columnar output include `truncate`, `center`, `wordwrap`, and `indent`. The **minijinja-contrib** crate adds `filesizeformat` and enhanced truncation. However, **no alignment padding filters exist**—these must be custom:

```rust
fn pad_left(value: String, width: usize) -> String {
    format!("{:>width$}", value)
}
fn pad_right(value: String, width: usize) -> String {
    format!("{:<width$}", value)
}
env.add_filter("pad_left", pad_left);
env.add_filter("pad_right", pad_right);
```

**Starship** demonstrates the most sophisticated declarative CLI formatting in the Rust ecosystem, using TOML configuration with module-based format strings:

```toml
[directory]
style = "bg:#DA627D"
format = "[ $path ]($style)"
truncation_length = 3
truncation_symbol = "…/"
```

This `[text](style)` syntax pattern and per-module format strings provide a proven model for declarative terminal output.

## Template syntax patterns from outside Rust

Python Rich's column API offers the clearest specification model, supporting fixed widths, ratio-based flexible widths, min/max bounds, and overflow handling:

```python
table.add_column("Name", width=20, min_width=10, overflow="ellipsis")
table.add_column("Desc", ratio=2)  # Proportional, requires expand=True
```

The **most elegant template-native patterns** combine Liquid/Jinja2 filter chains with Rich-style parameters:

| Pattern | Syntax example | Use case |
|---------|----------------|----------|
| Fixed width | `{{ name \| width(20) }}` | Character count |
| Flexible | `{{ desc \| flex(2) }}` | Proportional to remaining |
| Fill remaining | `{{ status \| width("*") }}` | Take all available space |
| Bounded | `{{ title \| width("10..30") }}` | Min/max constraints |
| Truncation | `{{ text \| truncate(50, "...", position="middle") }}` | With ellipsis position |
| Alignment | `{{ value \| align("right") }}` | Left/center/right |

Go's template ecosystem uses printf-style formatting (`{{printf "%-20s" .Name}}`) which is powerful but cryptic. Docker and GitHub CLIs parse `--format "table {{.Field}}\t{{.Field}}"` where the `table` prefix triggers automatic column width calculation.

## Recommended implementation approach

Given the ecosystem gaps, a **hybrid strategy** makes most sense:

**For basic tables**: Use **comfy-table** or **tabled** directly. Both handle the common case well—fixed columns, auto-width, end truncation, ANSI styling. Enable the `color` feature for tabled; use `custom_styling` for comfy-table with external ANSI.

**For interleaved or template-driven output**: Build a custom renderer using these primitives:

```rust
// Recommended stack
terminal_size → Get terminal dimensions  
console → ANSI-aware width measurement, padding, truncation
unicode-width → Character width foundation (console uses this)
textwrap → Content wrapping (strip ANSI first if styled)
minijinja → Template rendering with custom filters
```

**For the template syntax**, implement a MiniJinja filter set covering the gaps:

```rust
// Core filters to implement
fn col_width(value: String, width: usize) -> String;
fn col_align(value: String, align: &str, width: usize) -> String;
fn col_truncate(value: String, max: usize, ellipsis: &str, position: &str) -> String;
fn ansi_strip(value: String) -> String;
fn ansi_style(value: String, style: &str) -> String;
```

**A declarative column schema** in TOML could bridge configuration and rendering:

```toml
[[columns]]
field = "name"
width = { min = 10, max = 40 }
align = "left"
truncate = { max = 38, ellipsis = "…", position = "end" }
style = "bold blue"

[[columns]]
field = "size"
width = 10
align = "right"
format = "{{ value | filesizeformat }}"
```

## Conclusion

The Rust ecosystem provides strong primitives but no complete solution for template-declared columnar output with interleaved content. **comfy-table** and **tabled** handle 80% of table use cases excellently. The remaining 20%—interleaved output, start/middle truncation, template-native column declarations—requires custom implementation.

The clearest path forward: extend MiniJinja with ANSI-aware column filters, drawing API patterns from Python Rich (width/ratio/overflow), syntax from Liquid/Jinja2 (filter chains with named parameters), and configuration patterns from Starship (TOML with format strings). The `console` crate provides the essential low-level operations for correct ANSI and Unicode handling. Building on these foundations avoids reimplementing solved problems while addressing the genuine gaps in declarative columnar output.