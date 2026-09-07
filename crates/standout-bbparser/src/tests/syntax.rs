use super::*;

mod edge_cases {
    use super::*;

    #[test]
    fn empty_input() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse(""), "");
    }

    #[test]
    fn unclosed_tag_passthrough() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("[bold]hello"), "[bold]hello");
    }

    #[test]
    fn orphan_close_tag_passthrough() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("hello[/bold]"), "hello[/bold]");
    }

    #[test]
    fn mismatched_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(
            parser.parse("[bold]hello[/red][/bold]"),
            "[bold]hello[/red][/bold]"
        );
    }

    #[test]
    fn overlapping_tags_auto_close() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        let result = parser.parse("[bold][red]hello[/bold][/red]");
        assert!(result.contains("hello"));
    }

    #[test]
    fn empty_tag_content() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[bold][/bold]"), "");
    }

    #[test]
    fn brackets_in_content() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[bold]array[0][/bold]"), "array[0]");
    }

    #[test]
    fn invalid_tag_syntax_passthrough() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("[123]text[/123]"), "[123]text[/123]");
        assert_eq!(parser.parse("[-bad]text[/-bad]"), "[-bad]text[/-bad]");
        assert_eq!(parser.parse("[Bad]text[/Bad]"), "[Bad]text[/Bad]");
    }

    #[test]
    fn deeply_nested() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("[bold][red][dim]deep[/dim][/red][/bold]"),
            "deep"
        );
    }

    #[test]
    fn many_adjacent_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("[bold]a[/bold][red]b[/red][dim]c[/dim]"),
            "abc"
        );
    }

    #[test]
    fn unclosed_bracket() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("hello [bold world"), "hello [bold world");
    }

    #[test]
    fn multiline_content() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("[bold]line1\nline2\nline3[/bold]"),
            "line1\nline2\nline3"
        );
    }

    #[test]
    fn style_with_underscore() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[my_style]text[/my_style]"), "text");
    }

    #[test]
    fn style_with_dash() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("[style-with-dash]text[/style-with-dash]"),
            "text"
        );
    }
}

mod escapes {
    use super::*;

    #[test]
    fn escaped_open_bracket_is_literal() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("\\[bold\\]"), "[bold]");
    }

    #[test]
    fn escaped_brackets_inside_known_tag() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("[bold]hello \\[world\\][/bold]"),
            "hello [world]"
        );
    }

    #[test]
    fn escapes_keep_mode_emits_literal_brackets() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("\\[bold\\]"), "[bold]");
    }

    #[test]
    fn escapes_apply_mode_styles_around_literals() {
        let mut styles = HashMap::new();
        styles.insert("bold".to_string(), Style::new().bold().force_styling(true));
        let parser = BBParser::new(styles, TagTransform::Apply);
        let result = parser.parse("[bold]\\[x\\][/bold]");
        assert!(result.contains("[x]"));
        assert!(!result.contains("[bold]"));
    }

    #[test]
    fn lone_backslash_is_literal() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("path C:\\foo\\bar"), "path C:\\foo\\bar");
    }

    #[test]
    fn unescape_borrows_when_no_escape_present() {
        assert!(matches!(
            unescape("plain text"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(
            unescape("C:\\foo\\bar"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(unescape("\\d+"), std::borrow::Cow::Borrowed(_)));
        assert!(matches!(
            unescape("trailing\\"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(unescape("\\["), std::borrow::Cow::Owned(_)));
        assert!(matches!(unescape("\\]"), std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn trailing_backslash_is_literal() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("end\\"), "end\\");
    }

    #[test]
    fn escaped_backslash_does_not_escape_adjacent_tag() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        let (output, errors) = parser.parse_with_diagnostics(r"\\[bold]text[/bold]");
        assert_eq!(output, r"\text");
        assert!(errors.is_empty());
    }

    #[test]
    fn odd_backslash_count_escapes_the_bracket() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        let (output, errors) = parser.parse_with_diagnostics(r"\\\[bold\]");
        assert_eq!(output, r"\[bold]");
        assert!(errors.is_empty());
    }

    #[test]
    fn independently_escaped_text_preserves_adjacent_authored_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        for literal in [r"\", r"\\", r"\[", r"[draft].txt\", r"C:\dir\"] {
            let mut escaped = String::new();
            for character in literal.chars() {
                if matches!(character, '\\' | '[' | ']') {
                    escaped.push('\\');
                }
                escaped.push(character);
            }
            let source = format!("[bold]{escaped}[/bold][red]tail[/red]");
            let (output, errors) = parser.parse_with_diagnostics(&source);
            assert_eq!(output, format!("{literal}tail"));
            assert!(errors.is_empty());
            assert_eq!(strip_tags(&source), output);
        }
    }

    #[test]
    fn backslash_pairs_are_one_visible_character_and_one_selection_unit() {
        let source = r"[bold]a\\\[b\]z[/bold]";
        let styled = StyledText::parse(source);
        let mut visible = String::new();
        styled.visit_visible_chars(|character| visible.push(character));
        assert_eq!(visible, r"a\[b]z");
        assert_eq!(styled.select_range(0..visible.chars().count()), source);
        assert_eq!(styled.select_range(1..2), r"[bold]\\[/bold]");
        assert_eq!(styled.select_range(1..3), r"[bold]\\\[[/bold]");
        assert_eq!(strip_tags(&styled.select_range(1..2)), r"\");
        assert_eq!(strip_tags(&styled.select_range(1..3)), r"\[");
    }

    #[test]
    fn escaped_brackets_dont_create_unknown_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let (output, errors) = parser.parse_with_diagnostics("\\[unknown\\]");
        assert_eq!(output, "[unknown]");
        assert!(errors.is_empty());
    }

    #[test]
    fn escapes_pass_validation() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert!(parser.validate("\\[anything\\]").is_ok());
        assert!(parser.validate("[bold]a\\[b\\]c[/bold]").is_ok());
    }

    #[test]
    fn strip_tags_handles_escapes() {
        assert_eq!(strip_tags("\\[bold\\]"), "[bold]");
        assert_eq!(strip_tags("[bold]a\\[b\\]c[/bold]"), "a[b]c");
    }

    #[test]
    fn escape_does_not_apply_inside_tag_name() {
        // `\` is not a valid tag-name char, so this becomes InvalidTag.
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("[bo\\ld]"), "[bo\\ld]");
    }

    #[test]
    fn escapes_with_multibyte_text() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("café \\[é\\] 🎉"), "café [é] 🎉");
    }

    #[test]
    fn only_open_escaped_leaves_close_unmatched() {
        // Escaping only the open leaves the close unmatched.
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let (output, errors) = parser.parse_with_diagnostics("\\[bold]hi[/bold]");
        assert!(output.contains("[bold]hi"));
        assert!(output.contains("[/bold]"));
        assert!(!errors.is_empty());
        assert!(errors
            .errors
            .iter()
            .any(|e| e.kind == UnknownTagKind::UnexpectedClose));
    }
}
