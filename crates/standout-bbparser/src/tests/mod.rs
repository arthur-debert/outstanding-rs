use super::*;

fn test_styles() -> HashMap<String, Style> {
    let mut styles = HashMap::new();
    styles.insert("bold".to_string(), Style::new().bold());
    styles.insert("red".to_string(), Style::new().red());
    styles.insert("dim".to_string(), Style::new().dim());
    styles.insert("title".to_string(), Style::new().cyan().bold());
    styles.insert("error".to_string(), Style::new().red().bold());
    styles.insert("my_style".to_string(), Style::new().green());
    styles.insert("style-with-dash".to_string(), Style::new().yellow());
    styles
}

mod diagnostics;
mod rendering;
mod syntax;

mod strip_tags_tests {
    use super::super::strip_tags;

    #[test]
    fn strips_known_style_tags() {
        assert_eq!(strip_tags("[bold]hello[/bold]"), "hello");
    }

    #[test]
    fn strips_unknown_tags() {
        assert_eq!(strip_tags("[additions]+32[/additions]"), "+32");
    }

    #[test]
    fn strips_multiple_tags() {
        assert_eq!(
            strip_tags("[additions]+32[/additions]/[deletions]-0[/deletions]/32"),
            "+32/-0/32"
        );
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(strip_tags("no tags here"), "no tags here");
    }

    #[test]
    fn empty_string() {
        assert_eq!(strip_tags(""), "");
    }

    #[test]
    fn nested_tags() {
        assert_eq!(strip_tags("[a][b]text[/b][/a]"), "text");
    }
}
