//! Collects non-fatal framework warnings on a [`WarningBuffer`] instead of
//! `eprintln!`-ing them at the discovery site, so the CLI layer can render
//! them *after* the command's own output, styled through the active theme,
//! with a clear banner. Only framework-owned diagnostics (resource-loading
//! fallbacks, accepted-input warnings) belong here — handler-generated
//! stderr output stays interleaved as before.
//!
//! Warnings are styled using stderr color capability from
//! [`crate::TargetProperties`], independent of the primary render's stdout
//! capability, so piped stdout does not strip a still-interactive stderr.
//! `OutputMode::Text` opts out of styling entirely. There is no thread-local
//! collector; the buffer is passed explicitly through the run.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use crate::output::OutputMode;
use crate::theme::Theme;
use crate::TargetProperties;

#[derive(Clone, Default)]
pub struct WarningBuffer {
    inner: Rc<RefCell<Vec<String>>>,
}

impl std::fmt::Debug for WarningBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarningBuffer")
            .field("len", &self.inner.borrow().len())
            .finish()
    }
}

impl WarningBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, message: impl Into<String>) {
        self.inner.borrow_mut().push(message.into());
    }

    pub fn push_once(&self, message: impl Into<String>) {
        let message = message.into();
        let mut warnings = self.inner.borrow_mut();
        if !warnings.contains(&message) {
            warnings.push(message);
        }
    }

    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.inner.borrow_mut())
    }

    /// Drop every buffered warning for which `keep` returns false. The
    /// strict-style-tags gate uses this to remove the now-superseded
    /// "degraded to unstyled text" warning once it escalates the same tags to
    /// a hard error, so the failure is reported once rather than twice.
    pub fn retain(&self, keep: impl Fn(&str) -> bool) {
        self.inner.borrow_mut().retain(|warning| keep(warning));
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.inner.borrow().clone()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }
}

pub fn push_warning(buffer: &WarningBuffer, message: impl Into<String>) {
    buffer.push(message);
}

pub const WARNING_BANNER_STYLE: &str = "standout_warning_banner";

pub const WARNING_ITEM_STYLE: &str = "standout_warning_item";

const BANNER_TEXT: &str = " Standout :: Warnings ";

pub fn render_block_for_target(
    theme: &Theme,
    output_mode: OutputMode,
    target: TargetProperties,
    warnings: &[String],
) -> String {
    let use_color = should_style_stderr(output_mode, target);
    let styles = theme.resolve_styles(None);
    render_block(warnings, |style_name, text| {
        style_for_stderr(&styles, style_name, text, use_color)
    })
}

fn render_block(warnings: &[String], style: impl Fn(&str, &str) -> String) -> String {
    if warnings.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push('\n');
    out.push_str(&style(WARNING_BANNER_STYLE, BANNER_TEXT));
    out.push('\n');
    for w in warnings {
        out.push('\t');
        out.push_str(&style(WARNING_ITEM_STYLE, w));
        out.push('\n');
    }
    out
}

pub fn render_block_plain(warnings: &[String]) -> String {
    render_block(warnings, |_, text| text.to_string())
}

pub fn flush_to_stderr(
    theme: &Theme,
    output_mode: OutputMode,
    target: TargetProperties,
    warnings: &[String],
) {
    let block = render_block_for_target(theme, output_mode, target, warnings);
    if block.is_empty() {
        return;
    }

    // Single lock so the banner and its items can't interleave with other output.
    let stderr = std::io::stderr();
    let mut out = stderr.lock();
    let _ = write!(out, "{}", block).and_then(|()| out.flush());
}

fn style_for_stderr(
    styles: &crate::style::Styles,
    style_name: &str,
    text: &str,
    use_color: bool,
) -> String {
    if !use_color {
        return text.to_string();
    }
    match styles.resolve(style_name) {
        Some(style) => style
            .clone()
            .for_stderr()
            .force_styling(true)
            .apply_to(text)
            .to_string(),
        None => text.to_string(),
    }
}

fn should_style_stderr(output_mode: OutputMode, target: TargetProperties) -> bool {
    if matches!(output_mode, OutputMode::Text) {
        return false;
    }
    target.stderr_color_capability
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;

    fn sample_target(stdout_color: bool, stderr_color: bool) -> TargetProperties {
        TargetProperties {
            width: None,
            stdout_is_terminal: stdout_color,
            stderr_is_terminal: stderr_color,
            stdout_color_capability: stdout_color,
            stderr_color_capability: stderr_color,
            color_scheme: crate::ColorMode::Dark,
            icon_mode: crate::IconMode::Classic,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
        }
    }

    #[test]
    fn push_and_take_roundtrip() {
        let buffer = WarningBuffer::new();

        assert!(buffer.is_empty());
        buffer.push("first");
        push_warning(&buffer, String::from("second"));
        assert!(!buffer.is_empty());

        let drained = buffer.take();
        assert_eq!(drained, vec!["first".to_string(), "second".to_string()]);
        assert!(buffer.is_empty());
        assert!(buffer.take().is_empty());
    }

    #[test]
    fn push_once_deduplicates_pending_messages() {
        let buffer = WarningBuffer::new();
        buffer.push_once("same warning");
        buffer.push_once("same warning");
        assert_eq!(buffer.take(), ["same warning"]);
    }

    #[test]
    fn render_block_plain_lays_out_banner_and_indented_items() {
        assert_eq!(
            render_block_plain(&["first".to_string(), "second".to_string()]),
            format!("\n{}\n\tfirst\n\tsecond\n", BANNER_TEXT)
        );
    }

    #[test]
    fn render_block_plain_is_empty_without_warnings() {
        assert_eq!(render_block_plain(&[]), "");
    }

    #[test]
    fn default_theme_registers_warning_styles() {
        // Regression check: if Theme::default ever stops shipping these styles
        let theme = Theme::default();
        let styles = theme.resolve_styles(None);
        assert!(
            styles.has(WARNING_BANNER_STYLE),
            "Theme::default missing '{}'",
            WARNING_BANNER_STYLE
        );
        assert!(
            styles.has(WARNING_ITEM_STYLE),
            "Theme::default missing '{}'",
            WARNING_ITEM_STYLE
        );
    }

    #[test]
    fn style_for_stderr_plain_when_color_disabled() {
        let mut styles = crate::style::Styles::new();
        styles = styles.add("some_style", Style::new().red());
        let out = style_for_stderr(&styles, "some_style", "hello", false);
        assert_eq!(out, "hello");
    }

    #[test]
    fn style_for_stderr_plain_when_style_missing() {
        let styles = crate::style::Styles::new();
        let out = style_for_stderr(&styles, "no_such_style", "hello", true);
        // Fall back to plain text rather than emitting the missing-style marker.
        assert_eq!(out, "hello");
    }

    #[test]
    fn style_for_stderr_emits_ansi_when_enabled() {
        let styles = crate::style::Styles::new().add("warn", Style::new().red().bold());
        let out = style_for_stderr(&styles, "warn", "hello", true);
        assert!(
            out.contains("\x1b["),
            "expected ANSI escape in styled output, got: {:?}",
            out
        );
        assert!(out.contains("hello"));
    }

    #[test]
    fn piped_stdout_tty_stderr_keeps_warning_color() {
        let theme = Theme::default();
        let target = sample_target(false, true);
        let block = render_block_for_target(
            &theme,
            OutputMode::Auto,
            target,
            &["stylesheet fell back".to_string()],
        );
        assert!(
            block.contains("\x1b["),
            "piped stdout must not strip stderr warning color, got: {:?}",
            block
        );
        assert!(block.contains("stylesheet fell back"));
    }

    #[test]
    fn text_output_opts_out_of_warning_color_on_capable_stderr() {
        let theme = Theme::default();
        let target = sample_target(false, true);
        let block = render_block_for_target(
            &theme,
            OutputMode::Text,
            target,
            &["stylesheet fell back".to_string()],
        );
        assert!(
            !block.contains("\x1b["),
            "--output=text must keep the warning block plain, got: {:?}",
            block
        );
        assert!(block.contains("stylesheet fell back"));
    }

    #[test]
    fn piped_stderr_strips_warning_color() {
        let theme = Theme::default();
        let target = sample_target(true, false);
        let block = render_block_for_target(
            &theme,
            OutputMode::Auto,
            target,
            &["stylesheet fell back".to_string()],
        );
        assert!(
            !block.contains("\x1b["),
            "stderr without color capability must be plain, got: {:?}",
            block
        );
    }

    #[test]
    fn text_mode_strips_warning_color_even_when_stderr_is_capable() {
        let theme = Theme::default();
        let target = sample_target(false, true);
        let block = render_block_for_target(
            &theme,
            OutputMode::Text,
            target,
            &["stylesheet fell back".to_string()],
        );
        assert!(!block.contains("\x1b["));
    }
}
