use super::*;

use crate::Theme;
use console::Style;
use serde::Serialize;

#[test]
fn test_validate_template_all_known_tags() {
    let theme = Theme::new()
        .add("title", Style::new().bold())
        .add("count", Style::new().cyan());

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let result = validate_template(
        "[title]{{ name }}[/title]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
    );

    assert!(result.is_ok());
}

#[test]
fn test_validate_template_unknown_tag_fails() {
    let theme = Theme::new().add("known", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        name: String,
    }

    let result = validate_template(
        "[unknown]{{ name }}[/unknown]",
        &Data {
            name: "Hello".into(),
        },
        &theme,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let errors = err
        .downcast_ref::<standout_bbparser::UnknownTagErrors>()
        .expect("Expected UnknownTagErrors");
    assert_eq!(errors.len(), 2); // open and close tags
}

#[test]
fn test_validate_template_multiple_unknown_tags() {
    let theme = Theme::new().add("known", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        a: String,
        b: String,
    }

    let result = validate_template(
        "[foo]{{ a }}[/foo] and [bar]{{ b }}[/bar]",
        &Data {
            a: "x".into(),
            b: "y".into(),
        },
        &theme,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let errors = err
        .downcast_ref::<standout_bbparser::UnknownTagErrors>()
        .expect("Expected UnknownTagErrors");
    assert_eq!(errors.len(), 4); // foo open/close + bar open/close
}

#[test]
fn test_validate_template_plain_text_passes() {
    let theme = Theme::new();

    #[derive(Serialize)]
    struct Data {
        msg: String,
    }

    let result = validate_template("Just plain {{ msg }}", &Data { msg: "hi".into() }, &theme);

    assert!(result.is_ok());
}

#[test]
fn test_validate_template_mixed_known_and_unknown() {
    let theme = Theme::new().add("known", Style::new().bold());

    #[derive(Serialize)]
    struct Data {
        a: String,
        b: String,
    }

    let result = validate_template(
        "[known]{{ a }}[/known] [unknown]{{ b }}[/unknown]",
        &Data {
            a: "x".into(),
            b: "y".into(),
        },
        &theme,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    let errors = err
        .downcast_ref::<standout_bbparser::UnknownTagErrors>()
        .expect("Expected UnknownTagErrors");
    assert_eq!(errors.len(), 2);
    assert!(errors.errors.iter().any(|e| e.tag == "unknown"));
}

#[test]
fn test_validate_template_syntax_error_fails() {
    let theme = Theme::new();
    #[derive(Serialize)]
    struct Data {}

    let result = validate_template("{{ unclosed", &Data {}, &theme);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err
        .downcast_ref::<standout_bbparser::UnknownTagErrors>()
        .is_none());
    let msg = err.to_string();
    assert!(
        msg.contains("syntax error") || msg.contains("unexpected"),
        "Got: {}",
        msg
    );
}
