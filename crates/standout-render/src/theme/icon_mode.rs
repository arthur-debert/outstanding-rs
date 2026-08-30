#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconMode {
    Classic,
    NerdFont,
    Auto,
}

pub(crate) fn probe_icon_mode() -> IconMode {
    resolve_auto()
}

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
