use super::cells::format_value_with_policy;
use super::*;

#[cfg(test)]
fn resolve_sub_widths(sub_cols: &SubColumns, values: &[&str], parent_width: usize) -> Vec<usize> {
    resolve_sub_widths_with_policy(sub_cols, values, parent_width, AmbiguousWidth::Narrow)
}

fn resolve_sub_widths_with_policy(
    sub_cols: &SubColumns,
    values: &[&str],
    parent_width: usize,
    policy: AmbiguousWidth,
) -> Vec<usize> {
    let sep_width = visible_width_with_policy(&sub_cols.separator, policy);
    let n = sub_cols.columns.len();
    let mut widths = vec![0usize; n];
    let mut grower_index = 0;

    for (i, sub_col) in sub_cols.columns.iter().enumerate() {
        match &sub_col.width {
            Width::Fill => {
                grower_index = i;
            }
            Width::Fixed(w) => {
                widths[i] = *w;
            }
            Width::Bounded { min, max } => {
                let content_w = values
                    .get(i)
                    .map(|v| visible_width_with_policy(v, policy))
                    .unwrap_or(0);
                let min_w = min.unwrap_or(0);
                let max_w = max.unwrap_or(usize::MAX);
                widths[i] = content_w.max(min_w).min(max_w);
            }
            Width::Fraction(_) => {} // validated away at construction
        }
    }

    // The grower always counts toward the separator overhead, even at zero
    // width; only a zero-width non-grower column is elided from the join.
    let visible_non_growers = widths
        .iter()
        .enumerate()
        .filter(|&(i, &w)| i != grower_index && w > 0)
        .count();
    let visible_count = visible_non_growers + 1; // +1 for grower
    let sep_overhead = visible_count.saturating_sub(1) * sep_width;
    let available = parent_width.saturating_sub(sep_overhead);

    let non_grower_total: usize = widths
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != grower_index)
        .map(|(_, &w)| w)
        .sum();

    if non_grower_total > available {
        let mut excess = non_grower_total - available;
        for i in (0..n).rev() {
            if i == grower_index || widths[i] == 0 || excess == 0 {
                continue;
            }
            let reduction = excess.min(widths[i]);
            widths[i] -= reduction;
            excess -= reduction;
        }
    }

    let clamped_total: usize = widths
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != grower_index)
        .map(|(_, &w)| w)
        .sum();
    widths[grower_index] = available.saturating_sub(clamped_total);

    widths
}

#[cfg(test)]
fn format_sub_cells(sub_cols: &SubColumns, values: &[&str], parent_width: usize) -> String {
    format_sub_cells_with_policy(sub_cols, values, parent_width, AmbiguousWidth::Narrow)
}

pub(super) fn format_sub_cells_with_policy(
    sub_cols: &SubColumns,
    values: &[&str],
    parent_width: usize,
    policy: AmbiguousWidth,
) -> String {
    if parent_width == 0 {
        return String::new();
    }

    let widths = resolve_sub_widths_with_policy(sub_cols, values, parent_width, policy);
    let grower_index = sub_cols
        .columns
        .iter()
        .position(|c| matches!(c.width, Width::Fill))
        .unwrap_or(0);
    let sep = &sub_cols.separator;
    let mut parts: Vec<String> = Vec::new();

    for (i, (sub_col, &width)) in sub_cols.columns.iter().zip(widths.iter()).enumerate() {
        if width == 0 && i != grower_index {
            continue;
        }
        if width == 0 {
            parts.push(String::new());
        } else {
            let value = values.get(i).copied().unwrap_or(&sub_col.null_repr);
            parts.push(format_value_with_policy(
                value,
                width,
                sub_col.align,
                &sub_col.overflow,
                sub_col.style.as_deref(),
                policy,
            ));
        }
    }

    parts.join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{display_width, TabularSpec, Width};
    use crate::tabular::{SubCol, SubColumns};

    fn padz_spec() -> (FlatDataSpec, SubColumns) {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols.clone()))
            .column(Column::new(Width::Fixed(6)).right())
            .separator("  ")
            .build();

        (spec, sub_cols)
    }

    #[test]
    fn sub_column_basic_title_and_tag() {
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row_cells(&[
            CellValue::Single("1."),
            CellValue::Sub(vec!["Gallery Navigation", "[feature]"]),
            CellValue::Single("4d"),
        ]);

        let row =
            crate::template::apply_style_tags(&row, &crate::Styles::new(), crate::StyleMode::Plain);
        assert!(row.contains("Gallery Navigation"));
        assert!(row.contains("[feature]"));
        assert!(row.contains("1."));
        assert!(row.contains("4d"));
        assert_eq!(display_width(&row), 60);
    }

    #[test]
    fn sub_column_tag_absent() {
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row_cells(&[
            CellValue::Single("3."),
            CellValue::Sub(vec!["Fixing Layout of Image Nav", ""]),
            CellValue::Single("4d"),
        ]);

        assert!(row.contains("Fixing Layout of Image Nav"));
        assert_eq!(display_width(&row), 60);
    }

    #[test]
    fn sub_column_grower_gets_remaining_space() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(10)], "  ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", "fixed"], 50);
        assert_eq!(widths[0], 38);
        assert_eq!(widths[1], 10);
    }

    #[test]
    fn sub_column_non_grower_respects_fixed() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(15)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["x", "y"], 40);
        assert_eq!(widths[1], 15); // Always exact for Fixed
        assert_eq!(widths[0], 24); // 40 - 15 - 1
    }

    #[test]
    fn sub_column_non_grower_respects_bounded() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(5, 20)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", "short"], 40);
        assert_eq!(widths[1], 5);
        assert_eq!(widths[0], 34); // 40 - 5 - 1

        let widths2 = resolve_sub_widths(&sub_cols, &["title", "a very long tag value!"], 40);
        assert_eq!(widths2[1], 20);
        assert_eq!(widths2[0], 19); // 40 - 20 - 1

        let widths3 = resolve_sub_widths(&sub_cols, &["title", ""], 40);
        assert_eq!(widths3[1], 5);
    }

    #[test]
    fn sub_column_bounded_min_zero() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", ""], 40);
        assert_eq!(widths[1], 0);
        assert_eq!(widths[0], 40);
    }

    #[test]
    fn sub_column_separator_skipped_for_zero_width() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], "  ").unwrap();

        let result1 = format_sub_cells(&sub_cols, &["Title", "tag"], 30);
        assert!(result1.contains("  ")); // Separator present
        assert_eq!(display_width(&result1), 30);

        let result2 = format_sub_cells(&sub_cols, &["Title", ""], 30);
        assert_eq!(display_width(&result2), 30);
    }

    #[test]
    fn sub_column_alignment() {
        let sub_cols = SubColumns::new(
            vec![
                SubCol::fill(), // left-aligned by default
                SubCol::fixed(10).right(),
            ],
            " ",
        )
        .unwrap();

        let result = format_sub_cells(&sub_cols, &["Left", "Right"], 30);
        assert!(result.starts_with("Left"));
        assert!(result.ends_with("     Right"));
        assert_eq!(display_width(&result), 30);
    }

    #[test]
    fn sub_column_grower_truncation() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(15)], " ").unwrap();

        let result = format_sub_cells(
            &sub_cols,
            &["A very long title that exceeds", "fixed-col"],
            25,
        );
        assert_eq!(display_width(&result), 25);
        assert!(result.contains("…")); // Truncation marker
    }

    #[test]
    fn sub_column_style_application() {
        let sub_cols = SubColumns::new(
            vec![SubCol::fill(), SubCol::bounded(0, 20).right().style("tag")],
            " ",
        )
        .unwrap();

        let result = format_sub_cells(&sub_cols, &["Title", "feature"], 40);
        assert!(result.contains("[tag]"));
        assert!(result.contains("[/tag]"));
        assert!(result.contains("feature"));
    }

    #[test]
    fn sub_column_grower_zero_width() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(20)], " ").unwrap();

        let widths = resolve_sub_widths(&sub_cols, &["title", "fixed"], 20);
        assert_eq!(widths[0], 0); // Grower gets nothing
        assert_eq!(widths[1], 19); // Clamped: 20 - 1 sep = 19 available

        let result = format_sub_cells(&sub_cols, &["title", "fixed"], 20);
        assert_eq!(display_width(&result), 20);
    }

    #[test]
    fn sub_column_all_empty() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 20)], " ").unwrap();

        let result = format_sub_cells(&sub_cols, &["", ""], 30);
        assert_eq!(display_width(&result), 30);
    }

    #[test]
    fn sub_column_plain_string_fallback() {
        let (spec, _) = padz_spec();
        let formatter = TabularFormatter::new(&spec, 60);

        let row = formatter.format_row(&["1.", "Just a title", "4d"]);
        assert_eq!(display_width(&row), 60);
        assert!(row.contains("Just a title"));
    }

    #[test]
    fn sub_column_format_row_cells_api() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(3)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .separator("  ")
            .build();

        let formatter = TabularFormatter::new(&spec, 50);

        let row1 = formatter.format_row_cells(&[
            CellValue::Single("1."),
            CellValue::Sub(vec!["Title", "[bug]"]),
        ]);
        let row1 = crate::template::apply_style_tags(
            &row1,
            &crate::Styles::new(),
            crate::StyleMode::Plain,
        );
        assert_eq!(display_width(&row1), 50);
        assert!(row1.contains("Title"));
        assert!(row1.contains("[bug]"));

        let row2 = formatter.format_row_cells(&[
            CellValue::Single("2."),
            CellValue::Sub(vec!["Longer Title Here", ""]),
        ]);
        assert_eq!(display_width(&row2), 50);
        assert!(row2.contains("Longer Title Here"));
    }

    #[test]
    fn sub_column_via_template() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 50);

        let mut env = crate::template::new_environment();
        env.add_template("test", "{{ t.row(['1.', ['My Title', '[tag]']]) }}")
            .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { t => Value::from_object(formatter) })
            .unwrap();

        let output = standout_bbparser::strip_tags(&output);
        assert_eq!(display_width(&output), 50);
        assert!(output.contains("My Title"));
        assert!(output.contains("[tag]"));
    }

    #[test]
    fn sub_column_multiple_rows_alignment() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 15).right()], " ").unwrap();

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(4)))
            .column(Column::new(Width::Fill).sub_columns(sub_cols))
            .column(Column::new(Width::Fixed(4)).right())
            .separator("  ")
            .build();

        let formatter = TabularFormatter::new(&spec, 60);

        let rows = [
            vec![
                CellValue::Single("1."),
                CellValue::Sub(vec!["GitHub integration", "[feature]"]),
                CellValue::Single("8h"),
            ],
            vec![
                CellValue::Single("2."),
                CellValue::Sub(vec!["Bug : Static", "[bug]"]),
                CellValue::Single("4d"),
            ],
            vec![
                CellValue::Single("3."),
                CellValue::Sub(vec!["Fixing Layout of Image Nav", ""]),
                CellValue::Single("4d"),
            ],
        ];

        for (i, row) in rows.iter().enumerate() {
            let output = formatter.format_row_cells(row);
            let output = crate::template::apply_style_tags(
                &output,
                &crate::Styles::new(),
                crate::StyleMode::Plain,
            );
            assert_eq!(
                display_width(&output),
                60,
                "Row {} has wrong width: '{}'",
                i,
                output
            );
        }
    }

    #[test]
    fn resolve_sub_widths_bbcode() {
        use crate::tabular::{SubCol, SubColumns};
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 30).right()], " ").unwrap();
        let widths = resolve_sub_widths(&sub_cols, &["Title", "[dim][tag][/dim]"], 30);
        assert_eq!(
            widths[1], 5,
            "bounded sub-col should use visible width, not raw string length"
        );
        assert_eq!(
            widths[0] + widths[1] + 1, // +1 for separator " "
            30,
            "widths + separator should equal parent width"
        );
    }
}
#[cfg(test)]
mod proptests {
    use super::*;
    use crate::tabular::{display_width, SubCol, SubColumns};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn sub_column_output_width_equals_parent(
            parent_width in 10usize..100,
            title_len in 0usize..50,
            tag_len in 0usize..30,
            bounded_max in 5usize..30,
        ) {
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::bounded(0, bounded_max)],
                " ",
            ).unwrap();

            let title: String = "x".repeat(title_len);
            let tag: String = "y".repeat(tag_len);
            let values: Vec<&str> = vec![&title, &tag];

            let result = format_sub_cells(&sub_cols, &values, parent_width);
            prop_assert_eq!(
                display_width(&result),
                parent_width,
                "sub-cell output must exactly fill parent width. Got '{}' (dw={}), expected {}",
                result, display_width(&result), parent_width
            );
        }

        #[test]
        fn sub_column_non_grower_respects_bounds(
            parent_width in 30usize..100,
            min_w in 0usize..10,
            max_w_offset in 1usize..20,
            content_len in 0usize..40,
        ) {
            let max_w = min_w + max_w_offset; // ensure max > min
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::bounded(min_w, max_w)],
                " ",
            ).unwrap();

            let content: String = "z".repeat(content_len);
            let values = vec!["title", content.as_str()];
            let widths = resolve_sub_widths(&sub_cols, &values, parent_width);

            let bounded_width = widths[1];
            prop_assert!(
                bounded_width >= min_w,
                "bounded width {} < min {}", bounded_width, min_w
            );
            prop_assert!(
                bounded_width <= max_w,
                "bounded width {} > max {}", bounded_width, max_w
            );
        }

        #[test]
        fn sub_column_width_arithmetic(
            parent_width in 10usize..100,
            fixed_width in 1usize..15,
            title_len in 0usize..50,
        ) {
            let sub_cols = SubColumns::new(
                vec![SubCol::fill(), SubCol::fixed(fixed_width)],
                "  ",
            ).unwrap();

            let title: String = "t".repeat(title_len);
            let values = vec![title.as_str(), "fixed"];
            let widths = resolve_sub_widths(&sub_cols, &values, parent_width);

            let sep_width = display_width(&sub_cols.separator);
            let visible_non_growers: usize = if widths[1] > 0 { 1 } else { 0 };
            let visible_count: usize = visible_non_growers + 1; // +1 for grower
            let sep_overhead = visible_count.saturating_sub(1) * sep_width;
            let total: usize = widths.iter().sum::<usize>() + sep_overhead;

            prop_assert_eq!(
                total, parent_width,
                "widths {:?} + sep_overhead {} != parent {}",
                widths, sep_overhead, parent_width
            );
        }

        #[test]
        fn sub_column_output_three_sub_cols(
            parent_width in 20usize..100,
            prefix_len in 0usize..20,
            tag_len in 0usize..15,
        ) {
            let sub_cols = SubColumns::new(
                vec![
                    SubCol::bounded(0, 10),
                    SubCol::fill(),
                    SubCol::bounded(0, 15).right(),
                ],
                " ",
            ).unwrap();

            let prefix: String = "p".repeat(prefix_len);
            let tag: String = "t".repeat(tag_len);
            let values = vec![prefix.as_str(), "middle content", tag.as_str()];

            let result = format_sub_cells(&sub_cols, &values, parent_width);
            prop_assert_eq!(
                display_width(&result),
                parent_width,
                "three sub-cols output must fill parent width"
            );
        }
    }
}
