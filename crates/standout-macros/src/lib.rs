//! Proc macros for Standout.
//!
//! This crate provides macros for compile-time resource embedding and
//! declarative command dispatch configuration.
//!
//! # Available Macros
//!
//! ## Embedding Macros
//!
//! - [`embed_templates!`] - Embed template files (`.jinja`, `.jinja2`, `.j2`, `.txt`)
//! - [`embed_styles!`] - Embed stylesheet files (`.yaml`, `.yml`)
//!
//! ## Derive Macros
//!
//! - [`Dispatch`] - Generate dispatch configuration from clap `Subcommand` enums
//! - [`Tabular`] - Generate `TabularSpec` from struct field annotations
//! - [`TabularRow`] - Generate optimized row extraction without JSON serialization
//! - [`Seekable`] - Generate query-enabled accessor functions for Seeker
//!
//! ## Attribute Macros
//!
//! - [`handler`] - Transform pure functions into Standout-compatible handlers
//!
//! # Design Philosophy
//!
//! These macros return [`EmbeddedSource`] types that contain:
//!
//! 1. Embedded content (baked into binary at compile time)
//! 2. Source path (for debug hot-reload)
//!
//! This design enables:
//!
//! - Release builds: Use embedded content, zero file I/O
//! - Debug builds: Hot-reload from disk if source path exists
//!
//! # Examples
//!
//! For working examples, see:
//! - `standout/tests/embed_macros.rs` - embedding macros
//! - `standout/tests/dispatch_derive.rs` - dispatch derive macro
//!
//! [`EmbeddedSource`]: standout::EmbeddedSource
//! [`RenderSetup`]: standout::RenderSetup

mod resource;
mod dispatch;
mod embed;
mod handler;
mod seeker;
mod tabular;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, LitStr};

/// Embeds all template files from a directory at compile time.
///
/// This macro walks the specified directory, reads all files with recognized
/// template extensions, and returns an [`EmbeddedTemplates`] source that can
/// be used with [`RenderSetup`] or converted to a [`TemplateRegistry`].
///
/// # Supported Extensions
///
/// Files are recognized by extension (in priority order):
/// - `.jinja` (highest priority)
/// - `.jinja2`
/// - `.j2`
/// - `.txt` (lowest priority)
///
/// When multiple files share the same base name with different extensions
/// (e.g., `config.jinja` and `config.txt`), the higher-priority extension wins
/// for extensionless lookups.
///
/// # Hot Reload Behavior
///
/// - Release builds: Uses embedded content (zero file I/O)
/// - Debug builds: Reads from disk if source path exists (hot-reload)
///
/// For working examples, see `standout/tests/embed_macros.rs`.
///
/// # Compile-Time Errors
///
/// The macro will fail to compile if:
/// - The directory doesn't exist
/// - The directory is not readable
/// - Any file content is not valid UTF-8
///
/// [`EmbeddedTemplates`]: standout::EmbeddedTemplates
/// [`RenderSetup`]: standout::RenderSetup
/// [`TemplateRegistry`]: standout::TemplateRegistry
#[proc_macro]
pub fn embed_templates(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    embed::embed_templates_impl(path_lit).into()
}

/// Embeds all stylesheet files from a directory at compile time.
///
/// This macro walks the specified directory, reads all files with recognized
/// stylesheet extensions, and returns an [`EmbeddedStyles`] source that can
/// be used with [`RenderSetup`] or converted to a [`StylesheetRegistry`].
///
/// # Supported Extensions
///
/// Files are recognized by extension (in priority order):
/// - `.yaml` (highest priority)
/// - `.yml` (lowest priority)
///
/// When multiple files share the same base name with different extensions
/// (e.g., `dark.yaml` and `dark.yml`), the higher-priority extension wins.
///
/// # Hot Reload Behavior
///
/// - Release builds: Uses embedded content (zero file I/O)
/// - Debug builds: Reads from disk if source path exists (hot-reload)
///
/// For working examples, see `standout/tests/embed_macros.rs`.
///
/// # Compile-Time Errors
///
/// The macro will fail to compile if:
/// - The directory doesn't exist
/// - The directory is not readable
/// - Any file content is not valid UTF-8
///
/// [`EmbeddedStyles`]: standout::EmbeddedStyles
/// [`RenderSetup`]: standout::RenderSetup
/// [`StylesheetRegistry`]: standout::StylesheetRegistry
#[proc_macro]
pub fn embed_styles(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    embed::embed_styles_impl(path_lit).into()
}

/// Derives dispatch configuration from a clap `Subcommand` enum.
///
/// This macro eliminates boilerplate command-to-handler mappings by using
/// naming conventions with explicit overrides when needed.
///
/// For working examples, see `standout/tests/dispatch_derive.rs`.
///
/// # Convention-Based Defaults
///
/// - Handler: `{handlers_module}::{variant_snake_case}`
///   - `Add` → `handlers::add`
///   - `ListAll` → `handlers::list_all`
/// - Template: `{variant_snake_case}.j2`
///
/// # Container Attributes
///
/// | Attribute | Required | Description |
/// |-----------|----------|-------------|
/// | `handlers = path` | Yes | Module containing handler functions |
///
/// # Variant Attributes
///
/// | Attribute | Description | Default |
/// |-----------|-------------|---------|
/// | `handler = path` | Handler function | `{handlers}::{snake_case}` |
/// | `template = "path"` | Template file | `{snake_case}.j2` |
/// | `pre_dispatch = fn` | Pre-dispatch hook | None |
/// | `post_dispatch = fn` | Post-dispatch hook | None |
/// | `post_output = fn` | Post-output hook | None |
/// | `nested` | Treat as nested subcommand | false |
/// | `skip` | Skip this variant | false |
///
/// # Generated Code
///
/// Generates a `dispatch_config()` method returning a closure for
/// use with `App::builder().commands()`.
#[proc_macro_derive(Dispatch, attributes(dispatch))]
pub fn dispatch_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    dispatch::dispatch_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives a `TabularSpec` from struct field annotations.
///
/// This macro generates an implementation of the `Tabular` trait, which provides
/// a `tabular_spec()` method that returns a `TabularSpec` for the struct.
///
/// For working examples, see `standout/tests/tabular_derive.rs`.
///
/// # Field Attributes
///
/// | Attribute | Type | Description |
/// |-----------|------|-------------|
/// | `width` | `usize` or `"fill"` or `"Nfr"` | Column width strategy |
/// | `min` | `usize` | Minimum width (for bounded) |
/// | `max` | `usize` | Maximum width (for bounded) |
/// | `align` | `"left"`, `"right"`, `"center"` | Text alignment |
/// | `anchor` | `"left"`, `"right"` | Column position |
/// | `overflow` | `"truncate"`, `"wrap"`, `"clip"`, `"expand"` | Overflow handling |
/// | `truncate_at` | `"end"`, `"start"`, `"middle"` | Truncation position |
/// | `style` | string | Style name for the column |
/// | `style_from_value` | flag | Use cell value as style name |
/// | `header` | string | Header title (default: field name) |
/// | `null_repr` | string | Representation for null values |
/// | `key` | string | Data extraction key (supports dot notation) |
/// | `skip` | flag | Exclude this field from the spec |
///
/// # Container Attributes
///
/// | Attribute | Type | Description |
/// |-----------|------|-------------|
/// | `separator` | string | Column separator (default: "  ") |
/// | `prefix` | string | Row prefix |
/// | `suffix` | string | Row suffix |
///
/// # Example
///
/// ```ignore
/// use standout::tabular::Tabular;
/// use serde::Serialize;
///
/// #[derive(Serialize, Tabular)]
/// #[tabular(separator = " │ ")]
/// struct Task {
///     #[col(width = 8, style = "muted")]
///     id: String,
///
///     #[col(width = "fill", overflow = "wrap")]
///     title: String,
///
///     #[col(width = 12, align = "right")]
///     status: String,
/// }
///
/// let spec = Task::tabular_spec();
/// ```
#[proc_macro_derive(Tabular, attributes(col, tabular))]
pub fn tabular_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    tabular::tabular_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives optimized row extraction for tabular formatting.
///
/// This macro generates an implementation of the `TabularRow` trait, which provides
/// a `to_row()` method that converts the struct to a `Vec<String>` without JSON serialization.
///
/// For working examples, see `standout/tests/tabular_derive.rs`.
///
/// # Field Attributes
///
/// | Attribute | Description |
/// |-----------|-------------|
/// | `skip` | Exclude this field from the row |
///
/// # Example
///
/// ```ignore
/// use standout::tabular::TabularRow;
///
/// #[derive(TabularRow)]
/// struct Task {
///     id: String,
///     title: String,
///
///     #[col(skip)]
///     internal_state: u32,
///
///     status: String,
/// }
///
/// let task = Task {
///     id: "TSK-001".to_string(),
///     title: "Implement feature".to_string(),
///     internal_state: 42,
///     status: "pending".to_string(),
/// };
///
/// let row = task.to_row();
/// assert_eq!(row, vec!["TSK-001", "Implement feature", "pending"]);
/// ```
#[proc_macro_derive(TabularRow, attributes(col))]
pub fn tabular_row_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    tabular::tabular_row_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives the `Seekable` trait for query-enabled structs.
///
/// This macro generates an implementation of the `Seekable` trait from
/// `standout-seeker`, enabling type-safe field access for query operations.
///
/// # Field Attributes
///
/// | Attribute | Description |
/// |-----------|-------------|
/// | `String` | String field (supports Eq, Ne, Contains, StartsWith, EndsWith, Regex) |
/// | `Number` | Numeric field (supports Eq, Ne, Gt, Gte, Lt, Lte) |
/// | `Timestamp` | Timestamp field (supports Eq, Ne, Before, After, Gt, Gte, Lt, Lte) |
/// | `Enum` | Enum field (supports Eq, Ne, In) - requires `SeekerEnum` impl |
/// | `Bool` | Boolean field (supports Eq, Ne, Is) |
/// | `skip` | Exclude this field from queries |
/// | `rename = "..."` | Use a custom name for queries |
///
/// # Generated Code
///
/// The macro generates:
///
/// 1. Field name constants (e.g., `Task::NAME`, `Task::PRIORITY`)
/// 2. Implementation of `Seekable::seeker_field_value()`
///
/// # Example
///
/// ```ignore
/// use standout_macros::Seekable;
/// use standout_seeker::{Query, Seekable};
///
/// #[derive(Seekable)]
/// struct Task {
/// struct Task {
///     #[seek(String)]
///     name: String,
///
///     #[seek(Number)]
///     priority: u8,
///
///     #[seek(Bool)]
///     done: bool,
///
///     #[seek(skip)]
///     internal_id: u64,
/// }
///
/// let tasks = vec![
///     Task { name: "Write docs".into(), priority: 3, done: false, internal_id: 1 },
///     Task { name: "Fix bug".into(), priority: 5, done: true, internal_id: 2 },
/// ];
///
/// let query = Query::new()
///     .and_gte(Task::PRIORITY, 3u8)
///     .not_eq(Task::DONE, true)
///     .build();
///
/// let results = query.filter(&tasks, Task::accessor);
/// assert_eq!(results.len(), 1);
/// assert_eq!(results[0].name, "Write docs");
/// ```
///
/// # Enum Fields
///
/// For enum fields, implement `SeekerEnum` on your enum type:
///
/// ```ignore
/// use standout_seeker::SeekerEnum;
///
/// #[derive(Clone, Copy)]
/// enum Status { Pending, Active, Done }
///
/// impl SeekerEnum for Status {
///     fn seeker_discriminant(&self) -> u32 {
///         match self {
///             Status::Pending => 0,
///             Status::Active => 1,
///             Status::Done => 2,
///         }
///     }
/// }
///
/// #[derive(Seekable)]
/// struct Task {
///     #[seek(Enum)]
///     status: Status,
/// }
/// ```
///
/// # Timestamp Fields
///
/// For timestamp fields, implement `SeekerTimestamp` on your datetime type:
///
/// ```ignore
/// use standout_seeker::{SeekerTimestamp, Timestamp};
///
/// struct MyDateTime(i64);
///
/// impl SeekerTimestamp for MyDateTime {
///     fn seeker_timestamp(&self) -> Timestamp {
///         Timestamp::from_millis(self.0)
///     }
/// }
///
/// #[derive(Seekable)]
/// struct Event {
///     #[seek(Timestamp)]
///     created_at: MyDateTime,
/// }
/// ```
#[proc_macro_derive(Seekable, attributes(seek))]
pub fn seekable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    seeker::seekable_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Transforms a pure function into a Standout-compatible handler.
///
/// This macro generates a wrapper function that extracts CLI arguments from
/// `ArgMatches` and calls your pure function. The original function is preserved
/// for direct testing.
///
/// # Parameter Annotations
///
/// | Annotation | Type | Description |
/// |------------|------|-------------|
/// | `#[flag]` | `bool` | Boolean CLI flag |
/// | `#[flag(name = "x")]` | `bool` | Flag with custom CLI name |
/// | `#[arg]` | `T` | Required CLI argument |
/// | `#[arg]` | `Option<T>` | Optional CLI argument |
/// | `#[arg]` | `Vec<T>` | Multiple CLI arguments |
/// | `#[arg(name = "x")]` | `T` | Argument with custom CLI name |
/// | `#[ctx]` | `&CommandContext` | Access to command context |
/// | `#[matches]` | `&ArgMatches` | Raw matches (escape hatch) |
///
/// # Return Type Handling
///
/// | Return Type | Behavior |
/// |-------------|----------|
/// | `Result<T, E>` | Passed through (dispatch auto-wraps in Output::Render) |
/// | `Result<(), E>` | Wrapped in `HandlerResult<()>` with `Output::Silent` |
///
/// # Generated Code
///
/// For a function `fn foo(...)`, the macro generates `fn foo__handler(...)`.
///
/// # Example
///
/// ```rust,ignore
/// use standout_macros::handler;
///
/// // Pure function - easy to test
/// #[handler]
/// pub fn list(#[flag] all: bool, #[arg] limit: Option<usize>) -> Result<Vec<Item>, Error> {
///     storage::list(all, limit)
/// }
///
/// // Generates:
/// // pub fn list__handler(m: &ArgMatches) -> Result<Vec<Item>, Error> {
/// //     let all = m.get_flag("all");
/// //     let limit = m.get_one::<usize>("limit").cloned();
/// //     list(all, limit)
/// // }
///
/// // Use with Dispatch derive:
/// #[derive(Subcommand, Dispatch)]
/// #[dispatch(handlers = handlers)]
/// enum Commands {
///     #[dispatch(handler = list)]  // Uses list__handler
///     List { ... },
/// }
/// ```
///
/// # Testing
///
/// The original function is preserved, so you can test it directly:
///
/// ```rust,ignore
/// #[test]
/// fn test_list() {
///     let result = list(true, Some(10));
///     assert!(result.is_ok());
/// }
/// ```
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    handler::handler_impl(attr, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives Resource commands and handlers for a struct.
///
/// This macro generates a complete Resource CLI interface for the annotated struct,
/// including list, view, create, update, and delete commands with corresponding
/// handlers.
///
/// # Required Attributes
///
/// | Attribute | Description |
/// |-----------|-------------|
/// | `object = "name"` | Singular name for the object (e.g., "task") |
/// | `store = Type` | Type implementing `ResourceStore` trait |
///
/// # Optional Attributes
///
/// | Attribute | Description | Default |
/// |-----------|-------------|---------|
/// | `plural = "name"` | Plural name for the object | `"{object}s"` |
/// | `operations = [...]` | Subset of operations to generate | All operations |
///
/// # Field Attributes
///
/// | Attribute | Description |
/// |-----------|-------------|
/// | `id` | Marks field as primary identifier (required) |
/// | `readonly` | Excludes field from create/update |
/// | `skip` | Excludes field from all Resource operations |
/// | `default = "expr"` | Default value for create |
/// | `choices = ["a", "b"]` | Constrained values |
///
/// # Generated Code
///
/// For `#[resource(object = "task", store = TaskStore)]` on `Task`:
///
/// - `TaskCommands` enum with List, View, Create, Update, Delete variants
/// - `TaskCommands::dispatch_config()` for use with `App::builder().group()`
/// - Handler functions in `__task_resource_handlers` module
///
/// # Example
///
/// ```rust,ignore
/// use standout_macros::Resource;
///
/// #[derive(Clone, Resource)]
/// #[resource(object = "task", store = TaskStore)]
/// pub struct Task {
///     #[resource(id)]
///     pub id: String,
///
///     #[resource(arg(short, long))]
///     pub title: String,
///
///     #[resource(choices = ["pending", "done"])]
///     pub status: String,
///
///     #[resource(readonly)]
///     pub created_at: String,
/// }
///
/// // In main.rs:
/// App::builder()
///     .app_state(TaskStore::new())
///     .group("task", TaskCommands::dispatch_config())
///     .build()?
///     .run(Cli::command(), std::env::args_os());
/// ```
#[proc_macro_derive(Resource, attributes(resource))]
pub fn resource_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    resource::resource_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
