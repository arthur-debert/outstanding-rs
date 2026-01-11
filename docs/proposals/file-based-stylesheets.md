# File-Based Stylesheets

This document describes the design for file-based stylesheet loading in Outstanding, including a redesign of how themes and display modes interact.

## Prerequisites

This proposal depends on the **Generic File Loader** (see [file-loader.md](./file-loader.md)).

The file loader provides:
- Directory registration with extension priority (`.yaml`, `.yml`)
- Name derivation from relative paths (e.g., `darcula.yaml` → theme name `"darcula"`)
- Collision detection across directories
- Dev mode (hot reload from disk) vs release mode (embedded content)

**What this proposal receives from the file loader:**

A `HashMap<String, String>` where:
- **Keys** are resource names (derived from filename, e.g., `"darcula"`)
- **Values** are raw YAML content strings

This proposal focuses on:
1. Parsing YAML content into style definitions
2. The adaptive styles design (light/dark mode at style level)
3. Theme API and integration with rendering

## Motivation

### Current Design Problems

The current implementation conflates themes, display modes, and styles:

```rust
// Current: Two separate themes duplicating most content
let light = Theme::new()
    .add("header", Style::new().cyan().bold())  // Same in both
    .add("muted", Style::new().dim())           // Same in both
    .add("footer", Style::new().black());       // Different

let dark = Theme::new()
    .add("header", Style::new().cyan().bold())  // Duplicated!
    .add("muted", Style::new().dim())           // Duplicated!
    .add("footer", Style::new().white());       // Different

let adaptive = AdaptiveTheme::new(light, dark);
```

**Problems:**
- **Duplication**: Non-varying styles repeated in both themes
- **Obscured intent**: Can't easily see what actually varies by mode
- **Conceptual confusion**: `AdaptiveTheme` is really "one theme with mode variants", not two themes
- **Maintenance burden**: Change a shared style? Edit two places.

### Proposed Design

Clean separation of concerns:

1. **Themes** are named collections of styles (e.g., "darcula", "solarized")
2. **Styles** are adaptive - individual styles define their mode-specific variations
3. **Display modes** (light/dark) are resolved at the style level, not theme level

```yaml
# darcula.yaml - ONE theme, styles adapt internally
header:
  fg: cyan
  bold: true

muted:
  dim: true

# Only footer varies by mode - and it's obvious!
footer:
  fg: gray
  light:
    fg: black
  dark:
    fg: white
```

**Benefits:**
- **Clear intent**: Instantly see that `footer` varies by mode, `header` doesn't
- **No duplication**: Shared attributes written once
- **Simpler mental model**: Theme = named style collection. Period.
- **Localized complexity**: Mode resolution happens during style loading

## Core Concepts

### Themes

A theme is simply a named collection of styles. The theme name comes from the filename:

```
styles/
  darcula.yaml      → theme "darcula"
  solarized.yaml    → theme "solarized"
  monokai.yaml      → theme "monokai"
```

Switching themes means unloading the current style definitions and loading the new ones.

### Adaptive Styles

Individual styles can define mode-specific overrides. The mental model is:

1. **Base style**: Define shared attributes at the style root
2. **Mode overrides**: `light:` and `dark:` sections override/extend the base

Mode-specific values are **merged onto** the base style - new keys are added, existing keys are overwritten.

```yaml
footer:
  fg: gray        # Base: used if no mode override
  bold: true      # Shared across all modes
  light:
    fg: black     # Overrides fg, inherits bold
  dark:
    fg: white     # Overrides fg, inherits bold
```

Resolution for `footer` in dark mode:
1. Start with base: `{ fg: gray, bold: true }`
2. Merge dark onto base: `{ fg: white, bold: true }`

### Display Modes

Outstanding detects the OS display mode (light/dark) automatically via the `dark-light` crate. When resolving a style:

1. Check if the style has a mode-specific variant
2. If yes, use the merged mode-specific style
3. If no, use the base style

## YAML Schema

### Basic Style Definition

```yaml
# Simple styles (no mode variation)
header:
  fg: cyan
  bold: true

muted:
  dim: true

# Shorthand for single attribute
bold_text: bold
accent: cyan
```

### Adaptive Style Definition

```yaml
# Style with mode-specific overrides
panel:
  bg: gray           # Base background
  fg: black          # Base foreground
  light:
    bg: "#f5f5f5"    # Light mode: lighter background
  dark:
    bg: "#1a1a1a"    # Dark mode: darker background
    fg: white        # Dark mode: also override foreground
```

The `light:` and `dark:` sections only need to specify what differs - shared attributes stay at the root.

### Supported Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `fg` | color | Foreground (text) color |
| `bg` | color | Background color |
| `bold` | bool | Bold text |
| `dim` | bool | Dimmed/faded text |
| `italic` | bool | Italic text |
| `underline` | bool | Underlined text |
| `blink` | bool | Blinking text (limited terminal support) |
| `reverse` | bool | Swap fg/bg colors |
| `hidden` | bool | Hidden text |
| `strikethrough` | bool | Strikethrough text |

### Color Formats

```yaml
# Named colors (16 ANSI colors)
error:
  fg: red
  bg: white

# Bright variants
warning:
  fg: bright_yellow

# 256-color palette
accent:
  fg: 208  # Orange

# RGB hex
brand:
  fg: "#ff6b35"

# RGB tuple
highlight:
  fg: [255, 107, 53]
```

**Named colors**: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`
**Bright variants**: `bright_black`, `bright_red`, `bright_green`, etc.

### Aliases

Aliases reference other styles by name:

```yaml
# Visual layer - concrete styles
muted:
  dim: true

accent:
  fg: cyan
  bold: true

# Semantic layer - aliases
disabled: muted
emphasized: accent
timestamp: disabled
title: emphasized
```

If a value is a string (not an object), it's an alias. Alias chains are resolved at load time.

### Shorthand Syntax

For simple styles:

```yaml
# Single attribute
bold_text: bold
dim_text: dim

# Single color (implies fg)
error: red
success: green

# Multiple attributes
header: "cyan bold"
warning: "yellow italic"
```

## Implementation Architecture

### Internal Theme Structure

Each theme maintains three style registries:

```rust
pub struct Theme {
    name: String,
    /// Base styles (always populated)
    base: Styles,
    /// Light mode overrides (merged onto base)
    light: Styles,
    /// Dark mode overrides (merged onto base)
    dark: Styles,
}
```

### Style Resolution

```rust
impl Theme {
    pub fn resolve_style(&self, name: &str, mode: ColorMode) -> Option<&Style> {
        let mode_styles = match mode {
            ColorMode::Light => &self.light,
            ColorMode::Dark => &self.dark,
        };

        // Try mode-specific first, fall back to base
        mode_styles.get(name).or_else(|| self.base.get(name))
    }
}
```

### Loading Process

When parsing a style definition like:

```yaml
footer:
  fg: gray
  bold: true
  light:
    fg: black
  dark:
    fg: white
```

1. **Extract base attributes**: `{ fg: gray, bold: true }`
2. **Build base style**: `Style::new().fg(gray).bold()`
3. **Merge light onto base**: `{ fg: black, bold: true }` → `Style::new().fg(black).bold()`
4. **Merge dark onto base**: `{ fg: white, bold: true }` → `Style::new().fg(white).bold()`

Store in:
- `base["footer"]` = base style
- `light["footer"]` = merged light style (only if light section exists)
- `dark["footer"]` = merged dark style (only if dark section exists)

### StyleDefinition (Intermediate Representation)

```rust
/// Parsed from YAML before conversion to console::Style
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StyleDefinition {
    /// Alias to another style
    Alias(String),
    /// Shorthand: "cyan bold" or just "bold"
    Shorthand(String),
    /// Full definition with optional mode overrides
    Full(FullStyleDef),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FullStyleDef {
    // Base attributes
    pub fg: Option<ColorDef>,
    pub bg: Option<ColorDef>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub dim: bool,
    #[serde(default)]
    pub italic: bool,
    // ... other attributes

    // Mode overrides
    pub light: Option<StyleAttributes>,
    pub dark: Option<StyleAttributes>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StyleAttributes {
    pub fg: Option<ColorDef>,
    pub bg: Option<ColorDef>,
    #[serde(default)]
    pub bold: Option<bool>,
    // ... (Option<T> to distinguish "not set" from "set to false")
}
```

### StylesheetRegistry

Decoupled from filesystem for testability:

```rust
pub struct StylesheetRegistry {
    /// Parsed definitions before style building
    definitions: HashMap<String, StyleDefinition>,
    /// Source file for error reporting
    sources: HashMap<String, PathBuf>,
}

impl StylesheetRegistry {
    pub fn new() -> Self;

    /// Parse YAML content and add definitions
    pub fn add_from_yaml(&mut self, content: &str, source: PathBuf) -> Result<(), StylesheetError>;

    /// Build a Theme from the definitions
    pub fn build_theme(&self, name: &str) -> Result<Theme, StylesheetError>;
}
```

## API Design

### Theme API

```rust
impl Theme {
    /// Creates an empty theme with the given name.
    pub fn new(name: &str) -> Self;

    /// Loads a theme from a YAML file. Theme name = filename without extension.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, StylesheetError>;

    /// Loads a theme from YAML content.
    pub fn from_yaml(name: &str, content: &str) -> Result<Self, StylesheetError>;

    /// Adds a style programmatically (for mixed file + code usage).
    pub fn add<V: Into<StyleValue>>(self, name: &str, value: V) -> Self;

    /// Adds an adaptive style programmatically.
    pub fn add_adaptive(
        self,
        name: &str,
        base: Style,
        light: Option<Style>,
        dark: Option<Style>,
    ) -> Self;

    /// Reloads the theme from disk (dev mode hot reload).
    pub fn refresh(&mut self) -> Result<(), StylesheetError>;

    /// Returns the theme name.
    pub fn name(&self) -> &str;
}
```

### Rendering API

```rust
/// Renders with automatic OS mode detection.
pub fn render<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
) -> Result<String, Error>;

/// Renders with explicit mode.
pub fn render_with_mode<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: ColorMode,
) -> Result<String, Error>;
```

### Breaking Changes

The following types are **removed**:
- `AdaptiveTheme` - themes are now inherently adaptive
- `ThemeChoice` - no longer needed, just pass `&Theme`

Migration:
```rust
// Before
let light = Theme::new().add("x", style1);
let dark = Theme::new().add("x", style2);
let adaptive = AdaptiveTheme::new(light, dark);
render(template, &data, ThemeChoice::Adaptive(&adaptive))

// After
let theme = Theme::from_yaml("app", r#"
x:
  light:
    fg: black
  dark:
    fg: white
"#)?;
render(template, &data, &theme)
```

## Dev vs Release Mode

### Development Mode

In development (`debug_assertions` enabled):
- YAML files are re-read from disk on each render
- Parse errors include file path and line number
- Style changes are immediately visible

### Release Mode

In release builds:
- Stylesheets can be embedded at compile time
- Use `embed_stylesheet!()` macro or build script
- No filesystem access needed at runtime

```rust
// Embed at compile time
let theme = Theme::from_embedded(
    "darcula",
    include_str!("../styles/darcula.yaml")
)?;
```

## Error Handling

```rust
pub enum StylesheetError {
    /// IO error reading file
    Io { path: PathBuf, source: std::io::Error },

    /// YAML parse error
    Parse { path: PathBuf, line: Option<usize>, message: String },

    /// Invalid color format
    InvalidColor { style: String, value: String, path: Option<PathBuf> },

    /// Unknown attribute in style definition
    UnknownAttribute { style: String, attribute: String, path: Option<PathBuf> },

    /// Invalid shorthand syntax
    InvalidShorthand { style: String, value: String, path: Option<PathBuf> },

    /// Alias validation error (dangling reference or cycle)
    AliasError { source: StyleValidationError },
}
```

## Examples

### Complete Theme File

```yaml
# styles/darcula.yaml

# === Visual Layer ===
# Base colors and attributes

muted:
  dim: true

accent:
  fg: cyan
  bold: true

# === Adaptive Styles ===
# These vary by display mode

background:
  light:
    bg: "#f8f8f8"
  dark:
    bg: "#1e1e1e"

text:
  light:
    fg: "#333333"
  dark:
    fg: "#d4d4d4"

border:
  dim: true
  light:
    fg: "#cccccc"
  dark:
    fg: "#444444"

# === Semantic Layer ===
# Aliases for application use

header: accent
footer: muted
timestamp: muted
title: accent
error: red
success: green
warning: "yellow bold"
```

### Usage in Code

```rust
// Load theme from file
let theme = Theme::from_file("./styles/darcula.yaml")?;

// Render - mode detected automatically
let output = render(
    r#"{{ title | style("title") }}: {{ message | style("text") }}"#,
    &data,
    &theme,
)?;

// Or force a specific mode
let dark_output = render_with_mode(template, &data, &theme, ColorMode::Dark)?;
```

### Development Workflow

```bash
# Edit stylesheet
vim styles/darcula.yaml

# Re-run - changes visible immediately
cargo run -- show-report
```

## Summary

This design:

1. **Simplifies the mental model**: Theme = named style collection
2. **Eliminates duplication**: Mode variations defined inline with shared attributes
3. **Makes intent clear**: Easy to see what varies by mode
4. **Localizes complexity**: Mode resolution in style loading, not scattered throughout
5. **Removes API clutter**: No more `AdaptiveTheme`, `ThemeChoice`
6. **Enables file-based workflows**: YAML definitions with hot reload
