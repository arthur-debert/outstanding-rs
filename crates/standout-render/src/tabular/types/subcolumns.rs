use super::{Align, Overflow, Width};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubColumn {
    pub name: Option<String>,
    pub width: Width,
    pub align: Align,
    pub overflow: Overflow,
    pub null_repr: String,
    pub style: Option<String>,
}

impl Default for SubColumn {
    fn default() -> Self {
        SubColumn {
            name: None,
            width: Width::Fill,
            align: Align::Left,
            overflow: Overflow::default(),
            null_repr: String::new(),
            style: None,
        }
    }
}

impl SubColumn {
    pub fn new(width: Width) -> Self {
        SubColumn {
            width,
            ..Default::default()
        }
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn right(self) -> Self {
        self.align(Align::Right)
    }

    pub fn center(self) -> Self {
        self.align(Align::Center)
    }

    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn null_repr(mut self, null_repr: impl Into<String>) -> Self {
        self.null_repr = null_repr.into();
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubColumns {
    pub columns: Vec<SubColumn>,
    pub separator: String,
}

impl SubColumns {
    pub fn new(columns: Vec<SubColumn>, separator: impl Into<String>) -> Result<Self, String> {
        if columns.is_empty() {
            return Err("sub_columns must contain at least one sub-column".into());
        }

        let fill_count = columns
            .iter()
            .filter(|c| matches!(c.width, Width::Fill))
            .count();
        if fill_count != 1 {
            return Err(format!(
                "sub_columns must have exactly one Fill sub-column, found {}",
                fill_count
            ));
        }

        for (i, col) in columns.iter().enumerate() {
            if matches!(col.width, Width::Fraction(_)) {
                return Err(format!(
                    "sub_column[{}]: Fraction width is not supported for sub-columns",
                    i
                ));
            }
        }

        Ok(SubColumns {
            columns,
            separator: separator.into(),
        })
    }
}

pub struct SubCol;

impl SubCol {
    pub fn fill() -> SubColumn {
        SubColumn::new(Width::Fill)
    }

    pub fn fixed(width: usize) -> SubColumn {
        SubColumn::new(Width::Fixed(width))
    }

    pub fn bounded(min: usize, max: usize) -> SubColumn {
        SubColumn::new(Width::bounded(min, max))
    }

    pub fn max(max: usize) -> SubColumn {
        SubColumn::new(Width::max(max))
    }

    pub fn min(min: usize) -> SubColumn {
        SubColumn::new(Width::min(min))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_column_defaults() {
        let sc = SubColumn::default();
        assert_eq!(sc.width, Width::Fill);
        assert_eq!(sc.align, Align::Left);
        assert!(sc.name.is_none());
        assert!(sc.style.is_none());
        assert_eq!(sc.null_repr, "");
    }

    #[test]
    fn sub_column_fluent_api() {
        let sc = SubColumn::new(Width::Fixed(10))
            .named("tag")
            .right()
            .style("tag_style")
            .null_repr("N/A");

        assert_eq!(sc.width, Width::Fixed(10));
        assert_eq!(sc.name, Some("tag".to_string()));
        assert_eq!(sc.align, Align::Right);
        assert_eq!(sc.style, Some("tag_style".to_string()));
        assert_eq!(sc.null_repr, "N/A");
    }

    #[test]
    fn sub_col_shorthand_constructors() {
        let fill = SubCol::fill();
        assert_eq!(fill.width, Width::Fill);

        let fixed = SubCol::fixed(10);
        assert_eq!(fixed.width, Width::Fixed(10));

        let bounded = SubCol::bounded(0, 30);
        assert_eq!(
            bounded.width,
            Width::Bounded {
                min: Some(0),
                max: Some(30)
            }
        );

        let max = SubCol::max(20);
        assert_eq!(
            max.width,
            Width::Bounded {
                min: None,
                max: Some(20)
            }
        );

        let min = SubCol::min(5);
        assert_eq!(
            min.width,
            Width::Bounded {
                min: Some(5),
                max: None
            }
        );
    }

    #[test]
    fn sub_col_shorthand_chaining() {
        let sc = SubCol::bounded(0, 30).right().style("tag");
        assert_eq!(sc.align, Align::Right);
        assert_eq!(sc.style, Some("tag".to_string()));
    }

    #[test]
    fn sub_columns_valid_construction() {
        let result = SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 30)], " ");
        assert!(result.is_ok());
        let sc = result.unwrap();
        assert_eq!(sc.columns.len(), 2);
        assert_eq!(sc.separator, " ");
    }

    #[test]
    fn sub_columns_rejects_empty() {
        let result = SubColumns::new(vec![], " ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one"));
    }

    #[test]
    fn sub_columns_rejects_no_fill() {
        let result = SubColumns::new(vec![SubCol::fixed(10), SubCol::bounded(0, 30)], " ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly one Fill"));
    }

    #[test]
    fn sub_columns_rejects_two_fills() {
        let result = SubColumns::new(vec![SubCol::fill(), SubCol::fill()], " ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly one Fill"));
    }

    #[test]
    fn sub_columns_rejects_fraction() {
        let result = SubColumns::new(
            vec![SubCol::fill(), SubColumn::new(Width::Fraction(2))],
            " ",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Fraction"));
    }

    #[test]
    fn sub_columns_serde_roundtrip() {
        let sc = SubColumns::new(
            vec![
                SubCol::fill().named("title"),
                SubCol::bounded(0, 30).right().named("tag"),
            ],
            "  ",
        )
        .unwrap();

        let json = serde_json::to_string(&sc).unwrap();
        let parsed: SubColumns = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.columns.len(), 2);
        assert_eq!(parsed.separator, "  ");
        assert_eq!(parsed.columns[0].width, Width::Fill);
        assert_eq!(
            parsed.columns[1].width,
            Width::Bounded {
                min: Some(0),
                max: Some(30)
            }
        );
    }
}
