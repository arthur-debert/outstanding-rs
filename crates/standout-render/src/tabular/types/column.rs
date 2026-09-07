use super::{Align, Anchor, Overflow, SubColumns, TruncateAt, Width};
use crate::template::presentation::escape_text;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub name: Option<String>,
    pub width: Width,
    pub align: Align,
    pub anchor: Anchor,
    pub overflow: Overflow,
    pub null_repr: String,
    pub style: Option<String>,
    pub style_from_value: bool,
    pub key: Option<String>,
    pub header: Option<String>,
    pub sub_columns: Option<SubColumns>,
}

impl Default for Column {
    fn default() -> Self {
        Column {
            name: None,
            width: Width::default(),
            align: Align::default(),
            anchor: Anchor::default(),
            overflow: Overflow::default(),
            null_repr: "-".to_string(),
            style: None,
            style_from_value: false,
            key: None,
            header: None,
            sub_columns: None,
        }
    }
}

impl Column {
    pub fn new(width: Width) -> Self {
        Column {
            width,
            ..Default::default()
        }
    }

    pub fn builder() -> ColumnBuilder {
        ColumnBuilder::default()
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

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn anchor_right(self) -> Self {
        self.anchor(Anchor::Right)
    }

    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn wrap(self) -> Self {
        self.overflow(Overflow::wrap())
    }

    pub fn wrap_indent(self, indent: usize) -> Self {
        self.overflow(Overflow::wrap_with_indent(indent))
    }

    pub fn clip(self) -> Self {
        self.overflow(Overflow::Clip)
    }

    pub fn truncate(mut self, at: TruncateAt) -> Self {
        self.overflow = match self.overflow {
            Overflow::Truncate { marker, .. } => Overflow::Truncate { at, marker },
            _ => Overflow::truncate(at),
        };
        self
    }

    pub fn truncate_middle(self) -> Self {
        self.truncate(TruncateAt::Middle)
    }

    pub fn truncate_start(self) -> Self {
        self.truncate(TruncateAt::Start)
    }

    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.overflow = match self.overflow {
            Overflow::Truncate { at, .. } => Overflow::Truncate {
                at,
                marker: ellipsis.into(),
            },
            _ => Overflow::truncate_with_marker(TruncateAt::End, ellipsis),
        };
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

    pub fn style_from_value(mut self) -> Self {
        self.style_from_value = true;
        self
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.header = Some(header.into());
        self
    }

    pub fn sub_columns(mut self, sub_cols: SubColumns) -> Self {
        self.sub_columns = Some(sub_cols);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct ColumnBuilder {
    name: Option<String>,
    width: Option<Width>,
    align: Option<Align>,
    anchor: Option<Anchor>,
    overflow: Option<Overflow>,
    null_repr: Option<String>,
    style: Option<String>,
    style_from_value: bool,
    key: Option<String>,
    header: Option<String>,
    sub_columns: Option<SubColumns>,
}

impl ColumnBuilder {
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn width(mut self, width: Width) -> Self {
        self.width = Some(width);
        self
    }

    pub fn fixed(mut self, width: usize) -> Self {
        self.width = Some(Width::Fixed(width));
        self
    }

    pub fn fill(mut self) -> Self {
        self.width = Some(Width::Fill);
        self
    }

    pub fn bounded(mut self, min: usize, max: usize) -> Self {
        self.width = Some(Width::bounded(min, max));
        self
    }

    pub fn fraction(mut self, n: usize) -> Self {
        self.width = Some(Width::Fraction(n));
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }

    pub fn right(self) -> Self {
        self.align(Align::Right)
    }

    pub fn center(self) -> Self {
        self.align(Align::Center)
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    pub fn anchor_right(self) -> Self {
        self.anchor(Anchor::Right)
    }

    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = Some(overflow);
        self
    }

    pub fn wrap(self) -> Self {
        self.overflow(Overflow::wrap())
    }

    pub fn clip(self) -> Self {
        self.overflow(Overflow::Clip)
    }

    pub fn truncate(mut self, at: TruncateAt) -> Self {
        self.overflow = Some(match self.overflow {
            Some(Overflow::Truncate { marker, .. }) => Overflow::Truncate { at, marker },
            _ => Overflow::truncate(at),
        });
        self
    }

    pub fn ellipsis(mut self, ellipsis: impl Into<String>) -> Self {
        self.overflow = Some(match self.overflow {
            Some(Overflow::Truncate { at, .. }) => Overflow::Truncate {
                at,
                marker: ellipsis.into(),
            },
            _ => Overflow::truncate_with_marker(TruncateAt::End, ellipsis),
        });
        self
    }

    pub fn null_repr(mut self, null_repr: impl Into<String>) -> Self {
        self.null_repr = Some(null_repr.into());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn style_from_value(mut self) -> Self {
        self.style_from_value = true;
        self
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.header = Some(header.into());
        self
    }

    pub fn sub_columns(mut self, sub_cols: SubColumns) -> Self {
        self.sub_columns = Some(sub_cols);
        self
    }

    pub fn build(self) -> Column {
        let default = Column::default();
        Column {
            name: self.name,
            width: self.width.unwrap_or(default.width),
            align: self.align.unwrap_or(default.align),
            anchor: self.anchor.unwrap_or(default.anchor),
            overflow: self.overflow.unwrap_or(default.overflow),
            null_repr: self.null_repr.unwrap_or(default.null_repr),
            style: self.style,
            style_from_value: self.style_from_value,
            key: self.key,
            header: self.header,
            sub_columns: self.sub_columns,
        }
    }
}

pub struct Col;

impl Col {
    pub fn fixed(width: usize) -> Column {
        Column::new(Width::Fixed(width))
    }

    pub fn min(min: usize) -> Column {
        Column::new(Width::min(min))
    }

    pub fn max(max: usize) -> Column {
        Column::new(Width::max(max))
    }

    pub fn bounded(min: usize, max: usize) -> Column {
        Column::new(Width::bounded(min, max))
    }

    pub fn fill() -> Column {
        Column::new(Width::Fill)
    }

    pub fn fraction(n: usize) -> Column {
        Column::new(Width::Fraction(n))
    }
}

impl Column {
    pub(crate) fn prepared_text(&self) -> Self {
        let mut column = self.clone();
        column.null_repr = escape_text(&column.null_repr);
        prepare_overflow_text(&mut column.overflow);
        if let Some(sub) = &mut column.sub_columns {
            sub.separator = escape_text(&sub.separator);
            for column in &mut sub.columns {
                column.null_repr = escape_text(&column.null_repr);
                prepare_overflow_text(&mut column.overflow);
            }
        }
        column
    }
}

fn prepare_overflow_text(overflow: &mut Overflow) {
    if let Overflow::Truncate { marker, .. } = overflow {
        *marker = escape_text(marker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{SubCol, SubColumns};

    #[test]
    fn col_shorthand_constructors() {
        let fixed = Col::fixed(10);
        assert_eq!(fixed.width, Width::Fixed(10));

        let min = Col::min(5);
        assert_eq!(
            min.width,
            Width::Bounded {
                min: Some(5),
                max: None
            }
        );

        let bounded = Col::bounded(5, 20);
        assert_eq!(
            bounded.width,
            Width::Bounded {
                min: Some(5),
                max: Some(20)
            }
        );

        let fill = Col::fill();
        assert_eq!(fill.width, Width::Fill);

        let fraction = Col::fraction(3);
        assert_eq!(fraction.width, Width::Fraction(3));
    }

    #[test]
    fn col_shorthand_chaining() {
        let col = Col::fixed(10).right().anchor_right().style("header");
        assert_eq!(col.width, Width::Fixed(10));
        assert_eq!(col.align, Align::Right);
        assert_eq!(col.anchor, Anchor::Right);
        assert_eq!(col.style, Some("header".to_string()));
    }

    #[test]
    fn column_wrap_shorthand() {
        let col = Col::fill().wrap();
        assert!(matches!(col.overflow, Overflow::Wrap { indent: 0 }));

        let col_indent = Col::fill().wrap_indent(2);
        assert!(matches!(col_indent.overflow, Overflow::Wrap { indent: 2 }));
    }

    #[test]
    fn column_clip_shorthand() {
        let col = Col::fixed(10).clip();
        assert!(matches!(col.overflow, Overflow::Clip));
    }

    #[test]
    fn column_named() {
        let col = Col::fixed(10).named("author");
        assert_eq!(col.name, Some("author".to_string()));
    }

    #[test]
    fn column_defaults() {
        let col = Column::default();
        assert!(matches!(
            col.width,
            Width::Bounded {
                min: None,
                max: None
            }
        ));
        assert_eq!(col.align, Align::Left);
        assert_eq!(col.anchor, Anchor::Left);
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::End,
                ..
            }
        ));
        assert_eq!(col.null_repr, "-");
        assert!(col.style.is_none());
    }

    #[test]
    fn column_fluent_api() {
        let col = Column::new(Width::Fixed(10))
            .align(Align::Right)
            .truncate(TruncateAt::Middle)
            .ellipsis("...")
            .null_repr("N/A")
            .style("header");

        assert_eq!(col.width, Width::Fixed(10));
        assert_eq!(col.align, Align::Right);
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::Middle,
                ref marker
            } if marker == "..."
        ));
        assert_eq!(col.null_repr, "N/A");
        assert_eq!(col.style, Some("header".to_string()));
    }

    #[test]
    fn column_builder() {
        let col = Column::builder()
            .fixed(15)
            .align(Align::Center)
            .truncate(TruncateAt::Start)
            .build();

        assert_eq!(col.width, Width::Fixed(15));
        assert_eq!(col.align, Align::Center);
        assert!(matches!(
            col.overflow,
            Overflow::Truncate {
                at: TruncateAt::Start,
                ..
            }
        ));
    }

    #[test]
    fn column_builder_fill() {
        let col = Column::builder().fill().build();
        assert_eq!(col.width, Width::Fill);
    }

    #[test]
    fn column_with_sub_columns() {
        let sub_cols =
            SubColumns::new(vec![SubCol::fill(), SubCol::bounded(0, 30).right()], " ").unwrap();

        let col = Col::fill().sub_columns(sub_cols);
        assert!(col.sub_columns.is_some());
        assert_eq!(col.sub_columns.unwrap().columns.len(), 2);
    }

    #[test]
    fn column_builder_with_sub_columns() {
        let sub_cols = SubColumns::new(vec![SubCol::fill(), SubCol::fixed(8)], " ").unwrap();

        let col = Column::builder().fill().sub_columns(sub_cols).build();

        assert_eq!(col.width, Width::Fill);
        assert!(col.sub_columns.is_some());
    }
}
