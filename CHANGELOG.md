# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- **Added**:
  - **`#[derive(Dispatch)]` macro** - Convention-based command dispatch for clap `Subcommand` enums
    - Generates `dispatch_config()` method that maps variants to handlers automatically
    - PascalCase variants map to snake_case handlers (e.g., `AddTask` → `handlers::add_task`)
    - Container attribute: `#[dispatch(handlers = path)]` specifies handler module
    - Variant attributes for customization:
      - `#[dispatch(handler = custom_fn)]` - Override handler for specific variant
      - `#[dispatch(template = "path.j2")]` - Set template for variant
      - `#[dispatch(nested)]` - Delegate to nested subcommand's `dispatch_config()`
      - `#[dispatch(skip)]` - Skip variant (no handler registration)
    - Hook support: `pre_dispatch`, `post_dispatch`, `post_output` per variant
  - Re-exported `Dispatch` from `outstanding-clap` for ergonomic imports

- **Example**:

  ```rust
  use clap::Subcommand;
  use outstanding_clap::{Dispatch, Outstanding};

  #[derive(Subcommand, Dispatch)]
  #[dispatch(handlers = handlers)]
  enum Commands {
      Add { text: String },      // → handlers::add
      List,                      // → handlers::list

      #[dispatch(handler = custom::complete_task)]
      Complete { id: u32 },      // → custom::complete_task

      #[dispatch(nested)]
      Admin(AdminCommands),      // → delegates to AdminCommands::dispatch_config()
  }

  Outstanding::builder()
      .commands(Commands::dispatch_config())
      .run_and_print(cmd, args);
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
  use outstanding_clap::{dispatch, Outstanding, CommandResult};
  use serde_json::json;

  Outstanding::builder()
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
    - `OutstandingApp` for ready-to-use rendering with pre-loaded templates
  - **outstanding-clap integration** - `.styles()` and `.default_theme()` methods on `OutstandingBuilder`

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
  - `serde_json` dependency added to `outstanding-clap` (previously dev-only)

- **Example**:

  ```rust
  use outstanding_clap::{Outstanding, Hooks, HookError};
  use serde_json::json;

  Outstanding::builder()
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
  use outstanding_clap::{Outstanding, Hooks, Output, HookError};

  Outstanding::builder()
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
  - Code reorganization: split `lib.rs` into focused modules for both `outstanding` and `outstanding-clap` crates

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

[Unreleased]: https://github.com/arthur-debert/outstanding-rs/compare/v0.14.0...HEAD
[0.14.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.10.1...v0.11.1
[0.10.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.9.0...v0.10.1
[0.9.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.2...v0.9.0
[0.7.2]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/arthur-debert/outstanding-rs/releases/tag/v0.6.0
