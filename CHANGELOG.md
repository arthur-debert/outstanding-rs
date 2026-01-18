# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **BREAKING: Removed `clap` feature flag** - The `cli` module and clap integration are now always available. The `clap` feature has been removed.

  ```diff
  [dependencies]
  - standout = { version = "2", features = ["clap", "macros"] }
  + standout = "2"
  ```

  **Migration:** Remove `features = ["clap"]` from your `Cargo.toml`. If you only used `features = ["macros"]`, note that macros are also now always available.

- **`macros` feature is now a no-op** - The `macros` feature still exists for backwards compatibility but does nothing. All macros (`embed_templates!`, `embed_styles!`, `Dispatch`, `Tabular`, `TabularRow`) are now always available.

## [2.1.0] - 2026-01-18

### Added

- **New `standout-render` crate** - Extracted the rendering layer into a standalone crate for users who need rich terminal output without CLI framework features.

  The new crate provides:
  - Two-pass template rendering (MiniJinja + BBCode-style styling)
  - Adaptive themes with light/dark mode support
  - Output modes (Auto, Term, Text, JSON, YAML, CSV, XML)
  - Tabular formatting with Unicode support
  - File-based resources with hot-reload in dev, embedded in release

  **Usage:**
  ```rust
  // For render-only use cases (no CLI framework)
  use standout_render::{render, Theme};

  // Full framework users continue using standout (unchanged API)
  use standout::{render, Theme, cli::App};
  ```

  The main `standout` crate re-exports everything from `standout-render`, so existing code continues to work without changes.

### Changed

- **BREAKING: `App` is now generic over `HandlerMode`** - `App` and `LocalApp` have been unified into a single generic type `App<M: HandlerMode>`. `LocalApp` is now a type alias for `App<Local>`.

  ```diff
  - use standout::cli::App;
  - App::builder()
  + use standout::cli::{App, ThreadSafe};
  + App::<ThreadSafe>::builder()
  ```

  Note: `App::builder()` still works and defaults to `ThreadSafe`, but explicit type annotation is recommended for clarity.

- **BREAKING: Builder methods now return `Result`** - All `AppBuilder` command registration methods now return `Result<Self, SetupError>` instead of `Self`. This catches configuration errors at build time rather than runtime.

  ```diff
  App::builder()
  -     .command("list", handler, template)
  +     .command("list", handler, template)?
      .build()?
  ```

  **Migration:** Add `?` or `.unwrap()` after each `.command()`, `.command_with()`, `.command_handler()`, and `.group()` call.

- **Internal: Shared AppCore architecture** - Extracted common functionality from `App` and `LocalApp` into a shared `AppCore` struct. This ensures feature parity between both app types and eliminates code duplication.

### Added

- **Duplicate command detection** - Registering the same command path twice now returns `SetupError::DuplicateCommand` instead of silently overwriting. This catches configuration bugs early.

  ```rust
  App::builder()
      .command("list", handler1, template)?
      .command("list", handler2, template)?  // Error: duplicate command "list"
  ```

- **Design guidelines documentation** - Added `docs/dev/design-guidelines.md` codifying configuration safety principles, structural unification requirements, and testing requirements for contributors.

- **Comprehensive property-based testing** - Added `proptest` tests that verify rendering invariants across all configuration combinations:
  - 8 output modes (Auto, Term, Text, TermDebug, Json, Yaml, Xml, Csv)
  - 2 handler modes (ThreadSafe, Local)
  - Theme variations (none, empty, populated)
  - Template variations (simple, styled, nested)

### Fixed

- **LocalApp now supports `{% include %}` in templates** - LocalApp templates can now use `{% include %}` directives to include other templates from the registry, matching the behavior of `App`.

## [1.1.0] - 2026-01-18

### Added

- **LocalApp for mutable handlers** - New `LocalApp` and `LocalAppBuilder` types for CLI applications that need `FnMut` handlers with `&mut self` access to state, without requiring `Send + Sync` bounds or interior mutability wrappers.

  **When to use:**
  - Your handlers need `&mut self` access to state
  - You want to avoid `Arc<Mutex<_>>` wrappers
  - Your CLI is single-threaded (the common case)

  **New types:**
  - `LocalApp` - Single-threaded CLI application with mutable dispatch
  - `LocalAppBuilder` - Builder accepting `FnMut` handlers
  - `LocalHandler` trait - For struct-based handlers with `&mut self`

  **Example:**
  ```rust
  use standout::cli::{LocalApp, Output};

  let mut counter = 0u32;

  LocalApp::builder()
      .command("increment", |_m, _ctx| {
          counter += 1;  // FnMut allows direct mutation!
          Ok(Output::Render(json!({"count": counter})))
      }, "Count: {{ count }}")
      .build()?
      .run(cmd, args);
  ```

  **Comparison with App:**
  | Aspect | `App` | `LocalApp` |
  |--------|-------|------------|
  | Handler type | `Fn + Send + Sync` | `FnMut` |
  | State mutation | Via `Arc<Mutex<_>>` | Direct |
  | Thread safety | Yes | No |
  | Use case | Libraries, async | Simple CLIs |

- **Comprehensive tabular layout system** - New `standout::tabular` module for creating aligned, column-based terminal output with full Unicode support.

  **Template filters:**
  - `col(width, align=?, truncate=?, ellipsis=?)` - Format value to fit column width
  - `pad_left(width)`, `pad_right(width)`, `pad_center(width)` - Padding helpers
  - `truncate_at(width, position?, ellipsis?)` - Truncation with start/middle/end positions
  - `display_width` - Get visual width of Unicode strings
  - `style_as(style)` - Wrap value in style tags

  **Template functions:**
  - `tabular(columns, separator=?, width=?)` - Create a TabularFormatter for row-by-row output
  - `table(columns, border=?, header=?, header_style=?, row_separator=?, width=?)` - Create decorated tables with borders

  **Rust API:**
  - `TabularSpec` - Column layout specification with builder pattern
  - `TabularFormatter` - Row formatter with field extraction support
  - `Table` - Decorated table with borders, headers, and separators
  - `Col` - Shorthand column constructors (`Col::fixed()`, `Col::fill()`, `Col::min()`, etc.)

  **Features:**
  - Multiple width strategies: fixed, bounded (min/max), fill, fractional
  - Column anchoring (left/right edge positioning)
  - Overflow handling: truncate (start/middle/end), wrap, clip, expand
  - Automatic field extraction from structs via `row_from()`
  - Column styles with `style_from_value` for dynamic styling
  - Six border styles: none, ascii, light, heavy, double, rounded
  - Row separators between data rows
  - Headers from column specs via `header_from_columns()`
  - Full Unicode support (CJK characters, combining marks, ANSI codes)

### Changed

- **BREAKING: Renamed `table` module to `tabular`** - The module is now accessed as `standout::tabular` instead of `standout::table`. This better reflects its purpose of providing tabular layout functionality.
  - `use standout::table::*` → `use standout::tabular::*`

- **BREAKING: Renamed types for consistency:**
  - `TableFormatter` → `TabularFormatter`
  - `register_table_filters()` → `register_tabular_filters()`
  - Removed backward compatibility aliases (`TableSpec`, `TableSpecBuilder`)

## [2.2.0] - 2026-01-15

## [2.1.2] - 2026-01-15

### Added

- **Default command support** - Configure a command to run when no subcommand is specified
  - `AppBuilder::default_command("name")` - Set the default command imperatively
  - `#[dispatch(default)]` variant attribute - Mark a command as default in `#[derive(Dispatch)]`
  - When CLI is invoked without a subcommand (e.g., `myapp` or `myapp --verbose`), the default command is automatically used
  - Only one command can be marked as default per dispatch group

  ```rust
  // Imperative API
  App::builder()
      .default_command("list")
      .command("list", list_handler, "list.j2")
      .command("add", add_handler, "add.j2")

  // Macro API
  #[derive(Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      #[dispatch(default)]
      List,
      Add,
  }
  ```

## [2.1.1] - 2026-01-15

### Fixed

- **Fixed broken `clap` feature** - The `clap` feature was completely broken due to incorrect internal imports introduced during the rendering module reorganization:
  - `crate::render::TemplateRegistry` → `crate::TemplateRegistry`
  - `crate::stylesheet::StylesheetRegistry` → `crate::StylesheetRegistry`
  - `crate::render::filters::register_filters` → `crate::rendering::template::filters::register_filters`
  - `DispatchRenderedOutput` → `DispatchOutput`
  - `crate::cli::hooks::Output` → `crate::cli::hooks::RenderedOutput`

### Added

- **Pre-commit hook for feature validation** - Added `.githooks/pre-commit` to check all feature combinations compile before commit
- **CI feature matrix testing** - CI now tests all feature combinations (`default`, `macros`, `clap`, `all-features`) plus formatting and clippy checks

## [2.1.0] - 2026-01-15

### Changed

- **BREAKING: Reorganized rendering modules into `src/rendering/`** - All rendering-related code is now consolidated under the `rendering` module for clearer organization and potential future extraction to a standalone crate.
  - `render/` → `rendering/template/`
  - `theme/` → `rendering/theme/`
  - `style/` → `rendering/style/`
  - `stylesheet/` → merged into `rendering/style/`
  - `table/` → `rendering/table/`
  - `output.rs` → `rendering/output.rs`
  - `context.rs` → `rendering/context.rs`

- **BREAKING: Merged `stylesheet` module into `style`** - The `stylesheet` module has been absorbed into `style`. All YAML parsing functionality is now accessed through the `style` module.
  - `use standout::stylesheet::*` → `use standout::style::*`
  - Types like `StylesheetRegistry`, `parse_stylesheet`, `ThemeVariants` are now in `style`

### Added

- **`rendering::prelude` module** - Convenient imports for standalone rendering:

  ```rust
  use standout::rendering::prelude::*;
  ```

  Includes: `render`, `render_auto`, `render_with_output`, `render_with_mode`, `render_with_vars`, `Theme`, `ColorMode`, `OutputMode`, `Renderer`, `Style`

- **`render_with_vars()` function** - Simplified context injection for adding key-value pairs to templates without the full `ContextRegistry` system:

  ```rust
  let mut vars = HashMap::new();
  vars.insert("version", "1.0.0");
  let output = render_with_vars(template, &data, &theme, mode, vars)?;
  ```

## [2.0.0] - 2026-01-14

## [1.0.0] - 2026-01-14

## [1.0.0] - 2026-01-13

### 🚀 First Stable Release

Standout reaches 1.0 with a cleaner, more ergonomic template syntax.

### ⚠️ BREAKING CHANGE: Tag-Based Styling

**The MiniJinja `style` filter has been replaced with BBCode-style tags.**

```diff
- {{ title | style("heading") }}
+ [heading]{{ title }}[/heading]

- {{ "Error:" | style("error") }} {{ message }}
+ [error]Error:[/error] {{ message }}
```

**Migration is straightforward:** wrap your content with `[name]...[/name]` tags instead of piping through the `style` filter.

### Added

- **Tag-based style syntax** - Ergonomic `[name]content[/name]` syntax for applying styles
  - Two-pass rendering: MiniJinja first, then BBParser style tag processing
  - Output mode support: tags become ANSI codes (Term), stripped (Text), or preserved (TermDebug)
  - Unknown tags show `[tag?]` marker for easy debugging
- **Template validation** - `validate_template()` function to catch unknown style tags
  - Returns detailed error info with tag name and position
  - Re-exported `UnknownTagError`, `UnknownTagErrors`, `UnknownTagKind` types
- **New `standout-bbparser` crate** - Standalone BBCode-style tag parser for terminal styling
  - `BBParser` with configurable `TagTransform` (Apply/Remove/Keep)
  - `UnknownTagBehavior` (Passthrough with `?` marker, or Strip)
  - Strict validation for unbalanced/unexpected close tags
  - Optimized nested style application (reduced ANSI bloat)
  - CSS identifier rules for tag names
- **`#[derive(Dispatch)]` macro** - Convention-based command dispatch for clap `Subcommand` enums
  - Generates `dispatch_config()` method that maps variants to handlers automatically
  - PascalCase variants map to snake_case handlers (e.g., `AddTask` → `handlers::add_task`)
  - Container attribute: `#[dispatch(handlers = path)]` specifies handler module
  - Variant attributes: `handler`, `template`, `nested`, `skip`
  - Hook support: `pre_dispatch`, `post_dispatch`, `post_output` per variant

### Removed

- **`style` filter** - Use tag syntax `[name]{{ value }}[/name]` instead

### Example

```rust
use standout::{render_with_output, Theme, OutputMode};
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold())
    .add("count", Style::new().cyan());

// Tag syntax for all styled content
let template = r#"[title]Report[/title]: [count]{{ count }}[/count] items"#;

let output = render_with_output(template, &data, &theme, OutputMode::Term)?;
```

## [0.14.0] - 2026-01-12

- **Added**:
  - **Declarative command dispatch** - New `dispatch!` macro for defining command hierarchies with clean, Python-dict-like syntax
    - Simple command syntax: `name => handler`
    - Config block syntax: `name => { handler: ..., template: ..., pre_dispatch: ... }`
    - Nested groups: `group_name: { ... }`
    - Hook support inline: `pre_dispatch`, `post_dispatch`, `post_output`
  - **Nested builder API** - `.group()` method for programmatic command organization
    - `GroupBuilder` for building nested command groups
    - `CommandConfig` for inline handler configuration
    - `.command_with()` for inline template and hook configuration
  - **Convention-based template resolution** - Templates resolved automatically from command path
    - `.template_dir("templates")` sets base directory
    - `.template_ext(".j2")` sets extension (default: `.j2`)
    - Command `db.migrate` resolves to `templates/db/migrate.j2`
  - **`.commands()` method** - Accepts closure from `dispatch!` macro for bulk command registration

- **Example**:

  ```rust
  use standout_clap::{dispatch, Standout, CommandResult};
  use serde_json::json;

  Standout::builder()
      .template_dir("templates")
      .commands(dispatch! {
          db: {
              migrate => db::migrate,
              backup => {
                  handler: db::backup,
                  template: "backup.j2",
                  pre_dispatch: validate_auth,
              },
          },
          app: {
              start => app::start,
              config: {
                  get => config::get,
                  set => config::set,
              },
          },
          version => |_m, _ctx| CommandResult::Ok(json!({"v": "1.0"})),
      })
      .run_and_print(cmd, args);
  ```

## [0.13.0] - 2026-01-12

## [0.12.0] - 2026-01-12

- **Added**:
  - **Compile-time resource embedding macros** - Embed templates and stylesheets into binaries at compile time
    - `embed_templates!("./templates")` - Walks directory and embeds all template files
    - `embed_styles!("./styles")` - Walks directory and embeds all stylesheet files
    - Same resolution API as runtime loading (access by base name or with extension)
    - Extension priority preserved (e.g., `.jinja` > `.jinja2` > `.j2` > `.txt`)
  - **EmbeddedSource with debug hot-reload** - Macros return `EmbeddedSource<R>` type that supports automatic hot-reload
    - In debug mode: if source path exists, files are read from disk (hot-reload)
    - In release mode: embedded content is used (zero file I/O)
    - `EmbeddedTemplates` and `EmbeddedStyles` type aliases for convenience
    - `From` implementations for converting to `TemplateRegistry` and `StylesheetRegistry`
  - **RenderSetup builder** - Unified setup API for templates, styles, and themes
    - `RenderSetup::new().templates(...).styles(...).default_theme(...).build()`
    - `StandoutApp` for ready-to-use rendering with pre-loaded templates
  - **standout-clap integration** - `.styles()` and `.default_theme()` methods on `StandoutBuilder`

- **Changed**:
  - **Simplified embed macro architecture** - Macros are now "dumb" collectors that only walk directories
    - All smart logic (extension priority, name stripping, collision detection) moved to `from_embedded_entries()` methods
    - `TemplateRegistry::from_embedded_entries()` for compile-time template embedding
    - `StylesheetRegistry::from_embedded_entries()` for compile-time stylesheet embedding
  - **Consolidated file loader helpers** - Shared functions in `file_loader` module
    - `extension_priority()` - Returns priority index for filename extension
    - `strip_extension()` - Removes recognized extension from filename
    - `build_embedded_registry()` - Generic helper for building registries from embedded entries
  - **Updated template extensions** - Changed from `.tmpl` to `.jinja` as primary extension
    - New priority order: `.jinja`, `.jinja2`, `.j2`, `.txt`

- **Fixed**:
  - **Hot-reload mode now works correctly with `names()` iteration** - Previously, converting `EmbeddedSource` to registries in debug mode used lazy loading, causing `names()` to return empty. Now uses immediate loading for both templates and stylesheets.

## [0.11.1] - 2026-01-11

- **Added**:
  - **File-based stylesheet loading** - Load themes and styles from YAML files at runtime
    - `StylesheetRegistry` for managing file-based themes
    - YAML stylesheet parsing with full spec compliance
    - Adaptive themes that respond to terminal capabilities
  - **Auto output to file** - Automatically save command output to files
    - Configurable output path patterns
    - Support for all output formats (text, JSON, YAML, XML, CSV)

- **Changed**:
  - **Renamed `TableSpec` to `FlatDataSpec`** - Better reflects its purpose for flat data extraction across multiple formats (tables, CSV)
  - Improved data extraction for CSV export

## [0.10.1] - 2026-01-11

- **Added**:
  - **File-based template loading** - Load templates from `.txt` or `.jinja` files at runtime
    - `TemplateRegistry` for managing file-based templates
    - Hot reload support in debug mode for rapid iteration
    - Template caching in release mode for performance
  - **Multiple output format support**:
    - **YAML output** - Serialize data to YAML format
    - **XML output** - Serialize data to XML format
    - **CSV output** - Automatic flattening of nested data structures for tabular export
  - **Generic file loader infrastructure** - Reusable file loading utilities for templates, stylesheets, and other resources

- **Changed**:
  - Template caching is now enabled by default in release builds

## [0.9.0] - 2026-01-10

## [0.7.2] - 2026-01-10

- **Added**:
  - **Post-dispatch hooks** - New hook phase that runs after handler execution but before rendering
    - `post_dispatch` hooks receive raw handler data as `serde_json::Value`
    - Can inspect, modify, or replace data before it's rendered
    - Useful for data enrichment, validation, filtering, and normalization
    - Full access to `ArgMatches` and `CommandContext` in hook functions
  - `HookError::post_dispatch()` factory method for creating post-dispatch errors
  - `HookPhase::PostDispatch` variant for error phase tracking
  - `serde_json` dependency added to `standout-clap` (previously dev-only)

- **Example**:

  ```rust
  use standout_clap::{Standout, Hooks, HookError};
  use serde_json::json;

  Standout::builder()
      .command("list", handler, template)
      .hooks("list", Hooks::new()
          .pre_dispatch(|_m, ctx| {
              println!("Running: {}", ctx.command_path.join(" "));
              Ok(())
          })
          .post_dispatch(|_m, _ctx, mut data| {
              // Add metadata before rendering
              if let Some(obj) = data.as_object_mut() {
                  obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
              }
              Ok(data)
          })
          .post_output(|_m, _ctx, output| {
              // Transform or inspect output
              Ok(output)
          }))
      .run_and_print(cmd, args);
  ```

## [0.7.1] - 2026-01-10

## [0.7.0] - 2026-01-10

- **Added**:
  - **Hook system for pre/post command execution** - Register custom callbacks that run before and after command handlers execute
    - `pre_dispatch` hooks: Run before command handler, can abort execution
    - `post_output` hooks: Run after output is generated, can transform output or abort
    - Multiple hooks can be chained at each phase
    - Full access to `ArgMatches` and `CommandContext` in hook functions
  - New `Output` enum for hook output handling:
    - `Output::Text(String)` - Text output from templates
    - `Output::Binary(Vec<u8>, String)` - Binary output with filename
    - `Output::Silent` - No output
  - `HookError` type with phase information (`PreDispatch` / `PostOutput`)
  - `Hooks::new()` builder with fluent `.pre_dispatch()` and `.post_output()` methods

- **Example**:

  ```rust
  use standout_clap::{Standout, Hooks, Output, HookError};

  Standout::builder()
      .command("list", handler, template)
      .hooks("list", Hooks::new()
          .pre_dispatch(|matches, ctx| {
              println!("Running: {}", ctx.command_path.join(" "));
              Ok(())
          })
          .post_output(|matches, ctx, output| {
              // Transform or inspect output
              Ok(output)
          }))
      .run_and_print(cmd, args);
  ```

## [0.6.2] - 2025-01-10

- **Changed**:
  - Code reorganization: split `lib.rs` into focused modules for both `standout` and `standout-clap` crates

## [0.6.1] - 2025-01-09

- **Changed**:
  - Switched to cargo-release for publishing

## [0.6.0] - 2025-01-09

- **Added**:
  - Tabular output support with `TableFormatter` and MiniJinja filters
  - Width resolution algorithm for responsive column layouts
  - ANSI-aware text manipulation utilities
  - `OutputMode::Json` for structured output
  - `render_or_serialize()` method for conditional rendering/serialization
  - Command handler system with `dispatch_from` convenience method
  - Archive variant support in clap integration

[Unreleased]: https://github.com/arthur-debert/standout/compare/standout-v2.1.0...HEAD
[2.1.0]: https://github.com/arthur-debert/standout/compare/standout-v2.0.0...standout-v2.1.0
[1.1.0]: https://github.com/arthur-debert/standout/compare/standout-v1.0.0...standout-v1.1.0
[2.2.0]: https://github.com/arthur-debert/standout/compare/v2.1.2...v2.2.0
[2.1.2]: https://github.com/arthur-debert/standout/compare/v2.1.1...v2.1.2
[2.1.1]: https://github.com/arthur-debert/standout/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/arthur-debert/standout/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/arthur-debert/standout/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/arthur-debert/standout/compare/v0.15.0...v1.0.0
[0.14.0]: https://github.com/arthur-debert/standout/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/arthur-debert/standout/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/arthur-debert/standout/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/arthur-debert/standout/compare/v0.10.1...v0.11.1
[0.10.1]: https://github.com/arthur-debert/standout/compare/v0.9.0...v0.10.1
[0.9.0]: https://github.com/arthur-debert/standout/compare/v0.7.2...v0.9.0
[0.7.2]: https://github.com/arthur-debert/standout/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/arthur-debert/standout/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/arthur-debert/standout/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/arthur-debert/standout/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/arthur-debert/standout/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/arthur-debert/standout/releases/tag/v0.6.0
