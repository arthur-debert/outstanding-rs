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
//! - **Release builds**: Use embedded content, zero file I/O
//! - **Debug builds**: Hot-reload from disk if source path exists
//!
//! # Examples
//!
//! For working examples, see:
//! - `standout/tests/embed_macros.rs` - embedding macros
//! - `standout/tests/dispatch_derive.rs` - dispatch derive macro
//!
//! [`EmbeddedSource`]: standout::EmbeddedSource
//! [`RenderSetup`]: standout::RenderSetup

mod dispatch;
mod embed;
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
/// - **Release builds**: Uses embedded content (zero file I/O)
/// - **Debug builds**: Reads from disk if source path exists (hot-reload)
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
/// - **Release builds**: Uses embedded content (zero file I/O)
/// - **Debug builds**: Reads from disk if source path exists (hot-reload)
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
/// - **Handler**: `{handlers_module}::{variant_snake_case}`
///   - `Add` → `handlers::add`
///   - `ListAll` → `handlers::list_all`
/// - **Template**: `{variant_snake_case}.j2`
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
