//! Theme selection for rendering.

use super::adaptive::AdaptiveTheme;
use super::theme::Theme;

/// Reference to either a static theme or an adaptive theme.
///
/// This enum allows render functions to accept either a fixed theme
/// or an adaptive theme that responds to the system color mode.
#[derive(Debug)]
pub enum ThemeChoice<'a> {
    /// A fixed theme that doesn't change based on color mode.
    Theme(&'a Theme),
    /// An adaptive theme that selects light/dark based on OS settings.
    Adaptive(&'a AdaptiveTheme),
}

impl<'a> ThemeChoice<'a> {
    /// Resolves to a concrete theme.
    ///
    /// For fixed themes, returns a clone. For adaptive themes,
    /// detects the current color mode and returns the appropriate variant.
    pub(crate) fn resolve(&self) -> Theme {
        match self {
            ThemeChoice::Theme(theme) => (*theme).clone(),
            ThemeChoice::Adaptive(adaptive) => adaptive.resolve(),
        }
    }
}

impl<'a> From<&'a Theme> for ThemeChoice<'a> {
    fn from(theme: &'a Theme) -> Self {
        ThemeChoice::Theme(theme)
    }
}

impl<'a> From<&'a AdaptiveTheme> for ThemeChoice<'a> {
    fn from(adaptive: &'a AdaptiveTheme) -> Self {
        ThemeChoice::Adaptive(adaptive)
    }
}
