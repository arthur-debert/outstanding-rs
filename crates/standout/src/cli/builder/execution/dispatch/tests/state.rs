use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::cli::hooks::Hooks;
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn test_dispatch_with_app_state() {
    use serde_json::json;

    struct Database {
        url: String,
    }

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .app_state(Database {
            url: "postgres://localhost".into(),
        })
        .command_with(
            "list",
            FnHandler::new(|_m, ctx| {
                let db = ctx.app_state.get::<Database>().unwrap();
                Ok(HandlerOutput::Render(json!({"db_url": db.url.clone()})))
            }),
            |cfg| cfg.template_name("list-13"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("postgres://localhost"));
}

#[test]
fn test_dispatch_app_state_get_required() {
    use serde_json::json;

    struct Config {
        debug: bool,
    }

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .app_state(Config { debug: true })
        .command_with(
            "list",
            FnHandler::new(|_m, ctx| {
                let config = ctx.app_state.get_required::<Config>()?;
                Ok(HandlerOutput::Render(json!({"debug": config.debug})))
            }),
            |cfg| cfg.template_name("list-14"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("debug=true"));
}

#[test]
fn test_dispatch_app_state_missing_type_error() {
    use serde_json::json;

    struct NotProvided;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, ctx| {
                let _missing = ctx.app_state.get_required::<NotProvided>()?;
                Ok(HandlerOutput::Render(json!({})))
            }),
            |config| config.structured_only(),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_error(), "expected Error, got {:?}", result);
    let msg = result.error().unwrap();
    assert!(
        msg.contains("Extension missing"),
        "Expected 'Extension missing' in error, got: {}",
        msg
    );
}

#[test]
fn test_dispatch_app_state_with_multiple_types() {
    use serde_json::json;

    struct Database {
        name: String,
    }
    struct Config {
        version: i32,
    }

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .app_state(Database {
            name: "mydb".into(),
        })
        .app_state(Config { version: 42 })
        .command_with(
            "info",
            FnHandler::new(|_m, ctx| {
                let db = ctx.app_state.get_required::<Database>()?;
                let config = ctx.app_state.get_required::<Config>()?;
                Ok(HandlerOutput::Render(json!({
                    "db": db.name,
                    "version": config.version
                })))
            }),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("info"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "info"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("db=mydb, version=42"));
}

#[test]
fn test_dispatch_app_state_and_extensions_together() {
    use serde_json::json;

    struct Database {
        name: String,
    }
    struct UserScope {
        user_id: String,
    }

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .app_state(Database {
            name: "maindb".into(),
        })
        .command_with(
            "list",
            FnHandler::new(|_m, ctx| {
                let db = ctx.app_state.get_required::<Database>()?;

                let scope = ctx.extensions.get_required::<UserScope>()?;

                Ok(HandlerOutput::Render(json!({
                    "db": db.name,
                    "user": scope.user_id
                })))
            }),
            |cfg| cfg.template_name("list-15"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().pre_dispatch(|_, ctx| {
                ctx.extensions.insert(UserScope {
                    user_id: "user123".into(),
                });
                Ok(())
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("db=maindb, user=user123"));
}

#[test]
fn test_built_app_dispatch_with_app_state() {
    use serde_json::json;

    struct ApiConfig {
        base_url: String,
    }

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .app_state(ApiConfig {
            base_url: "https://api.example.com".into(),
        })
        .command_with(
            "fetch",
            FnHandler::new(|_m, ctx| {
                let config = ctx.app_state.get_required::<ApiConfig>()?;
                Ok(HandlerOutput::Render(json!({"url": config.base_url})))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("fetch"));
    let result = app.run_with(
        cmd,
        ["app", "fetch"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("https://api.example.com"));
}
