use super::*;

#[test]
#[serial]
fn simple_handler_returns_rendered_text() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "echo", "hello"]);
    result.assert_success();
    result.assert_stdout_eq("hello");
    result.assert_exit_status(ExitStatus::SUCCESS);
}
#[test]
#[serial]
fn output_mode_override_forces_json() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        echo_command(),
        vec!["app", "echo", "hello"],
    );
    let out = result.stdout();
    assert!(out.contains("\"msg\""));
    assert!(out.contains("\"hello\""));
}
#[test]
#[serial]
fn rustloc_fixture_uses_configured_csv_projection() {
    let projection = StructuredOutputProjection::csv(
        CsvProjection::builder("items")
            .column(
                Column::new(Width::default())
                    .key("language")
                    .header("LANGUAGE"),
            )
            .column(Column::new(Width::default()).key("code").header("CODE"))
            .derived_column(
                Column::new(Width::default()).key("net").header("NET"),
                |row, _root| {
                    json!(row["code"].as_i64().unwrap_or(0) - row["comments"].as_i64().unwrap_or(0))
                },
            )
            .synthetic_row(|root| {
                json!({
                    "language": "TOTAL",
                    "code": root["totals"]["code"],
                    "comments": root["totals"]["comments"]
                })
            })
            .conditional_row(|root| {
                (root["skipped"].as_u64().unwrap_or(0) > 0)
                    .then(|| json!({ "language": "SKIPPED" }))
            })
            .build(),
    );
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "summary",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(json!({
                    "items": [
                        { "language": "Rust", "code": 120, "comments": 20 },
                        { "language": "Python", "code": 70, "comments": 10 }
                    ],
                    "totals": { "code": 190, "comments": 30 },
                    "skipped": 1
                })))
            }),
            |config| {
                config
                    .structured_only()
                    .structured_output_projection(projection)
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("rustloc").subcommand(Command::new("summary"));
    let result =
        TestHarness::new()
            .output_mode(Representation::Csv)
            .run(&app, cmd, ["rustloc", "summary"]);
    result.assert_stdout_eq(
        "LANGUAGE,CODE,NET\nRust,120,100\nPython,70,60\nTOTAL,190,160\nSKIPPED,-,0\n",
    );
}
#[test]
#[serial]
fn output_flag_name_is_configurable() {
    let app = standout::cli::App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_flag(Some("format"))
        .command_with(
            "echo",
            FnHandler::new(|m, _ctx| {
                let msg = m
                    .get_one::<String>("msg")
                    .cloned()
                    .unwrap_or_else(|| "no-arg".into());
                Ok(Output::Render(json!({ "msg": msg })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new()
        .output_mode(Representation::Json)
        .output_flag_name("format")
        .run(&app, echo_command(), vec!["app", "echo", "hello"]);
    let out = result.stdout();
    assert!(out.contains("\"msg\""), "expected JSON output, got: {out}");
    assert!(out.contains("\"hello\""));
}
