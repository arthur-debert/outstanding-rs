use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{
    ExitStatus, FnHandler, HandlerResult, Output as HandlerOutput, StreamSink,
};
use crate::cli::hooks::{HookError, Hooks, RenderedOutput, TextOutput};
use crate::ColorPolicy;
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn test_run_command_with_hooks() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        value: i32,
    }

    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "test",
            Hooks::new().post_output(|_, _ctx, output| {
                if let RenderedOutput::Text(text_output) = output {
                    Ok(RenderedOutput::Text(TextOutput::new(
                        format!("wrapped: {}", text_output.formatted),
                        format!("wrapped: {}", text_output.raw),
                    )))
                } else {
                    Ok(output)
                }
            }),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 }))),
        crate::TemplateRef::Inline(("{{ value }}").to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output.as_text(), Some("wrapped: 42"));
}

#[test]
fn test_run_command_pre_dispatch_abort() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "test",
            Hooks::new().pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("access denied"))),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            panic!("Handler should not be called");
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("access denied"));
}

#[test]
fn test_run_command_without_hooks() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        msg: String,
    }

    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| {
            Ok(HandlerOutput::Render(Data {
                msg: "hello".into(),
            }))
        }),
        crate::TemplateRef::Inline(("{{ msg }}").to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_text(), Some("hello"));
}

#[test]
fn test_run_command_silent() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> { Ok(HandlerOutput::Silent) }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_ok());
    assert!(result.unwrap().is_silent());
}

#[test]
fn test_run_command_binary() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "export",
            Hooks::new().post_output(|_, _ctx, output| {
                assert!(output.is_binary());
                Ok(output)
            }),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("export"));
    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let sub_matches = matches.subcommand_matches("export").unwrap();

    let result = standout.run_command(
        "export",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            Ok(HandlerOutput::Binary {
                data: vec![0xDE, 0xAD],
                filename: "data.bin".into(),
            })
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.is_binary());
    let (bytes, filename) = output.as_binary().unwrap();
    assert_eq!(bytes, &[0xDE, 0xAD]);
    assert_eq!(filename, "data.bin");
}

fn status_without_a_carrier_message(error: HookError) -> String {
    let source = error.source.expect("the carrier error is the source");
    source.to_string()
}

#[test]
fn run_command_rejects_a_declared_status_on_binary_output() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("export"));
    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let sub_matches = matches.subcommand_matches("export").unwrap();

    let result = standout.run_command(
        "export",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            Ok(HandlerOutput::Binary {
                data: vec![0xDE, 0xAD],
                filename: "data.bin".into(),
            }
            .with_exit_status(ExitStatus::from(2)))
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    let message = status_without_a_carrier_message(result.unwrap_err());
    assert!(
        message.contains("exit status 2 was declared on binary output"),
        "{message}"
    );
}

#[test]
fn run_command_rejects_a_declared_success_status_on_binary_output() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("export"));
    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let sub_matches = matches.subcommand_matches("export").unwrap();

    let result = standout.run_command(
        "export",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            Ok(HandlerOutput::Binary {
                data: vec![0xDE, 0xAD],
                filename: "data.bin".into(),
            }
            .with_exit_status(ExitStatus::SUCCESS))
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    let message = status_without_a_carrier_message(result.unwrap_err());
    assert!(
        message.contains("exit status 0 was declared on binary output"),
        "{message}"
    );
}

#[test]
fn run_command_rejects_a_declared_success_status_on_artifact_output() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("export"));
    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let sub_matches = matches.subcommand_matches("export").unwrap();

    let result = standout.run_command(
        "export",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            Ok(HandlerOutput::Artifact(
                crate::cli::Artifact::new(vec![1u8]).suggest_destination("out.bin"),
            )
            .with_exit_status(ExitStatus::SUCCESS))
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    let message = status_without_a_carrier_message(result.unwrap_err());
    assert!(
        message.contains("exit status 0 was declared on artifact output"),
        "{message}"
    );
}

#[test]
fn run_command_rejects_a_declared_status_on_artifact_output() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("export"));
    let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
    let sub_matches = matches.subcommand_matches("export").unwrap();

    let result = standout.run_command(
        "export",
        sub_matches,
        FnHandler::new(|_m, _ctx| -> HandlerResult<()> {
            Ok(HandlerOutput::Artifact(
                crate::cli::Artifact::new(vec![1u8]).suggest_destination("out.bin"),
            )
            .with_exit_status(ExitStatus::from(2)))
        }),
        crate::TemplateRef::Absent,
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    let message = status_without_a_carrier_message(result.unwrap_err());
    assert!(
        message.contains("exit status 2 was declared on artifact output"),
        "{message}"
    );
}

#[test]
fn run_command_rejects_a_declared_status_a_post_output_hook_turns_into_bytes() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "test",
            Hooks::new().post_output(|_, _ctx, output| match output {
                RenderedOutput::Text(text) => Ok(RenderedOutput::Binary(
                    text.raw.into_bytes(),
                    "rendered.bin".into(),
                )),
                other => Ok(other),
            }),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| {
            Ok(HandlerOutput::Render(serde_json::json!({"value": 1}))
                .with_exit_status(ExitStatus::from(2)))
        }),
        crate::TemplateRef::Inline("{{ value }}".to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    let message = status_without_a_carrier_message(result.unwrap_err());
    assert!(
        message.contains("exit status 2 was declared on binary output"),
        "{message}"
    );
}

#[test]
fn run_command_drops_a_declared_status_on_render_output() {
    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| {
            Ok(HandlerOutput::Render(serde_json::json!({"value": 1}))
                .with_exit_status(ExitStatus::from(2)))
        }),
        crate::TemplateRef::Inline("{{ value }}".to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert_eq!(result.unwrap().as_text(), Some("1"));
}

#[test]
fn test_run_command_with_post_dispatch_hook() {
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct Data {
        value: i32,
    }

    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "test",
            Hooks::new().post_dispatch(|_, _ctx, mut data| {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("added_by_hook".into(), json!("yes").into());
                }
                Ok(data)
            }),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 }))),
        crate::TemplateRef::Inline(("value={{ value }}, added={{ added_by_hook }}").to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output.as_text(), Some("value=42, added=yes"));
}

#[test]
fn test_run_command_post_dispatch_abort() {
    use crate::cli::hooks::HookPhase;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Data {
        valid: bool,
    }

    let standout = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .hooks(
            "test",
            Hooks::new().post_dispatch(|_, _ctx, data| {
                if data
                    .get("valid")
                    .and_then(standout_render::RenderData::as_bool)
                    == Some(false)
                {
                    return Err(HookError::post_dispatch("invalid data"));
                }
                Ok(data)
            }),
        )
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    let sub_matches = matches.subcommand_matches("test").unwrap();

    let result = standout.run_command(
        "test",
        sub_matches,
        FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(Data { valid: false }))),
        crate::TemplateRef::Inline(("{{ valid }}").to_string()),
        ColorPolicy::Auto,
        StreamSink::new(Vec::new()),
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "invalid data");
    assert_eq!(err.phase, HookPhase::PostDispatch);
}
