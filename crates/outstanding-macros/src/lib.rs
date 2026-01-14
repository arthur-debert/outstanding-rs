//! Proc macros for Outstanding.
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
//! # Usage with RenderSetup
//!
//! The recommended way to use these macros is with [`RenderSetup`]:
//!
//! ```rust,ignore
//! use outstanding::{embed_templates, embed_styles, RenderSetup};
//!
//! let app = RenderSetup::new()
//!     .templates(embed_templates!("src/templates"))
//!     .styles(embed_styles!("src/styles"))
//!     .build()?;
//!
//! // In debug: reads from disk for hot-reload (if path exists)
//! // In release: uses embedded content
//! let output = app.render("list", &data)?;
//! ```
//!
//! [`EmbeddedSource`]: outstanding::EmbeddedSource
//! [`RenderSetup`]: outstanding::RenderSetup

mod dispatch;
mod embed;

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
/// # Example
///
/// ```rust,ignore
/// use outstanding::{embed_templates, RenderSetup};
///
/// // Recommended: use with RenderSetup
/// let app = RenderSetup::new()
///     .templates(embed_templates!("src/templates"))
///     .build()?;
///
/// // Or convert directly to registry
/// let registry: TemplateRegistry = embed_templates!("src/templates").into();
/// ```
///
/// # Compile-Time Errors
///
/// The macro will fail to compile if:
/// - The directory doesn't exist
/// - The directory is not readable
/// - Any file content is not valid UTF-8
///
/// [`EmbeddedTemplates`]: outstanding::EmbeddedTemplates
/// [`RenderSetup`]: outstanding::RenderSetup
/// [`TemplateRegistry`]: outstanding::TemplateRegistry
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
/// # Example
///
/// ```rust,ignore
/// use outstanding::{embed_styles, RenderSetup};
///
/// // Recommended: use with RenderSetup
/// let app = RenderSetup::new()
///     .styles(embed_styles!("src/styles"))
///     .build()?;
///
/// // Or convert directly to registry
/// let registry: StylesheetRegistry = embed_styles!("src/styles").into();
/// ```
///
/// # Compile-Time Errors
///
/// The macro will fail to compile if:
/// - The directory doesn't exist
/// - The directory is not readable
/// - Any file content is not valid UTF-8
///
/// [`EmbeddedStyles`]: outstanding::EmbeddedStyles
/// [`RenderSetup`]: outstanding::RenderSetup
/// [`StylesheetRegistry`]: outstanding::stylesheet::StylesheetRegistry
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
/// # Motivation
///
/// Without this macro, you must explicitly map every command:
///
/// ```rust,ignore
/// dispatch! {
///     add => handlers::add,
///     list => handlers::list,
///     complete => handlers::complete,
/// }
/// ```
///
/// With `#[derive(Dispatch)]`, the mapping becomes implicit:
///
/// ```rust,ignore
/// #[derive(Subcommand, Dispatch)]
/// #[dispatch(handlers = handlers)]
/// enum Commands {
///     Add(AddArgs),      // → handlers::add
///     List(ListArgs),    // → handlers::list
///     Complete(Args),    // → handlers::complete
/// }
/// ```
///
/// # Convention-Based Defaults
///
/// - **Handler**: `{handlers_module}::{variant_snake_case}`
/// - **Template**: `{variant_snake_case}.j2`
///
/// # Explicit Overrides
///
/// ```rust,ignore
/// #[derive(Subcommand, Dispatch)]
/// #[dispatch(handlers = handlers)]
/// enum Commands {
///     #[dispatch(template = "custom/add.j2")]
///     Add(AddArgs),
///
///     #[dispatch(
///         handler = custom::list_all,
///         pre_dispatch = validate_auth,
///         post_dispatch = log_action,
///     )]
///     List(ListArgs),
/// }
/// ```
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
/// `App::builder().commands()`:
///
/// ```rust,ignore
/// App::builder()
///     .commands(Commands::dispatch_config())
///     .build()?
///     .run(Cli::command(), std::env::args());
/// ```
///
/// # Nested Subcommands
///
/// Nested enums are handled recursively, but must use `#[dispatch(nested)]`:
///
/// ```rust,ignore
/// #[derive(Subcommand, Dispatch)]
/// #[dispatch(handlers = handlers)]
/// enum Commands {
///     #[dispatch(nested)]
///     Db(DbCommands),  // Delegates to DbCommands::dispatch_config()
/// }
///
/// #[derive(Subcommand, Dispatch)]
/// #[dispatch(handlers = handlers::db)]
/// enum DbCommands {
///     Migrate(MigrateArgs),  // → handlers::db::migrate
/// }
/// ```
#[proc_macro_derive(Dispatch, attributes(dispatch))]
pub fn dispatch_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    dispatch::dispatch_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
