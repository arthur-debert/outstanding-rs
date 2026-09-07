use super::*;

mod validation {
    use super::*;

    #[test]
    fn validate_all_known_tags_passes() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert!(parser.validate("[bold]text[/bold]").is_ok());
    }

    #[test]
    fn validate_nested_known_tags_passes() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert!(parser.validate("[bold][red]text[/red][/bold]").is_ok());
    }

    #[test]
    fn validate_unknown_tag_fails() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[unknown]text[/unknown]");
        assert!(result.is_err());
    }

    #[test]
    fn validate_returns_correct_error_count() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[unknown]text[/unknown]");
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn validate_error_contains_tag_name() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[foobar]text[/foobar]");
        let errors = result.unwrap_err();
        assert!(errors.errors.iter().all(|e| e.tag == "foobar"));
    }

    #[test]
    fn validate_error_distinguishes_open_and_close() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[unknown]text[/unknown]");
        let errors = result.unwrap_err();

        let open_count = errors
            .errors
            .iter()
            .filter(|e| e.kind == UnknownTagKind::Open)
            .count();
        let close_count = errors
            .errors
            .iter()
            .filter(|e| e.kind == UnknownTagKind::Close)
            .count();

        assert_eq!(open_count, 1);
        assert_eq!(close_count, 1);
    }

    #[test]
    fn validate_error_has_correct_positions() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let input = "[unknown]text[/unknown]";
        let result = parser.validate(input);
        let errors = result.unwrap_err();

        let open_error = errors
            .errors
            .iter()
            .find(|e| e.kind == UnknownTagKind::Open)
            .unwrap();
        assert_eq!(open_error.start, 0);
        assert_eq!(open_error.end, 9);

        let close_error = errors
            .errors
            .iter()
            .find(|e| e.kind == UnknownTagKind::Close)
            .unwrap();
        assert_eq!(close_error.start, 13);
        assert_eq!(close_error.end, 23);
    }

    #[test]
    fn validate_multiple_unknown_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[foo]a[/foo][bar]b[/bar]");
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 4);

        let tags: std::collections::HashSet<_> =
            errors.errors.iter().map(|e| e.tag.as_str()).collect();
        assert!(tags.contains("foo"));
        assert!(tags.contains("bar"));
    }

    #[test]
    fn validate_mixed_known_and_unknown() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.validate("[bold][unknown]text[/unknown][/bold]");
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 2);

        for error in &errors.errors {
            assert_eq!(error.tag, "unknown");
        }
    }

    #[test]
    fn validate_plain_text_passes() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert!(parser.validate("plain text without tags").is_ok());
    }

    #[test]
    fn validate_empty_string_passes() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert!(parser.validate("").is_ok());
    }
}

mod parse_with_diagnostics {
    use super::*;

    #[test]
    fn returns_output_and_errors() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        let (output, errors) = parser.parse_with_diagnostics("[unknown]text[/unknown]");

        assert_eq!(output, "[unknown?]text[/unknown?]");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn output_uses_strip_behavior() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply)
            .unknown_behavior(UnknownTagBehavior::Strip);
        let (output, errors) = parser.parse_with_diagnostics("[unknown]text[/unknown]");

        assert_eq!(output, "text");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn no_errors_for_known_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let (_, errors) = parser.parse_with_diagnostics("[bold]text[/bold]");
        assert!(errors.is_empty());
    }

    #[test]
    fn errors_iterable() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let (_, errors) = parser.parse_with_diagnostics("[a]x[/a][b]y[/b]");

        let mut count = 0;
        for error in &errors {
            assert!(error.tag == "a" || error.tag == "b");
            count += 1;
        }
        assert_eq!(count, 4);
    }
}

mod error_display {
    use super::*;

    #[test]
    fn unknown_tag_error_display() {
        let error = UnknownTagError {
            tag: "foo".to_string(),
            kind: UnknownTagKind::Open,
            start: 0,
            end: 5,
        };
        let display = format!("{}", error);
        assert!(display.contains("foo"));
        assert!(display.contains("opening"));
        assert!(display.contains("0..5"));
    }

    #[test]
    fn unknown_tag_errors_display() {
        let mut errors = UnknownTagErrors::new();
        errors.push(UnknownTagError {
            tag: "foo".to_string(),
            kind: UnknownTagKind::Open,
            start: 0,
            end: 5,
        });
        errors.push(UnknownTagError {
            tag: "foo".to_string(),
            kind: UnknownTagKind::Close,
            start: 9,
            end: 15,
        });

        let display = format!("{}", errors);
        assert!(display.contains("2 unknown tag"));
    }
}
