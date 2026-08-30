//! The `dispatch!` macro declares a command hierarchy that expands to
//! [`GroupBuilder`](crate::cli::GroupBuilder) method calls.
//!
//! ```rust,ignore
//! App::builder().commands(dispatch! {
//!     db: { migrate => db::migrate, backup => db::backup },
//!     version => version,
//! });
//! ```

#[macro_export]
macro_rules! dispatch {
    { $($tokens:tt)* } => {
        |__builder: $crate::cli::GroupBuilder| -> $crate::cli::GroupBuilder {
            $crate::dispatch_internal!(__builder; $($tokens)*)
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! dispatch_internal {
    ($builder:expr;) => {
        $builder
    };

    ($builder:expr; $name:ident : { $($inner:tt)* } , $($rest:tt)*) => {
        $crate::dispatch_internal!(
            $builder.group(stringify!($name), |__g| {
                $crate::dispatch_internal!(__g; $($inner)*)
            });
            $($rest)*
        )
    };

    ($builder:expr; $name:ident : { $($inner:tt)* }) => {
        $builder.group(stringify!($name), |__g| {
            $crate::dispatch_internal!(__g; $($inner)*)
        })
    };

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

    ($builder:expr; $name:ident => { $($config:tt)* }) => {
        $builder.command_with(
            stringify!($name),
            $crate::dispatch_extract_handler!($($config)*),
            |__cfg| { $crate::dispatch_apply_config!(__cfg; $($config)*) }
        )
    };

    ($builder:expr; $name:ident => $handler:expr , $($rest:tt)*) => {
        $crate::dispatch_internal!(
            $builder.command(stringify!($name), $handler);
            $($rest)*
        )
    };

    ($builder:expr; $name:ident => $handler:expr) => {
        $builder.command(stringify!($name), $handler)
    };
}

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

#[macro_export]
#[doc(hidden)]
macro_rules! dispatch_apply_config {
    ($cfg:expr;) => { $cfg };

    ($cfg:expr; handler : $handler:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg; $($rest)*)
    };
    ($cfg:expr; handler : $handler:expr) => { $cfg };

    ($cfg:expr; template : $template:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.template($template); $($rest)*)
    };
    ($cfg:expr; template : $template:expr) => {
        $cfg.template($template)
    };

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

    ($cfg:expr; pre_dispatch : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.pre_dispatch($hook); $($rest)*)
    };
    ($cfg:expr; pre_dispatch : $hook:expr) => {
        $cfg.pre_dispatch($hook)
    };

    ($cfg:expr; post_dispatch : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.post_dispatch($hook); $($rest)*)
    };
    ($cfg:expr; post_dispatch : $hook:expr) => {
        $cfg.post_dispatch($hook)
    };

    ($cfg:expr; post_output : $hook:expr , $($rest:tt)*) => {
        $crate::dispatch_apply_config!($cfg.post_output($hook); $($rest)*)
    };
    ($cfg:expr; post_output : $hook:expr) => {
        $cfg.post_output($hook)
    };

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
