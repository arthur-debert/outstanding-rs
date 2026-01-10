//! Utility functions for text processing and color conversion.

/// Converts an RGB triplet to the nearest ANSI 256-color palette index.
///
/// # Example
///
/// ```rust
/// use outstanding::rgb_to_ansi256;
///
/// // Pure red maps to ANSI 196
/// assert_eq!(rgb_to_ansi256((255, 0, 0)), 196);
///
/// // Pure green maps to ANSI 46
/// assert_eq!(rgb_to_ansi256((0, 255, 0)), 46);
/// ```
pub fn rgb_to_ansi256((r, g, b): (u8, u8, u8)) -> u8 {
    if r == g && g == b {
        if r < 8 {
            16
        } else if r > 248 {
            231
        } else {
            232 + ((r as u16 - 8) * 24 / 247) as u8
        }
    } else {
        let red = (r as u16 * 5 / 255) as u8;
        let green = (g as u16 * 5 / 255) as u8;
        let blue = (b as u16 * 5 / 255) as u8;
        16 + 36 * red + 6 * green + blue
    }
}

/// Placeholder helper for true-color output.
///
/// Currently returns the RGB triplet unchanged so it can be handed
/// to future true-color aware APIs.
pub fn rgb_to_truecolor(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    rgb
}

/// Truncates a string to fit within a maximum display width, adding ellipsis if needed.
///
/// Uses Unicode width calculations for proper handling of CJK and other wide characters.
/// If the string fits within `max_width`, it is returned unchanged. If truncation is
/// needed, characters are removed from the end and replaced with `…` (ellipsis).
///
/// # Arguments
///
/// * `s` - The string to truncate
/// * `max_width` - Maximum display width (in terminal columns)
///
/// # Example
///
/// ```rust
/// use outstanding::truncate_to_width;
///
/// assert_eq!(truncate_to_width("Hello", 10), "Hello");
/// assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
/// ```
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    // If the string fits, return it unchanged
    if s.width() <= max_width {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    // Reserve 1 char for ellipsis
    let limit = max_width.saturating_sub(1);

    for c in s.chars() {
        let char_width = c.width().unwrap_or(0);
        if current_width + char_width > limit {
            result.push('…');
            return result;
        }
        result.push(c);
        current_width += char_width;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_ansi256_grayscale() {
        assert_eq!(rgb_to_ansi256((0, 0, 0)), 16);
        assert_eq!(rgb_to_ansi256((255, 255, 255)), 231);
        let mid = rgb_to_ansi256((128, 128, 128));
        assert!((232..=255).contains(&mid));
    }

    #[test]
    fn test_rgb_to_ansi256_color_cube() {
        assert_eq!(rgb_to_ansi256((255, 0, 0)), 196);
        assert_eq!(rgb_to_ansi256((0, 255, 0)), 46);
        assert_eq!(rgb_to_ansi256((0, 0, 255)), 21);
    }

    #[test]
    fn test_truncate_to_width_no_truncation() {
        assert_eq!(truncate_to_width("Hello", 10), "Hello");
        assert_eq!(truncate_to_width("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_to_width_with_truncation() {
        assert_eq!(truncate_to_width("Hello World", 6), "Hello…");
        assert_eq!(truncate_to_width("Hello World", 7), "Hello …");
    }

    #[test]
    fn test_truncate_to_width_empty() {
        assert_eq!(truncate_to_width("", 5), "");
    }

    #[test]
    fn test_truncate_to_width_exact_fit() {
        assert_eq!(truncate_to_width("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_to_width_one_over() {
        assert_eq!(truncate_to_width("123456", 5), "1234…");
    }

    #[test]
    fn test_truncate_to_width_zero_width() {
        assert_eq!(truncate_to_width("Hello", 0), "…");
    }

    #[test]
    fn test_truncate_to_width_one_width() {
        assert_eq!(truncate_to_width("Hello", 1), "…");
    }
}
