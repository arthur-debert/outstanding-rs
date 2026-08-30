use console::Style;
use standout_bbparser::{BBParser, TagTransform};
use std::collections::HashMap;

fn test_styles() -> HashMap<String, Style> {
    let mut styles = HashMap::new();
    styles.insert("red".to_string(), Style::new().red().force_styling(true));
    styles.insert("bold".to_string(), Style::new().bold().force_styling(true));
    styles
}

#[test]
fn test_output_modes() {
    let styles = test_styles();
    let input = "[red]hello[/red] [bold]world[/bold]";

    let parser = BBParser::new(styles.clone(), TagTransform::Keep);
    assert_eq!(parser.parse(input), input);

    let parser = BBParser::new(styles.clone(), TagTransform::Remove);
    assert_eq!(parser.parse(input), "hello world");

    let parser = BBParser::new(styles.clone(), TagTransform::Apply);
    let output = parser.parse(input);

    assert!(output.contains("\x1b[31m"));
    assert!(output.contains("\x1b[1m"));
    assert!(output.contains("hello"));
    assert!(output.contains("world"));
}

#[test]
fn test_nested_ansi_bloat() {
    let styles = test_styles();
    let parser = BBParser::new(styles, TagTransform::Apply);

    let input = "[bold][red]text[/red][/bold]";
    let output = parser.parse(input);

    let escape_count = output.matches("\x1b[").count();

    assert!(
        escape_count <= 3,
        "Output too bloated! Found {} escapes. Output: {:?}",
        escape_count,
        output
    );
}

#[test]
fn test_nested_tags_apply() {
    let styles = test_styles();
    let parser = BBParser::new(styles, TagTransform::Apply);

    let output = parser.parse("[bold][red]hi[/red][/bold]");

    assert!(output.contains("hi"));
}

#[test]
fn test_unbalanced_tag_raises_error() {
    let styles = test_styles();
    let parser = BBParser::new(styles, TagTransform::Apply);

    let (_, errors) = parser.parse_with_diagnostics("[bold]unfinished");
    assert!(
        !errors.is_empty(),
        "Expected errors for unbalanced tag '[bold]unfinished'"
    );
    let error_str = errors.to_string();
    assert!(
        error_str.contains("unbalanced") || error_str.contains("unexpected"),
        "Error message should mention unbalanced/unexpected tag. Got: {}",
        error_str
    );
}

#[test]
fn test_unexpected_close_raises_error() {
    let styles = test_styles();
    let parser = BBParser::new(styles, TagTransform::Apply);
    let (_, errors) = parser.parse_with_diagnostics("text[/bold]");
    assert!(
        !errors.is_empty(),
        "Expected errors for unexpected close tag 'text[/bold]'"
    );
}
