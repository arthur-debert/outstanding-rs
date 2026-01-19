# Introduction to Rendering

Terminal outputs have significant limitations: single font, single size, no graphics. But modern terminals provide many facilities like true colors, light/dark mode support, adaptive sizing, and more. Rich, helpful, and clear outputs are within reach.

The development reality explains why such output remains rare. From a primitive syntax born in the 1970s to the scattered ecosystem support, it's been a major effort to craft great outputs—and logically, it rarely makes sense to invest that time.

`standout-render` is designed to make crafting polished outputs a breeze by leveraging ideas, tools, and workflows from web applications—a domain in which rich interface authoring has evolved into the best model we've got. (But none of the JavaScript ecosystem chaos, rest assured.)

In this guide, we'll explore what makes great outputs and how `standout-render` helps you get there.

**See Also:**

- [Styling System](../topics/styling-system.md) - themes, adaptive attributes, CSS syntax
- [Templating](../topics/templating.md) - MiniJinja, style tags, processing modes
- [Introduction to Tabular](intro-to-tabular.md) - column layouts and tables

---

## What Polished Output Entails

If you're building your CLI in Rust, chances are it's not a throwaway grep-formatting script—if that were the case, nothing beats shells. More likely, your program deals with complex data, logic, and computation, and the full power of Rust matters. In the same way, clear, well-presented, and designed outputs improve your users' experience when parsing that information.

Creating good results depends on discipline, consistency, and above all, experimentation—from exploring options to fine-tuning small details. Unlike code, good layout is experimental and takes many iterations: change, view result, change again, judge the new change, and so on.

The classical setup for shell UIs is anything but conducive to this. All presentation is mixed with code, often with complicated logic, if not coupled to it. Additionally, from escape codes to whitespace handling to spreading visual information across many lines of code, it becomes hard to visualize and change things.

The edit-code-compile-run cycle makes small tweaks take minutes. Sometimes a full hour for a minor change. In that scenario, it's no surprise that people don't bother.

---

## Our Example: A Report Generator

We'll use a simple report generator to demonstrate the rendering layer. Here's our data:

```rust
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pending, Done }

#[derive(Clone, Serialize)]
pub struct Task {
    pub title: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct Report {
    pub message: Option<String>,
    pub tasks: Vec<Task>,
}
```

Our goal: transform this raw data into polished, readable output that adapts to the terminal, respects user preferences, and takes minutes to iterate on—not hours.

---

## The Separation Principle

`standout-render` is designed around a strict separation of data and presentation. This isn't just architectural nicety—it unlocks a fundamentally better workflow.

### Without Separation

Here's the typical approach, tangling logic and output:

```rust
fn print_report(tasks: &[Task]) {
    println!("\x1b[1;36mYour Tasks\x1b[0m");
    println!("──────────");
    for (i, task) in tasks.iter().enumerate() {
        let marker = if matches!(task.status, Status::Done) { "[x]" } else { "[ ]" };
        println!("{}. {} {}", i + 1, marker, task.title);
    }
    println!("\n{} tasks total", tasks.len());
}
```

Problems:

- Escape codes are cryptic and error-prone
- Changes require recompilation
- Logic and presentation are intertwined
- Testing is brittle
- No easy way to support multiple output formats

### With Separation

The same output, properly separated:

```rust
use standout_render::{render, Theme};
use console::Style;

// Data preparation (your logic layer)
let report = Report {
    message: Some(format!("{} tasks total", tasks.len())),
    tasks,
};

// Theme definition (can be in a separate CSS/YAML file)
let theme = Theme::new()
    .add("title", Style::new().cyan().bold())
    .add("done", Style::new().green())
    .add("pending", Style::new().yellow())
    .add("muted", Style::new().dim());

// Template (can be in a separate .jinja file)
let template = r#"
[title]Your Tasks[/title]
──────────
{% for task in tasks %}
[{{ task.status }}]{{ task.status }}[/{{ task.status }}]  {{ task.title }}
{% endfor %}

{% if message %}[muted]{{ message }}[/muted]{% endif %}
"#;

let output = render(template, &report, &theme)?;
print!("{}", output);
```

Now:

- Logic is testable without output concerns
- Presentation is declarative and readable
- Styles are centralized and named semantically
- Changes to appearance don't require recompilation (with file-based templates)

---

## Quick Iteration and Workflow

The separation principle enables a radically better workflow. Here's what `standout-render` provides:

### 1. File-Based Flow

Dedicated files for templates and styles:

- Lower risk of breaking code—especially relevant for non-developer types like technical designers
- Simpler diffs and easier navigation
- Trivial to experiment with variations (duplicate files, swap names)

**Directory structure:**

```text
src/
├── main.rs
└── templates/
    └── report.jinja
styles/
└── default.css
```

### 2. Hot Live Reload

During development, you edit the template or styles and re-run. No compilation. No long turnaround.

This changes the entire experience. You can make and verify small adjustments in seconds. You can extensively fine-tune output quickly, then polish the full app in a focused session. Time efficiency aside, the quick iterative cycles encourage caring about smaller details, consistency—the things you forgo when iteration is painful.

(When released, files can be compiled into the binary using embedded macros, costing no performance or path-handling headaches in distribution.)

See [File System Resources](../topics/file-system-resources.md) for details on how hot reload works.

---

## Best-of-Breed Specialized Formats

### Templates: MiniJinja

`standout-render` uses MiniJinja templates—a Rust implementation of Jinja2, a de facto standard for rich and powerful templating. The simple syntax and powerful features let you map template text to actual output much easier than `println!` spreads.

```jinja
{% if message %}[accent]{{ message }}[/accent]{% endif %}

{% for task in tasks %}
[{{ task.status }}]{{ task.status | upper }}[/{{ task.status }}]  {{ task.title }}
{% endfor %}
```

Benefits:

- Simple, readable syntax
- Powerful control flow (loops, conditionals, filters)
- **Partials support**: templates can include other templates, enabling reuse
- **Custom filters**: for complex presentation needs, write small bits of code and keep templates clean

See [Templating](../topics/templating.md) for template filters and advanced usage.

### Styles: CSS Themes

The styling layer uses CSS files with the familiar syntax you already know, but with simpler semantics tailored for terminals:

```css
.title {
    color: cyan;
    font-weight: bold;
}

.done { color: green; }
.blocked { color: red; }
.pending { color: yellow; }

/* Adaptive for light/dark mode */
@media (prefers-color-scheme: light) {
    .panel { color: black; }
}

@media (prefers-color-scheme: dark) {
    .panel { color: white; }
}
```

Features:

- **Adaptive attributes**: a style can render different values for light and dark modes
- **Theming support**: swap the entire visual appearance at once
- **True color**: RGB values for precise colors (`#ff6b35` or `[255, 107, 53]`)
- **Aliases**: semantic names resolve to visual styles (`commit-message: title`)

YAML syntax is also supported as an alternative. See [Styling System](../topics/styling-system.md) for complete style options.

---

## Template Integration with Styling

Styles are applied with BBCode-like syntax: `[style]content[/style]`. A familiar, simple, and accessible form.

```jinja
[title]Your Tasks[/title]
{% for task in tasks %}
[{{ task.status }}]{{ task.title }}[/{{ task.status }}]
{% endfor %}
```

Style tags:

- Nest properly: `[outer][inner]text[/inner][/outer]`
- Can span multiple lines
- Can contain template logic: `[title]{% if x %}{{ x }}{% endif %}[/title]`

### Output Modes: Rich, Plain, and Debug

`standout-render` processes style tags differently based on the output mode:

```rust
use standout_render::{render_with_output, OutputMode};

// Rich terminal output (ANSI codes)
let rich = render_with_output(template, &data, &theme, OutputMode::Term)?;

// Plain text (strips style tags)
let plain = render_with_output(template, &data, &theme, OutputMode::Text)?;

// Debug mode (keeps tags visible)
let debug = render_with_output(template, &data, &theme, OutputMode::TermDebug)?;
```

**Single template for rich and plain text.** The same template serves both—no duplication needed.

```rust
// Auto-detect based on terminal capabilities
let output = render_with_output(template, &data, &theme, OutputMode::Auto)?;
```

In auto mode:
- TTY with color support → rich output
- Pipe or redirect → plain text

> **For standout framework users:** The framework's `--output` flag automatically sets the output mode. See the standout documentation for CLI integration.

### Debug Mode

Use `OutputMode::TermDebug` for debugging:

```text
[title]Your Tasks[/title]
[pending]pending[/pending]  Implement auth
[done]done[/done]  Fix tests
```

Style tags remain visible, making it easy to verify correct placement. Useful for testing and automation tools.

---

## Tabular Layout

Many outputs are lists of things—log entries, servers, tasks. These benefit from vertically aligned layouts. Aligning fields seems simple at first, but when you factor in ANSI awareness, flexible size ranges, wrapping behavior, truncation, justification, and expanding cells, it becomes really hard.

Tabular gives you a declarative API, both in Rust and in templates, that handles all of this:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([loop.index, task.status | style_as(task.status), task.title]) }}
{% endfor %}
```

Output adapts to terminal width:

```text
1.    pending     Implement user authentication
2.    done        Review pull request #142
3.    pending     Update dependencies
```

Features:

- Fixed, range, fill, and fractional widths
- Truncation (start, middle, end) with custom ellipsis
- Word wrapping for long content
- Per-column styling
- Automatic field extraction from structs

See [Introduction to Tabular](intro-to-tabular.md) for a comprehensive walkthrough.

---

## Structured Output

Beyond textual output, `standout-render` supports structured formats:

```rust
use standout_render::{render_auto, OutputMode};

// For Term/Text: renders template
// For Json/Yaml/etc: serializes data directly
let json_output = render_auto(template, &data, &theme, OutputMode::Json)?;
let yaml_output = render_auto(template, &data, &theme, OutputMode::Yaml)?;
```

**Structured output for free.** Because your data is `Serialize`-able, JSON/YAML outputs work automatically. Automation (tests, scripts, other programs) no longer needs to reverse-engineer data from formatted output.

Same data types—different output format. This enables API-like behavior from CLI apps without writing separate code paths.

---

## Putting It All Together

Here's a complete example:

```rust
use standout_render::{render, Theme};
use console::Style;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pending, Done }

#[derive(Clone, Serialize)]
pub struct Task {
    pub title: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct Report {
    pub message: Option<String>,
    pub tasks: Vec<Task>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::from_yaml(r#"
        title: { fg: cyan, bold: true }
        done: green
        pending: yellow
        muted: { dim: true }
    "#)?;

    let tasks = vec![
        Task { title: "Implement user authentication".into(), status: Status::Pending },
        Task { title: "Review pull request #142".into(), status: Status::Done },
        Task { title: "Update dependencies".into(), status: Status::Pending },
    ];

    let pending_count = tasks.iter()
        .filter(|t| matches!(t.status, Status::Pending))
        .count();

    let report = Report {
        message: Some(format!("{} pending", pending_count)),
        tasks,
    };

    let template = r#"
[title]My Tasks[/title]

{% for task in tasks %}
{{ loop.index }}.  [{{ task.status }}]{{ task.status }}[/{{ task.status }}]  {{ task.title }}
{% endfor %}

{% if message %}[muted]{{ message }}[/muted]{% endif %}
"#;

    let output = render(template, &report, &theme)?;
    print!("{}", output);
    Ok(())
}
```

**Output (terminal):**

```text
My Tasks

1.  pending  Implement user authentication
2.  done     Review pull request #142
3.  pending  Update dependencies

2 pending
```

With colors, "pending" appears yellow, "done" appears green.

---

## Summary

`standout-render` transforms CLI output from a chore into a pleasure:

1. **Separation of concerns**: Data stays separate from templates. Templates define structure. Styles control appearance.

2. **Fast iteration**: Hot reload means edit-and-see in seconds, not minutes. This changes what's practical.

3. **Familiar tools**: MiniJinja for templates (Jinja2 syntax), CSS or YAML for styles. No new languages to learn.

4. **Graceful degradation**: One template serves rich terminals, plain pipes, and everything in between.

5. **Structured output for free**: JSON, YAML outputs work automatically from your serializable types.

6. **Tabular layouts**: Declarative column definitions handle alignment, wrapping, truncation, and ANSI-awareness.

The rendering system makes it practical to care about details. When iteration is fast and changes are safe, polish becomes achievable—not aspirational.

For complete API details, see the [API documentation](https://docs.rs/standout-render).
