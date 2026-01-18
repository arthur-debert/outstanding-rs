use console::Style;
use serde::Serialize;
use standout::{OutputMode, Renderer, Theme};

#[derive(Serialize)]
struct Empty {}

#[test]
fn test_nesting_complex() {
    let theme = Theme::new()
        .add("title", Style::new().bold())
        .add("critical", Style::new().red());

    let mut renderer = Renderer::with_output(theme, OutputMode::Term).unwrap();

    renderer
        .add_template("inner", "Inner: [critical]CRIT[/critical]")
        .unwrap();
    renderer
        .add_template("outer", "[title]Outer\n{% include 'inner' %}\nEnd[/title]")
        .unwrap();

    let output = renderer.render("outer", &Empty {}).unwrap();
    println!("Output Complex: {:?}", output);

    // Validate output contains expected sequences
    // We expect "Outer" to be bold
    // "Inner: " to be bold (inherited from outer)
    // "CRIT" to be bold AND red
    // "End" to be bold
}
