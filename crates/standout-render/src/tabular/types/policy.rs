use serde::{Deserialize, Serialize};

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
}
