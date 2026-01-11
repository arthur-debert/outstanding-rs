//! Color value parsing for stylesheets.
//!
//! Supports multiple color formats:
//!
//! - **Named colors**: `red`, `green`, `blue`, etc. (16 ANSI colors)
//! - **Bright variants**: `bright_red`, `bright_green`, etc.
//! - **256-color palette**: `0` through `255`
//! - **RGB hex**: `"#ff6b35"` or `"#fff"` (3 or 6 digit)
//! - **RGB tuple**: `[255, 107, 53]`
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::stylesheet::ColorDef;
//!
//! // Parse from YAML values
//! let red = ColorDef::parse_value(&serde_yaml::Value::String("red".into()))?;
//! let hex = ColorDef::parse_value(&serde_yaml::Value::String("#ff6b35".into()))?;
//! let palette = ColorDef::parse_value(&serde_yaml::Value::Number(208.into()))?;
//! let rgb = ColorDef::parse_value(&serde_yaml::Value::Sequence(vec![
//!     serde_yaml::Value::Number(255.into()),
//!     serde_yaml::Value::Number(107.into()),
//!     serde_yaml::Value::Number(53.into()),
//! ]))?;
//! ```

use console::Color;

/// Parsed color definition from stylesheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorDef {
    /// Named ANSI color.
    Named(Color),
    /// 256-color palette index.
    Color256(u8),
    /// True color RGB.
    Rgb(u8, u8, u8),
}

impl ColorDef {
    /// Parses a color definition from a YAML value.
    ///
    /// Supports:
    /// - Strings: named colors, bright variants, hex codes
    /// - Numbers: 256-color palette indices
    /// - Sequences: RGB tuples `[r, g, b]`
    pub fn parse_value(value: &serde_yaml::Value) -> Result<Self, String> {
        match value {
            serde_yaml::Value::String(s) => Self::parse_string(s),
            serde_yaml::Value::Number(n) => {
                let index = n
                    .as_u64()
                    .ok_or_else(|| format!("Invalid color palette index: {}", n))?;
                if index > 255 {
                    return Err(format!(
                        "Color palette index {} out of range (0-255)",
                        index
                    ));
                }
                Ok(ColorDef::Color256(index as u8))
            }
            serde_yaml::Value::Sequence(seq) => Self::parse_rgb_tuple(seq),
            _ => Err(format!("Invalid color value: {:?}", value)),
        }
    }

    /// Parses a color from a string value.
    ///
    /// Supports:
    /// - Named colors: `red`, `green`, `blue`, etc.
    /// - Bright variants: `bright_red`, `bright_green`, etc.
    /// - Hex codes: `#ff6b35` or `#fff`
    pub fn parse_string(s: &str) -> Result<Self, String> {
        let s = s.trim();

        // Check for hex color
        if let Some(hex) = s.strip_prefix('#') {
            return Self::parse_hex(hex);
        }

        // Check for named color
        Self::parse_named(s)
    }

    /// Parses a hex color code (without the # prefix).
    fn parse_hex(hex: &str) -> Result<Self, String> {
        match hex.len() {
            // 3-digit hex: #rgb -> #rrggbb
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?
                    * 17;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?
                    * 17;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?
                    * 17;
                Ok(ColorDef::Rgb(r, g, b))
            }
            // 6-digit hex: #rrggbb
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| format!("Invalid hex: {}", hex))?;
                Ok(ColorDef::Rgb(r, g, b))
            }
            _ => Err(format!(
                "Invalid hex color: #{} (must be 3 or 6 digits)",
                hex
            )),
        }
    }

    /// Parses a named color (including bright variants).
    fn parse_named(name: &str) -> Result<Self, String> {
        let name_lower = name.to_lowercase();

        // Check for bright_ prefix
        if let Some(base) = name_lower.strip_prefix("bright_") {
            return Self::parse_bright_color(base);
        }

        // Standard colors
        let color = match name_lower.as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "white" => Color::White,
            // Also accept gray/grey as aliases
            "gray" | "grey" => Color::White,
            _ => return Err(format!("Unknown color name: {}", name)),
        };

        Ok(ColorDef::Named(color))
    }

    /// Parses a bright color variant.
    fn parse_bright_color(base: &str) -> Result<Self, String> {
        // console crate uses Color256 for bright colors (indices 8-15)
        let index = match base {
            "black" => 8,
            "red" => 9,
            "green" => 10,
            "yellow" => 11,
            "blue" => 12,
            "magenta" => 13,
            "cyan" => 14,
            "white" => 15,
            _ => return Err(format!("Unknown bright color: bright_{}", base)),
        };

        Ok(ColorDef::Color256(index))
    }

    /// Parses an RGB tuple from a YAML sequence.
    fn parse_rgb_tuple(seq: &[serde_yaml::Value]) -> Result<Self, String> {
        if seq.len() != 3 {
            return Err(format!(
                "RGB tuple must have exactly 3 values, got {}",
                seq.len()
            ));
        }

        let mut components = [0u8; 3];
        for (i, val) in seq.iter().enumerate() {
            let n = val
                .as_u64()
                .ok_or_else(|| format!("RGB component {} is not a number", i))?;
            if n > 255 {
                return Err(format!("RGB component {} out of range (0-255): {}", i, n));
            }
            components[i] = n as u8;
        }

        Ok(ColorDef::Rgb(components[0], components[1], components[2]))
    }

    /// Converts this color definition to a `console::Color`.
    pub fn to_console_color(&self) -> Color {
        match self {
            ColorDef::Named(c) => *c,
            ColorDef::Color256(n) => Color::Color256(*n),
            ColorDef::Rgb(r, g, b) => Color::Color256(crate::rgb_to_ansi256((*r, *g, *b))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    // =========================================================================
    // Named color tests
    // =========================================================================

    #[test]
    fn test_parse_named_colors() {
        assert_eq!(
            ColorDef::parse_string("red").unwrap(),
            ColorDef::Named(Color::Red)
        );
        assert_eq!(
            ColorDef::parse_string("green").unwrap(),
            ColorDef::Named(Color::Green)
        );
        assert_eq!(
            ColorDef::parse_string("blue").unwrap(),
            ColorDef::Named(Color::Blue)
        );
        assert_eq!(
            ColorDef::parse_string("yellow").unwrap(),
            ColorDef::Named(Color::Yellow)
        );
        assert_eq!(
            ColorDef::parse_string("magenta").unwrap(),
            ColorDef::Named(Color::Magenta)
        );
        assert_eq!(
            ColorDef::parse_string("cyan").unwrap(),
            ColorDef::Named(Color::Cyan)
        );
        assert_eq!(
            ColorDef::parse_string("white").unwrap(),
            ColorDef::Named(Color::White)
        );
        assert_eq!(
            ColorDef::parse_string("black").unwrap(),
            ColorDef::Named(Color::Black)
        );
    }

    #[test]
    fn test_parse_named_colors_case_insensitive() {
        assert_eq!(
            ColorDef::parse_string("RED").unwrap(),
            ColorDef::Named(Color::Red)
        );
        assert_eq!(
            ColorDef::parse_string("Red").unwrap(),
            ColorDef::Named(Color::Red)
        );
    }

    #[test]
    fn test_parse_gray_aliases() {
        assert_eq!(
            ColorDef::parse_string("gray").unwrap(),
            ColorDef::Named(Color::White)
        );
        assert_eq!(
            ColorDef::parse_string("grey").unwrap(),
            ColorDef::Named(Color::White)
        );
    }

    #[test]
    fn test_parse_unknown_color() {
        assert!(ColorDef::parse_string("purple").is_err());
        assert!(ColorDef::parse_string("orange").is_err());
    }

    // =========================================================================
    // Bright color tests
    // =========================================================================

    #[test]
    fn test_parse_bright_colors() {
        assert_eq!(
            ColorDef::parse_string("bright_red").unwrap(),
            ColorDef::Color256(9)
        );
        assert_eq!(
            ColorDef::parse_string("bright_green").unwrap(),
            ColorDef::Color256(10)
        );
        assert_eq!(
            ColorDef::parse_string("bright_blue").unwrap(),
            ColorDef::Color256(12)
        );
        assert_eq!(
            ColorDef::parse_string("bright_black").unwrap(),
            ColorDef::Color256(8)
        );
        assert_eq!(
            ColorDef::parse_string("bright_white").unwrap(),
            ColorDef::Color256(15)
        );
    }

    #[test]
    fn test_parse_unknown_bright_color() {
        assert!(ColorDef::parse_string("bright_purple").is_err());
    }

    // =========================================================================
    // Hex color tests
    // =========================================================================

    #[test]
    fn test_parse_hex_6_digit() {
        assert_eq!(
            ColorDef::parse_string("#ff6b35").unwrap(),
            ColorDef::Rgb(255, 107, 53)
        );
        assert_eq!(
            ColorDef::parse_string("#000000").unwrap(),
            ColorDef::Rgb(0, 0, 0)
        );
        assert_eq!(
            ColorDef::parse_string("#ffffff").unwrap(),
            ColorDef::Rgb(255, 255, 255)
        );
    }

    #[test]
    fn test_parse_hex_3_digit() {
        assert_eq!(
            ColorDef::parse_string("#fff").unwrap(),
            ColorDef::Rgb(255, 255, 255)
        );
        assert_eq!(
            ColorDef::parse_string("#000").unwrap(),
            ColorDef::Rgb(0, 0, 0)
        );
        assert_eq!(
            ColorDef::parse_string("#f80").unwrap(),
            ColorDef::Rgb(255, 136, 0)
        );
    }

    #[test]
    fn test_parse_hex_case_insensitive() {
        assert_eq!(
            ColorDef::parse_string("#FF6B35").unwrap(),
            ColorDef::Rgb(255, 107, 53)
        );
        assert_eq!(
            ColorDef::parse_string("#FFF").unwrap(),
            ColorDef::Rgb(255, 255, 255)
        );
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert!(ColorDef::parse_string("#ff").is_err());
        assert!(ColorDef::parse_string("#ffff").is_err());
        assert!(ColorDef::parse_string("#gggggg").is_err());
    }

    // =========================================================================
    // YAML value tests
    // =========================================================================

    #[test]
    fn test_parse_value_string() {
        let val = Value::String("red".into());
        assert_eq!(
            ColorDef::parse_value(&val).unwrap(),
            ColorDef::Named(Color::Red)
        );
    }

    #[test]
    fn test_parse_value_number() {
        let val = Value::Number(208.into());
        assert_eq!(
            ColorDef::parse_value(&val).unwrap(),
            ColorDef::Color256(208)
        );
    }

    #[test]
    fn test_parse_value_number_out_of_range() {
        let val = Value::Number(256.into());
        assert!(ColorDef::parse_value(&val).is_err());
    }

    #[test]
    fn test_parse_value_sequence() {
        let val = Value::Sequence(vec![
            Value::Number(255.into()),
            Value::Number(107.into()),
            Value::Number(53.into()),
        ]);
        assert_eq!(
            ColorDef::parse_value(&val).unwrap(),
            ColorDef::Rgb(255, 107, 53)
        );
    }

    #[test]
    fn test_parse_value_sequence_wrong_length() {
        let val = Value::Sequence(vec![Value::Number(255.into()), Value::Number(107.into())]);
        assert!(ColorDef::parse_value(&val).is_err());
    }

    #[test]
    fn test_parse_value_sequence_out_of_range() {
        let val = Value::Sequence(vec![
            Value::Number(256.into()),
            Value::Number(107.into()),
            Value::Number(53.into()),
        ]);
        assert!(ColorDef::parse_value(&val).is_err());
    }

    // =========================================================================
    // to_console_color tests
    // =========================================================================

    #[test]
    fn test_to_console_color_named() {
        let c = ColorDef::Named(Color::Red);
        assert_eq!(c.to_console_color(), Color::Red);
    }

    #[test]
    fn test_to_console_color_256() {
        let c = ColorDef::Color256(208);
        assert_eq!(c.to_console_color(), Color::Color256(208));
    }

    #[test]
    fn test_to_console_color_rgb() {
        let c = ColorDef::Rgb(255, 107, 53);
        // RGB gets converted to 256 color via rgb_to_ansi256
        if let Color::Color256(_) = c.to_console_color() {
            // OK - it converted
        } else {
            panic!("Expected Color256");
        }
    }
}
