# The Styling System

`standout-render` uses a theme-based styling system where named styles are applied to content through bracket notation tags. Instead of embedding ANSI codes in your templates, you define semantic style names (`error`, `title`, `muted`) and let the theme decide the visual representation.

This separation provides several benefits:

- **Readability**: Templates use meaningful names, not escape codes
- **Maintainability**: Change colors in one place, update everywhere
- **Adaptability**: Themes can respond to light/dark mode automatically
- **Consistency**: Enforce visual hierarchy across your application

---

## Themes

A `Theme` is a named collection of styles. Each style maps a name (like `title` or `error`) to visual attributes (bold cyan, dim red, etc.).

### CSS Themes

Define styles in standard CSS syntax — a subset of CSS Level 3 tailored for terminals:

```css
/* theme.css */
.title {
    color: cyan;
    font-weight: bold;
}

.error {
    color: red;
    font-weight: bold;
}

.muted {
    opacity: 0.5;  /* maps to dim */
}

.success {
    color: green;
}

/* Shorthand works too */
.warning { color: yellow; }
```

Load CSS themes:

```rust
use standout_render::Theme;

let theme = Theme::from_css(css_content)?;
```

`Theme` parses a CSS string; reading the file is the caller's job:

```rust
let css = std::fs::read_to_string("styles/theme.css")?;
let theme = Theme::from_css(&css)?;
```

For a whole directory of themes with hot reload in debug builds, use
`AppBuilder::styles_dir` (see [App Configuration](../../../topics/app-configuration.md))
rather than reading a single file.

CSS gives you syntax highlighting in editors, linting tools, and familiarity for web developers.

### Programmatic Themes

Build themes in code using the builder pattern:

```rust
use standout_render::Theme;
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("error", Style::new().red().bold())
    .add("muted", Style::new().dim())
    .add("success", Style::new().green());
```

> **Legacy format:** YAML themes are still supported via `Theme::from_yaml()`. CSS is the recommended format for all new projects.

---

## Supported Attributes

### Colors

| Attribute | CSS Property | Description             |
| --------- | ------------ | ----------------------- |
| `fg`      | `color`      | Foreground (text) color |
| `bg`      | `background` | Background color        |

### Color Formats

```css
/* Named colors (16 ANSI colors) */
.example { color: red; }
.example { color: green; }
.example { color: cyan; }
.example { color: magenta; }
.example { color: yellow; }
.example { color: white; }
.example { color: black; }

/* Bright variants */
.example { color: bright_red; }
.example { color: bright_green; }

/* 256-color palette (0-255) */
.example { color: 208; }

/* RGB hex */
.example { color: #ff6b35; }
.example { color: #f63; }     /* shorthand */

/* Theme-relative cube colors */
.example { color: cube(60%, 20%, 0%); }
```

Cube colors express a position in a color cube whose 8 corners are the base ANSI
colors of the user's terminal theme. The same `cube(60%, 20%, 0%)` produces earthy
tones in Gruvbox, pastels in Catppuccin, and muted shades in Solarized.
Interpolation is done in CIE LAB space for perceptually uniform gradients.
Attach a palette to a theme with `Theme::with_palette()`.

### Text Attributes

| CSS Property                    | Effect            |
| ------------------------------- | ----------------- |
| `font-weight: bold`             | Bold text         |
| `opacity: 0.5`                  | Dimmed/faint text |
| `font-style: italic`            | Italic text       |
| `text-decoration: underline`    | Underlined text   |
| `text-decoration: blink`        | Blinking text     |
| `text-decoration: line-through` | Strikethrough     |
| `visibility: hidden`            | Hidden text       |

---

## Adaptive Styles (Light/Dark Mode)

Terminal applications run in both light and dark environments. A color that looks great on a dark background may be illegible on a light one. `standout-render` solves this with adaptive styles.

### How It Works

Instead of defining separate "light theme" and "dark theme" files, you define mode-specific overrides at the style level:

```css
.panel {
    font-weight: bold;
    color: gray;        /* Default/fallback */
}

@media (prefers-color-scheme: light) {
    .panel { color: black; }   /* Override for light mode */
}

@media (prefers-color-scheme: dark) {
    .panel { color: white; }   /* Override for dark mode */
}
```

When resolving `panel` in dark mode:

1. Start with base attributes (`bold`, `gray`)
2. Merge dark overrides (`white` replaces `gray`)
3. Result: bold white text

This is efficient: most styles (bold, italic, semantic colors like green/red) look fine in both modes. Only a handful need adjustment—typically foreground colors for contrast.

### Programmatic API

```rust
use standout_render::Theme;
use console::{Style, Color};

let theme = Theme::new()
    .add_adaptive(
        "panel",
        Style::new().bold(),                     // Base (shared)
        Some(Style::new().fg(Color::Black)),     // Light mode
        Some(Style::new().fg(Color::White)),     // Dark mode
    );
```

### Color Mode Detection

`standout-render` auto-detects the OS color scheme when the caller probes the process:

```rust
use standout_render::{ColorMode, TargetProperties};

let properties = TargetProperties::detect();
match properties.color_scheme {
    ColorMode::Light => println!("Light mode"),
    ColorMode::Dark => println!("Dark mode"),
}
```

`TargetProperties::detect()` is the one process probe, at the crate edge. Convenience wrappers call it then pass the result into `render_request`. Tests construct `TargetProperties` with an explicit `color_scheme` rather than installing a detector; `set_theme_detector` and the other detector override APIs are removed.

---

## Style Aliasing

Aliases let semantic names resolve to visual styles. This is useful when multiple concepts share the same appearance:

```rust
let theme = Theme::new()
    // Define the visual style once
    .add("title", Style::new().bold().cyan())
    // Aliases — pass a string to reference another style by name
    .add("commit-message", "title")
    .add("section-header", "title")
    .add("heading", "title");
```

Now `[commit-message]`, `[section-header]`, and `[heading]` all render identically to `[title]`.

Benefits:

- Templates use meaningful, context-specific names
- Visual changes propagate automatically
- Refactoring visual design doesn't touch templates

Aliases can chain: `a` → `b` → `c` → concrete style. Cycles are detected and rejected at load time.

---

## Unknown Style Tags

When a template references a style tag not defined in the active theme,
`standout-render` degrades it to unstyled text instead of failing the render.
`Term` and `Text` mode treat unknown tags identically; the difference between
them is only whether *known* tags render as ANSI (`Term`) or as plain text
(`Text`). What happens to an unknown tag depends on whether its open and close markers
are balanced:

- A **balanced pair**, `[unknown]x[/unknown]`, has its markers removed; the
  inner text `x` is kept as unstyled text.
- An **unbalanced** tag, `[unknown]` with no matching close, is emitted verbatim
  as literal text — the brackets survive, so the output contains `[unknown]`.
  This is why a stray `[compute]` appears verbatim under `--output text`.

`TermDebug` mode keeps every tag, known or unknown, as literal text for
inspection.

There is no `?` marker: an unknown tag is never rewritten to `[unknown?]` in
rendered output. Instead, each unresolved tag is recorded as a warning (see
[Unresolved-tag warning](#unresolved-tag-warning) below), whether it was
stripped or emitted verbatim.

A tag counts as unknown when it is absent from the *active theme's* resolved
style map. A tag your app defines in some themes but not the one currently
selected is unresolved in that theme and degrades the same way; the warning
does not distinguish a never-defined tag name from one merely missing in the
active theme.

To emit a literal `[` that must not be read as a tag, escape it as `\[` (and
`\]` for `]`). See [Literal brackets](templating.md#literal-brackets) in the
templating topic.

### Unresolved-tag warning

Each render pass records the tags it left unresolved as one warning line, naming
them all (sorted and de-duplicated):

```text
Unresolved style tag(s) degraded to unstyled text: compute, status
```

Where that warning goes depends on the entry point that drove the render:

- `App::run` writes it to stderr, after the command's own output.
- `App::run_with` and `App::dispatch` collect it into the returned
  `CompletedRun` and write nothing; the caller reads it with `.warnings()` and
  decides. `TestHarness` reads it this way.
- `App::render_with` and the standalone `standout-render` render APIs render
  with warnings disabled, so an unresolved tag degrades to unstyled text without
  recording or emitting anything.

An application whose specification pins its stderr bytes must account for the
`App::run` line. An escaped bracket (`\[`) is not a tag and raises no warning.

### Validation

For strict checking at startup:

```rust
use standout_render::validate_template;

if let Err(error) = validate_template(template, &sample_data, &theme) {
    eprintln!("Unknown style tag: {}", error);
    std::process::exit(1);
}
```

---

## Built-in Styles

`Theme::default()` includes adaptive styles for alternating table row backgrounds. These are used automatically when you pass `row_styles=true` (or a tint name) to the `table()` template function.

| Style name              | Purpose                                 |
| ----------------------- | --------------------------------------- |
| `table_row_even`        | Even rows — no background (transparent) |
| `table_row_odd`         | Odd rows — subtle gray background shift |
| `table_row_even_gray`   | Alias for `table_row_even`              |
| `table_row_odd_gray`    | Alias for `table_row_odd`               |
| `table_row_even_blue`   | Even rows for blue tint                 |
| `table_row_odd_blue`    | Odd rows — dark navy / lavender bg      |
| `table_row_even_red`    | Even rows for red tint                  |
| `table_row_odd_red`     | Odd rows — dark crimson / blush bg      |
| `table_row_even_green`  | Even rows for green tint                |
| `table_row_odd_green`   | Odd rows — dark forest / mint bg        |
| `table_row_even_purple` | Even rows for purple tint               |
| `table_row_odd_purple`  | Odd rows — dark plum / lilac bg         |

All odd-row styles are adaptive: they resolve to a dark variant when the terminal is in dark mode, and a light variant in light mode. You can override any of these by defining the same style name in your theme.

---

## Best Practices

### Semantic, Presentation, and Visual Layers

Organize your styles in three conceptual layers:

**1. Visual primitives** (low-level appearance):

```css
._cyan-bold { color: cyan; font-weight: bold; }
._dim { opacity: 0.5; }
._red-bold { color: red; font-weight: bold; }
```

**2. Presentation roles** (UI concepts — use aliases in code):

```rust
theme.add("heading", "_cyan-bold")
     .add("secondary", "_dim")
     .add("danger", "_red-bold");
```

**3. Semantic names** (domain concepts — aliases to presentation):

```rust
// In templates, use these
theme.add("task-title", "heading")
     .add("task-status-done", "success")
     .add("task-status-pending", "warning")
     .add("error-message", "danger");
```

Templates use semantic names (`task-title`), which resolve to presentation roles (`heading`), which resolve to visual primitives (`_cyan-bold`).

This layering lets you:

- Refactor visuals without touching templates
- Maintain consistency across domains
- Document the purpose of each style

### Naming Conventions

```css
/* Good: descriptive, semantic */
.error-message { ... }
.file-path { ... }
.command-name { ... }

/* Avoid: visual descriptions */
.red-text { ... }
.bold-cyan { ... }
```

### Keep Themes Focused

One theme per "look". Don't mix concerns:

```text
styles/
├── default.css          # your app's default look
├── colorblind.css       # accessibility variant
└── monochrome.css       # for piped output
```

---

## API Reference

### Theme Creation

```rust
// From CSS string
let theme = Theme::from_css(css_str)?;

// Empty theme (for programmatic building)
let theme = Theme::new();

// Legacy: YAML is still supported
let theme = Theme::from_yaml(yaml_str)?;
```

### Adding Styles

```rust
// Static style
theme.add("name", Style::new().bold());

// Adaptive style
theme.add_adaptive("name", base_style, light_override, dark_override);

// Alias
theme.add("alias", "target_style");
```

### Resolving Styles

```rust
// Get the mode-agnostic style
let style: Option<Style> = theme.get_style("title", None);

// Get style resolved for a specific mode
let style = theme.get_style("panel", Some(ColorMode::Dark));
```

### Color Mode

```rust
use standout_render::{ColorMode, TargetProperties};

// Auto-detect at the crate edge
let properties = TargetProperties::detect();
let mode = properties.color_scheme;

// Tests construct TargetProperties instead of installing a detector
let mut target = properties;
target.color_scheme = ColorMode::Light;
```
