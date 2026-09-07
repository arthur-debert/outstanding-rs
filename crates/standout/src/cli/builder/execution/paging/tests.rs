use super::*;
use crate::cli::builder::test_support::EXECUTION_TEMPLATES as TEMPLATES;
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::EmbeddedTemplates;
use crate::{ColorPolicy, Representation};
use clap::Command;

#[test]
#[serial_test::serial]
fn resolve_run_decides_paging_for_every_entry_point() {
    let app = App::builder()
        .name("myapp")
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(serde_json::json!({})))),
            |cfg| cfg.pageable(),
        )
        .unwrap()
        .command_with(
            "add",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(serde_json::json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = app.augment_framework_surface(
        Command::new("myapp")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add")),
    );
    let mut target = TargetProperties::detect();
    target.stdout_is_terminal = true;
    let resolve = |args: &[&str]| {
        let matches = cmd.clone().get_matches_from(args);
        app.resolve_run(
            &matches,
            None,
            None,
            ColorPolicy::Auto,
            Representation::Human,
            target,
        )
        .pager
        .map(|pager| pager.command().to_string())
    };

    let env = standout_test::ScopedEnv::new()
        .set("MYAPP_PAGER", "sed -n 1p")
        .remove("PAGER");

    assert_eq!(resolve(&["myapp", "list"]), Some("sed -n 1p".to_string()));
    assert_eq!(resolve(&["myapp", "list", "--no-pager"]), None);
    assert_eq!(resolve(&["myapp", "add"]), None);

    let _env = env.remove("MYAPP_PAGER");
    assert_eq!(resolve(&["myapp", "list"]), None);
}
