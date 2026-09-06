use minijinja::Value;
use proptest::prelude::*;
use standout_render::context::{ContextRegistry, RenderContext};
use standout_render::{
    escape_control_characters, render_with_context, render_with_output, AmbiguousWidth,
    ColorPolicy, FormattedText, RenderData, Representation, StyleMode, Theme,
};

#[derive(Clone, Debug)]
enum Text {
    Literal(String),
    Semantic(String),
    Sgr(String),
}

impl Text {
    fn plain(&self) -> &str {
        match self {
            Self::Literal(text) | Self::Semantic(text) | Self::Sgr(text) => text,
        }
    }

    fn data(&self) -> RenderData {
        match self {
            Self::Literal(text) => RenderData::String(text.clone()),
            Self::Semantic(text) => FormattedText::text(text).styled("heading").unwrap().into(),
            Self::Sgr(text) => FormattedText::from_ansi_sgr("\x1b[31mR\x1b[0m")
                .append(text.as_str())
                .into(),
        }
    }

    fn projected(&self) -> String {
        match self {
            Self::Sgr(text) => format!("R{text}"),
            _ => self.plain().to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
enum Tree {
    Text(Text),
    Sequence(Vec<Tree>),
    Map(Vec<Tree>),
}

impl Tree {
    fn data(&self) -> RenderData {
        match self {
            Self::Text(text) => text.data(),
            Self::Sequence(children) => {
                RenderData::Array(children.iter().map(Self::data).collect())
            }
            Self::Map(children) => RenderData::Object(
                children
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (format!("k{i}"), child.data()))
                    .collect(),
            ),
        }
    }

    fn json(&self) -> serde_json::Value {
        match self {
            Self::Text(text) => text.projected().into(),
            Self::Sequence(children) => children.iter().map(Self::json).collect(),
            Self::Map(children) => serde_json::Value::Object(
                children
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (format!("k{i}"), child.json()))
                    .collect(),
            ),
        }
    }

    fn leaves(&self, path: String, result: &mut Vec<(String, Text)>) {
        match self {
            Self::Text(text) => result.push((path, text.clone())),
            Self::Sequence(children) | Self::Map(children) => {
                for (i, child) in children.iter().enumerate() {
                    let key = if matches!(self, Self::Map(_)) {
                        format!(".k{i}")
                    } else {
                        format!("[{i}]")
                    };
                    child.leaves(format!("{path}{key}"), result);
                }
            }
        }
    }
}

fn hostile_text(multiline: bool) -> BoxedStrategy<String> {
    let mut tokens = vec![
        "x",
        "[",
        "]",
        "\\",
        "[heading]",
        "[/heading]",
        "[draft]",
        "\\[",
        "\\\\",
        "é",
        "中",
        "🙂",
        "e\u{301}",
        "Ω",
        "\u{e000}",
        "\u{e000}0\u{e001}",
        "[[standout:0]]",
        "_standout_fragment_0",
        "__standout_0__",
        "\x00",
        "\x07",
        "\r",
        "\x7f",
        "\u{80}",
        "\u{9b}",
        "\u{9f}",
        "\x1b[31m",
        "\x1b[2J",
        "\x1b]52;c;x\x07",
        "\x1bPq\x1b\\",
    ];
    if multiline {
        tokens.extend(["\n", "\t"]);
    }
    let controls: Vec<_> = (0u8..=31)
        .chain(127..=159)
        .filter(|value| multiline || !matches!(value, 9 | 10))
        .map(|value| char::from(value).to_string())
        .collect();
    let token = prop_oneof![
        4 => prop::sample::select(tokens).prop_map(str::to_owned),
        1 => prop::sample::select(controls),
    ];
    prop::collection::vec(token, 0..7)
        .prop_map(|tokens| tokens.concat())
        .boxed()
}

fn text(multiline: bool) -> BoxedStrategy<Text> {
    (hostile_text(multiline), 0u8..3)
        .prop_map(|(text, kind)| match kind {
            0 => Text::Literal(text),
            1 => Text::Semantic(format!("x{text}")),
            _ => Text::Sgr(text),
        })
        .boxed()
}

fn tree() -> BoxedStrategy<Tree> {
    text(true)
        .prop_map(Tree::Text)
        .prop_recursive(3, 12, 3, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 1..3).prop_map(Tree::Sequence),
                prop::collection::vec(inner, 1..3).prop_map(Tree::Map),
            ]
        })
        .boxed()
}

fn theme() -> Theme {
    Theme::new().add("heading", console::Style::new().bold())
}

fn data(value: RenderData) -> RenderData {
    RenderData::Object([("value".into(), value)].into_iter().collect())
}

fn output(template: &str, value: &RenderData, colors: ColorPolicy) -> String {
    render_with_output(template, value, &theme(), Representation::Human, colors).unwrap()
}

fn layout(template: &str, value: &RenderData, policy: AmbiguousWidth) -> String {
    let theme = theme();
    let context = RenderContext::with_ambiguous_width(
        Representation::Human,
        StyleMode::Ansi,
        Some(256),
        policy,
        &theme,
        value,
    );
    render_with_context(
        template,
        value,
        &theme,
        Representation::Human,
        ColorPolicy::Never,
        &ContextRegistry::new(),
        &context,
        None,
    )
    .unwrap()
}

fn columns(text: &str, policy: AmbiguousWidth) -> usize {
    text.chars()
        .map(|character| match character {
            '\u{301}' => 0,
            '中' | '🙂' => 2,
            'é' | 'Ω' | '\u{e000}' | '\u{e001}' | '…' => match policy {
                AmbiguousWidth::Narrow => 1,
                AmbiguousWidth::Wide => 2,
            },
            character if character.is_ascii() && !character.is_control() => 1,
            character => panic!("character outside single-line width palette: {character:?}"),
        })
        .sum()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        max_shrink_iters: 2048,
        ..ProptestConfig::default()
    })]

    #[test]
    fn recursive_json_projection_has_only_original_text(tree in tree()) {
        let rendered = render_with_output("unused", &tree.data(), &theme(), Representation::Json, ColorPolicy::Always).unwrap();
        prop_assert_eq!(serde_json::from_str::<serde_json::Value>(&rendered).unwrap(), tree.json());
    }

    #[test]
    fn recursive_direct_macro_capture_and_join_preserve_text_and_styles(tree in tree()) {
        let data = data(tree.data());
        let mut leaves = Vec::new();
        tree.leaves("value".into(), &mut leaves);
        for (path, leaf) in leaves {
            let expected = escape_control_characters(leaf.projected());
            let direct = output(&format!("{{{{ {path} }}}}tail"), &data, ColorPolicy::Always);
            prop_assert_eq!(console::strip_ansi_codes(&direct), format!("{expected}tail"));
            if !matches!(leaf, Text::Literal(_)) {
                prop_assert!(direct.contains('\x1b'), "typed style lost: {leaf:?}: {direct:?}");
            }
            for template in [
                format!("{{% macro show(v) %}}{{{{ v }}}}{{% endmacro %}}{{{{ show({path}) }}}}tail"),
                format!("{{% set captured %}}{{{{ {path} }}}}{{% endset %}}{{{{ captured }}}}tail"),
                format!("{{{{ [{path}] | join }}}}tail"),
            ] {
                prop_assert_eq!(output(&template, &data, ColorPolicy::Always), direct.clone(), "route: {}", template);
                prop_assert_eq!(output(&template, &data, ColorPolicy::Never), format!("{expected}tail"));
            }
        }
    }

    #[test]
    fn recursive_context_agrees_with_direct_rendering(tree in tree()) {
        let data = data(tree.data());
        let mut registry = ContextRegistry::new();
        registry.add_static("extra", tree.data());
        let theme = theme();
        let context = RenderContext::new(Representation::Human, StyleMode::Ansi, None, &theme, &data);
        let mut leaves = Vec::new();
        tree.leaves("".into(), &mut leaves);
        for (path, _) in leaves {
            let direct = output(&format!("{{{{ value{path} }}}}tail"), &data, ColorPolicy::Always);
            let from_context = render_with_context(&format!("{{{{ extra{path} }}}}tail"), &data, &theme,
                Representation::Human, ColorPolicy::Always, &registry, &context, None).unwrap();
            prop_assert_eq!(from_context, direct, "context path: {}", path);
        }
    }

    #[test]
    fn table_rows_agree_with_direct_typed_values(leaf in text(false)) {
        let data = data(leaf.data());
        let direct = output("{{ value }}tail", &data, ColorPolicy::Always);
        let width = columns(&escape_control_characters(leaf.projected()), AmbiguousWidth::Narrow).max(1);
        let padded = output(&format!("{{{{ value | pad_right({width}) }}}}tail"), &data, ColorPolicy::Always);
        for constructor in ["tabular", "table"] {
            for expression in ["value", "captured"] {
                let template = format!("{{% set captured %}}{{{{ value }}}}{{% endset %}}{{% set t = {constructor}([{{'key': 'nested.0', 'width': {width}}}]) %}}{{{{ t.row_from({{'nested': [{expression}]}}) }}}}tail");
                let row = output(&template, &data, ColorPolicy::Always);
                prop_assert_eq!(row, padded.clone(), "route: {}", template);
            }
        }
        prop_assert_eq!(console::strip_ansi_codes(&direct), format!("{}tail", escape_control_characters(leaf.projected())));
    }

    #[test]
    fn width_and_padding_measure_emitted_single_line_text(leaf in text(false), extra in 0usize..8) {
        let data = data(leaf.data());
        for policy in [AmbiguousWidth::Narrow, AmbiguousWidth::Wide] {
            let visible = layout("{{ value }}", &data, policy);
            prop_assert_eq!(&visible, &escape_control_characters(leaf.projected()));
            let width = columns(&visible, policy);
            prop_assert_eq!(layout("{{ value | display_width }}", &data, policy), width.to_string());
            for filter in ["pad_left", "pad_right", "pad_center"] {
                let padded = layout(&format!("{{{{ value | {filter}({}) }}}}", width + extra), &data, policy);
                prop_assert_eq!(columns(&padded, policy), width + extra, "{}: {:?}", filter, padded);
                prop_assert!(padded.contains(&visible), "padding changed text: {padded:?}");
            }
        }
    }

    #[test]
    fn truncation_obeys_budget_and_preserves_text_that_fits(leaf in text(false), limit in 0usize..24) {
        let data = data(leaf.data());
        for policy in [AmbiguousWidth::Narrow, AmbiguousWidth::Wide] {
            let visible = layout("{{ value }}", &data, policy);
            for position in ["start", "middle", "end"] {
                let truncated = layout(&format!("{{{{ value | truncate_at({limit}, '{position}') }}}}"), &data, policy);
                prop_assert!(columns(&truncated, policy) <= limit, "{policy:?}/{position}/{limit}: {truncated:?}");
                if columns(&visible, policy) <= limit {
                    prop_assert_eq!(truncated, visible.clone());
                }
            }
        }
    }

    #[test]
    fn table_borders_match_actual_rows(separator in hostile_text(false), leaf in text(false)) {
        let value = RenderData::Object([
            ("value".into(), leaf.data()),
            ("separator".into(), RenderData::String(separator)),
        ].into_iter().collect());
        for policy in [AmbiguousWidth::Narrow, AmbiguousWidth::Wide] {
            let cell_width = columns(&escape_control_characters(leaf.projected()), policy).max(1);
            let rendered = layout(&format!("{{% set t = table([{{'width': {cell_width}}}, {{'width': 1}}], separator=separator, border='ascii') %}}{{{{ t.top_border() }}}}\n{{{{ t.row([value, 'z']) }}}}\n{{{{ t.bottom_border() }}}}"), &value, policy);
            let widths: Vec<_> = rendered.lines().map(|line| columns(line, policy)).collect();
            prop_assert_eq!(widths.len(), 3, "table: {:?}", rendered);
            prop_assert_eq!(widths[0], widths[1], "table: {:?}", rendered);
            prop_assert_eq!(widths[1], widths[2], "table: {:?}", rendered);
        }
    }

    #[test]
    fn native_text_configuration_and_width_helpers_agree(text in hostile_text(false)) {
        use standout_render::tabular::{CellValue, Col, Overflow, SubCol, SubColumns, Table, TabularFormatter, TabularSpec, TruncateAt};
        let expected = escape_control_characters(text.clone());
        for policy in [AmbiguousWidth::Narrow, AmbiguousWidth::Wide] {
            let width = columns(&expected, policy);
            let total = 2 + 3 * width;
            let spec = TabularSpec::builder().column(Col::fixed(1)).column(Col::fill())
                .separator(&text).prefix(&text).suffix(&text).build();
            prop_assert_eq!(spec.decorations.overhead_with_policy(2, policy), 3 * width);
            let resolved = spec.resolve_widths_with_policy(total, policy);
            prop_assert_eq!(&resolved.widths, &vec![1, 1]);
            let formatter = TabularFormatter::from_resolved_with_width_and_policy(&spec, resolved, total, policy);
            let row = native_plain(&formatter.format_row(&["a", "b"]));
            prop_assert_eq!(&row, &format!("{expected}a{expected}b{expected}"));
            let formatter = TabularFormatter::with_widths_and_ambiguous_width(vec![Col::fixed(1), Col::fixed(1)], vec![1, 1], policy)
                .separator(&text).prefix(&text).suffix(&text);
            prop_assert_eq!(native_plain(&formatter.format_row(&["a", "b"])), row);

            let measured_spec = TabularSpec::builder().column(Col::bounded(0, 256)).column(Col::fill()).build();
            prop_assert_eq!(measured_spec.resolve_widths_from_data_with_policy(256, &[vec![&text]], policy).widths[0], width);

            let fallback = TabularSpec::builder().column(Col::fixed(width).null_repr(&text)).build();
            prop_assert_eq!(native_plain(&TabularFormatter::with_ambiguous_width(&fallback, width, policy).format_row::<&str>(&[])), expected.clone());
            let sub = SubColumns::new(vec![SubCol::fill().null_repr(&text)], &text).unwrap();
            let fallback = TabularSpec::builder().column(Col::fixed(width).sub_columns(sub)).build();
            prop_assert_eq!(native_plain(&TabularFormatter::with_ambiguous_width(&fallback, width, policy).format_row_cells(&[CellValue::Sub(vec![])])), expected.clone());

            let truncation = TabularSpec::builder().column(Col::fixed(width + 1).overflow(Overflow::truncate_with_marker(TruncateAt::End, &text))).build();
            prop_assert_eq!(native_plain(&TabularFormatter::with_ambiguous_width(&truncation, width + 1, policy).format_row(&["x".repeat(512)])), format!("x{expected}"));

            let table = Table::with_ambiguous_width(spec, total, policy).border(standout_render::tabular::BorderStyle::Ascii);
            let output = native_plain(&table.render(&[vec!["a", "b"]]));
            let widths: Vec<_> = output.lines().map(|line| columns(line, policy)).collect();
            prop_assert_eq!(widths, vec![total + 2; 3]);
        }
    }

    #[test]
    fn semantic_names_cannot_authorize_sgr(flags in 1u8..=255, color in prop::option::of(any::<u8>())) {
        let foreground = color.map_or_else(|| "none".into(), |color| format!("i{color}"));
        let name = format!("_standout_sgr_{flags}_{foreground}_none");
        let theme = Theme::new().add(&name, console::Style::new());
        let ordinary = data(RenderData::String("x".into()));
        let mut cases = vec![
            (format!("[{name}]{{{{ value }}}}[/{name}]"), ordinary.clone()),
            (format!("{{{{ value | style_as('{name}') }}}}"), ordinary),
        ];
        if let Ok(formatted) = FormattedText::text("x").styled(&name) {
            cases.push(("{{ value }}".into(), data(formatted.into())));
        }
        for (template, data) in cases {
            if let Ok(rendered) = render_with_output(&template, &data, &theme, Representation::Human, ColorPolicy::Always) {
                prop_assert_eq!(rendered, "x", "semantic name {} authorized terminal bytes", name);
            }
        }
    }

    #[test]
    fn safe_strings_and_presentation_shaped_json_remain_ordinary(text in hostile_text(true)) {
        let shaped = serde_json::json!({"nodes": [{"Styled": {"style": {"Sgr": {"flags": 1}}, "children": [{"Text": text}]}}]});
        let value = RenderData::from_serialize(&shaped).unwrap();
        let rendered = output("{{ value.nodes[0].Styled.children[0].Text }}", &data(value), ColorPolicy::Always);
        prop_assert_eq!(rendered, escape_control_characters(text.clone()));
        let safe = RenderData::from_template_value(Value::from_safe_string(text.clone())).unwrap();
        prop_assert_eq!(output("{{ value | safe }}", &data(safe), ColorPolicy::Always), escape_control_characters(text));
    }
}

#[test]
fn native_string_rows_keep_brackets_and_controls_literal() {
    use standout_render::tabular::{CellValue, Col, TabularFormatter, TabularSpec};
    let value = "[heading]x[/heading]\x1b[2J";
    let expected = escape_control_characters(value.into());
    let spec = TabularSpec::builder()
        .column(Col::fixed(expected.len()))
        .build();
    let formatter = TabularFormatter::new(&spec, expected.len());
    for row in [
        formatter.format_row(&[value]),
        formatter.format_row_cells(&[CellValue::Single(value)]),
        formatter.format_row_lines(&[value]).join("\n"),
    ] {
        let rendered = standout_render::template::apply_style_tags(
            &row,
            &standout_render::Styles::new(),
            StyleMode::Plain,
        );
        assert_eq!(rendered, expected);
    }
}

#[test]
fn nul_separator_has_the_same_width_in_borders_and_rows() {
    let value = RenderData::Object(
        [("separator".into(), RenderData::String("\0".into()))]
            .into_iter()
            .collect(),
    );
    let rendered = output("{% set t = table([{'width': 1}, {'width': 1}], separator=separator, border='ascii') %}{{ t.top_border() }}\n{{ t.row(['', 'z']) }}\n{{ t.bottom_border() }}", &value, ColorPolicy::Never);
    let widths: Vec<_> = rendered
        .lines()
        .map(|line| columns(line, AmbiguousWidth::Narrow))
        .collect();
    assert_eq!(widths, vec![9, 9, 9]);
}

#[test]
fn native_formatted_rows_remain_typed_when_nested_in_render_data() {
    use standout_render::tabular::{Col, Table, TabularFormatter, TabularSpec};
    let value = FormattedText::text("[draft]\x1b[2J")
        .styled("heading")
        .unwrap();
    let expected = escape_control_characters(value.plain_text());
    let spec = TabularSpec::builder()
        .column(Col::fixed(expected.len()))
        .build();
    let formatter = TabularFormatter::new(&spec, expected.len());
    let table = Table::new(spec, expected.len());
    let mut rows = formatter.format_formatted_row_lines(std::slice::from_ref(&value));
    rows.push(formatter.format_formatted_row(std::slice::from_ref(&value)));
    rows.push(table.row_formatted(std::slice::from_ref(&value)));
    for row in rows {
        let nested = RenderData::from_serialize(vec![row]).unwrap();
        let rendered = output("{{ value[0] }}tail", &data(nested), ColorPolicy::Always);
        assert_eq!(
            console::strip_ansi_codes(&rendered),
            format!("{expected}tail")
        );
        assert!(rendered.contains("\x1b[1m"));
        assert!(rendered.ends_with("\x1b[0mtail"));
    }
}

#[test]
fn invalid_style_from_value_stays_literal_and_fits_its_column() {
    use standout_render::tabular::{Col, TabularFormatter, TabularSpec};
    let spec = TabularSpec::builder()
        .column(Col::fixed(12).style_from_value())
        .build();
    let formatter = TabularFormatter::new(&spec, 12);
    for value in ["[draft]", "Foo Bar", "bad/name", "\x1b[2J"] {
        let expected = output(
            "{{ value | pad_right(12) }}",
            &data(RenderData::String(value.into())),
            ColorPolicy::Never,
        );
        let row = formatter.format_row(&[value]);
        let rendered = standout_render::template::apply_style_tags(
            &row,
            &standout_render::Styles::new(),
            StyleMode::Ansi,
        );
        assert_eq!(rendered, expected);
    }
}

#[test]
fn native_typed_cells_preserve_styles_in_single_and_nested_cells() {
    use standout_render::tabular::{
        CellValue, Col, SubCol, SubColumns, TabularFormatter, TabularSpec,
    };
    let value = FormattedText::from_ansi_sgr("\x1b[31m[draft]\x1b[0m");
    let nested = SubColumns::new(vec![SubCol::fill()], " ").unwrap();
    for (column, cell) in [
        (Col::fixed(7), CellValue::Formatted(&value)),
        (
            Col::fixed(7).sub_columns(nested),
            CellValue::SubFormatted(vec![&value]),
        ),
    ] {
        let spec = TabularSpec::builder().column(column).build();
        let row = TabularFormatter::new(&spec, 7).format_row_cells(&[cell]);
        let rendered = standout_render::template::apply_style_tags(
            &row,
            &standout_render::Styles::new(),
            StyleMode::Ansi,
        );
        assert_eq!(console::strip_ansi_codes(&rendered), "[draft]");
        assert!(rendered.contains('\x1b'));
    }
}

#[test]
fn native_headers_treat_explicit_and_inferred_labels_as_literal_text() {
    use standout_render::tabular::{Col, Table, TabularSpec};
    let value = "[heading]label[/heading]\\\x1b[31mred\x1b[0m";
    let expected = escape_control_characters(value.into());
    for source in 0..4 {
        let column = match source {
            1 => Col::fixed(expected.len()).header(value),
            2 => Col::fixed(expected.len()).key(value),
            3 => Col::fixed(expected.len()).named(value),
            _ => Col::fixed(expected.len()),
        };
        let table = Table::new(
            TabularSpec::builder().column(column).build(),
            expected.len(),
        );
        let table = if source == 0 {
            table.header([value])
        } else {
            table.header_from_columns()
        };
        let rendered = standout_render::template::apply_style_tags(
            &table.header_row(),
            &standout_render::Styles::new(),
            StyleMode::Ansi,
        );
        assert_eq!(rendered, expected, "header source {source}");
    }
}

fn native_plain(markup: &str) -> String {
    standout_render::template::apply_style_tags(
        markup,
        &standout_render::Styles::new(),
        StyleMode::Plain,
    )
}

#[test]
fn native_and_template_headers_preserve_explicit_formatting() {
    use standout_render::tabular::{Col, Table, TabularSpec};
    let value = FormattedText::from_ansi_sgr("\x1b[31mx\x1b[0m").append("[draft]");
    let table = Table::new(TabularSpec::builder().column(Col::fixed(8)).build(), 8)
        .header_formatted([value.clone()]);
    let rendered = standout_render::template::apply_style_tags(
        &table.header_row(),
        &standout_render::Styles::new(),
        StyleMode::Ansi,
    );
    assert_eq!(console::strip_ansi_codes(&rendered), "x[draft]");
    assert!(rendered.contains('\x1b'));
    for expression in ["value", "captured"] {
        let template = format!("{{% set captured %}}{{{{ value }}}}{{% endset %}}{{% set t = table([{{'width': 8}}], header=[{expression}]) %}}{{{{ t.header_row() }}}}");
        let rendered = output(&template, &data(value.clone().into()), ColorPolicy::Always);
        assert_eq!(console::strip_ansi_codes(&rendered), "x[draft]");
        assert!(rendered.contains('\x1b'));
    }
}

#[test]
fn invalid_native_header_and_row_style_names_do_not_become_text() {
    use standout_render::tabular::{Col, Table, TabularSpec};
    let table = Table::new(TabularSpec::builder().column(Col::fixed(4)).build(), 4)
        .header(["head"])
        .header_style("bad]name")
        .row_styles("bad]even", "bad]odd");
    assert_eq!(
        native_plain(&table.render(&[vec!["row1"], vec!["row2"]])),
        "head\nrow1\nrow2"
    );
}
