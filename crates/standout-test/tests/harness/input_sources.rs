use super::*;

#[test]
#[serial]
fn piped_stdin_reaches_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "read",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("nothing".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("read"));
    let result = TestHarness::new()
        .piped_stdin("piped-in")
        .run(&app, cmd, vec!["app", "read"]);
    result.assert_stdout_eq("piped-in");
}
#[test]
#[serial]
fn interactive_stdin_falls_through_to_default() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "read",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(StdinSource::new())
                    .default("no-pipe".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("read"));
    let result = TestHarness::new()
        .interactive_stdin()
        .run(&app, cmd, vec!["app", "read"]);
    result.assert_stdout_eq("no-pipe");
}
#[test]
#[serial]
fn clipboard_reaches_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "paste",
            FnHandler::new(|_m, ctx| {
                let v = InputChain::<String>::new()
                    .try_source(ClipboardSource::new())
                    .default("empty".into())
                    .resolve_from(_m, ctx.input_sources())
                    .unwrap();
                Ok(Output::Render(json!({ "val": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("paste"));
    let result =
        TestHarness::new()
            .clipboard("clipboard-content")
            .run(&app, cmd, vec!["app", "paste"]);
    result.assert_stdout_eq("clipboard-content");
}
#[test]
#[serial]
fn scripted_prompts_drive_a_wizard_handler() {
    use standout_input::{
        ConfirmPromptSource, PromptResponse, ScriptedResponder, TextPromptSource,
    };
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let sources = ctx.input_sources();
                let name = TextPromptSource::new("Name: ")
                    .prompt_from(sources)
                    .unwrap();
                let proceed = ConfirmPromptSource::new("Continue? ")
                    .prompt_from(sources)
                    .unwrap();
                let title = TextPromptSource::new("Title: ")
                    .prompt_from(sources)
                    .unwrap();
                Ok(Output::Render(json!({
                    "name": name,
                    "proceed": proceed,
                    "title": title,
                })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let responder = Arc::new(ScriptedResponder::new([
        PromptResponse::text("Ada"),
        PromptResponse::Bool(true),
        PromptResponse::text("Engineer"),
    ]));
    let result = TestHarness::new()
        .prompts(responder)
        .run(&app, cmd, vec!["app", "wizard"]);
    result.assert_stdout_eq("Ada/true/Engineer");
}
#[test]
#[serial]
fn scripted_cancel_propagates_to_handler() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let body = match TextPromptSource::new("Name: ").prompt_from(ctx.input_sources()) {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            }),
            |cfg| cfg.template_name("wizard-2"),
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let responder = Arc::new(ScriptedResponder::new([PromptResponse::Cancel]));
    let result = TestHarness::new()
        .prompts(responder)
        .run(&app, cmd, vec!["app", "wizard"]);
    result.assert_stdout_contains("err:");
    result.assert_stdout_contains("cancelled");
}
#[test]
#[serial]
fn responder_is_reset_between_runs() {
    use standout_input::{PromptResponse, ScriptedResponder, TextPromptSource};
    use std::sync::Arc;
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "wizard",
            FnHandler::new(|_m, ctx| {
                let body = match TextPromptSource::new("Name: ").prompt_from(ctx.input_sources()) {
                    Ok(name) => format!("ok:{name}"),
                    Err(e) => format!("err:{e}"),
                };
                Ok(Output::Render(json!({ "body": body })))
            }),
            |cfg| cfg.template_name("wizard-2"),
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("wizard"));
    let first = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([PromptResponse::text(
            "Ada",
        )])))
        .run(&app, cmd.clone(), vec!["app", "wizard"]);
    first.assert_stdout_eq("ok:Ada");
    drop(first);
    let second = TestHarness::new().run(&app, cmd, vec!["app", "wizard"]);
    second.assert_stdout_contains("err:");
}
