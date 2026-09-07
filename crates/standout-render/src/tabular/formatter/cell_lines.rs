use super::cells::{apply_style, pad_visible_value};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellOutput {
    Single(String),
    Multi(Vec<String>),
}

impl CellOutput {
    pub fn is_single(&self) -> bool {
        matches!(self, CellOutput::Single(_))
    }

    pub fn line_count(&self) -> usize {
        match self {
            CellOutput::Single(_) => 1,
            CellOutput::Multi(lines) => lines.len().max(1),
        }
    }

    pub fn line(&self, index: usize, width: usize, align: Align) -> String {
        self.line_with_policy(index, width, align, AmbiguousWidth::Narrow)
    }

    pub fn line_with_policy(
        &self,
        index: usize,
        width: usize,
        align: Align,
        policy: AmbiguousWidth,
    ) -> String {
        let content = match self {
            CellOutput::Single(s) if index == 0 => s.as_str(),
            CellOutput::Multi(lines) => lines.get(index).map(|s| s.as_str()).unwrap_or(""),
            _ => "",
        };

        let content_width = visible_width_with_policy(content, policy);
        if content_width >= width {
            return content.to_string();
        }
        let padding = width - content_width;
        match align {
            Align::Left => format!("{}{}", content, " ".repeat(padding)),
            Align::Right => format!("{}{}", " ".repeat(padding), content),
            Align::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!(
                    "{}{}{}",
                    " ".repeat(left_pad),
                    content,
                    " ".repeat(right_pad)
                )
            }
        }
    }

    pub fn to_single(&self) -> String {
        match self {
            CellOutput::Single(s) => s.clone(),
            CellOutput::Multi(lines) => lines.first().cloned().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
fn format_cell_lines(value: &str, width: usize, col: &Column) -> CellOutput {
    format_cell_lines_with_policy(value, width, col, AmbiguousWidth::Narrow)
}

pub(super) fn format_cell_lines_with_policy(
    value: &str,
    width: usize,
    col: &Column,
    policy: AmbiguousWidth,
) -> CellOutput {
    if width == 0 {
        return CellOutput::Single(String::new());
    }

    let current_width = visible_width_with_policy(value, policy);

    let style = if col.style_from_value {
        Some(value)
    } else {
        col.style.as_deref()
    };

    match &col.overflow {
        Overflow::Wrap { indent } => {
            if current_width <= width {
                let padding = width - current_width;
                let padded = match col.align {
                    Align::Left => format!("{}{}", value, " ".repeat(padding)),
                    Align::Right => format!("{}{}", " ".repeat(padding), value),
                    Align::Center => {
                        let left_pad = padding / 2;
                        let right_pad = padding - left_pad;
                        format!("{}{}{}", " ".repeat(left_pad), value, " ".repeat(right_pad))
                    }
                };
                CellOutput::Single(apply_style(&padded, style))
            } else {
                let wrapped = wrap_visible_indent_with_policy(value, width, *indent, policy);
                let padded: Vec<String> = wrapped
                    .into_iter()
                    .map(|line| {
                        let padded_line = pad_visible_value(&line, width, col.align, policy);
                        apply_style(&padded_line, style)
                    })
                    .collect();
                if padded.len() == 1 {
                    CellOutput::Single(padded.into_iter().next().unwrap())
                } else {
                    CellOutput::Multi(padded)
                }
            }
        }
        _ => CellOutput::Single(format_cell_with_policy(value, width, col, policy)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{display_width, Width};

    #[test]
    fn format_cell_wrap_single_line() {
        let col = Column::new(Width::Fixed(20)).wrap();
        let output = format_cell_lines("Short text", 20, &col);

        assert!(output.is_single());
        assert_eq!(output.line_count(), 1);
        assert_eq!(display_width(&output.to_single()), 20);
    }

    #[test]
    fn format_cell_wrap_multi_line() {
        let col = Column::new(Width::Fixed(10)).wrap();
        let output = format_cell_lines("This is a longer text that wraps", 10, &col);

        assert!(!output.is_single());
        assert!(output.line_count() > 1);

        if let CellOutput::Multi(lines) = &output {
            for line in lines {
                assert_eq!(display_width(line), 10);
            }
        }
    }

    #[test]
    fn format_cell_wrap_with_indent() {
        let col = Column::new(Width::Fixed(15)).overflow(Overflow::Wrap { indent: 2 });
        let output = format_cell_lines("First line then continuation", 15, &col);

        if let CellOutput::Multi(lines) = output {
            assert!(lines[0].starts_with("First"));
            if lines.len() > 1 {
                let second_trimmed = lines[1].trim_start();
                assert!(lines[1].len() > second_trimmed.len()); // Has leading spaces
            }
        }
    }

    #[test]
    fn cell_output_single_accessors() {
        let cell = CellOutput::Single("Hello".to_string());

        assert!(cell.is_single());
        assert_eq!(cell.line_count(), 1);
        assert_eq!(cell.to_single(), "Hello");
    }

    #[test]
    fn cell_output_multi_accessors() {
        let cell = CellOutput::Multi(vec!["Line 1".to_string(), "Line 2".to_string()]);

        assert!(!cell.is_single());
        assert_eq!(cell.line_count(), 2);
        assert_eq!(cell.to_single(), "Line 1");
    }

    #[test]
    fn cell_output_line_accessor() {
        let cell = CellOutput::Multi(vec!["First".to_string(), "Second".to_string()]);

        let line0 = cell.line(0, 10, Align::Left);
        assert_eq!(line0, "First     ");
        assert_eq!(display_width(&line0), 10);

        let line1 = cell.line(1, 10, Align::Right);
        assert_eq!(line1, "    Second");

        let line2 = cell.line(2, 10, Align::Left);
        assert_eq!(line2, "          ");
    }

    #[test]
    fn format_cell_lines_with_style() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).wrap().style("text"))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let lines = formatter.format_row_lines(&["This is a long text that wraps"]);

        for line in &lines {
            assert!(line.contains("[text]"));
            assert!(line.contains("[/text]"));
        }
    }

    #[test]
    fn format_cell_lines_bbcode_wrap() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Wrap { indent: 0 });
        let result = format_cell_lines("[bold]hello world foo[/bold]", 10, &col);
        match result {
            CellOutput::Multi(lines) => {
                assert_eq!(
                    lines,
                    [
                        "[bold]hello[/bold]     ",
                        "[bold]world[/bold] [bold]foo[/bold] ",
                    ]
                );
                for line in &lines {
                    assert!(
                        visible_width_with_policy(line, AmbiguousWidth::Narrow) <= 10,
                        "wrapped line '{}' exceeds column width (visible: {})",
                        line,
                        visible_width_with_policy(line, AmbiguousWidth::Narrow)
                    );
                }
            }
            CellOutput::Single(s) => {
                assert!(
                    visible_width_with_policy(&s, AmbiguousWidth::Narrow) <= 10,
                    "single line should fit"
                );
            }
        }
    }

    #[test]
    fn format_cell_lines_bbcode_fits_preserves_tags() {
        let col = Column::new(Width::Fixed(10)).overflow(Overflow::Wrap { indent: 0 });
        let result = format_cell_lines("[bold]hi[/bold]", 10, &col);
        match result {
            CellOutput::Single(s) => {
                assert!(
                    s.contains("[bold]hi[/bold]"),
                    "tags should be preserved when content fits"
                );
                assert_eq!(visible_width_with_policy(&s, AmbiguousWidth::Narrow), 10);
            }
            _ => panic!("expected Single output"),
        }
    }

    #[test]
    fn cell_output_line_bbcode_padding() {
        let output = CellOutput::Single("[green]ok[/green]".to_string());
        let line = output.line(0, 8, Align::Left);
        assert_eq!(
            visible_width_with_policy(&line, AmbiguousWidth::Narrow),
            8,
            "CellOutput::line should pad to correct visible width"
        );
    }
}
