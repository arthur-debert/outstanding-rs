use clap::ArgMatches;
use serde::Serialize;
use standout::cli::handler::{CommandContext, Output};
use standout::cli::App;
use standout::cli::FnHandler;
use standout::EmbeddedTemplates;
use standout::Representation;

const TEMPLATES: &[(&str, &str)] = &[("run", "Name: {{ name }}, Count: {{ count }}")];

#[derive(Serialize)]
struct TestData {
    name: String,
    count: i32,
    items: Vec<String>,
}

impl TestData {
    fn sample() -> Self {
        Self {
            name: "test".to_string(),
            count: 42,
            items: vec!["a".to_string(), "b".to_string()],
        }
    }
}

fn simple_template() -> &'static str {
    "Name: {{ name }}, Count: {{ count }}"
}

#[test]
fn test_app_output_mode_auto() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_term() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_text() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_json() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Json,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON output");
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["count"], 42);
}

#[test]
fn test_app_output_mode_yaml() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Yaml,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("name: test"));
    assert!(output.contains("count: 42"));
}

#[test]
fn test_app_output_mode_csv() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let error = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Csv,
            standout::TargetProperties::detect(),
        )
        .expect_err("a nested `items` array is not a flat record")
        .to_string();
    assert!(error.contains("`items` is an array"), "{error}");
    assert!(error.contains("CsvProjection"), "{error}");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &serde_json::json!({ "name": "test", "count": 42 }),
            Representation::Csv,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");
    assert_eq!(output, "name,count\ntest,42\n");
}

#[test]
fn test_local_app_output_mode_auto() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_term() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_text() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_json() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Json,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON output");
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["count"], 42);
}

#[test]
fn test_local_app_output_mode_yaml() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Yaml,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("name: test"));
    assert!(output.contains("count: 42"));
}

#[test]
fn test_local_app_output_mode_csv() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let error = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &TestData::sample(),
            Representation::Csv,
            standout::TargetProperties::detect(),
        )
        .expect_err("a nested `items` array is not a flat record")
        .to_string();
    assert!(error.contains("CsvProjection"), "{error}");
}

#[test]
fn test_render_inline_json_consistency() {
    let data = TestData::sample();

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output1 = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &data,
            Representation::Json,
            standout::TargetProperties::detect(),
        )
        .expect("First render failed");
    let output2 = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &data,
            Representation::Json,
            standout::TargetProperties::detect(),
        )
        .expect("Second render failed");

    let json1: serde_json::Value = serde_json::from_str(&output1).expect("Invalid JSON");
    let json2: serde_json::Value = serde_json::from_str(&output2).expect("Invalid JSON");

    assert_eq!(json1, json2);
}

#[test]
fn test_render_inline_text_consistency() {
    let data = TestData::sample();

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output1 = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &data,
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("First render failed");
    let output2 = app
        .render_with(
            standout::TemplateRef::Inline((simple_template()).to_string()),
            &data,
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Second render failed");

    assert_eq!(output1, output2);
}

#[test]
fn test_style_tags_in_term_mode() {
    use console::Style;
    use standout::Theme;

    let template = "[title]{{ name }}[/title]";

    let style = Style::new().blue().bold().force_styling(true);
    let theme = Theme::new().add("title", style);

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((template).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("test") || output.contains("\x1b"));
}

#[test]
fn test_style_tags_stripped_in_text_mode() {
    use console::Style;
    use standout::Theme;

    let template = "[title]{{ name }}[/title]";

    let style = Style::new().blue().bold().force_styling(true);
    let theme = Theme::new().add("title", style);

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((template).to_string()),
            &TestData::sample(),
            Representation::Human,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("test"));
    assert!(!output.contains("\x1b"));
    assert!(!output.contains("[title]"));
}

#[test]
fn test_style_tags_kept_in_term_debug_mode() {
    use console::Style;
    use standout::Theme;

    let template = "[title]{{ name }}[/title]";

    let style = Style::new().blue().bold().force_styling(true);
    let theme = Theme::new().add("title", style);

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "run",
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(TestData::sample()))
            }),
            |cfg| cfg.template_name("run"),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_with(
            standout::TemplateRef::Inline((template).to_string()),
            &TestData::sample(),
            Representation::TermDebug,
            standout::TargetProperties::detect(),
        )
        .expect("Render failed");

    assert!(output.contains("[title]"));
    assert!(output.contains("[/title]"));
    assert!(output.contains("test"));
}
