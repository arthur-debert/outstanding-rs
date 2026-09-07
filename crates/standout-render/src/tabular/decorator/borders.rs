use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    #[default]
    None,
    Ascii,
    Light,
    Heavy,
    Double,
    Rounded,
}

impl BorderStyle {
    fn chars(&self) -> BorderChars {
        match self {
            BorderStyle::None => BorderChars::empty(),
            BorderStyle::Ascii => BorderChars {
                horizontal: '-',
                vertical: '|',
                top_left: '+',
                top_right: '+',
                bottom_left: '+',
                bottom_right: '+',
                left_t: '+',
                cross: '+',
                right_t: '+',
                top_t: '+',
                bottom_t: '+',
            },
            BorderStyle::Light => BorderChars {
                horizontal: '─',
                vertical: '│',
                top_left: '┌',
                top_right: '┐',
                bottom_left: '└',
                bottom_right: '┘',
                left_t: '├',
                cross: '┼',
                right_t: '┤',
                top_t: '┬',
                bottom_t: '┴',
            },
            BorderStyle::Heavy => BorderChars {
                horizontal: '━',
                vertical: '┃',
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
                left_t: '┣',
                cross: '╋',
                right_t: '┫',
                top_t: '┳',
                bottom_t: '┻',
            },
            BorderStyle::Double => BorderChars {
                horizontal: '═',
                vertical: '║',
                top_left: '╔',
                top_right: '╗',
                bottom_left: '╚',
                bottom_right: '╝',
                left_t: '╠',
                cross: '╬',
                right_t: '╣',
                top_t: '╦',
                bottom_t: '╩',
            },
            BorderStyle::Rounded => BorderChars {
                horizontal: '─',
                vertical: '│',
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                left_t: '├',
                cross: '┼',
                right_t: '┤',
                top_t: '┬',
                bottom_t: '┴',
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BorderChars {
    horizontal: char,
    vertical: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    left_t: char,
    cross: char,
    right_t: char,
    top_t: char,
    bottom_t: char,
}

impl BorderChars {
    fn empty() -> Self {
        BorderChars {
            horizontal: ' ',
            vertical: ' ',
            top_left: ' ',
            top_right: ' ',
            bottom_left: ' ',
            bottom_right: ' ',
            left_t: ' ',
            cross: ' ',
            right_t: ' ',
            top_t: ' ',
            bottom_t: ' ',
        }
    }
}

impl Table {
    pub(super) fn rebuild_formatter(&mut self) {
        let policy = self.formatter.ambiguous_width();
        let mut formatter_width = self.requested_width;

        // Preserve the legacy Narrow layout exactly. Under Wide, Unicode border
        // glyphs occupy two cells, so the requested table width becomes a hard
        // maximum and the formatter receives the remaining interior width.
        if policy == AmbiguousWidth::Wide
            && matches!(
                self.border,
                BorderStyle::Light
                    | BorderStyle::Heavy
                    | BorderStyle::Double
                    | BorderStyle::Rounded
            )
        {
            let calculator = WidthCalculator::new(policy);
            let vertical = self.border.chars().vertical;
            formatter_width =
                formatter_width.saturating_sub(calculator.char_width(vertical).saturating_mul(2));
        }

        self.formatter = match &self.data_widths {
            Some(measured) => {
                let resolved =
                    self.spec
                        .resolve_prepared_widths_measured(formatter_width, measured, policy);
                TabularFormatter::from_prepared_resolved(
                    &self.spec,
                    resolved,
                    formatter_width,
                    policy,
                )
            }
            None => TabularFormatter::from_prepared_spec(&self.spec, formatter_width, policy),
        };
        if policy == AmbiguousWidth::Wide && self.border != BorderStyle::None {
            self.formatter.limit_to_width(formatter_width);
        }
    }

    pub fn separator_row(&self) -> String {
        self.horizontal_line(LineType::Middle)
    }

    pub fn top_border(&self) -> String {
        self.horizontal_line(LineType::Top)
    }

    pub fn bottom_border(&self) -> String {
        self.horizontal_line(LineType::Bottom)
    }

    pub(super) fn wrap_row(&self, content: &str) -> String {
        if self.border == BorderStyle::None {
            return content.to_string();
        }

        let chars = self.border.chars();
        format!("{}{}{}", chars.vertical, content, chars.vertical)
    }

    fn horizontal_line(&self, line_type: LineType) -> String {
        if self.border == BorderStyle::None {
            return String::new();
        }

        let chars = self.border.chars();
        let total_content = self.formatter.rendered_width();

        let (left, _joint, right) = match line_type {
            LineType::Top => (chars.top_left, chars.top_t, chars.top_right),
            LineType::Middle => (chars.left_t, chars.cross, chars.right_t),
            LineType::Bottom => (chars.bottom_left, chars.bottom_t, chars.bottom_right),
        };

        let horizontal_width = WidthCalculator::new(self.formatter.ambiguous_width())
            .char_width(chars.horizontal)
            .max(1);
        format!(
            "{}{}{}",
            left,
            std::iter::repeat_n(chars.horizontal, total_content / horizontal_width)
                .collect::<String>(),
            right
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineType {
    Top,
    Middle,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::decorator::tests::simple_spec;
    use crate::tabular::Col;
    use crate::WidthCalculator;

    #[test]
    fn table_no_border() {
        let table = Table::new(simple_spec(), 80);
        let row = table.row(&["Hello", "World"]);
        assert!(!row.contains('│'));
        assert!(row.contains("Hello"));
    }

    #[test]
    fn table_with_ascii_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Ascii);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('|'));
        assert!(row.ends_with('|'));
    }

    #[test]
    fn table_with_light_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
    }

    #[test]
    fn table_with_heavy_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Heavy);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('┃'));
        assert!(row.ends_with('┃'));
    }

    #[test]
    fn table_with_double_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Double);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('║'));
        assert!(row.ends_with('║'));
    }

    #[test]
    fn table_with_rounded_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Rounded);
        let row = table.row(&["Hello", "World"]);
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
    }

    #[test]
    fn table_separator_row() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let sep = table.separator_row();
        assert!(sep.contains('─'));
        assert!(sep.starts_with('├'));
        assert!(sep.ends_with('┤'));
    }

    #[test]
    fn table_top_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let top = table.top_border();
        assert!(top.contains('─'));
        assert!(top.starts_with('┌'));
        assert!(top.ends_with('┐'));
    }

    #[test]
    fn table_bottom_border() {
        let table = Table::new(simple_spec(), 80).border(BorderStyle::Light);
        let bottom = table.bottom_border();
        assert!(bottom.contains('─'));
        assert!(bottom.starts_with('└'));
        assert!(bottom.ends_with('┘'));
    }

    #[test]
    fn table_render_no_border() {
        let table = Table::new(simple_spec(), 80).header(vec!["Name", "Value"]);

        let data = vec![vec!["Alice", "100"]];

        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        assert!(lines.len() >= 2);
        assert!(lines[0].contains("Name"));
        assert!(lines[1].contains("Alice"));
    }

    #[test]
    fn border_style_default() {
        assert_eq!(BorderStyle::default(), BorderStyle::None);
    }

    #[test]
    fn table_row_from_with_border() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Item {
            id: u32,
            value: String,
        }

        let spec = TabularSpec::builder()
            .column(Col::fixed(5).key("id"))
            .column(Col::fixed(10).key("value"))
            .build();

        let table = Table::new(spec, 80).border(BorderStyle::Light);
        let item = Item {
            id: 42,
            value: "test".to_string(),
        };

        let row = table.row_from(&item);
        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
        assert!(row.contains("42"));
        assert!(row.contains("test"));
    }

    #[test]
    fn table_row_separator_option() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .build();

        let table = Table::new(spec, 80)
            .border(BorderStyle::Light)
            .row_separator(true);

        let data = vec![vec!["A", "1"], vec!["B", "2"], vec!["C", "3"]];
        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        let sep_count = lines.iter().filter(|l| l.starts_with('├')).count();
        assert_eq!(sep_count, 2, "Expected 2 separators between 3 rows");
    }

    #[test]
    fn table_row_separator_disabled_by_default() {
        let spec = TabularSpec::builder()
            .column(Col::fixed(10))
            .column(Col::fixed(8))
            .build();

        let table = Table::new(spec, 80).border(BorderStyle::Light);

        let data = vec![vec!["A", "1"], vec!["B", "2"]];
        let output = table.render(&data);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn unicode_borders_honor_wide_maximum_without_ascii_fallback() {
        let styles = [
            (BorderStyle::Light, '┌'),
            (BorderStyle::Heavy, '┏'),
            (BorderStyle::Double, '╔'),
            (BorderStyle::Rounded, '╭'),
        ];
        let calculator = WidthCalculator::new(AmbiguousWidth::Wide);

        for (style, expected_corner) in styles {
            for requested in [20, 21] {
                let spec = TabularSpec::builder().column(Col::fill()).build();
                let table = Table::with_ambiguous_width(spec, requested, AmbiguousWidth::Wide)
                    .border(style);
                let rendered = table.render(&[vec!["≈Δ"]]);

                for line in rendered.lines() {
                    let width = calculator.visible_width(line);
                    assert!(
                        width <= requested,
                        "{style:?} line exceeded {requested}: {line}"
                    );
                    assert!(
                        requested - width <= 1,
                        "{style:?} underfilled {requested}: {line}"
                    );
                }
                assert!(table.top_border().starts_with(expected_corner));
                assert!(
                    !rendered.contains('+'),
                    "must not substitute ASCII: {rendered}"
                );
            }
        }
    }

    #[test]
    fn narrow_unicode_border_layout_remains_compatible() {
        let calculator = WidthCalculator::new(AmbiguousWidth::Narrow);

        for style in [
            BorderStyle::Light,
            BorderStyle::Heavy,
            BorderStyle::Double,
            BorderStyle::Rounded,
        ] {
            let spec = TabularSpec::builder().column(Col::fill()).build();
            let default = Table::new(spec.clone(), 21)
                .border(style)
                .render(&[vec!["≈Δ"]]);
            let explicit = Table::with_ambiguous_width(spec, 21, AmbiguousWidth::Narrow)
                .border(style)
                .render(&[vec!["≈Δ"]]);

            assert_eq!(default, explicit);
            assert!(default
                .lines()
                .all(|line| calculator.visible_width(line) == 23));
        }
    }
}
