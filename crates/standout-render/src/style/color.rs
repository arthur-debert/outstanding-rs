//! Color value parsing for stylesheets: named colors (`red`, `bright_red`,
//! ...), 256-color palette indices, RGB hex (`#ff6b35`/`#fff`), RGB tuples
//! (`[255, 107, 53]`), and theme-relative cube coordinates (`cube(60%, 20%,
//! 0%)`, resolved via [`ThemePalette`]).

use console::Color;

use crate::colorspace::{CubeCoord, ThemePalette};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorDef {
    Named(Color),
    Color256(u8),
    Rgb(u8, u8, u8),
    Cube(CubeCoord),
}

impl ColorDef {
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

    pub fn parse_string(s: &str) -> Result<Self, String> {
        let s = s.trim();

        if s.starts_with("cube(") && s.ends_with(')') {
            return Self::parse_cube(s);
        }

        if let Some(hex) = s.strip_prefix('#') {
            return Self::parse_hex(hex);
        }

        Self::parse_named(s)
    }

    fn parse_cube(s: &str) -> Result<Self, String> {
        let inner = &s[5..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() != 3 {
            return Err(format!(
                "cube() requires exactly 3 components, got {}",
                parts.len()
            ));
        }

        let mut values = [0.0f64; 3];
        for (i, part) in parts.iter().enumerate() {
            let num_str = part.strip_suffix('%').unwrap_or(part).trim();
            values[i] = num_str
                .parse::<f64>()
                .map_err(|_| format!("Invalid cube component '{}': expected a number", part))?;
        }

        let coord = CubeCoord::from_percentages(values[0], values[1], values[2])?;
        Ok(ColorDef::Cube(coord))
    }

    fn parse_hex(hex: &str) -> Result<Self, String> {
        match hex.len() {
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

    fn parse_named(name: &str) -> Result<Self, String> {
        let name_lower = name.to_lowercase();

        if let Some(base) = name_lower.strip_prefix("bright_") {
            return Self::parse_bright_color(base);
        }

        let color = match name_lower.as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "white" => Color::White,
            "gray" | "grey" => Color::White,
            _ => return Err(format!("Unknown color name: {}", name)),
        };

        Ok(ColorDef::Named(color))
    }

    fn parse_bright_color(base: &str) -> Result<Self, String> {
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

    pub fn to_console_color(&self, palette: Option<&ThemePalette>) -> Color {
        match self {
            ColorDef::Named(c) => *c,
            ColorDef::Color256(n) => Color::Color256(*n),
            ColorDef::Rgb(r, g, b) => Color::Color256(crate::rgb_to_ansi256((*r, *g, *b))),
            ColorDef::Cube(coord) => {
                let p;
                let palette = match palette {
                    Some(pal) => pal,
                    None => {
                        p = ThemePalette::default_xterm();
                        &p
                    }
                };
                let rgb = palette.resolve(coord);
                Color::Color256(crate::rgb_to_ansi256((rgb.0, rgb.1, rgb.2)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

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

    #[test]
    fn test_to_console_color_named() {
        let c = ColorDef::Named(Color::Red);
        assert_eq!(c.to_console_color(None), Color::Red);
    }

    #[test]
    fn test_to_console_color_256() {
        let c = ColorDef::Color256(208);
        assert_eq!(c.to_console_color(None), Color::Color256(208));
    }

    #[test]
    fn test_to_console_color_rgb() {
        let c = ColorDef::Rgb(255, 107, 53);
        if let Color::Color256(_) = c.to_console_color(None) {
        } else {
            panic!("Expected Color256");
        }
    }

    #[test]
    fn test_parse_cube_percentages() {
        let c = ColorDef::parse_string("cube(60%, 20%, 0%)").unwrap();
        match c {
            ColorDef::Cube(coord) => {
                assert!((coord.r - 0.6).abs() < 0.001);
                assert!((coord.g - 0.2).abs() < 0.001);
                assert!((coord.b - 0.0).abs() < 0.001);
            }
            _ => panic!("Expected Cube"),
        }
    }

    #[test]
    fn test_parse_cube_without_percent_sign() {
        let c = ColorDef::parse_string("cube(100, 50, 0)").unwrap();
        match c {
            ColorDef::Cube(coord) => {
                assert!((coord.r - 1.0).abs() < 0.001);
                assert!((coord.g - 0.5).abs() < 0.001);
                assert!((coord.b - 0.0).abs() < 0.001);
            }
            _ => panic!("Expected Cube"),
        }
    }

    #[test]
    fn test_parse_cube_corners() {
        let c = ColorDef::parse_string("cube(0%, 0%, 0%)").unwrap();
        assert!(matches!(c, ColorDef::Cube(_)));

        let c = ColorDef::parse_string("cube(100%, 100%, 100%)").unwrap();
        assert!(matches!(c, ColorDef::Cube(_)));
    }

    #[test]
    fn test_parse_cube_out_of_range() {
        assert!(ColorDef::parse_string("cube(101%, 0%, 0%)").is_err());
        assert!(ColorDef::parse_string("cube(-1%, 0%, 0%)").is_err());
    }

    #[test]
    fn test_parse_cube_wrong_arg_count() {
        assert!(ColorDef::parse_string("cube(60%, 20%)").is_err());
        assert!(ColorDef::parse_string("cube(60%, 20%, 0%, 10%)").is_err());
    }

    #[test]
    fn test_parse_cube_invalid_number() {
        assert!(ColorDef::parse_string("cube(abc, 20%, 0%)").is_err());
    }

    #[test]
    fn test_to_console_color_cube() {
        use crate::colorspace::CubeCoord;
        let coord = CubeCoord::from_percentages(60.0, 20.0, 0.0).unwrap();
        let c = ColorDef::Cube(coord);
        if let Color::Color256(_) = c.to_console_color(None) {
        } else {
            panic!("Expected Color256 from cube resolution");
        }
    }

    #[test]
    fn test_to_console_color_cube_with_palette() {
        use crate::colorspace::{CubeCoord, Rgb, ThemePalette};
        let palette = ThemePalette::new([
            Rgb(40, 40, 40),
            Rgb(204, 36, 29),
            Rgb(152, 151, 26),
            Rgb(215, 153, 33),
            Rgb(69, 133, 136),
            Rgb(177, 98, 134),
            Rgb(104, 157, 106),
            Rgb(168, 153, 132),
        ]);
        let coord = CubeCoord::from_percentages(0.0, 0.0, 0.0).unwrap();
        let c = ColorDef::Cube(coord);
        if let Color::Color256(_) = c.to_console_color(Some(&palette)) {
        } else {
            panic!("Expected Color256");
        }
    }

    #[test]
    fn test_parse_value_cube_string() {
        let val = Value::String("cube(50%, 50%, 50%)".into());
        let c = ColorDef::parse_value(&val).unwrap();
        assert!(matches!(c, ColorDef::Cube(_)));
    }
}
