//! Declarative dispatch macro for command definition.
//!
//! This module provides the [`dispatch!`] macro for defining command hierarchies
//! with a clean, declarative syntax that expands to builder method calls.
//!
//! # Basic Usage
//!
//! ```rust,ignore
//! use standout::cli::{dispatch, App};
//!
//! let builder = App::builder()
//!     .templates_dir("templates")?
//!     .commands(dispatch! {
//!         db: {
//!             migrate => db::migrate,
//!             backup => db::backup,
//!         },
//!         app: {
//!             start => app::start,
//!             stop => app::stop,
//!         },
//!         version => version,
//!     });
//! ```
//!
//! # With Options
//!
//! ```rust,ignore
//! dispatch! {
//!     db: {
//!         migrate => {
//!             handler: db::migrate,
//!             template: "Migrated {{ count }} rows",
//!             pre_dispatch: validate_db,
//!         },
//!     },
//! }
//! ```

/// Declarative macro for defining command dispatch tables.
///
/// The macro expands to a closure that configures a [`GroupBuilder`] with
/// the specified commands and groups.
///
/// # Syntax
///
/// ```text
/// dispatch! {
///     // Simple command (template from convention)
///     command_name => handler_fn,
///
///     // Command with options
///     command_name => {
///         handler: handler_fn,
///         template: "inline {{ value }}",    // optional
///         structured_only: true,              // optional; or silent/binary
///         pre_dispatch: hook_fn,             // optional
///         post_dispatch: hook_fn,            // optional
///         post_output: hook_fn,              // optional
///         structured_output_projection: projection, // optional
///     },
///
///     // Nested group
///     group_name: {
///         // commands and nested groups...
///     },
/// }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{dispatch, App, HandlerResult, Output};
/// use serde_json::json;
///
/// fn migrate_handler(_m: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
///     Ok(Output::Render(json!({"migrated": true})))
/// }
///
/// fn backup_handler(_m: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
///     Ok(Output::Render(json!({"backed_up": true})))
/// }
///
/// let builder = App::builder()
///     .templates_dir("templates")?
///     .commands(dispatch! {
///         db: {
///             migrate => migrate_handler,
///             backup => {
///                 handler: backup_handler,
///                 template: "Backed up {{ count }} files",
///             },
///         },
///         version => |_m, _ctx| Ok(Output::Render(json!({"version": "1.0.0"}))),
///     });
/// ```
#[macro_export]
macro_rules! dispatch {
    // Entry point - creates a closure that builds a GroupBuilder
    { $($tokens:tt)* } => {
        |__builder: $crate::cli::GroupBuilder| -> $crate::cli::GroupBuilder {
            $crate::dispatch_internal!(__builder; $($tokens)*)
        }
    };
}

/// Internal macro for processing dispatch entries.
/// Uses a different name to avoid ambiguity in recursion.
#[macro_export]
#[doc(hidden)]
macro_rules! dispatch_internal {
    // Base case: no more tokens
    ($builder:expr;) => {
        $builder
    };

    // Nested group with trailing comma: `name: { ... },`
    ($builder:expr; $name:ident : { $($inner:tt)* } , $($rest:tt)*) => {
        $crate::dispatch_internal!(
            $builder.group(stringify!($name), |__g| {
                $crate::dispatch_internal!(__g; $($inner)*)
            });
            $($rest)*
        )
    };

    // Nested group without trailing comma: `name: { ... }`
    ($builder:expr; $name:ident : { $($inner:tt)* }) => {
        $builder.group(stringify!($name), |__g| {
            $crate::dispatch_internal!(__g; $($inner)*)
        })
    };

    // Command with config block and trailing comma: `name => { ... },`
    ($builder:expr; $name:ident => { $($config:tt)* } , $($rest:tt)*) => {
        $crate::dispatch_internal!(
            $builder.command_with(
                stringify!($name),
                $crate::dispatch_extract_handler!($($config)*),
                |__cfg| { $crate::dispatch_apply_config!(__cfg; $($config)*) }
            );
            $($rest)*
        )
    };

    // Command with config block without trailing comma: `name => { ... }`
    ($builder:expr; $name:ident => { $($config:tt)* }) => {
        $builder.command_with(
            stringify!($name),
            $crate::dispatch_extract_handler!($($config)*),
            |__cfg| { $crate::dispatch_apply_config!(__cfg; $($config)*) }
        )
    };

    // Simple command with trailing comma: `name => handler,`
    ($builder:expr; $name:ident => $handler:expr , $($rest:tt)*) => {
        $crate::dispatch_internal!(
            $builder.command(stringify!($name), $handler);
            $($rest)*
        )
    };

    // Simple command without trailing comma: `name => handler`
    ($builder:expr; $name:ident => $handler:expr) => {
        $builder.command(stringify!($name), $handler)
    };
}

/// Extract handler from config block
#[macro_export]
#[doc(hidden)]
macro_rules! dispatch_extract_handler {
    (handler : $handler:expr , $($rest:tt)*) => {
        $handler
    };
    (handler : $handler:expr) => {
        $handler
    };
}

/// Apply config options to CommandConfig
#[macro_export]
#[doc(hidden)]
macro_rules! dispatch_apply_config {
    // Base case
    ($cfg:expr;) => { $cfg };

    // Skip handler (already extracted)
    ($cfg:expr; handler : $handler:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg; $($rest)*)
    };
    ($cfg:expr; handler : $handler:expr) => { $cfg };

    // Template option
    ($cfg:expr; template : $template:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.template($template); $($rest)*)
    };
    ($cfg:expr; template : $template:expr) => {
        $cfg.template($template)
    };

    // Template absence options
    ($cfg:expr; structured_only : true , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.structured_only(); $($rest)*)
    };
    ($cfg:expr; structured_only : true) => {
        $cfg.structured_only()
    };
    ($cfg:expr; silent : true , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.silent(); $($rest)*)
    };
    ($cfg:expr; silent : true) => {
        $cfg.silent()
    };
    ($cfg:expr; binary : true , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.binary(); $($rest)*)
    };
    ($cfg:expr; binary : true) => {
        $cfg.binary()
    };

    // Pre-dispatch hook
    ($cfg:expr; pre_dispatch : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.pre_dispatch($hook); $($rest)*)
    };
    ($cfg:expr; pre_dispatch : $hook:expr) => {
        $cfg.pre_dispatch($hook)
    };

    // Post-dispatch hook
    ($cfg:expr; post_dispatch : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.post_dispatch($hook); $($rest)*)
    };
    ($cfg:expr; post_dispatch : $hook:expr) => {
        $cfg.post_dispatch($hook)
    };

    // Post-output hook
    ($cfg:expr; post_output : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.post_output($hook); $($rest)*)
    };
    ($cfg:expr; post_output : $hook:expr) => {
        $cfg.post_output($hook)
    };

    // Structured-output projection
    ($cfg:expr; structured_output_projection : $projection:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!(
            $cfg.structured_output_projection($projection);
            $($rest)*
        )
    };
    ($cfg:expr; structured_output_projection : $projection:expr) => {
        $cfg.structured_output_projection($projection)
    };
}

#[cfg(test)]
mod tests {
    use crate::cli::handler::{CommandContext, Output};
    use crate::cli::GroupBuilder;
    use crate::tabular::{Column, Width};
    use crate::{CsvProjection, StructuredOutputProjection};
    use clap::ArgMatches;
    use serde_json::json;

    #[test]
    fn test_dispatch_simple_command() {
        let configure = dispatch! {
            list => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({"ok": true})))
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("list"));
    }

    #[test]
    fn test_dispatch_multiple_commands() {
        let configure = dispatch! {
            list => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
            show => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("list"));
        assert!(builder.entries.contains_key("show"));
    }

    #[test]
    fn test_dispatch_nested_group() {
        let configure = dispatch! {
            db: {
                migrate => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
            },
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("db"));
    }

    #[test]
    fn test_dispatch_command_with_template() {
        let configure = dispatch! {
            list => {
                handler: |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
                template: "items: {{ items | length }}",
            },
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("list"));
    }

    #[test]
    fn test_dispatch_command_with_structured_output_projection() {
        let projection = StructuredOutputProjection::csv(
            CsvProjection::builder("items")
                .column(Column::new(Width::default()).key("name"))
                .build(),
        );
        let configure = dispatch! {
            list => {
                handler: |_m: &ArgMatches, _ctx: &CommandContext| {
                    Ok(Output::Render(json!({ "items": [{ "name": "one" }] })))
                },
                structured_output_projection: projection,
            },
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("list"));
    }

    #[test]
    fn test_dispatch_mixed() {
        let configure = dispatch! {
            version => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({"v": "1.0"}))),
            db: {
                migrate => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
                backup => {
                    handler: |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
                    template: "backup complete",
                },
            },
            cache: {
                clear => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
            },
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("version"));
        assert!(builder.entries.contains_key("db"));
        assert!(builder.entries.contains_key("cache"));
    }

    #[test]
    fn test_dispatch_deeply_nested() {
        let configure = dispatch! {
            app: {
                config: {
                    get => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
                    set => |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(json!({}))),
                },
            },
        };

        let builder = configure(GroupBuilder::new());
        assert!(builder.entries.contains_key("app"));
    }
}
