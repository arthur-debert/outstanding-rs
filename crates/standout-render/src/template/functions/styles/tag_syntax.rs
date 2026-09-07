use crate::output::Representation;
use crate::template::functions::*;
use crate::{ColorPolicy, Theme};
use console::Style;
use serde::Serialize;

#[test]
fn test_tag_syntax_text_mode() {
    let theme = Theme::new().add("title", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let output = render_with_output(
        "[title]{{ name }}[/title]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap();

    assert_eq!(output, "Hello");
}

#[test]
fn test_tag_syntax_term_mode() {
    let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let output = render_with_output(
        "[bold]{{ name }}[/bold]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
        Representation::Human,
        ColorPolicy::Always,
    )
    .unwrap();

    assert!(output.contains("\x1b[1m"));
    assert!(output.contains("Hello"));
}

#[test]
fn test_tag_syntax_debug_mode() {
    let theme = Theme::new().add("title", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let output = render_with_output(
        "[title]{{ name }}[/title]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
        Representation::TermDebug,
        ColorPolicy::Auto,
    )
    .unwrap();

    assert_eq!(output, "[title]Hello[/title]");
}

#[test]
fn test_tag_syntax_unknown_tag_degrades_to_text() {
    let theme = Theme::new().add("known", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let output = render_with_output(
        "[unknown]{{ name }}[/unknown]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
        Representation::Human,
        ColorPolicy::Always,
    )
    .unwrap();

    assert_eq!(output, "Hello");

    let text_output = render_with_output(
        "[unknown]{{ name }}[/unknown]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap();

    assert_eq!(text_output, "Hello");
}

#[test]
fn test_tag_syntax_nested() {
    let theme = Theme::new()
        .add("bold", Style::new().bold().force_styling(true))
        .add("red", Style::new().red().force_styling(true));

    #[derive(Serialize)]
    struct Data {
        word: String,
    }

    let output = render_with_output(
        "[bold][red]{{ word }}[/red][/bold]",
        &Data {
            word: "test".into(),
        },
        &theme,
        Representation::Human,
        ColorPolicy::Always,
    )
    .unwrap();

    assert!(output.contains("\x1b[1m")); // Bold
    assert!(output.contains("\x1b[31m")); // Red
    assert!(output.contains("test"));
}

#[test]
fn test_tag_syntax_multiple_styles() {
    let theme = Theme::new()
        .add("title", Style::new().bold())
        .add("count", Style::new().cyan());

    #[derive(Serialize)]
    struct Data {
        name: String,
        num: usize,
    }

    let output = render_with_output(
        r#"[title]{{ name }}[/title]: [count]{{ num }}[/count]"#,
        &Data {
            name: "Items".into(),
            num: 42,
        },
        &theme,
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap();

    assert_eq!(output, "Items: 42");
}

#[test]
fn test_tag_syntax_in_loop() {
    let theme = Theme::new().add("item", Style::new().cyan());

    #[derive(Serialize)]
    struct Data {
        items: Vec<String>,
    }

    let output = render_with_output(
        "{% for item in items %}[item]{{ item }}[/item]\n{% endfor %}",
        &Data {
            items: vec!["one".into(), "two".into()],
        },
        &theme,
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap();

    assert_eq!(output, "one\ntwo\n");
}

#[test]
fn test_tag_syntax_literal_brackets() {
    let theme = Theme::new();

    #[derive(Serialize)]
    struct Data {
        msg: String,
    }

    let output = render_with_output(
        "Array: [1, 2, 3] and {{ msg }}",
        &Data { msg: "done".into() },
        &theme,
        Representation::Human,
        ColorPolicy::Never,
    )
    .unwrap();

    assert_eq!(output, "Array: [1, 2, 3] and done");
}
