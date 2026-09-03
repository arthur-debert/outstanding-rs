//! Visual test for alternating table row background colors with tint variants.
//!
//! Run with: cargo run -p standout-render --example table_row_styles

use serde::Serialize;
use standout_render::{render_with_mode, ColorMode, ColorPolicy, Representation, Theme};

#[derive(Serialize)]
struct Data {
    rows: Vec<Row>,
}

#[derive(Clone, Serialize)]
struct Row {
    file: String,
    code: String,
    tests: String,
    docs: String,
    total: String,
}

fn main() {
    console::set_colors_enabled(true);

    let rows = vec![
        r(
            "crates/dodot-lib/src/testing/mod.rs",
            "241",
            "172",
            "60",
            "572",
        ),
        r(
            "crates/dodot-lib/src/rules/mod.rs",
            "230",
            "381",
            "45",
            "777",
        ),
        r(
            "crates/dodot-lib/src/execution/mod.rs",
            "174",
            "211",
            "12",
            "459",
        ),
        r(
            "crates/dodot-lib/src/handlers/symlink.rs",
            "172",
            "171",
            "22",
            "463",
        ),
        r(
            "crates/dodot-lib/src/paths/mod.rs",
            "167",
            "93",
            "40",
            "363",
        ),
        r("crates/dodot-cli/src/main.rs", "162", "0", "0", "175"),
        r("crates/dodot-lib/src/commands/up.rs", "91", "0", "3", "106"),
        r(
            "crates/dodot-lib/src/commands/mod.rs",
            "85",
            "2",
            "14",
            "114",
        ),
        r(
            "crates/dodot-lib/src/commands/fill.rs",
            "85",
            "100",
            "10",
            "231",
        ),
        r(
            "crates/dodot-lib/src/render/mod.rs",
            "83",
            "67",
            "19",
            "207",
        ),
        r(
            "crates/dodot-lib/src/datastore/mod.rs",
            "79",
            "0",
            "56",
            "156",
        ),
        r(
            "crates/dodot-lib/src/handlers/mod.rs",
            "69",
            "37",
            "49",
            "175",
        ),
        r(
            "crates/dodot-lib/src/packs/mod.rs",
            "64",
            "112",
            "19",
            "236",
        ),
        r(
            "crates/dodot-lib/src/commands/status.rs",
            "62",
            "0",
            "2",
            "74",
        ),
        r(
            "crates/dodot-lib/src/commands/down.rs",
            "55",
            "0",
            "2",
            "67",
        ),
        r(
            "crates/dodot-lib/src/commands/adopt.rs",
            "52",
            "0",
            "3",
            "73",
        ),
    ];

    let tints = ["gray", "blue", "red", "green", "purple"];
    let theme = Theme::default().add("header", console::Style::new().cyan().bold());

    for mode in [ColorMode::Dark, ColorMode::Light] {
        let mode_name = match mode {
            ColorMode::Dark => "DARK",
            ColorMode::Light => "LIGHT",
        };

        println!("\n{}", "=".repeat(60));
        println!("  {} MODE", mode_name);
        println!("{}\n", "=".repeat(60));

        for tint in &tints {
            let template = format!(
                r#"
{{%- set t = table(
    [{{"width": "fill"}},
     {{"width": 16, "align": "right"}},
     {{"width": 16, "align": "right"}},
     {{"width": 16, "align": "right"}},
     {{"width": 16, "align": "right"}}],
    separator="  ",
    header=["File", "Code", "Tests", "Docs", "Total"],
    header_style="header",
    row_styles="{}",
    width=110
) -%}}
{{{{ t.header_row() }}}}
{{%- for _ in range(110) %}}─{{%- endfor %}}
{{%- for row in rows %}}
{{{{ t.row([row.file, row.code, row.tests, row.docs, row.total]) }}}}
{{%- endfor %}}
{{%- for _ in range(110) %}}─{{%- endfor %}}
{{{{ t.row(["Total (35 files)", "3334", "2690", "589", "8002"]) }}}}
"#,
                tint
            );

            let data = Data { rows: rows.clone() };
            let output = render_with_mode(
                &template,
                &data,
                &theme,
                Representation::Human,
                ColorPolicy::Always,
                mode,
            )
            .unwrap();
            println!("--- tint: {} ---\n{}\n", tint, output);
        }
    }
}

fn r(file: &str, code: &str, tests: &str, docs: &str, total: &str) -> Row {
    Row {
        file: file.to_string(),
        code: code.to_string(),
        tests: tests.to_string(),
        docs: docs.to_string(),
        total: total.to_string(),
    }
}
