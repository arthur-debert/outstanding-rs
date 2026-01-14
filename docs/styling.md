# Styling Guide

Outstanding separates presentation from logic by using named styles. Instead of embedding ANSI codes in your output, you define styles by name and reference them in templates.

## Basic Concepts

### Themes

A `Theme` is a collection of named styles. Each style maps a name to formatting attributes (colors, bold, italic, etc.).

```rust
use outstanding::Theme;
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("error", Style::new().red().bold())
    .add("muted", Style::new().dim());
```

### Using Styles in Templates

Styles are applied in templates using tag syntax:

```
[title]{{ title }}[/title]
[error]Error:[/error] {{ message }}
[heading]Report[/heading]
```

See [Templates](templates.md) for complete template syntax.

## Style Aliasing

Styles can reference other styles by name, enabling layered styling architectures. This is useful for separating concerns:

```rust
let theme = Theme::new()
    // Visual layer - actual formatting
    .add("dim_text", Style::new().dim())
    .add("cyan_bold", Style::new().cyan().bold())

    // Presentation layer - UI concepts
    .add("muted", "dim_text")
    .add("accent", "cyan_bold")

    // Semantic layer - data meaning
    .add("timestamp", "muted")
    .add("heading", "accent");
```

The `add()` method accepts either:
- A `console::Style` for concrete formatting
- A `&str` or `String` to alias another style

### Three-Layer Pattern

This pattern separates styling into three layers:

1. **Semantic Layer** (used in templates): Names that describe the data
   - `"timestamp"`, `"username"`, `"error_message"`

2. **Presentation Layer**: Cross-cutting UI concepts
   - `"muted"`, `"highlighted"`, `"disabled"`, `"accent"`

3. **Visual Layer**: Actual colors and decorations
   - `Style::new().dim()`, `Style::new().red().bold()`

Benefits:
- Templates use meaningful names, not presentation details
- Consistent styling across the application
- Easy to iterate on visual design without touching templates
- Light/dark mode only needs different visual layer definitions

### Validation

Style aliases are validated at render time. Errors are reported for:
- **Dangling aliases**: Referencing a non-existent style
- **Cycles**: Circular references like `a -> b -> a`

```rust
// This will fail at render time
let theme = Theme::new()
    .add("orphan", "nonexistent");  // Error: unresolved alias

// Explicit validation for early error detection
theme.validate()?;
```

## Adaptive Themes

For light/dark mode support, use `AdaptiveTheme`:

```rust
use outstanding::{AdaptiveTheme, Theme};
use console::Style;

let light = Theme::new()
    .add("emphasis", Style::new().blue());

let dark = Theme::new()
    .add("emphasis", Style::new().cyan().bold());

let adaptive = AdaptiveTheme::new(light, dark);
```

Outstanding automatically detects the OS color preference (via the `dark-light` crate) and selects the appropriate theme.

## Available Style Attributes

Outstanding uses the `console` crate for styling. Available attributes:

### Colors (Foreground)
- `.black()`, `.red()`, `.green()`, `.yellow()`
- `.blue()`, `.magenta()`, `.cyan()`, `.white()`
- `.color256(n)` - 256-color palette
- `.bright()` - Use bright variant

### Colors (Background)
- `.on_black()`, `.on_red()`, `.on_green()`, `.on_yellow()`
- `.on_blue()`, `.on_magenta()`, `.on_cyan()`, `.on_white()`
- `.on_color256(n)` - 256-color palette
- `.on_bright()` - Use bright variant

### Text Attributes
- `.bold()` - Bold text
- `.dim()` - Dimmed text
- `.italic()` - Italic text
- `.underlined()` - Underlined text
- `.blink()` - Blinking text
- `.reverse()` - Swap foreground/background
- `.hidden()` - Hidden text
- `.strikethrough()` - Strikethrough text

### Chaining

Attributes can be chained:

```rust
Style::new().bold().red().on_white().underlined()
```

## Output Modes

Control how styles are rendered with `OutputMode`:

| Mode | Behavior |
|------|----------|
| `Auto` | Detect terminal capabilities (default) |
| `Term` | Always emit ANSI codes |
| `Text` | Plain text, no styling |
| `TermDebug` | Render as `[name]text[/name]` for debugging |

```rust
use outstanding::{render_with_output, OutputMode};

// Force plain text output
let plain = render_with_output(template, &data, theme, OutputMode::Text)?;

// Debug mode to see which styles are applied
let debug = render_with_output(template, &data, theme, OutputMode::TermDebug)?;
// Output: [title]Hello[/title]
```

## Missing Style Indicator

When a template references an undefined style, Outstanding prepends an indicator (default: `(!?)`) to help catch typos:

```
(!?) Hello  // "greeting" style not found
```

Customize or disable this:

```rust
use outstanding::Styles;

let styles = Styles::new()
    .missing_indicator("[MISSING]")  // Custom indicator
    .add("ok", Style::new().green());

// Or disable entirely
let styles = Styles::new()
    .missing_indicator("")  // No indicator
    .add("ok", Style::new().green());
```

## Best Practices

1. **Use semantic names**: Name styles after what they represent, not how they look
   - Good: `"timestamp"`, `"warning"`, `"selected"`
   - Avoid: `"red_text"`, `"bold_cyan"`

2. **Define all styles upfront**: Create a complete theme at startup to catch missing styles early

3. **Validate during development**: Call `theme.validate()` explicitly during development to catch alias errors before render time

4. **Use aliasing for consistency**: Define presentation-layer styles and alias semantic styles to them

5. **Keep visual layer small**: Most styles should be aliases; only define concrete `Style` values in the visual layer
