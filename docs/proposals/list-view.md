# ListView — Generic List View Specification

**Status:** Draft
**Created:** 2026-01-29
**Location:** `standout` (core framework)

## Overview

ListView provides a standardized pattern for displaying collections in CLI applications. It is the first building block toward a Django-style CRUD system where common operations are generated from annotated structs.

**Goals:**
1. Minimal boilerplate for common list displays
2. Consistent structure: intro → items → ending → messages
3. First-class tabular integration (zero-template lists)
4. Seeker integration for filtering/searching
5. Designed for future CRUD operations (detail, create, update, delete)

---

## Design Principles

### Struct-Centric, Not View-Centric

The object struct is the source of truth. View behavior derives from annotations on the struct rather than separate view classes:

```rust
// The struct defines what can be listed, filtered, and displayed
#[derive(Clone, Serialize, Seekable, Tabular)]
pub struct Task {
    #[seek(String)]
    pub name: String,

    #[seek(Enum)]
    #[tabular(width = 10, style = "status")]
    pub status: Status,
}
```

Views are dispatch configurations that reference this struct, not new types.

### Progressive Disclosure

Three tiers of complexity:

| Tier | Use Case | What You Provide |
|------|----------|------------------|
| **Zero-config** | Quick prototypes | Struct + `#[derive(Tabular)]` |
| **Styled** | Production lists | Struct + tabular spec + optional intro/ending templates |
| **Custom** | Complex layouts | Full template override |

### Composition Over Inheritance

ListView is a configuration pattern, not a base class. It composes:
- **Tabular** for layout (optional)
- **Seeker** for filtering (optional)
- **Templates** for custom rendering (optional)
- **Messages** for status/warnings (standard Result feature)

---

## Core Concepts

### ListView Result

A list view produces a `ListViewResult<T>` that the framework renders:

```rust
pub struct ListViewResult<T> {
    /// Items to display (post-filtering, post-ordering)
    pub items: Vec<T>,

    /// Text shown before the list (optional)
    pub intro: Option<String>,

    /// Text shown after the list (optional)
    pub ending: Option<String>,

    /// Status messages (info, warning, error)
    pub messages: Vec<Message>,

    /// Total count before limit/offset (for "showing X of Y")
    pub total_count: Option<usize>,

    /// Applied filters summary (for "filtered by: ...")
    pub filter_summary: Option<String>,
}
```

This is not a new concept—it's a structured way to return data that the framework-supplied `list-view.jinja` template renders. [1]

### Handler Pattern

List handlers return `ListViewResult<T>` where `T: Serialize`:

```rust
fn list_tasks(args: &ArgMatches) -> HandlerResult<ListViewResult<Task>> {
    let tasks = load_tasks()?;

    Ok(Output::Render(ListViewResult {
        items: tasks,
        intro: Some("Your tasks:".into()),
        ending: None,
        messages: vec![],
        total_count: Some(tasks.len()),
        filter_summary: None,
    }))
}
```

With the `list_view` helper: [2]

```rust
fn list_tasks(args: &ArgMatches) -> HandlerResult<ListViewResult<Task>> {
    let tasks = load_tasks()?;
    Ok(list_view(tasks).intro("Your tasks:").build())
}
```

---

## Rendering Modes

### Mode 1: Tabular (Default, Zero-Template)

When the item type implements `Tabular`, the framework renders items using the tabular spec. No template required.

```rust
#[derive(Serialize, Tabular)]
#[tabular(separator = "  ")]
pub struct Task {
    #[tabular(width = 4, align = "right", style = "muted")]
    pub id: u32,

    #[tabular(width = "fill")]
    pub name: String,

    #[tabular(width = 10)]
    pub status: Status,
}
```

Renders as:

```
Your tasks:

   1  Implement authentication          pending
   2  Fix payment bug                   done
   3  Update documentation              pending

3 tasks
```

The framework-supplied `list-view.jinja` handles this automatically. [3]

### Mode 2: Item Template

For custom per-item rendering, provide an item template. The list view template iterates and renders each item:

```jinja
{# templates/task-item.jinja #}
[{% if item.status == "done" %}done{% else %}pending{% endif %}] {{ item.name }}
```

Configure via dispatch:

```rust
#[derive(Dispatch)]
enum Commands {
    #[dispatch(handler = list_tasks, item_template = "task-item")]
    List,
}
```

### Mode 3: Full Template Override

For complete control, provide a full list template:

```jinja
{# templates/task-list.jinja #}
{% if intro %}{{ intro }}{% endif %}

{% for task in items %}
  {{ loop.index }}. {{ task.name }} — {{ task.status | style_as(task.status) }}
{% endfor %}

{% if ending %}{{ ending }}{% endif %}

{% for msg in messages %}
[{{ msg.level }}] {{ msg.text }}
{% endfor %}
```

---

## Seeker Integration

ListView integrates with Seeker for filtering. When the item type implements `Seekable`, filtering is automatic.

### Handler with Filtering

```rust
fn list_tasks(args: &ArgMatches, query: Query) -> HandlerResult<ListViewResult<Task>> {
    let all_tasks = load_tasks()?;
    let filtered = query.filter(&all_tasks, Task::accessor);

    Ok(list_view(filtered)
        .total_count(all_tasks.len())
        .filter_summary_from(&query)
        .build())
}
```

### CLI Arguments (Phase 4 Integration)

When using `#[derive(FilterArgs)]` from Seeker Phase 4, the dispatch automatically:
1. Generates filter arguments (`--name-contains`, `--status-eq`, etc.)
2. Parses them into a `Query`
3. Passes the query to the handler

```rust
#[derive(Dispatch)]
enum Commands {
    #[dispatch(
        handler = list_tasks,
        filterable,  // Enables Seeker integration
    )]
    List,
}
```

The user runs:

```bash
$ myapp list --status-eq=pending --name-contains=auth --limit=10
```

And gets filtered results with a summary:

```
Showing 2 of 15 tasks (filtered by: status=pending, name contains "auth")

   5  Implement authentication          pending
  12  Auth token refresh                pending
```

---

## CRUD Foundation

ListView is designed as the first piece of a larger CRUD system. The same struct annotations power all views:

| View | Derives Used | Template Default |
|------|--------------|------------------|
| ListView | `Tabular`, `Seekable` | `list-view.jinja` |
| DetailView | `Tabular` (vertical) | `detail-view.jinja` |
| DeleteView | — | `delete-confirm.jinja` |
| CreateView | `Validatable` | `form-view.jinja` |
| UpdateView | `Validatable` | `form-view.jinja` |

### Future: Object Dispatch

The end goal is object-centric dispatch:

```rust
#[derive(Crud)]
#[crud(object = "task")]
pub struct Task { /* ... */ }
```

Generates:

```
myapp task list [--filters...]
myapp task view <id>
myapp task delete <id>
myapp task create [--fields...]
myapp task update <id> [--fields...]
```

ListView is step one. The `#[crud]` macro will compose `#[dispatch]` configurations for each action.

---

## Dispatch Configuration

### Minimal (Tabular Mode)

```rust
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(list_view, item_type = "Task")]
    List,
}
```

The `list_view` marker tells dispatch to:
1. Use `list-view.jinja` template
2. Expect `ListViewResult<Task>` from handler
3. Use `Task`'s `Tabular` impl for item rendering

### With Filtering

```rust
#[dispatch(list_view, item_type = "Task", filterable)]
List,
```

Adds Seeker argument generation and query injection.

### With Custom Templates

```rust
#[dispatch(
    list_view,
    item_type = "Task",
    template = "custom-task-list",      // Override full template
    // OR
    item_template = "custom-task-item", // Override item only
)]
List,
```

### Complete Example

```rust
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(
        list_view,
        item_type = "Task",
        filterable,
        intro_template = "task-list-intro",  // Optional intro template
    )]
    List,

    #[dispatch(handler = show_task)]
    Show { id: u32 },
}
```

---

## Template Reference

### Framework-Supplied: `list-view.jinja`

```jinja
{% if intro %}
{{ intro }}

{% endif %}
{% if items | length == 0 %}
{{ empty_message | default("No items found.") }}
{% else %}
{% if tabular_spec %}
{# Use tabular rendering #}
{% set t = tabular(tabular_spec) %}
{% for item in items %}
{{ t.row_from(item) }}
{% endfor %}
{% else %}
{# Use item template #}
{% for item in items %}
{% include item_template with item = item %}
{% endfor %}
{% endif %}
{% endif %}
{% if ending %}

{{ ending }}
{% endif %}
{% if filter_summary %}
[muted]({{ filter_summary }})[/muted]
{% endif %}
{% for msg in messages %}
[{{ msg.level }}]{{ msg.text }}[/{{ msg.level }}]
{% endfor %}
```

### Template Variables

| Variable | Type | Description |
|----------|------|-------------|
| `items` | `Vec<T>` | The items to display |
| `intro` | `Option<String>` | Header text |
| `ending` | `Option<String>` | Footer text |
| `messages` | `Vec<Message>` | Status messages |
| `total_count` | `Option<usize>` | Total before filtering |
| `filter_summary` | `Option<String>` | Applied filters description |
| `tabular_spec` | `Option<TabularSpec>` | From `T::tabular_spec()` if available |
| `item_template` | `Option<String>` | Item template name if configured |
| `empty_message` | `Option<String>` | Custom "no items" message |

---

## API Summary

### Derive Macros

| Macro | Purpose | Required For |
|-------|---------|--------------|
| `#[derive(Tabular)]` | Column layout spec | Zero-template mode |
| `#[derive(TabularRow)]` | Optimized row extraction | Performance |
| `#[derive(Seekable)]` | Query field access | Filtering |
| `#[derive(SeekerSchema)]` | CLI argument generation | Filter CLI args |

### Dispatch Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `list_view` | marker | Enable list view mode |
| `item_type` | string | The item struct name |
| `filterable` | marker | Enable Seeker integration |
| `template` | string | Full template override |
| `item_template` | string | Per-item template |
| `intro_template` | string | Intro section template |
| `ending_template` | string | Ending section template |
| `empty_message` | string | Custom empty list message |

### Helper Functions

```rust
// Quick builder
pub fn list_view<T>(items: impl IntoIterator<Item = T>) -> ListViewBuilder<T>;

// Builder methods
impl<T> ListViewBuilder<T> {
    pub fn intro(self, text: impl Into<String>) -> Self;
    pub fn ending(self, text: impl Into<String>) -> Self;
    pub fn message(self, level: Level, text: impl Into<String>) -> Self;
    pub fn total_count(self, n: usize) -> Self;
    pub fn filter_summary(self, s: impl Into<String>) -> Self;
    pub fn filter_summary_from(self, query: &Query) -> Self;
    pub fn build(self) -> Output<ListViewResult<T>>;
}
```

---

## Framework-Supplied Assets

As standout develops higher-level features (list views, detail views, CRUD), it needs to provide stock templates and styles. This creates namespace management challenges.

### The Problem

If the framework provides `list-view.jinja` and a user's project also has `list-view.jinja`, behavior is ambiguous:
- Which takes precedence?
- How does the user intentionally override vs accidentally collide?
- How does the user reference the framework version explicitly?

### Namespacing Strategy

**Templates:** Framework templates live in the `standout/` namespace.

```
Framework provides:     standout/list-view.jinja
User creates:           list-view.jinja (no collision)
User overrides:         standout/list-view.jinja (intentional)
```

In dispatch configuration, framework templates use the full path:

```rust
#[dispatch(list_view, item_type = "Task")]  // Uses "standout/list-view" implicitly
#[dispatch(template = "standout/list-view")] // Explicit framework template
#[dispatch(template = "my-custom-list")]     // User template
```

**Styles:** Framework styles use the `standout-` prefix.

```css
/* Framework provides */
standout-muted { color: gray; }
standout-list-header { font-weight: bold; }

/* User defines */
muted { color: #888; }  /* No collision */
```

**Resolution Order:**

1. User templates/styles (project's `templates/` and `styles/` dirs)
2. Embedded user assets (via `embed_templates!`, `embed_styles!`)
3. Framework assets (bundled in standout crate)

This means users can override any framework asset by creating a file with the same namespaced path.

### Opting Out

Users can disable framework defaults entirely:

```rust
let app = App::builder()
    .include_framework_templates(false)  // Disable standout/* templates
    .include_framework_styles(false)     // Disable standout-* styles
    .build();
```

Default is `true` for both. Disabling is useful when:
- Building a completely custom UI system
- Avoiding any implicit behavior
- Debugging template resolution

### Bootstrap CLI

To help users customize framework assets, standout provides a CLI for scaffolding and inspection.

**Scaffold UI directory:**

```bash
$ standout init ui ./ui

Created:
  ui/
    templates/
      standout/
        list-view.jinja      # Copy of framework template
        detail-view.jinja
    styles/
      standout.css           # Copy of framework styles
```

This copies framework assets into the project, allowing customization. The CLI never overwrites existing files.

**Print individual assets:**

```bash
# Print to stdout (pipe to file as needed)
$ standout show template standout/list-view
$ standout show style standout-muted

# List available framework assets
$ standout list templates
$ standout list styles
```

**Usage pattern:**

```bash
# Start with framework default, customize one template
$ standout show template standout/list-view > ui/templates/standout/list-view.jinja
$ $EDITOR ui/templates/standout/list-view.jinja
```

### Framework Asset Inventory

| Asset Type | Name | Purpose |
|------------|------|---------|
| Template | `standout/list-view` | Generic list rendering with tabular support |
| Template | `standout/detail-view` | Single item detail display |
| Template | `standout/empty-list` | "No items found" message |
| Template | `standout/filter-summary` | "Showing X of Y, filtered by..." |
| Style | `standout-muted` | De-emphasized text |
| Style | `standout-error` | Error messages |
| Style | `standout-warning` | Warning messages |
| Style | `standout-info` | Info messages |
| Style | `standout-success` | Success indicators |
| Style | `standout-header` | Section headers |

### Implementation Notes

**Embedding:** Framework assets are compiled into the standout crate via `include_str!`. No runtime file dependencies.

**Hot reload:** In debug builds, if the framework source exists (developer working on standout itself), hot reload from source. In release or when source unavailable, use embedded.

**Versioning:** Framework assets are versioned with the crate. Users who copy assets for customization should note the version in comments for future reference.

---

## Implementation Phases

### Phase 1: Core ListView

- [ ] `ListViewResult<T>` struct
- [ ] `list_view()` builder helper
- [ ] Framework `list-view.jinja` template
- [ ] Tabular rendering integration in template
- [ ] `list_view` dispatch attribute support

### Phase 2: Template Modes

- [ ] Item template support (`item_template` attribute)
- [ ] Intro/ending template support
- [ ] Empty message customization
- [ ] Template variable injection

### Phase 3: Seeker Integration

- [ ] `filterable` attribute support
- [ ] Query injection into handlers
- [ ] Filter summary generation
- [ ] "Showing X of Y" display

### Phase 4: CRUD Foundation

- [ ] DetailView (vertical tabular)
- [ ] DeleteView with confirmation
- [ ] `#[crud]` macro prototype

---

## Footnotes

[1] **ListViewResult serialization**: The struct serializes cleanly to JSON for structured output modes. When `--output=json`, the framework bypasses templating and serializes directly.

[2] **Builder pattern**: The `list_view()` helper is a convenience. Direct struct construction works fine for complex cases:
```rust
ListViewResult {
    items,
    intro: Some(format!("Tasks for project {}", project.name)),
    ending: Some(format!("Run `task add` to create more")),
    messages: vec![Message::warning("2 tasks overdue")],
    total_count: Some(total),
    filter_summary: query.summary(),
}
```

[3] **Tabular detection**: The template checks if `tabular_spec` is present (set by framework when `T: Tabular`). If not present, it falls back to item templates or basic iteration.

[4] **Handler injection**: The `filterable` attribute causes the dispatch system to:
1. Generate Seeker CLI args via `#[derive(FilterArgs)]`
2. Parse args into a `Query` before handler invocation
3. Pass `Query` as second parameter to handler (after `&ArgMatches`)

This follows the same pattern as other dispatch injections (format, theme).

[5] **Tabular + Seeker synergy**: When both are derived on the same struct, you get:
- Automatic column layout from `#[tabular(...)]`
- Automatic filter args from `#[seek(...)]`
- Zero templates needed for a fully-featured filterable list

```rust
#[derive(Clone, Serialize, Seekable, Tabular)]
pub struct LogEntry {
    #[seek(Timestamp)]
    #[tabular(width = 20)]
    pub timestamp: DateTime<Utc>,

    #[seek(Enum)]
    #[tabular(width = 8, style = "level")]
    pub level: Level,

    #[seek(String)]
    #[tabular(width = "fill", overflow = "truncate")]
    pub message: String,
}

// CLI: myapp logs --level-eq=error --after=2024-01-01 --limit=50
// Produces a filtered, formatted table with zero template code
```
