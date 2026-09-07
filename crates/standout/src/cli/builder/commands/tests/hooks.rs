use super::*;

#[test]
fn test_hooks_registration() {
    use crate::cli::hooks::Hooks;

    let builder = AppBuilder::new().hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())));

    assert!(builder.command_hooks.contains_key("list"));
}

#[test]
fn test_command_config_and_builder_hooks_same_phase_errors() {
    use crate::cli::hooks::Hooks;
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
            |cfg| cfg.template_name("list-2").pre_dispatch(|_, _| Ok(())),
        )
        .unwrap()
        .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())));

    let error = match builder.build() {
        Ok(_) => panic!("expected duplicate hook registration to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("command `list`"));
    assert!(error.contains("pre-dispatch"));
    assert!(error.contains("CommandConfig"));
    assert!(error.contains("AppBuilder::hooks"));
}

#[test]
fn test_builder_and_command_config_hooks_same_phase_errors_in_either_order() {
    use crate::cli::hooks::{Hooks, RenderedOutput};
    use serde_json::json;

    let error = match AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks("list", Hooks::new().post_output(|_, _, output| Ok(output)))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
            |cfg| {
                cfg.template_name("list-2")
                    .post_output(|_, _, output: RenderedOutput| Ok(output))
            },
        ) {
        Ok(_) => panic!("expected duplicate hook registration to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("command `list`"));
    assert!(error.contains("post-output"));
}

#[test]
fn test_builder_and_command_config_hooks_different_phases_are_combined() {
    use crate::cli::hooks::{Hooks, RenderedOutput};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let calls = Arc::new(AtomicUsize::new(0));
    let pre_calls = calls.clone();
    let post_calls = calls.clone();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "list",
            Hooks::new().post_output(move |_, _, output: RenderedOutput| {
                post_calls.fetch_add(1, Ordering::SeqCst);
                Ok(output)
            }),
        )
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
            move |cfg| {
                cfg.template_name("list-2").pre_dispatch(move |_, _| {
                    pre_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            },
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = app.dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("true"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_commands_and_builder_hooks_same_phase_errors_in_either_order() {
    use crate::cli::hooks::{Hooks, RenderedOutput};
    use serde_json::json;

    let error = match AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
        .commands(|g| {
            g.command_with(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                |cfg| cfg.template_name("list-2").pre_dispatch(|_, _| Ok(())),
            )
        }) {
        Ok(_) => panic!("expected duplicate hook registration to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("command `list`"));
    assert!(error.contains("pre-dispatch"));
    assert!(error.contains("CommandConfig"));
    assert!(error.contains("AppBuilder::hooks"));

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .commands(|g| {
            g.command_with(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                |cfg| {
                    cfg.template_name("list-2")
                        .post_output(|_, _, output: RenderedOutput| Ok(output))
                },
            )
        })
        .unwrap()
        .hooks("list", Hooks::new().post_output(|_, _, output| Ok(output)));

    let error = match builder.build() {
        Ok(_) => panic!("expected duplicate hook registration to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("command `list`"));
    assert!(error.contains("post-output"));
}

#[test]
fn test_commands_and_builder_hooks_different_phases_are_combined() {
    use crate::cli::hooks::{Hooks, RenderedOutput};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let calls = Arc::new(AtomicUsize::new(0));
    let pre_calls = calls.clone();
    let post_calls = calls.clone();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "list",
            Hooks::new().post_output(move |_, _, output: RenderedOutput| {
                post_calls.fetch_add(1, Ordering::SeqCst);
                Ok(output)
            }),
        )
        .commands(|g| {
            g.command_with(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                move |cfg| {
                    cfg.template_name("list-2").pre_dispatch(move |_, _| {
                        pre_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = app.dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("true"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
