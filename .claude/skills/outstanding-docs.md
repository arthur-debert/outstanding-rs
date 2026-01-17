# Skill: Outstanding Documentation

Write and review documentation for the Outstanding CLI framework.

## Documentation Structure

### README
- `README.md` is the main project README
- `crates/outstanding/README.md` links to the main README (no duplication)

### Guides (`docs/guides/`)
Step-by-step walkthroughs covering principles, rationale, and features. They educate readers about the "why" and overall design, not just the "how."

- `intro-to-outstanding.md` - Adopting Outstanding in a working CLI
- `intro-to-rendering.md` - Creating polished terminal output
- `intro-to-tabular.md` - Building aligned, readable tabular layouts
- `tldr-intro-to-outstanding.md` - Fast-paced intro for experienced developers

### Topics (`docs/topics/`)
Focused, in-depth documentation on specific systems and use cases. Still well-written with context and rationale, not dry API dumps.

- `handler-contract.md` - Handler trait, HandlerResult, Output enum
- `rendering-system.md` - Templates, styles, themes, two-pass architecture
- `output-modes.md` - OutputMode enum, --output flag
- `execution-model.md` - Pipeline, dispatch, hooks
- `app-configuration.md` - AppBuilder API
- `tabular.md` - Tabular layout system
- `partial-adoption.md` - Incremental migration
- `render-only.md` - Standalone rendering
- `topics-system.md` - Help topics
- `index.md` - Index of all topics with descriptions

## Canonical Forms

Always show the ONE preferred way in examples. Hint at alternatives via comments only.

| Feature | Canonical Form | Alternatives (mention sparingly) |
|---------|----------------|----------------------------------|
| Styling | File-based CSS (`styles/default.css`) | YAML, programmatic, inline strings |
| Command Setup | `#[derive(Dispatch)]` macro | Explicit `App::builder().command()` |
| Dispatch | Auto dispatch via derive macro | Manual dispatch with ArgMatches |
| Template/Style Loading | `embed_templates!`, `embed_styles!` macros | Runtime file loading |
| Running | `app.run()` with fallback pattern | `run_to_string()` only when capturing output |

### The Run Pattern

**Always use this form:**
```rust
if let Some(matches) = app.run(cli, std::env::args()) {
    // Outstanding didn't handle this command, fall back to legacy
    legacy_dispatch(matches);
}
```

**Only use `run_to_string()` when explicitly discussing:**
- Capturing output for testing
- Post-processing output before printing
- Logging/recording generated output

## Example Domain: tdoo

All documentation uses `tdoo`, a simple todo list manager. This provides consistency and familiarity.

### Data Model
```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pending, Done }

#[derive(Clone, Serialize)]
pub struct Todo {
    pub title: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct TodoResult {
    pub message: Option<String>,
    pub todos: Vec<Todo>,
}
```

### Example Todos
```rust
let todos = vec![
    Todo { title: "Implement user authentication".into(), status: Status::Pending },
    Todo { title: "Fix payment gateway timeout".into(), status: Status::Pending },
    Todo { title: "Update documentation for API v2".into(), status: Status::Done },
    Todo { title: "Review pull request #142".into(), status: Status::Pending },
];
```

### Example Template
```jinja
[title]My Todos[/title]

{% for todo in todos %}
[{{ todo.status }}]{{ todo.status }}[/{{ todo.status }}]  {{ todo.title }}
{% endfor %}

{% if message %}[muted]{{ message }}[/muted]{% endif %}
```

### Example Styles
```css
.title { color: cyan; font-weight: bold; }
.done { color: green; }
.pending { color: yellow; }
.muted { opacity: 0.6; }
```

## Writing Guidelines

### Guides vs Topics
- **Guides**: Walk users through a flow/need/subsystem. Educate on principles and rationale. Progressive, building on concepts.
- **Topics**: Document a focused area in depth. Include use cases and patterns, then dive into details.

### Cross-Linking
Generously link to related topics. In guides, point to topics for in-depth information:
```markdown
See [Output Modes](../topics/output-modes.md) for complete documentation.
```

### Showing Alternatives
In canonical examples, hint at alternatives via comments:
```rust
// Template matched by convention from command name, but can be set explicitly
.command("list", list_handler, "list.j2")
```

Only document alternatives in depth when:
- The doc is specifically about that system ("How to use X standalone")
- The topic explicitly covers multiple approaches

### Code Examples
- Use consistent formatting
- Include necessary imports
- Show realistic, working code
- Keep examples focused on the concept being explained

### Tone
- Direct and practical
- Respect developer time
- Explain the "why" alongside the "how"
- No unnecessary superlatives or marketing language

## Feedback Integration

Key feedback points to address in documentation:

1. **Lead with testability** - The main value prop is testable CLI logic, not just pretty output
2. **Add visual proof** - Screenshots/GIFs showing before/after
3. **Ecosystem comparison** - Why Outstanding vs comfy-table + termimad + manual dispatch
4. **Surface testability** - Show how to unit test handlers and templates
5. **Prominent migration path** - Make partial adoption visible
6. **Address runtime templates** - Explain the trade-off vs compile-time safety
