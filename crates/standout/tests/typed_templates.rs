use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, AppBuilder, DispatchResult, EventsFnHandler, HandlerResult, Output};
use standout::cli::{FnHandler, Results};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout::{EmbeddedSource, Representation, TemplateResource};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("show", "report.txt"), ("show-4", "docs/output")];

static ORDERED_TEMPLATES: &[(&str, &str)] = &[("show.jinja", "Hello {{ name }}")];
static BAD_TEMPLATES: &[(&str, &str)] = &[("show.jinja", "{% if")];
static EVENT_ONLY_TEMPLATES: &[(&str, &str)] = &[("show.event", "starting {{ event.name }}")];
static SUMMARY_ONLY_TEMPLATES: &[(&str, &str)] = &[("show", "{{ done }} done")];

#[derive(serde::Serialize)]
struct Started {
    name: &'static str,
}

/// One incremental command named `show`, so a test varies only which of its
/// two templates the registry holds.
fn incremental_show_app(templates: &'static [(&'static str, &'static str)]) -> AppBuilder {
    App::builder()
        .templates(EmbeddedTemplates::new(templates, ""))
        .command_with(
            "show",
            EventsFnHandler::new(
                |_m, _ctx, results: &mut Results<Started>| -> HandlerResult<serde_json::Value> {
                    results.emit(Started { name: "web" })?;
                    Ok(Output::Render(json!({"done": 1})))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
}

fn command() -> Command {
    Command::new("app").subcommand(Command::new("show"))
}

fn group_command() -> Command {
    Command::new("app").subcommand(Command::new("db").subcommand(Command::new("show")))
}

fn build_error(builder: AppBuilder) -> String {
    match builder.build() {
        Ok(_) => panic!("expected build to fail"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn build_fails_for_missing_named_template_with_near_match() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                ORDERED_TEMPLATES,
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("shoe.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("command `show` references template `shoe.jinja`"));
    assert!(error.contains(".templates(embed_templates!"));
    assert!(error.contains(".templates_dir(\"path/to/templates\")"));
    assert!(error.contains("`show.jinja`"));
}

#[test]
fn build_fails_for_missing_template_registry_with_builder_call() {
    let error = build_error(
        App::builder()
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("show.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("command `show` references template `show.jinja`"));
    assert!(error.contains("no application templates are configured"));
    assert!(error.contains(".templates(embed_templates!"));
    assert!(error.contains(".templates_dir(\"path/to/templates\")"));
    assert!(!error.contains("standout/list-view"), "{error}");
}

#[test]
fn build_fails_for_missing_named_template_with_available_names() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                &[
                    ("alpha.jinja", "Alpha {{ name }}"),
                    ("beta.jinja", "Beta {{ name }}"),
                ],
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("unmatchable-template-name.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("available templates"), "{error}");
    assert!(error.contains("`alpha.jinja`"));
    assert!(error.contains("`beta.jinja`"));
}

#[test]
fn available_template_names_are_sorted_unique_and_limited() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                &[
                    ("zeta.jinja", "Zeta {{ name }}"),
                    ("alpha.jinja", "Alpha {{ name }}"),
                    ("beta.jinja", "Beta {{ name }}"),
                    ("gamma.jinja", "Gamma {{ name }}"),
                    ("delta.jinja", "Delta {{ name }}"),
                    ("epsilon.jinja", "Epsilon {{ name }}"),
                ],
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("unmatchable-template-name.jinja"),
            )
            .unwrap(),
    );

    assert!(
        error.contains(
            "available templates: `alpha.jinja`, `beta.jinja`, `delta.jinja`, `epsilon.jinja`, `gamma.jinja`"
        ),
        "{error}"
    );
    assert!(!error.contains("`zeta.jinja`"), "{error}");
}

#[test]
fn template_suggestions_follow_extension_priority() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                &[
                    ("report.j2", "Short {{ name }}"),
                    ("report.jinja", "Standard {{ name }}"),
                ],
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("report.jinj"),
            )
            .unwrap(),
    );

    assert!(error.contains("did you mean `report.jinja`?"), "{error}");
    assert!(!error.contains("`report.j2`"), "{error}");
}

#[test]
fn templates_after_commands_resolve_at_build() {
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "show",
                |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
                |cfg| cfg.template_name("show.jinja"),
            )
        })
        .unwrap()
        .templates(EmbeddedSource::<TemplateResource>::new(
            ORDERED_TEMPLATES,
            "/path/that/does/not/exist",
        ))
        .build()
        .unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
fn build_fails_when_registered_template_does_not_compile() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                BAD_TEMPLATES,
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg.template_name("show.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("template error"));
}

#[test]
fn named_template_renders_through_extension_fallback() {
    let app = App::builder()
        .templates(EmbeddedSource::<TemplateResource>::new(
            ORDERED_TEMPLATES,
            "/path/that/does/not/exist",
        ))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show.j2"),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
fn explicit_structured_only_without_application_registry_allows_structured_output() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result =
        TestHarness::new()
            .output_mode(Representation::Json)
            .run(&app, command(), ["app", "show"]);

    result.assert_success();
    assert_eq!(result.stdout(), "{\n  \"name\": \"Ada\"\n}");
}

#[test]
fn convention_resolves_a_jinja_entry_without_extension_configuration() {
    let app = App::builder()
        .templates(EmbeddedSource::<TemplateResource>::new(
            ORDERED_TEMPLATES,
            "/path/that/does/not/exist",
        ))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
fn convention_resolves_a_nested_jinja_entry_without_extension_configuration() {
    let app = App::builder()
        .templates(EmbeddedSource::<TemplateResource>::new(
            &[("db/show.jinja", "Hello {{ name }}")],
            "/path/that/does/not/exist",
        ))
        .commands(|g| {
            g.group("db", |g| {
                g.command_with(
                    "show",
                    |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
                    |cfg| cfg,
                )
            })
        })
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new().color(ColorPolicy::Never).run(
        &app,
        group_command(),
        ["app", "db", "show"],
    );

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
#[serial]
fn templates_dir_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("show.jinja"), "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Hello Ada");

    std::fs::write(dir.path().join("show.jinja"), "Bye {{ name }}").unwrap();

    let second = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "Bye Ada");
}

#[test]
#[serial]
fn file_backed_extended_template_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.jinja");
    std::fs::write(&base_path, "Old {% block body %}{% endblock %}").unwrap();
    std::fs::write(
        dir.path().join("show.jinja"),
        "{% extends 'base' %}{% block body %}{{ name }}{% endblock %}",
    )
    .unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Old Ada");

    std::fs::write(&base_path, "New {% block body %}{% endblock %}").unwrap();

    let second = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "New Ada");
}

#[test]
#[serial]
fn file_backed_imported_template_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    let macros_path = dir.path().join("macros.jinja");
    std::fs::write(
        &macros_path,
        "{% macro greeting(name) %}Hello {{ name }}{% endmacro %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("show.jinja"),
        "{% import 'macros' as macros %}{{ macros.greeting(name) }}",
    )
    .unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Hello Ada");

    std::fs::write(
        &macros_path,
        "{% macro greeting(name) %}Bye {{ name }}{% endmacro %}",
    )
    .unwrap();

    let second = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "Bye Ada");
}

#[test]
#[serial]
fn file_backed_dynamic_include_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("partial.jinja");
    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();
    std::fs::write(
        dir.path().join("show.jinja"),
        "{% include partial ~ suffix %}",
    )
    .unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Render(
                    json!({"name": "Ada", "partial": "partial", "suffix": ""}),
                ))
            }),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Hello Ada");

    std::fs::write(&partial_path, "Bye {{ name }}").unwrap();

    let second = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "Bye Ada");
}

#[test]
#[serial]
fn file_backed_dynamic_include_discovers_new_template_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("partial.jinja");
    std::fs::write(
        dir.path().join("show.jinja"),
        "{% include partial ~ suffix %}",
    )
    .unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Render(
                    json!({"name": "Ada", "partial": "partial", "suffix": ""}),
                ))
            }),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
#[serial]
fn file_backed_whitespace_control_include_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("partial.jinja");
    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();
    std::fs::write(dir.path().join("show.jinja"), "{%- include 'partial' -%}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Hello Ada");

    std::fs::write(&partial_path, "Bye {{ name }}").unwrap();

    let second = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "Bye Ada");
}

#[test]
#[serial]
fn deleted_file_backed_template_errors_at_render() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::remove_file(&template_path).unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    result.assert_error_contains("template `show`");
    result.assert_error_contains(&template_path.display().to_string());
}

#[test]
#[serial]
fn corrupted_file_backed_template_errors_at_render() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::write(&template_path, "{% if").unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    result.assert_error_contains("template `show`");
    result.assert_error_contains(&template_path.display().to_string());
}

#[test]
#[serial]
fn corrupted_file_backed_include_names_include_path_at_render() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    let partial_path = dir.path().join("_partial.jinja");
    std::fs::write(&template_path, "Hello {% include '_partial' %}").unwrap();
    std::fs::write(&partial_path, "{{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::write(&partial_path, "{% if").unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    result.assert_error_contains("template `_partial`");
    result.assert_error_contains(&partial_path.display().to_string());
}

#[test]
#[serial]
fn app_render_refreshes_changed_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("_partial.jinja");
    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();
    std::fs::write(dir.path().join("show.jinja"), "{% include '_partial' %}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        "Hello Ada"
    );

    std::fs::write(&partial_path, "Bye {{ name }}").unwrap();

    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        "Bye Ada"
    );
}

#[test]
#[serial]
fn app_render_refreshes_newly_added_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let show_path = dir.path().join("show.jinja");
    std::fs::write(&show_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        "Hello Ada"
    );

    std::fs::write(dir.path().join("_partial.jinja"), "Bye {{ name }}").unwrap();
    std::fs::write(&show_path, "{% include '_partial' %}").unwrap();

    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        "Bye Ada"
    );
}

#[test]
#[serial]
fn app_render_reports_deleted_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("_partial.jinja");
    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();
    std::fs::write(dir.path().join("show.jinja"), "{% include '_partial' %}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();
    std::fs::remove_file(&partial_path).unwrap();

    let error = app
        .render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .unwrap_err()
        .to_string();

    assert!(error.contains("template `_partial`"), "{error}");
}

#[test]
#[serial]
fn app_render_reports_corrupted_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let partial_path = dir.path().join("_partial.jinja");
    std::fs::write(&partial_path, "Hello {{ name }}").unwrap();
    std::fs::write(dir.path().join("show.jinja"), "{% include '_partial' %}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();
    std::fs::write(&partial_path, "{% if").unwrap();

    let error = app
        .render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .unwrap_err()
        .to_string();

    assert!(error.contains("template `_partial`"), "{error}");
    assert!(
        error.contains(&partial_path.display().to_string()),
        "{error}"
    );
}

#[test]
#[serial]
fn standalone_app_render_warning_cannot_leak_into_a_later_run() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::write(&template_path, "[missing]Hello {{ name }}[/missing]").unwrap();
    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        "Hello Ada"
    );

    std::fs::write(&template_path, "Hello {{ name }}").unwrap();
    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .run(&app, command(), ["app", "show"]);
    result.assert_success();
    assert!(result.warnings().is_empty(), "{:?}", result.warnings());
}

#[test]
#[serial]
fn dependency_scanner_ignores_non_tag_syntax_and_quoted_delimiters() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("_partial.jinja"), "Hello {{ name }}").unwrap();
    std::fs::write(
        dir.path().join("show.jinja"),
        concat!(
            "{# example: {% include 'missing-comment' %} #}",
            "{{ \"{% include 'missing-variable' %}\" }}",
            "{{ \"}} {% include 'missing-variable-delimiter' %}\" }}",
            "{% set marker = \"%} {% include 'missing-statement-delimiter' %}\" %}",
            "{{ marker }}",
            "{% raw %}{% include 'missing-raw' %}{% endraw %}",
            "{% include '_partial' %}",
        ),
    )
    .unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        app.render_with(
            standout::TemplateRef::Named(("show").to_string()),
            &json!({"name": "Ada"}),
            Representation::Human,
            standout::TargetProperties::detect()
        )
        .unwrap(),
        concat!(
            "{% include 'missing-variable' %}",
            "}} {% include 'missing-variable-delimiter' %}",
            "%} {% include 'missing-statement-delimiter' %}",
            "{% include 'missing-raw' %}",
            "Hello Ada",
        )
    );
}

#[test]
fn structured_only_serializes_every_representation_but_the_style_tag_view() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    for mode in [
        Representation::Human,
        Representation::Json,
        Representation::Yaml,
        Representation::Csv,
    ] {
        let matches = command().try_get_matches_from(["app", "show"]).unwrap();
        let result = app.dispatch(matches, mode);
        assert!(
            matches!(result.outcome(), DispatchResult::Handled(_)),
            "expected {mode:?} to serialize, got {result:?}"
        );
    }

    let matches = command().try_get_matches_from(["app", "show"]).unwrap();
    match app
        .dispatch(matches, Representation::TermDebug)
        .into_outcome()
    {
        DispatchResult::Error(error) => {
            let message = error.to_string();
            assert!(message.contains("command `show` is declared structured-only"));
            assert!(message.contains("--output"));
            assert!(message.contains(".template(...)"));
            assert!(message.contains(".template_name(...)"));
        }
        other => panic!("expected term-debug to reject structured-only output, got {other:?}"),
    }
}

#[test]
#[serial]
fn structured_only_omitted_output_serializes_json_through_run_to_string() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new().run(&app, command(), ["app", "show"]);

    result.assert_success();
    let value: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(value["name"], "Ada");
}

#[test]
fn build_fails_for_a_batch_command_with_only_an_event_template() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedTemplates::new(EVENT_ONLY_TEMPLATES, ""))
            .command_with(
                "show",
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"name": "Ada"})))),
                |cfg| cfg,
            )
            .unwrap(),
    );

    assert!(
        error.contains("command `show` references template `show`"),
        "{error}"
    );
}

#[test]
fn build_fails_for_an_incremental_command_with_only_a_summary_template() {
    let error = build_error(incremental_show_app(SUMMARY_ONLY_TEMPLATES));

    assert!(
        error.contains("renders each event from template `show.event`"),
        "{error}"
    );
}

/// A handler that returns `Output::Silent` renders no summary, so the event
/// template alone is the whole presentation.
#[test]
fn an_incremental_command_builds_with_only_an_event_template() {
    incremental_show_app(EVENT_ONLY_TEMPLATES).build().unwrap();
}
