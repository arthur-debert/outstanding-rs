use super::*;
use console::strip_ansi_codes;
use proptest::prelude::*;

fn valid_tag_name() -> impl Strategy<Value = String> {
    "[a-z_][a-z0-9_-]{0,10}"
}

fn plain_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,!?:;'\"]{0,50}"
        .prop_filter("no brackets", |s| !s.contains('[') && !s.contains(']'))
}

fn ansi_control() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "\x1b[31m",
        "\x1b[0m",
        "\x1b(0",
        "\x1b(B",
        "\x1b)0",
        "\x1b)B",
        "\u{9b}31m",
        "\u{9b}0m",
    ])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn keep_mode_roundtrip(content in plain_text()) {
        let parser = BBParser::new(HashMap::new(), TagTransform::Keep);
        prop_assert_eq!(parser.parse(&content), content);
    }

    #[test]
    fn remove_mode_plain_text_unchanged(content in plain_text()) {
        let parser = BBParser::new(HashMap::new(), TagTransform::Remove);
        prop_assert_eq!(parser.parse(&content), content);
    }

    #[test]
    fn styled_text_matches_established_ansi_stripping_semantics(
        prefix in prop::collection::vec(ansi_control(), 0..4),
        content in plain_text(),
        suffix in prop::collection::vec(ansi_control(), 0..4),
    ) {
        let input = format!(
            "{}[outer]{}[/outer]{}",
            prefix.concat(),
            content,
            suffix.concat()
        );
        let styled = StyledText::parse(&input);
        let mut visible = String::new();
        styled.visit_visible_chars(|character| visible.push(character));
        let expected = strip_tags(&strip_ansi_codes(&input));

        prop_assert_eq!(&visible, &expected);
        if !visible.is_empty() {
            prop_assert_eq!(styled.select_range(0..visible.chars().count()), input);
        }
    }

    #[test]
    fn valid_tag_names_accepted(tag in valid_tag_name()) {
        prop_assert!(is_valid_tag_name(&tag));
    }

    #[test]
    fn remove_strips_known_tags(tag in valid_tag_name(), content in plain_text()) {
        let mut styles = HashMap::new();
        styles.insert(tag.clone(), Style::new());

        let parser = BBParser::new(styles, TagTransform::Remove);
        let input = format!("[{}]{}[/{}]", tag, content, tag);
        let result = parser.parse(&input);

        prop_assert_eq!(result, content);
    }

    #[test]
    fn keep_preserves_structure(tag in valid_tag_name(), content in plain_text()) {
        let parser = BBParser::new(HashMap::new(), TagTransform::Keep);
        let input = format!("[{}]{}[/{}]", tag, content, tag);
        let result = parser.parse(&input);

        prop_assert_eq!(result, input);
    }

    #[test]
    fn nested_tags_balanced(
        outer in valid_tag_name(),
        inner in valid_tag_name(),
        content in plain_text()
    ) {
        let mut styles = HashMap::new();
        styles.insert(outer.clone(), Style::new());
        styles.insert(inner.clone(), Style::new());

        let parser = BBParser::new(styles, TagTransform::Remove);
        let input = format!("[{}][{}]{}[/{}][/{}]", outer, inner, content, inner, outer);
        let result = parser.parse(&input);

        prop_assert_eq!(result, content);
    }

    #[test]
    fn validate_finds_unknown_tags(tag in valid_tag_name(), content in plain_text()) {
        let parser = BBParser::new(HashMap::new(), TagTransform::Apply);
        let input = format!("[{}]{}[/{}]", tag, content, tag);
        let result = parser.validate(&input);

        prop_assert!(result.is_err());
        let errors = result.unwrap_err();
        prop_assert_eq!(errors.len(), 2);
    }

    #[test]
    fn invalid_start_digit_rejected(n in 0..10u8, rest in "[a-z0-9_-]{0,5}") {
        let tag = format!("{}{}", n, rest);
        prop_assert!(!is_valid_tag_name(&tag));
    }

    #[test]
    fn invalid_start_hyphen_rejected(rest in "[a-z0-9_-]{0,5}") {
        let tag = format!("-{}", rest);
        prop_assert!(!is_valid_tag_name(&tag));
    }

    #[test]
    fn uppercase_rejected(tag in "[A-Z][a-zA-Z0-9_-]{0,5}") {
        prop_assert!(!is_valid_tag_name(&tag));
    }
}
