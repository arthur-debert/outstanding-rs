use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, HandlerResult, Output as HandlerOutput};
use crate::cli::hooks::{HookError, Hooks, RenderedOutput, TextOutput};
use crate::EmbeddedTemplates;
use crate::Representation;
use clap::Command;

#[test]
fn test_dispatch_with_pre_dispatch_hook() {
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let hook_called = Arc::new(AtomicBool::new(false));
    let hook_called_clone = hook_called.clone();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1})))),
            |cfg| cfg.template_name("list-4"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().pre_dispatch(move |_, _ctx| {
                hook_called_clone.store(true, Ordering::SeqCst);
                Ok(())
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert!(hook_called.load(Ordering::SeqCst));
    assert_eq!(result.output(), Some("1"));
}

#[test]
fn test_dispatch_pre_dispatch_hook_abort() {
    let builder = AppBuilder::new()
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                panic!("Handler should not be called");
            }),
            |config| config.silent(),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("blocked by hook"))),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_error(), "expected Error, got {:?}", result);
    let msg = result.error().unwrap();
    assert_eq!(msg, "Error: hook error (pre-dispatch): blocked by hook");
}

#[test]
fn test_dispatch_with_post_output_hook() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
            |cfg| cfg.template_name("list-5"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().post_output(|_, _ctx, output| {
                if let RenderedOutput::Text(text_output) = output {
                    Ok(RenderedOutput::Text(TextOutput::new(
                        text_output.formatted.to_uppercase(),
                        text_output.raw.to_uppercase(),
                    )))
                } else {
                    Ok(output)
                }
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("HELLO"));
}

#[test]
fn test_dispatch_post_output_hook_chain() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "test"})))),
            |cfg| cfg.template_name("list-5"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new()
                .post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text_output) = output {
                        Ok(RenderedOutput::Text(TextOutput::new(
                            format!("[{}]", text_output.formatted),
                            format!("[{}]", text_output.raw),
                        )))
                    } else {
                        Ok(output)
                    }
                })
                .post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text_output) = output {
                        Ok(RenderedOutput::Text(TextOutput::new(
                            text_output.formatted.to_uppercase(),
                            text_output.raw.to_uppercase(),
                        )))
                    } else {
                        Ok(output)
                    }
                }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("[TEST]"));
}

#[test]
fn test_dispatch_post_output_hook_abort() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
            |cfg| cfg.template_name("list-5"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().post_output(|_, _ctx, _output| {
                Err(HookError::post_output("post-processing failed"))
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_error(), "expected Error, got {:?}", result);
    let msg = result.error().unwrap();
    assert_eq!(
        msg,
        "Error: hook error (post-output): post-processing failed"
    );
}

#[test]
fn test_dispatch_hooks_for_nested_command() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "config.get",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "secret"})))),
            |cfg| cfg.template_name("config/get-2"),
        )
        .unwrap()
        .hooks(
            "config.get",
            Hooks::new().post_output(|_, _ctx, output| {
                if let RenderedOutput::Text(_) = output {
                    Ok(RenderedOutput::Text(TextOutput::plain("***".into())))
                } else {
                    Ok(output)
                }
            }),
        );

    let cmd =
        Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

    let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("***"));
}

#[test]
fn test_dispatch_no_hooks_for_command() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "list"})))),
            |cfg| cfg.template_name("list-5"),
        )
        .unwrap()
        .command_with(
            "other",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "other"})))),
            |cfg| cfg,
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().post_output(|_, _ctx, _| {
                panic!("Should not be called for 'other' command");
            }),
        );

    let cmd = Command::new("app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("other"));

    let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("other"));
}

#[test]
fn test_dispatch_binary_output_with_hook() {
    let builder = AppBuilder::new()
        .command_with(
            "export",
            FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
                Ok(HandlerOutput::Binary {
                    data: vec![1, 2, 3],
                    filename: "out.bin".into(),
                })
            }),
            |config| config.binary(),
        )
        .unwrap()
        .hooks(
            "export",
            Hooks::new().post_output(|_, _ctx, output| {
                if let RenderedOutput::Binary(mut bytes, filename) = output {
                    bytes.push(4);
                    Ok(RenderedOutput::Binary(bytes, filename))
                } else {
                    Ok(output)
                }
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("export"));

    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_binary());
    let (bytes, filename) = result.binary().unwrap();
    assert_eq!(bytes, &[1, 2, 3, 4]);
    assert_eq!(filename, "out.bin");
}

#[test]
fn test_hooks_passed_to_built_standout() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
        .build()
        .unwrap();

    assert!(standout.command_hooks.contains_key("list"));
    assert!(!standout.command_hooks.contains_key("other"));
}

#[test]
fn test_dispatch_with_post_dispatch_hook() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5})))),
            |cfg| cfg.template_name("list-6"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().post_dispatch(|_, _ctx, mut data| {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("modified".into(), json!(true).into());
                }
                Ok(data)
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    let output = result.output().unwrap();
    assert!(output.contains("Count: 5"));
    assert!(output.contains("Modified: true"));
}

#[test]
fn test_dispatch_post_dispatch_hook_abort() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []})))),
            |cfg| cfg.template_name("list-7"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new().post_dispatch(|_, _ctx, data| {
                if data
                    .get("items")
                    .and_then(|v| v.as_array())
                    .map(|a| a.is_empty())
                    == Some(true)
                {
                    return Err(HookError::post_dispatch("no items to display"));
                }
                Ok(data)
            }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_error(), "expected Error, got {:?}", result);
    let msg = result.error().unwrap();
    assert_eq!(
        msg,
        "Error: hook error (post-dispatch): no items to display"
    );
}

#[test]
fn test_dispatch_all_three_hooks() {
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let call_order = Arc::new(AtomicUsize::new(0));
    let pre_order = call_order.clone();
    let post_dispatch_order = call_order.clone();
    let post_output_order = call_order.clone();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"})))),
            |cfg| cfg.template_name("list-5"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new()
                .pre_dispatch(move |_, _ctx| {
                    assert_eq!(pre_order.fetch_add(1, Ordering::SeqCst), 0);
                    Ok(())
                })
                .post_dispatch(move |_, _ctx, data| {
                    assert_eq!(post_dispatch_order.fetch_add(1, Ordering::SeqCst), 1);
                    Ok(data)
                })
                .post_output(move |_, _ctx, output| {
                    assert_eq!(post_output_order.fetch_add(1, Ordering::SeqCst), 2);
                    Ok(output)
                }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(call_order.load(Ordering::SeqCst), 3);
}
