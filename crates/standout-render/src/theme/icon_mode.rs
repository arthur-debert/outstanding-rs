//! Icon mode detection for adaptive icon rendering.
//!
//! In [`IconMode::Auto`], the mode is resolved by checking the `NERD_FONT`
//! environment variable. Callers detect at the crate edge via
//! [`crate::TargetProperties::detect`]; tests construct
//! [`crate::TargetProperties`] with an explicit [`IconMode`]. Template
//! functions never call this probe.

/// The icon rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconMode {
    /// Use classic Unicode characters (works in all terminals).
    Classic,
    /// Use Nerd Font glyphs (requires a Nerd Font to be installed).
    NerdFont,
    /// Auto-detect: check `NERD_FONT` env var, fall back to Classic.
    Auto,
}

/// Detects the current icon mode.
///
/// Always returns a resolved mode (`Classic` or `NerdFont`), never `Auto`.
///
/// # Returns
///
/// - [`IconMode::NerdFont`] if `NERD_FONT` is `1` / `true` / `yes`
/// - [`IconMode::Classic`] otherwise
pub(crate) fn probe_icon_mode() -> IconMode {
    resolve_auto()
}

/// Resolves Auto mode by checking the `NERD_FONT` environment variable.
fn resolve_auto() -> IconMode {
    icon_mode_from_nerd_font_var(std::env::var("NERD_FONT").ok().as_deref())
}

fn icon_mode_from_nerd_font_var(val: Option<&str>) -> IconMode {
    match val {
        Some(val)
            if val == "1"
                || val.eq_ignore_ascii_case("true")
                || val.eq_ignore_ascii_case("yes") =>
        {
            IconMode::NerdFont
        }
        _ => IconMode::Classic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_font_var_truthy_values_select_nerd_font() {
        for value in ["1", "true", "TRUE", "yes", "Yes"] {
            assert_eq!(
                icon_mode_from_nerd_font_var(Some(value)),
                IconMode::NerdFont,
                "{value}"
            );
        }
    }

    #[test]
    fn nerd_font_var_absent_or_falsey_selects_classic() {
        assert_eq!(icon_mode_from_nerd_font_var(None), IconMode::Classic);
        for value in ["0", "false", "", "maybe"] {
            assert_eq!(
                icon_mode_from_nerd_font_var(Some(value)),
                IconMode::Classic,
                "{value}"
            );
        }
    }
}
