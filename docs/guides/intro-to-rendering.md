# Introduction to Rendering

Terminal outputs have significant limitations: single font, single size, no graphics. But modern terminals provide many facilities like true colors, light/dark mode support, adaptive sizing, and more. Rich, helpful, and clear outputs are within reach.

The development reality explains why such output remains rare. From a primitive syntax born in the 1970s to the scattered ecosystem support, it's been a major effort to craft great outputs—and logically, it rarely makes sense to invest that time.

In the past few years, we've made rapid progress. Interactive TUIs have a rich and advanced ecosystem. For non-interactive, textual outputs, we've certainly come far with good crates and tools, but it's still sub-par.

Outstanding's rendering layer is designed to make crafting polished outputs a breeze by leveraging ideas, tools, and workflows from web applications—a domain in which rich interface authoring has evolved into the best model we've got. (But none of the JavaScript ecosystem chaos, rest assured.)

In this guide, we'll explore what makes great outputs and how Outstanding helps you get there.

**See Also:**

- [Rendering System](rendering-system.md) - complete rendering API reference
- [Output Modes](output-modes.md) - all output format options
- [Full Tutorial](full-tutorial.md) - end-to-end adoption guide

---

## What Polished Output Entails

If you're building your CLI in Rust, chances are it's not a throwaway grep-formatting script—if that were the case, nothing beats shells. More likely, your program deals with complex data, logic, and computation, and the full power of Rust matters. In the same way, clear, well-presented, and designed outputs improve your users' experience when parsing that information.

Creating good results depends on discipline, consistency, and above all, experimentation—from exploring options to fine-tuning small details. Unlike code, good layout is experimental and takes many iterations: change, view result, change again, judge the new change, and so on.

The classical setup for shell UIs is anything but conducive to this. All presentation is mixed with code, often with complicated logic, if not coupled to it. Additionally, from escape codes to whitespace handling to spreading visual information across many lines of code, it becomes hard to visualize and change things.

The edit-code-compile-run cycle makes small tweaks take minutes. Sometimes a full hour for a minor change. In that scenario, it's no surprise that people don't bother.

---

## Our Example: A Task Manager

We'll use a simple task manager CLI to demonstrate the rendering layer. Here's our data:

```rust
#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: String,  // "pending", "done", "blocked"
    pub due: String,
}

#[derive(Serialize)]
pub struct TaskListResult {
    pub tasks: Vec<Task>,
    pub summary: String,
}
```

Our goal: transform this raw data into polished, readable output that adapts to the terminal, respects user preferences, and takes minutes to iterate on—not hours.

---

## The Separation Principle

Outstanding is designed around a strict separation of logic and presentation. This isn't just architectural nicety—it unlocks a fundamentally better workflow.

### Without Separation

Here's the typical approach, tangling logic and output:

```rust
fn list_tasks(show_all: bool) {
    let tasks = storage::list().unwrap();
    println!("\x1b[1;36mYour Tasks\x1b[0m");
    println!("──────────");
    for task in &tasks {
        if show_all || task.status != "done" {
            let color = match task.status.as_str() {
                "done" => "\x1b[32m",
                "blocked" => "\x1b[31m",
                _ => "\x1b[33m",
            };
            println!("{}[{}]\x1b[0m {} - {}", color, task.status, task.title, task.due);
        }
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
// Handler: pure logic, returns data
pub fn list(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TaskListResult> {
    let show_all = matches.get_flag("all");
    let tasks = storage::list()?;

    let filtered: Vec<Task> = if show_all {
        tasks
    } else {
        tasks.into_iter().filter(|t| t.status != "done").collect()
    };

    Ok(Output::Render(TaskListResult {
        summary: format!("{} tasks total", filtered.len()),
        tasks: filtered,
    }))
}
```

```jinja
{# Template: list.jinja #}
[title]Your Tasks[/title]
──────────
{% for task in tasks %}
[{{ task.status }}][{{ task.status }}][/{{ task.status }}] {{ task.title }} - [muted]{{ task.due }}[/muted]
{% endfor %}

[muted]{{ summary }}[/muted]
```

```yaml
# Styles: theme.yaml
title:
  fg: cyan
  bold: true
done: green
blocked: red
pending: yellow
muted:
  dim: true
```

Now:

- Logic is testable without output concerns
- Presentation is declarative and readable
- Styles are centralized and named semantically
- Changes to appearance don't require recompilation

---

## Quick Iteration and Workflow

The separation principle enables a radically better workflow. Here's what Outstanding provides:

### 1. File-Based Flow

Dedicated files for templates and styles:

- Lower risk of breaking code—especially relevant for non-developer types like technical designers
- Simpler diffs and easier navigation
- Trivial to experiment with variations (duplicate files, swap names)

**Directory structure:**

```text
src/
├── handlers.rs        # Logic
└── templates/
    └── list.jinja     # Content template
styles/
└── default.yaml       # Visual styling
```

### 2. Hot Live Reload

During development, you edit the template or styles and re-run. No compilation. No long turnaround.

This changes the entire experience. You can make and verify small adjustments in seconds. You can extensively fine-tune a command output quickly, then polish the full app in a focused session. Time efficiency aside, the quick iterative cycles encourage caring about smaller details, consistency—the things you forgo when iteration is painful.

(When released, files are compiled into the binary, costing no performance or path-handling headaches in distribution.)

See [Rendering System](rendering-system.md#hot-reloading) for details on how hot reload works.

---

## Best-of-Breed Specialized Formats

### Templates: MiniJinja

Outstanding uses MiniJinja templates—a Rust implementation of Jinja, a de facto standard for rich and powerful templating. The simple syntax and powerful features let you map template text to actual output much easier than `println!` spreads.

```jinja
{% if message %}[accent]{{ message }}[/accent]{% endif %}

{% for task in tasks %}
[{{ task.status }}]{{ task.status | upper }}[/{{ task.status }}]  {{ task.title }}
{% endfor %}
```

Benefits:

- Simple, readable syntax
- Powerful control flow (loops, conditionals, filters)
- **Partials support**: templates can include other templates, enabling reuse across commands
- **Custom filters**: for complex presentation needs, write small bits of code and keep templates clean

See [Rendering System](rendering-system.md) for template filters and context injection.

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
- **Theming support**: swap the entire visual appearance at once, with themes extending other themes
- **True color**: RGB values for precise colors (`#ff6b35` or `[255, 107, 53]`)
- **Aliases**: semantic names resolve to visual styles (`commit-message: title`)

YAML syntax is also supported as an alternative. See [Rendering System](rendering-system.md#themes-and-styles) for complete style options.

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

### Graceful Degradation

**Single template for rich and plain text.** Outstanding degrades gracefully based on terminal capabilities:

```bash
myapp list              # Rich colors (if terminal supports)
myapp list > file.txt   # Plain text (not a TTY)
myapp list | less       # Plain text (pipe)
```

No separate templates for different output modes. The same template serves both.

### Debug Mode

Override auto behavior with `--output=term-debug` for debugging:

```text
[title]Your Tasks[/title]
[pending]pending[/pending]  Implement auth
[done]done[/done]  Fix tests
```

Style tags remain visible, making it easy to verify correct placement. Useful for testing and automation tools.

See [Output Modes](output-modes.md) for all available output formats.

---

## Tabular Layout

Many commands output lists of things—log entries, servers, tasks. These benefit from vertically aligned layouts. Aligning fields seems simple at first, but when you factor in ANSI awareness, flexible size ranges, wrapping behavior, truncation, justification, and expanding cells, it becomes really hard. Those one-off bugs that drive you mad—yeah, those.

Tabular gives you a declarative API, both in Rust and in templates, that handles all of this:

```jinja
{% set t = tabular([
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"},
    {"name": "due", "width": 10, "align": "right"}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([task.status | style_as(task.status), task.title, task.due]) }}
{% endfor %}
```

Output adapts to terminal width:

```text
pending     Implement user authentication    2024-01-20
blocked     Fix payment gateway timeout      2024-01-18
done        Review pull request #142         2024-01-15
```

Features:

- Fixed, range, fill, and fractional widths
- Truncation (start, middle, end) with custom ellipsis
- Word wrapping for long content
- Per-column styling
- Automatic field extraction from structs

See [Introduction to Tabular](intro-to-tabular.md) for a comprehensive walkthrough.

---

## Output Control

Outstanding supports various output formats at runtime with the `--output` option:

```bash
myapp list                    # Auto: rich or plain based on terminal
myapp list --output=term      # Force rich terminal output
myapp list --output=text      # Force plain text
myapp list --output=term-debug # Show style tags for debugging
myapp list --output=json      # JSON serialization
myapp list --output=yaml      # YAML serialization
myapp list --output=csv       # CSV serialization
```

**Structured output for free.** Because your handler returns a `Serialize`-able type, JSON/YAML/CSV outputs work automatically. Automation (tests, scripts, other programs) no longer needs to reverse-engineer data from formatted output.

```bash
myapp list --output=json | jq '.tasks[] | select(.status == "blocked")'
```

Same handler, same types—different output format. This enables API-like behavior from CLI apps without writing separate code paths.

See [Output Modes](output-modes.md) for complete documentation.

---

## Putting It All Together

Here's a complete example of a polished task list command:

**Handler (`src/handlers.rs`):**

```rust
use outstanding::cli::{CommandContext, HandlerResult, Output};
use clap::ArgMatches;
use serde::Serialize;

#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: String,
    pub due: String,
}

#[derive(Serialize)]
pub struct TaskListResult {
    pub tasks: Vec<Task>,
    pub pending_count: usize,
}

pub fn list(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TaskListResult> {
    let show_all = matches.get_flag("all");
    let tasks = storage::list()?;

    let filtered: Vec<Task> = if show_all {
        tasks
    } else {
        tasks.into_iter().filter(|t| t.status != "done").collect()
    };

    let pending_count = filtered.iter().filter(|t| t.status == "pending").count();

    Ok(Output::Render(TaskListResult { tasks: filtered, pending_count }))
}
```

**Template (`src/templates/list.jinja`):**

```jinja
[title]Tasks[/title]

{% set t = tabular([
    {"name": "id", "width": 8, "style": "muted"},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 10},
    {"name": "due", "width": 10, "align": "right", "style": "muted"}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([task.id, task.title, task.status | style_as(task.status), task.due]) }}
{% endfor %}

[muted]{{ pending_count }} pending[/muted]
```

**Styles (`src/styles/default.yaml`):**

```yaml
title:
  fg: cyan
  bold: true

done: green
blocked: red
pending: yellow

muted:
  dim: true
  light:
    fg: "#666666"
  dark:
    fg: "#999999"
```

**Output (terminal):**

```text
Tasks

TSK-001   Implement user authentication    pending     2024-01-20
TSK-002   Fix payment gateway timeout      blocked     2024-01-18
TSK-003   Review pull request #142         done        2024-01-15

2 pending
```

With colors, "pending" appears yellow, "blocked" appears red, "done" appears green. The ID and due columns are dimmed. The title column fills available space.

**Output (`--output=json`):**

```json
{
  "tasks": [
    {"id": "TSK-001", "title": "Implement user authentication", "status": "pending", "due": "2024-01-20"},
    {"id": "TSK-002", "title": "Fix payment gateway timeout", "status": "blocked", "due": "2024-01-18"},
    {"id": "TSK-003", "title": "Review pull request #142", "status": "done", "due": "2024-01-15"}
  ],
  "pending_count": 2
}
```

Same handler. No additional code.

---

## Summary

Outstanding's rendering layer transforms CLI output from a chore into a pleasure:

1. **Separation of concerns**: Logic returns data. Templates define structure. Styles control appearance.

2. **Fast iteration**: Hot reload means edit-and-see in seconds, not minutes. This changes what's practical.

3. **Familiar tools**: MiniJinja for templates (Jinja syntax), CSS or YAML for styles. No new languages to learn.

4. **Graceful degradation**: One template serves rich terminals, plain pipes, and everything in between.

5. **Structured output for free**: JSON, YAML, and CSV outputs work automatically from your serializable types.

6. **Tabular layouts**: Declarative column definitions handle alignment, wrapping, truncation, and ANSI-awareness.

The rendering system makes it practical to care about details. When iteration is fast and changes are safe, polish becomes achievable—not aspirational.

For complete API details, see [Rendering System](rendering-system.md).
