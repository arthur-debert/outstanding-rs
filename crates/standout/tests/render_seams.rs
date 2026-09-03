use clap::Command;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout_test::TestHarness;

const UNSIZED_LIST: &str = r#"{% set t = tabular([
    {"width": {"min": 0}},
    {"width": {"min": 0}},
    {"width": {"min": 0}}
], separator="  ") %}{% for row in rows %}{{ t.row(row) }}
{% endfor %}"#;

const SIZED_LIST: &str = r#"{% set t = tabular([
    {"width": {"min": 0}},
    {"width": {"min": 0}},
    {"width": {"min": 0}}
], separator="  ", rows=rows) %}{% for row in rows %}{{ t.row(row) }}
{% endfor %}"#;

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(
            &[("list", UNSIZED_LIST), ("list-sized", SIZED_LIST)],
            "",
        ))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(rows()))),
            |cfg| cfg.template_name("list"),
        )
        .unwrap()
        .command_with(
            "list-sized",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(rows()))),
            |cfg| cfg.template_name("list-sized"),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn rows() -> serde_json::Value {
    json!({"rows": [
        ["#12", "open", "Add pagination to the repository list"],
        ["#7", "merged", "Fix retry backoff"],
    ]})
}

fn command() -> Command {
    Command::new("ghlike")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("list-sized"))
}

fn lines(args: [&str; 2]) -> Vec<String> {
    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .terminal_width(80)
        .run(&app(), command(), args);
    result.assert_success();
    result
        .stdout_plain()
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect()
}

#[test]
fn min_width_columns_collapse_without_the_rows_to_measure() {
    assert_eq!(
        lines(["ghlike", "list"]),
        vec![
            "    Add pagination to the repository list",
            "    Fix retry backoff",
        ],
        "a column bounded by `min` alone has nothing to grow it, which is the \
         collapse the ghlike pilot run reported"
    );
}

#[test]
fn min_width_columns_grow_to_fit_every_row() {
    assert_eq!(
        lines(["ghlike", "list-sized"]),
        vec![
            "#12  open    Add pagination to the repository list",
            "#7   merged  Fix retry backoff",
        ]
    );
}
