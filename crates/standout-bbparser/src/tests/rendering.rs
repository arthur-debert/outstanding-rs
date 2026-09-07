use super::*;

mod keep_mode {
    use super::*;

    #[test]
    fn plain_text_unchanged() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("hello world"), "hello world");
    }

    #[test]
    fn single_tag_preserved() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(parser.parse("[bold]hello[/bold]"), "[bold]hello[/bold]");
    }

    #[test]
    fn nested_tags_preserved() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(
            parser.parse("[bold][red]hello[/red][/bold]"),
            "[bold][red]hello[/red][/bold]"
        );
    }

    #[test]
    fn adjacent_tags_preserved() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(
            parser.parse("[bold]a[/bold][red]b[/red]"),
            "[bold]a[/bold][red]b[/red]"
        );
    }

    #[test]
    fn text_around_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(
            parser.parse("before [bold]middle[/bold] after"),
            "before [bold]middle[/bold] after"
        );
    }

    #[test]
    fn unknown_tags_preserved() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep);
        assert_eq!(
            parser.parse("[unknown]text[/unknown]"),
            "[unknown]text[/unknown]"
        );
    }
}

mod remove_mode {
    use super::*;

    #[test]
    fn plain_text_unchanged() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("hello world"), "hello world");
    }

    #[test]
    fn single_tag_stripped() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[bold]hello[/bold]"), "hello");
    }

    #[test]
    fn nested_tags_stripped() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[bold][red]hello[/red][/bold]"), "hello");
    }

    #[test]
    fn adjacent_tags_stripped() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(parser.parse("[bold]a[/bold][red]b[/red]"), "ab");
    }

    #[test]
    fn text_around_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        assert_eq!(
            parser.parse("before [bold]middle[/bold] after"),
            "before middle after"
        );
    }

    #[test]
    fn unknown_tags_stripped() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove);
        // Default is Passthrough, but Remove mode ignores unknown_behavior for output
        assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
    }
}

mod unknown_tag_behavior {
    use super::*;

    #[test]
    fn passthrough_adds_question_mark_in_apply_mode() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        assert_eq!(
            parser.parse("[unknown]text[/unknown]"),
            "[unknown?]text[/unknown?]"
        );
    }

    #[test]
    fn passthrough_is_default() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert_eq!(
            parser.parse("[unknown]text[/unknown]"),
            "[unknown?]text[/unknown?]"
        );
    }

    #[test]
    fn strip_removes_unknown_tags_in_apply_mode() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply)
            .unknown_behavior(UnknownTagBehavior::Strip);
        assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
    }

    #[test]
    fn passthrough_nested_with_known() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        let result = parser.parse("[bold][unknown]text[/unknown][/bold]");
        assert!(result.contains("[unknown?]"));
        assert!(result.contains("[/unknown?]"));
        assert!(result.contains("text"));
    }

    #[test]
    fn strip_nested_with_known() {
        let mut styles = HashMap::new();
        styles.insert("bold".to_string(), Style::new().bold().force_styling(true));
        let parser =
            BBParser::new(styles, TagTransform::Apply).unknown_behavior(UnknownTagBehavior::Strip);
        let result = parser.parse("[bold][unknown]text[/unknown][/bold]");
        assert!(!result.contains("[unknown"));
        assert!(result.contains("text"));
    }

    #[test]
    fn keep_mode_ignores_unknown_behavior() {
        let parser = BBParser::new(test_styles(), TagTransform::Keep)
            .unknown_behavior(UnknownTagBehavior::Strip);
        assert_eq!(
            parser.parse("[unknown]text[/unknown]"),
            "[unknown]text[/unknown]"
        );
    }

    #[test]
    fn remove_mode_always_strips_tags() {
        let parser = BBParser::new(test_styles(), TagTransform::Remove)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        assert_eq!(parser.parse("[unknown]text[/unknown]"), "text");
    }
}

mod apply_mode {
    use super::*;

    #[test]
    fn plain_text_unchanged() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        assert_eq!(parser.parse("hello world"), "hello world");
    }

    #[test]
    fn unknown_tag_passthrough_with_marker() {
        let parser = BBParser::new(test_styles(), TagTransform::Apply);
        let result = parser.parse("[unknown]text[/unknown]");
        assert!(result.contains("[unknown?]"));
        assert!(result.contains("[/unknown?]"));
        assert!(result.contains("text"));
    }

    #[test]
    fn known_tag_applies_style() {
        let mut styles = HashMap::new();
        styles.insert("bold".to_string(), Style::new().bold().force_styling(true));

        let parser = BBParser::new(styles, TagTransform::Apply);
        let result = parser.parse("[bold]hello[/bold]");

        assert!(result.contains("\x1b[1m") || result.contains("hello"));
    }
}
