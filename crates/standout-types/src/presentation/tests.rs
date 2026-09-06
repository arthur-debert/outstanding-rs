use super::*;

fn sgr_styles(value: &FormattedText) -> Vec<SgrStyle> {
    value
        .nodes()
        .iter()
        .filter_map(|node| match node {
            PresentationNode::Styled {
                style: PresentationStyle::Sgr(style),
                ..
            } => Some(*style),
            _ => None,
        })
        .collect()
}

#[test]
fn composition_keeps_literal_children_and_nested_styles() {
    let hostile = "[/heading]\x1b[2J[draft].txt";
    let formatted = FormattedText::text("Header ")
        .append(FormattedText::text(hostile).styled("path").unwrap())
        .styled("heading")
        .unwrap()
        .append(" tail");
    assert_eq!(formatted.plain_text(), format!("Header {hostile} tail"));
    let PresentationNode::Styled { children, .. } = &formatted.nodes()[0] else {
        panic!("expected the enclosing heading style");
    };
    assert!(matches!(children[1], PresentationNode::Styled { .. }));
    assert_eq!(
        serde_json::to_value(&formatted).unwrap(),
        format!("Header {hostile} tail")
    );
}

#[test]
fn style_names_accept_only_tag_identifiers() {
    for name in ["heading", "_private", "a0", "a-b_c"] {
        assert!(FormattedText::text("text").styled(name).is_ok());
    }
    for name in ["", "A", "0name", "-name", "a b", "a]b", "a/b", "é", "a\x1b"] {
        assert!(FormattedText::text("text").styled(name).is_err());
    }
}

#[test]
fn render_data_preserves_nested_type_identity() {
    let formatted = FormattedText::text("[literal]").styled("heading").unwrap();
    let nested = vec![vec![formatted.clone()]];
    let value = crate::RenderData::from_serialize(&nested)
        .unwrap()
        .to_template_value();
    let inner = value.get_item(&Value::from(0)).unwrap();
    let item = inner.get_item(&Value::from(0)).unwrap();
    assert_eq!(FormattedText::from_value(&item), Some(&formatted));
    assert_eq!(item.to_string(), "[literal]");
    assert_eq!(
        serde_json::to_value(&nested).unwrap(),
        serde_json::json!([["[literal]"]])
    );
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        serde_json::json!([["[literal]"]])
    );
}

#[test]
fn ordinary_metadata_shaped_data_cannot_become_formatted() {
    let value = Value::from_serialize(serde_json::json!({
        "nodes": [{"Styled": {"style": {"Semantic": "heading"}, "children": []}}]
    }));
    assert!(FormattedText::from_value(&value).is_none());
}

#[test]
fn sgr_import_tracks_individual_style_resets() {
    let formatted =
        FormattedText::from_ansi_sgr("\x1b[1;2;3;4;5;7;8;9mone\x1b[22;23;24;25;27;28;29mtwo");
    assert_eq!(formatted.plain_text(), "onetwo");
    assert_eq!(
        sgr_styles(&formatted),
        [SgrStyle {
            bold: true,
            dim: true,
            italic: true,
            underline: true,
            blink: true,
            reverse: true,
            hidden: true,
            strikethrough: true,
            ..SgrStyle::default()
        }]
    );
    assert_eq!(formatted.nodes()[1], PresentationNode::Text("two".into()));
}

#[test]
fn sgr_import_supports_standard_bright_indexed_and_rgb_colors() {
    let formatted = FormattedText::from_ansi_sgr(
        "\x1b[31;44ma\x1b[97;100mb\x1b[38;5;255;48;2;1;2;3mc\x1b[39;49md",
    );
    assert_eq!(formatted.plain_text(), "abcd");
    let styles = sgr_styles(&formatted);
    assert_eq!(styles[0].foreground, Some(SgrColor::Indexed(1)));
    assert_eq!(styles[0].background, Some(SgrColor::Indexed(4)));
    assert_eq!(styles[1].foreground, Some(SgrColor::Indexed(15)));
    assert_eq!(styles[1].background, Some(SgrColor::Indexed(8)));
    assert_eq!(styles[2].foreground, Some(SgrColor::Indexed(255)));
    assert_eq!(styles[2].background, Some(SgrColor::Rgb(1, 2, 3)));
    assert_eq!(formatted.nodes()[3], PresentationNode::Text("d".into()));
}

#[test]
fn empty_parameters_and_c1_csi_follow_sgr_semantics() {
    let formatted = FormattedText::from_ansi_sgr("\u{9b}31ma\x1b[mb\x1b[1;;32mc\x1b[0md");
    assert_eq!(formatted.plain_text(), "abcd");
    let styles = sgr_styles(&formatted);
    assert_eq!(styles.len(), 2);
    assert_eq!(styles[0].foreground, Some(SgrColor::Indexed(1)));
    assert_eq!(styles[1].foreground, Some(SgrColor::Indexed(2)));
    assert!(!styles[1].bold);
}

#[test]
fn invalid_or_unsupported_sgr_is_preserved_atomically() {
    for sequence in [
        "\x1b[1;999m",
        "\x1b[38;5;256m",
        "\x1b[38;2;1;2m",
        "\x1b[38;2;1;2;256m",
        "\x1b[38;3;1m",
        "\x1b[31:1m",
        "\x1b[?31m",
        "\x1b[6m",
        "\x1b[21m",
        "\x1b[999999999999999999999m",
    ] {
        let input = format!("\x1b[32ma{sequence}b\x1b[0mc");
        let formatted = FormattedText::from_ansi_sgr(&input);
        assert_eq!(formatted.plain_text(), format!("a{sequence}bc"));
        let styles = sgr_styles(&formatted);
        assert!(styles.iter().all(|style| !style.bold));
        assert!(styles
            .iter()
            .all(|style| style.foreground == Some(SgrColor::Indexed(2))));
    }
}

#[test]
fn control_strings_do_not_import_embedded_sgr() {
    for input in [
        "\x1b]52;c;\x1b[31msecret\x07after",
        "\x1b]0;\x1b[31mtitle\x1b\\after",
        "\x1bPpayload\x1b[31mred\x1b\\after",
        "\x1b_payload\x1b[31mred\x1b\\after",
        "\x1b^payload\x1b[31mred\x1b\\after",
        "\x1bXpayload\x1b[31mred\x1b\\after",
        "\u{90}payload\x1b[31mred\u{9c}after",
        "\u{9d}payload\x1b[31mred\u{9c}after",
        "\x1b]unterminated\x1b[31mred",
        "\x1bPunterminated\x1b[31mred",
    ] {
        let formatted = FormattedText::from_ansi_sgr(input);
        assert_eq!(formatted.plain_text(), input);
        assert!(sgr_styles(&formatted).is_empty());
    }
}

#[test]
fn non_sgr_controls_and_incomplete_sequences_remain_text() {
    for input in [
        "\x1b[2J",
        "\x1b[6n",
        "\x1b[",
        "\x1b[31",
        "\x1b",
        "\x00\r\x7f\u{85}",
        "日本語\x1b[31",
    ] {
        let formatted = FormattedText::from_ansi_sgr(input);
        assert_eq!(formatted.plain_text(), input);
        assert!(sgr_styles(&formatted).is_empty());
    }
}

#[test]
fn sgr_parameter_limits_leave_the_entire_sequence_as_text() {
    for parameters in ["1;".repeat(32), "0".repeat(129)] {
        let input = format!("\x1b[{parameters}mtext");
        let formatted = FormattedText::from_ansi_sgr(&input);
        assert_eq!(formatted.plain_text(), input);
        assert!(sgr_styles(&formatted).is_empty());
    }
    let input = format!("\x1b[{}1mtext", "0;".repeat(31));
    assert_eq!(FormattedText::from_ansi_sgr(&input).plain_text(), "text");
}

#[test]
fn supported_sgr_is_removed_only_from_formatted_structured_projection() {
    let original = "\x1b[31m[draft].txt\x1b[0m\x1b[2J";
    let formatted = FormattedText::from_ansi_sgr(original);
    assert_eq!(
        serde_json::to_value(&formatted).unwrap(),
        "[draft].txt\x1b[2J"
    );
    assert_eq!(serde_json::to_value(original).unwrap(), original);
}
