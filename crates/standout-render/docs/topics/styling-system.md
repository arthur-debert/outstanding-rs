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

### YAML Themes

For file-based configuration, YAML provides a concise syntax:

```yaml
# theme.yaml
title:
  fg: cyan
  bold: true

error:
  fg: red
  bold: true

muted:
  dim: true

success:
  fg: green

# Shorthand: single attribute or space-separated
warning: yellow
emphasis: "bold italic"
```

Load YAML themes:

```rust
use standout_render::Theme;

// From string
let theme = Theme::from_yaml(yaml_content)?;

// From file (with hot reload in debug builds)
let theme = Theme::from_yaml_file("styles/theme.yaml")?;
```

### CSS Themes

For developers who prefer standard CSS syntax, `standout-render` supports a subset of CSS Level 3 tailored for terminals:

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
let theme = Theme::from_css_file("styles/theme.css")?;
```

> CSS is the recommended format for new projects. It enables syntax highlighting in editors, linting tools, and familiarity for web developers.

---

## Supported Attributes

### Colors

| Attribute | CSS Property | Description |
|-----------|--------------|-------------|
| `fg` | `color` | Foreground (text) color |
| `bg` | `background` | Background color |

### Color Formats

```yaml
# Named colors (16 ANSI colors)
fg: red
fg: green
fg: blue
fg: cyan
fg: magenta
fg: yellow
fg: white
fg: black

# Bright variants
fg: bright_red
fg: bright_green

# 256-color palette (0-255)
fg: 208

# RGB hex
fg: "#ff6b35"
fg: "#f63"      # shorthand

# RGB array
fg: [255, 107, 53]
```

### Text Attributes

| YAML | CSS | Effect |
|------|-----|--------|
| `bold: true` | `font-weight: bold` | Bold text |
| `dim: true` | `opacity: 0.5` | Dimmed/faint text |
| `italic: true` | `font-style: italic` | Italic text |
| `underline: true` | `text-decoration: underline` | Underlined text |
| `blink: true` | `text-decoration: blink` | Blinking text |
| `reverse: true` | — | Swap fg/bg colors |
| `hidden: true` | `visibility: hidden` | Hidden text |
| `strikethrough: true` | `text-decoration: line-through` | Strikethrough |

---

## Adaptive Styles (Light/Dark Mode)

Terminal applications run in both light and dark environments. A color that looks great on a dark background may be illegible on a light one. `standout-render` solves this with adaptive styles.

### How It Works

Instead of defining separate "light theme" and "dark theme" files, you define mode-specific overrides at the style level:

```yaml
panel:
  bold: true          # Shared across all modes
  fg: gray            # Default/fallback
  light:
    fg: black         # Override for light mode
  dark:
    fg: white         # Override for dark mode
```

When resolving `panel` in dark mode:
1. Start with base attributes (`bold: true`, `fg: gray`)
2. Merge dark overrides (`fg: white` replaces `fg: gray`)
3. Result: bold white text

This is efficient: most styles (bold, italic, semantic colors like green/red) look fine in both modes. Only a handful need adjustment—typically foreground colors for contrast.

### CSS Syntax

```css
.panel {
    font-weight: bold;
    color: gray;
}

@media (prefers-color-scheme: light) {
    .panel { color: black; }
}

@media (prefers-color-scheme: dark) {
    .panel { color: white; }
}
```

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

`standout-render` auto-detects the OS color scheme:

```rust
use standout_render::{detect_color_mode, ColorMode};

let mode = detect_color_mode();
match mode {
    ColorMode::Light => println!("Light mode"),
    ColorMode::Dark => println!("Dark mode"),
}
```

Override for testing:

```rust
use standout_render::set_theme_detector;

set_theme_detector(|| ColorMode::Dark);  // Force dark mode
```

---

## Style Aliasing

Aliases let semantic names resolve to visual styles. This is useful when multiple concepts share the same appearance:

```yaml
# Define the visual style once
title:
  fg: cyan
  bold: true

# Aliases
commit-message: title
section-header: title
heading: title
```

Now `[commit-message]`, `[section-header]`, and `[heading]` all render identically to `[title]`.

Benefits:

- Templates use meaningful, context-specific names
- Visual changes propagate automatically
- Refactoring visual design doesn't touch templates

Aliases can chain: `a` → `b` → `c` → concrete style. Cycles are detected and rejected at load time.

---

## Unknown Style Tags

When a template references a style not defined in the theme, `standout-render` handles it gracefully:

| Output Mode | Behavior |
|-------------|----------|
| `Term` | Unknown tags get a `?` marker: `[unknown?]text[/unknown?]` |
| `Text` | Tags stripped (plain text) |
| `TermDebug` | Tags preserved as-is |

The `?` marker helps catch typos during development without crashing production apps.

### Validation

For strict checking at startup:

```rust
use standout_render::validate_template;

let errors = validate_template(template, &sample_data, &theme);
if !errors.is_empty() {
    for error in errors {
        eprintln!("Unknown style: {}", error.tag_name);
    }
    std::process::exit(1);
}
```

---

## Best Practices

### Semantic, Presentation, and Visual Layers

Organize your styles in three conceptual layers:

**1. Visual primitives** (low-level appearance):
```yaml
_cyan-bold:
  fg: cyan
  bold: true

_dim:
  dim: true

_red-bold:
  fg: red
  bold: true
```

**2. Presentation roles** (UI concepts):
```yaml
heading: _cyan-bold
secondary: _dim
danger: _red-bold
```

**3. Semantic names** (domain concepts):
```yaml
# In templates, use these
task-title: heading
task-status-done: success
task-status-pending: warning
error-message: danger
```

Templates use semantic names (`task-title`), which resolve to presentation roles (`heading`), which resolve to visual primitives (`_cyan-bold`).

This layering lets you:
- Refactor visuals without touching templates
- Maintain consistency across domains
- Document the purpose of each style

### Naming Conventions

```yaml
# Good: descriptive, semantic
error-message: ...
file-path: ...
command-name: ...

# Avoid: visual descriptions
red-text: ...
bold-cyan: ...
```

### Keep Themes Focused

One theme per "look". Don't mix concerns:

```yaml
# theme-default.yaml - your app's default look
# theme-colorblind.yaml - accessibility variant
# theme-monochrome.yaml - for piped output
```

---

## API Reference

### Theme Creation

```rust
// Empty theme
let theme = Theme::new();

// From YAML string
let theme = Theme::from_yaml(yaml_str)?;

// From CSS string
let theme = Theme::from_css(css_str)?;

// From files (hot reload in debug)
let theme = Theme::from_yaml_file(path)?;
let theme = Theme::from_css_file(path)?;
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
// Get resolved style for current color mode
let style: Option<Style> = theme.get("title");

// Get style for specific mode
let style = theme.get_for_mode("panel", ColorMode::Dark);
```

### Color Mode

```rust
use standout_render::{detect_color_mode, set_theme_detector, ColorMode};

// Auto-detect
let mode = detect_color_mode();

// Override (for testing)
set_theme_detector(|| ColorMode::Light);
```
