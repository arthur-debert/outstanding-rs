use super::*;

#[cfg(test)]
fn format_cell(value: &str, width: usize, col: &Column) -> String {
    format_cell_with_policy(value, width, col, AmbiguousWidth::Narrow)
}

pub(super) fn format_cell_with_policy(
    value: &str,
    width: usize,
    col: &Column,
    policy: AmbiguousWidth,
) -> String {
    let style_override = if col.style_from_value {
        Some(value)
    } else {
        None
    };
    let style = style_override.or(col.style.as_deref());
    format_value_with_policy(value, width, col.align, &col.overflow, style, policy)
}

#[cfg(test)]
fn format_value(
    value: &str,
    width: usize,
    align: Align,
    overflow: &Overflow,
    style: Option<&str>,
) -> String {
    format_value_with_policy(value, width, align, overflow, style, AmbiguousWidth::Narrow)
}

pub(super) fn format_value_with_policy(
    value: &str,
    width: usize,
    align: Align,
    overflow: &Overflow,
    style: Option<&str>,
    policy: AmbiguousWidth,
) -> String {
    if width == 0 {
        return String::new();
    }

    let current_width = visible_width_with_policy(value, policy);

    if current_width > width {
        let truncated = match overflow {
            Overflow::Truncate { at, marker } => match at {
                TruncateAt::End => truncate_visible_end_with_policy(value, width, marker, policy),
                TruncateAt::Start => {
                    truncate_visible_start_with_policy(value, width, marker, policy)
                }
                TruncateAt::Middle => {
                    truncate_visible_middle_with_policy(value, width, marker, policy)
                }
            },
            Overflow::Clip => truncate_visible_end_with_policy(value, width, "", policy),
            Overflow::Expand => {
                return apply_style(value, style);
            }
            Overflow::Wrap { .. } => truncate_visible_end_with_policy(value, width, "…", policy),
        };

        let padded = pad_visible_value(&truncated, width, align, policy);
        apply_style(&padded, style)
    } else {
        let padded = pad_visible_value(value, width, align, policy);
        apply_style(&padded, style)
    }
}

pub(super) fn pad_visible_value(
    value: &str,
    width: usize,
    align: Align,
    policy: AmbiguousWidth,
) -> String {
    let padding = width.saturating_sub(visible_width_with_policy(value, policy));
    match align {
        Align::Left => format!("{}{}", value, " ".repeat(padding)),
        Align::Right => format!("{}{}", " ".repeat(padding), value),
        Align::Center => {
            let left = padding / 2;
            format!(
                "{}{}{}",
                " ".repeat(left),
                value,
                " ".repeat(padding - left)
            )
        }
    }
}

#[derive(Clone, Debug)]
pub enum CellValue<'a> {
    Single(&'a str),
    Sub(Vec<&'a str>),
    Formatted(&'a FormattedText),
    SubFormatted(Vec<&'a FormattedText>),
}

impl CellValue<'_> {
    pub(super) fn to_markup(&self) -> OwnedCellValue {
        match self {
            Self::Single(text) => OwnedCellValue::Single(escape_text(text)),
            Self::Sub(values) => {
                OwnedCellValue::Sub(values.iter().map(|text| escape_text(text)).collect())
            }
            Self::Formatted(text) => OwnedCellValue::Single(markup(&Value::from((*text).clone()))),
            Self::SubFormatted(values) => OwnedCellValue::Sub(
                values
                    .iter()
                    .map(|text| markup(&Value::from((*text).clone())))
                    .collect(),
            ),
        }
    }
}

impl<'a> From<&'a str> for CellValue<'a> {
    fn from(text: &'a str) -> Self {
        Self::Single(text)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum MarkupCellValue<'a> {
    Single(&'a str),
    Sub(Vec<&'a str>),
}

pub(crate) enum OwnedCellValue {
    Single(String),
    Sub(Vec<String>),
}

impl OwnedCellValue {
    pub(crate) fn as_borrowed(&self) -> MarkupCellValue<'_> {
        match self {
            Self::Single(text) => MarkupCellValue::Single(text),
            Self::Sub(values) => MarkupCellValue::Sub(values.iter().map(String::as_str).collect()),
        }
    }
}

pub(super) fn apply_style(content: &str, style: Option<&str>) -> String {
    match style {
        Some(s) if standout_bbparser::is_valid_tag_name(s) => format!("[{}]{}[/{}]", s, content, s),
        _ => content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{display_width, Width};

    #[test]
    fn format_cell_clip_no_marker() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(5)).clip())
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello World"]);
        assert_eq!(display_width(&output), 5);
        assert!(!output.contains("…"));
        assert!(output.starts_with("Hello"));
    }

    #[test]
    fn format_cell_expand_overflows() {
        let col = Column::new(Width::Fixed(5)).overflow(Overflow::Expand);
        let output = format_cell("Hello World", 5, &col);

        assert_eq!(output, "Hello World");
        assert_eq!(display_width(&output), 11); // Full width
    }

    #[test]
    fn format_cell_expand_pads_when_short() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Expand);
        let output = format_cell("Hi", 10, &col);

        assert_eq!(output, "Hi        ");
        assert_eq!(display_width(&output), 10);
    }

    #[test]
    fn format_cell_with_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).style("header"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello"]);
        assert!(output.starts_with("[header]"));
        assert!(output.ends_with("[/header]"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn format_cell_style_from_value() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).style_from_value())
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["error"]);
        assert!(output.contains("[error]"));
        assert!(output.contains("[/error]"));
    }

    #[test]
    fn format_cell_no_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["Hello"]);
        assert!(!output.contains("["));
        assert!(!output.contains("]"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn format_cell_style_overrides_style_from_value() {
        let mut col = Column::new(Width::Fixed(10));
        col.style = Some("default".to_string());
        col.style_from_value = true;

        let spec = FlatDataSpec::builder().column(col).build();
        let formatter = TabularFormatter::new(&spec, 80);

        let output = formatter.format_row(&["custom"]);
        assert!(output.contains("[custom]"));
        assert!(output.contains("[/custom]"));
    }

    #[test]
    fn format_value_bbcode_preserves_tags_when_fitting() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        let result = format_value("[bold]hello[/bold]", 10, Align::Left, &overflow, None);
        assert_eq!(
            visible_width_with_policy(&result, AmbiguousWidth::Narrow),
            10,
            "visible width should be 10"
        );
        assert!(
            result.contains("[bold]hello[/bold]"),
            "tags should be preserved when content fits"
        );
    }

    #[test]
    fn format_value_bbcode_truncation() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        let result = format_value("[red]hello world[/red]", 8, Align::Left, &overflow, None);
        assert_eq!(
            visible_width_with_policy(&result, AmbiguousWidth::Narrow),
            8,
            "truncated output should be exactly 8 visible columns"
        );
        assert_eq!(result, "[red]hello w[/red]…");
    }

    #[test]
    fn overflowing_highlighted_table_cell_keeps_match_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(12)))
            .build();
        let formatter = TabularFormatter::new(&spec, 12);

        let value = FormattedText::text("prefix ")
            .append(FormattedText::text("needle").styled("match").unwrap())
            .append(" suffix");
        let result = formatter.format_formatted_row(&[value]);

        assert_eq!(result.plain_text(), "prefix need…");
        let result = markup(&Value::from(result));
        assert_eq!(result, "prefix [match]need[/match]…");
        assert_eq!(
            visible_width_with_policy(&result, AmbiguousWidth::Narrow),
            12
        );
    }

    #[test]
    fn format_value_bbcode_right_align() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        let result = format_value("[dim]hi[/dim]", 6, Align::Right, &overflow, None);
        assert_eq!(
            visible_width_with_policy(&result, AmbiguousWidth::Narrow),
            6
        );
        assert!(result.contains("[dim]hi[/dim]"));
        assert!(result.starts_with("    "));
    }

    #[test]
    fn format_value_bbcode_with_style() {
        let overflow = Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        };
        let result = format_value("[dim]ok[/dim]", 8, Align::Left, &overflow, Some("green"));
        assert_eq!(
            visible_width_with_policy(&result, AmbiguousWidth::Narrow),
            8
        );
        assert!(result.starts_with("[green]"));
        assert!(result.ends_with("[/green]"));
    }
}
