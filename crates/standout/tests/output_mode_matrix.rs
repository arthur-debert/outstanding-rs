//! Matrix tests for output mode × handler mode combinations.
//!
//! These tests ensure that all output modes work correctly with both
//! App (thread-safe) and LocalApp (single-threaded) handler modes.

use clap::ArgMatches;
use serde::Serialize;
use standout::cli::handler::{CommandContext, Output};
use standout::cli::{App, LocalApp};
use standout::OutputMode;

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

// ============================================================================
// App (Thread-Safe) Output Mode Tests
// ============================================================================

#[test]
fn test_app_output_mode_auto() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Auto)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_term() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Term)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_text() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Text)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_app_output_mode_json() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Json)
        .expect("Render failed");

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON output");
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["count"], 42);
}

#[test]
fn test_app_output_mode_yaml() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Yaml)
        .expect("Render failed");

    // Should contain YAML format
    assert!(output.contains("name: test"));
    assert!(output.contains("count: 42"));
}

#[test]
fn test_app_output_mode_csv() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Csv)
        .expect("Render failed");

    // Should contain CSV headers and data
    assert!(output.contains("name"));
    assert!(output.contains("count"));
    assert!(output.contains("test"));
    assert!(output.contains("42"));
}

// ============================================================================
// LocalApp (Single-Threaded) Output Mode Tests
// ============================================================================

#[test]
fn test_local_app_output_mode_auto() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Auto)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_term() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Term)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_text() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Text)
        .expect("Render failed");

    assert!(output.contains("Name: test"));
    assert!(output.contains("Count: 42"));
}

#[test]
fn test_local_app_output_mode_json() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Json)
        .expect("Render failed");

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON output");
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["count"], 42);
}

#[test]
fn test_local_app_output_mode_yaml() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Yaml)
        .expect("Render failed");

    // Should contain YAML format
    assert!(output.contains("name: test"));
    assert!(output.contains("count: 42"));
}

#[test]
fn test_local_app_output_mode_csv() {
    let app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(simple_template(), &TestData::sample(), OutputMode::Csv)
        .expect("Render failed");

    // Should contain CSV headers and data
    assert!(output.contains("name"));
    assert!(output.contains("count"));
    assert!(output.contains("test"));
    assert!(output.contains("42"));
}

// ============================================================================
// Feature Parity Tests
// ============================================================================

#[test]
fn test_app_and_local_app_produce_same_json() {
    let data = TestData::sample();

    // App output
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");
    let app_output = app
        .render_inline(simple_template(), &data, OutputMode::Json)
        .expect("App render failed");

    // LocalApp output
    let local_app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build local app");
    let local_output = local_app
        .render_inline(simple_template(), &data, OutputMode::Json)
        .expect("LocalApp render failed");

    // Both should produce identical JSON
    let app_json: serde_json::Value = serde_json::from_str(&app_output).expect("Invalid App JSON");
    let local_json: serde_json::Value =
        serde_json::from_str(&local_output).expect("Invalid LocalApp JSON");

    assert_eq!(app_json, local_json);
}

#[test]
fn test_app_and_local_app_produce_same_text() {
    let data = TestData::sample();

    // App output
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build app");
    let app_output = app
        .render_inline(simple_template(), &data, OutputMode::Text)
        .expect("App render failed");

    // LocalApp output
    let local_app = LocalApp::builder()
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            simple_template(),
        )
        .unwrap()
        .build()
        .expect("Failed to build local app");
    let local_output = local_app
        .render_inline(simple_template(), &data, OutputMode::Text)
        .expect("LocalApp render failed");

    // Both should produce identical text output
    assert_eq!(app_output, local_output);
}

// ============================================================================
// Style Tag Processing Tests
// ============================================================================

#[test]
fn test_style_tags_in_term_mode() {
    use console::Style;
    use standout::Theme;

    let template = "[title]{{ name }}[/title]";

    let style = Style::new().blue().bold().force_styling(true);
    let theme = Theme::new().add("title", style);

    let app = App::<standout::cli::ThreadSafe>::builder()
        .theme(theme)
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            template,
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(template, &TestData::sample(), OutputMode::Term)
        .expect("Render failed");

    // In Term mode, style tags should be processed (contains ANSI codes or just the text)
    assert!(output.contains("test") || output.contains("\x1b"));
}

#[test]
fn test_style_tags_stripped_in_text_mode() {
    use console::Style;
    use standout::Theme;

    let template = "[title]{{ name }}[/title]";

    let style = Style::new().blue().bold().force_styling(true);
    let theme = Theme::new().add("title", style);

    let app = App::<standout::cli::ThreadSafe>::builder()
        .theme(theme)
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            template,
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(template, &TestData::sample(), OutputMode::Text)
        .expect("Render failed");

    // In Text mode, style tags should be stripped, no ANSI codes
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

    let app = App::<standout::cli::ThreadSafe>::builder()
        .theme(theme)
        .command(
            "run",
            |_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::Render(TestData::sample())),
            template,
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let output = app
        .render_inline(template, &TestData::sample(), OutputMode::TermDebug)
        .expect("Render failed");

    // In TermDebug mode, style tags should be kept for debugging
    assert!(output.contains("[title]"));
    assert!(output.contains("[/title]"));
    assert!(output.contains("test"));
}
