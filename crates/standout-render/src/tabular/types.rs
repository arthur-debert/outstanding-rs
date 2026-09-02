//! Table layout configuration: column widths, alignment, truncation, decorations.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TruncateAt {
    #[default]
    End,
    Start,
    Middle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Truncate { at: TruncateAt, marker: String },
    Wrap { indent: usize },
    Clip,
    Expand,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Truncate {
            at: TruncateAt::End,
            marker: "…".to_string(),
        }
    }
}

impl Overflow {
    pub fn truncate(at: TruncateAt) -> Self {
        Overflow::Truncate {
            at,
            marker: "…".to_string(),
        }
    }

    pub fn truncate_with_marker(at: TruncateAt, marker: impl Into<String>) -> Self {
        Overflow::Truncate {
            at,
            marker: marker.into(),
        }
    }

    pub fn wrap() -> Self {
        Overflow::Wrap { indent: 0 }
    }

    pub fn wrap_with_indent(indent: usize) -> Self {
        Overflow::Wrap { indent }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    #[default]
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WidthRaw", into = "WidthRaw")]
pub enum Width {
    Fixed(usize),
    Bounded {
        min: Option<usize>,
        max: Option<usize>,
    },
    Fill,
    Fraction(usize),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum WidthRaw {
    Fixed(usize),
    Bounded {
        #[serde(default)]
        min: Option<usize>,
        #[serde(default)]
        max: Option<usize>,
    },
    StringVariant(String),
}

impl From<Width> for WidthRaw {
    fn from(width: Width) -> Self {
        match width {
            Width::Fixed(w) => WidthRaw::Fixed(w),
            Width::Bounded { min, max } => WidthRaw::Bounded { min, max },
            Width::Fill => WidthRaw::StringVariant("fill".to_string()),
            Width::Fraction(n) => WidthRaw::StringVariant(format!("{}fr", n)),
        }
    }
}

impl TryFrom<WidthRaw> for Width {
    type Error = String;

    fn try_from(raw: WidthRaw) -> Result<Self, Self::Error> {
        match raw {
            WidthRaw::Fixed(w) => Ok(Width::Fixed(w)),
            WidthRaw::Bounded { min, max } => Ok(Width::Bounded { min, max }),
            WidthRaw::StringVariant(s) if s == "fill" => Ok(Width::Fill),
            WidthRaw::StringVariant(s) if s.ends_with("fr") => {
                let num_str = s.trim_end_matches("fr");
                num_str
                    .parse::<usize>()
                    .map(Width::Fraction)
                    .map_err(|_| format!("Invalid fraction: '{}'. Expected format like '2fr'.", s))
            }
            WidthRaw::StringVariant(s) => Err(format!(
                "Invalid width string: '{}'. Expected 'fill' or '<n>fr'.",
                s
            )),
        }
    }
}

impl Default for Width {
    fn default() -> Self {
        Width::Bounded {
            min: None,
            max: None,
        }
    }
}

impl Width {
    pub fn fixed(width: usize) -> Self {
        Width::Fixed(width)
    }

    pub fn bounded(min: usize, max: usize) -> Self {
        Width::Bounded {
            min: Some(min),
            max: Some(max),
        }
    }

    pub fn min(min: usize) -> Self {
        Width::Bounded {
            min: Some(min),
            max: None,
        }
    }

    pub fn max(max: usize) -> Self {
        Width::Bounded {
            min: None,
            max: Some(max),
        }
    }

    pub fn fill() -> Self {
        Width::Fill
    }

    pub fn fraction(n: usize) -> Self {
        Width::Fraction(n)
    }
}

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Decorations {
    pub column_sep: String,
    pub row_prefix: String,
    pub row_suffix: String,
}

impl Decorations {
    pub fn with_separator(sep: impl Into<String>) -> Self {
        Decorations {
            column_sep: sep.into(),
            row_prefix: String::new(),
            row_suffix: String::new(),
        }
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.column_sep = sep.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.row_prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.row_suffix = suffix.into();
        self
    }

    pub fn overhead(&self, num_columns: usize) -> usize {
        self.overhead_with_policy(num_columns, crate::AmbiguousWidth::Narrow)
    }

    pub fn overhead_with_policy(&self, num_columns: usize, policy: crate::AmbiguousWidth) -> usize {
        use crate::tabular::visible_width_with_policy;
        let prefix_width = visible_width_with_policy(&self.row_prefix, policy);
        let suffix_width = visible_width_with_policy(&self.row_suffix, policy);
        let sep_width = visible_width_with_policy(&self.column_sep, policy);
        let sep_count = num_columns.saturating_sub(1);
        prefix_width + suffix_width + (sep_width * sep_count)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatDataSpec {
    pub columns: Vec<Column>,
    pub decorations: Decorations,
}

impl FlatDataSpec {
    pub fn new(columns: Vec<Column>) -> Self {
        FlatDataSpec {
            columns,
            decorations: Decorations::default(),
        }
    }

    pub fn builder() -> FlatDataSpecBuilder {
        FlatDataSpecBuilder::default()
    }

    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    pub fn has_fill_column(&self) -> bool {
        self.columns.iter().any(|c| matches!(c.width, Width::Fill))
    }

    pub fn extract_header(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| {
                col.header
                    .as_deref()
                    .or(col.key.as_deref())
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

    pub fn extract_row(&self, data: &Value) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| {
                if let Some(key) = &col.key {
                    extract_value(data, key).unwrap_or(col.null_repr.clone())
                } else {
                    col.null_repr.clone()
                }
            })
            .collect()
    }
}

fn extract_value(data: &Value, path: &str) -> Option<String> {
    let mut current = data;
    for part in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(part)?;
            }
            _ => return None,
        }
    }

    match current {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        v => Some(v.to_string()),
    }
}

#[derive(Clone, Debug, Default)]
pub struct FlatDataSpecBuilder {
    columns: Vec<Column>,
    decorations: Decorations,
}

impl FlatDataSpecBuilder {
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.decorations.column_sep = sep.into();
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.decorations.row_prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.decorations.row_suffix = suffix.into();
        self
    }

    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn build(self) -> FlatDataSpec {
        FlatDataSpec {
            columns: self.columns,
            decorations: self.decorations,
        }
    }
}

pub type TabularSpec = FlatDataSpec;
pub type TabularSpecBuilder = FlatDataSpecBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_default_is_left() {
        assert_eq!(Align::default(), Align::Left);
    }

    #[test]
    fn align_serde_roundtrip() {
        let values = [Align::Left, Align::Right, Align::Center];
        for align in values {
            let json = serde_json::to_string(&align).unwrap();
            let parsed: Align = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, align);
        }
    }

    #[test]
    fn truncate_at_default_is_end() {
        assert_eq!(TruncateAt::default(), TruncateAt::End);
    }

    #[test]
    fn truncate_at_serde_roundtrip() {
        let values = [TruncateAt::End, TruncateAt::Start, TruncateAt::Middle];
        for truncate in values {
            let json = serde_json::to_string(&truncate).unwrap();
            let parsed: TruncateAt = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, truncate);
        }
    }

    #[test]
    fn width_constructors() {
        assert_eq!(Width::fixed(10), Width::Fixed(10));
        assert_eq!(
            Width::bounded(5, 20),
            Width::Bounded {
                min: Some(5),
                max: Some(20)
            }
        );
        assert_eq!(
            Width::min(5),
            Width::Bounded {
                min: Some(5),
                max: None
            }
        );
        assert_eq!(
            Width::max(20),
            Width::Bounded {
                min: None,
                max: Some(20)
            }
        );
        assert_eq!(Width::fill(), Width::Fill);
    }

    #[test]
    fn width_serde_fixed() {
        let width = Width::Fixed(10);
        let json = serde_json::to_string(&width).unwrap();
        assert_eq!(json, "10");
        let parsed: Width = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, width);
    }

    #[test]
    fn width_serde_bounded() {
        let width = Width::Bounded {
            min: Some(5),
            max: Some(20),
        };
        let json = serde_json::to_string(&width).unwrap();
        let parsed: Width = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, width);
    }

    #[test]
    fn width_serde_fill() {
        let width = Width::Fill;
        let json = serde_json::to_string(&width).unwrap();
        assert_eq!(json, "\"fill\"");

        let parsed: Width = serde_json::from_str("\"fill\"").unwrap();
        assert_eq!(parsed, width);
    }

    #[test]
    fn width_serde_fraction() {
        let width = Width::Fraction(2);
        let json = serde_json::to_string(&width).unwrap();
        assert_eq!(json, "\"2fr\"");

        let parsed: Width = serde_json::from_str("\"2fr\"").unwrap();
        assert_eq!(parsed, width);

        let parsed_1: Width = serde_json::from_str("\"1fr\"").unwrap();
        assert_eq!(parsed_1, Width::Fraction(1));
    }

    #[test]
    fn width_fraction_constructor() {
        assert_eq!(Width::fraction(3), Width::Fraction(3));
    }

    #[test]
    fn overflow_default() {
        let overflow = Overflow::default();
        assert!(matches!(
            overflow,
            Overflow::Truncate {
                at: TruncateAt::End,
                ..
            }
        ));
    }

    #[test]
    fn overflow_constructors() {
        let truncate = Overflow::truncate(TruncateAt::Middle);
        assert!(matches!(
            truncate,
            Overflow::Truncate {
                at: TruncateAt::Middle,
                ref marker
            } if marker == "…"
        ));

        let truncate_custom = Overflow::truncate_with_marker(TruncateAt::Start, "...");
        assert!(matches!(
            truncate_custom,
            Overflow::Truncate {
                at: TruncateAt::Start,
                ref marker
            } if marker == "..."
        ));

        let wrap = Overflow::wrap();
        assert!(matches!(wrap, Overflow::Wrap { indent: 0 }));

        let wrap_indent = Overflow::wrap_with_indent(4);
        assert!(matches!(wrap_indent, Overflow::Wrap { indent: 4 }));
    }

    #[test]
    fn anchor_default() {
        assert_eq!(Anchor::default(), Anchor::Left);
    }

    #[test]
    fn anchor_serde_roundtrip() {
        let values = [Anchor::Left, Anchor::Right];
        for anchor in values {
            let json = serde_json::to_string(&anchor).unwrap();
            let parsed: Anchor = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, anchor);
        }
    }

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
    fn decorations_default() {
        let dec = Decorations::default();
        assert_eq!(dec.column_sep, "");
        assert_eq!(dec.row_prefix, "");
        assert_eq!(dec.row_suffix, "");
    }

    #[test]
    fn decorations_with_separator() {
        let dec = Decorations::with_separator("  ");
        assert_eq!(dec.column_sep, "  ");
    }

    #[test]
    fn decorations_overhead() {
        let dec = Decorations::default()
            .separator("  ")
            .prefix("│ ")
            .suffix(" │");

        assert_eq!(dec.overhead(3), 8);
        assert_eq!(dec.overhead(1), 4);
        assert_eq!(dec.overhead(0), 4);
    }

    #[test]
    fn flat_data_spec_builder() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(10)))
            .separator("  ")
            .build();

        assert_eq!(spec.num_columns(), 3);
        assert!(spec.has_fill_column());
        assert_eq!(spec.decorations.column_sep, "  ");
    }

    #[test]
    fn table_spec_no_fill() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fixed(10)))
            .build();

        assert!(!spec.has_fill_column());
    }

    #[test]
    fn extract_fields_from_json() {
        let json = serde_json::json!({
            "name": "Alice",
            "meta": {
                "age": 30,
                "role": "admin"
            }
        });

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("name"))
            .column(Column::new(Width::Fixed(5)).key("meta.age"))
            .column(Column::new(Width::Fixed(10)).key("meta.role"))
            .column(Column::new(Width::Fixed(10)).key("missing.field"))
            .build();

        let row = spec.extract_row(&json);
        assert_eq!(row[0], "Alice");
        assert_eq!(row[1], "30");
        assert_eq!(row[2], "admin");
        assert_eq!(row[3], "-");
    }

    #[test]
    fn extract_header_row() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).header("Name").key("name"))
            .column(Column::new(Width::Fixed(5)).key("age"))
            .column(Column::new(Width::Fixed(10)))
            .build();

        let header = spec.extract_header();
        assert_eq!(header[0], "Name");
        assert_eq!(header[1], "age");
        assert_eq!(header[2], "");
    }

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
