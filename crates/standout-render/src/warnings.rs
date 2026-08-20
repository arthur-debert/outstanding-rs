//! Framework warning collection and deferred rendering.
//!
//! Some framework paths can encounter non-fatal problems during application
//! startup or pre-dispatch. Examples include embedded-resource hot reload in
//! [`crate::embedded`] falling back to a compile-time copy, or an accepted
//! questionnaire answer sheet containing a suspicious tag-like fragment that
//! should be shown to the user without rejecting the run. Historically these
//! were emitted via `eprintln!` at the discovery site, which meant they printed
//! *before* the command's own output and as plain text, even when rendering into
//! a rich terminal.
//!
//! This module collects those messages on an explicit [`WarningBuffer`] so
//! the CLI layer can return them on the run result and render them *after*
//! the command output, styled through the active theme from stderr color
//! capability on [`crate::TargetProperties`], with a clear banner separating
//! them from the rest of the terminal session. There is no thread-local
//! collector.
//!
//! # Scope
//!
//! Only *framework warnings* should go through this module: non-fatal
//! framework-owned setup, resource-loading, or accepted-input diagnostics whose
//! ordering belongs to the run boundary. User-facing diagnostics that are part
//! of a handler's legitimate output - clipboard access failures, input
//! validation feedback, handler-generated I/O errors - stay on stderr as
//! before; interleaving them with other output is the correct behavior.
//!
//! # Usage
//!
//! Inside the framework, call [`WarningBuffer::push`] instead of `eprintln!`:
//!
//! ```rust,ignore
//! use standout_render::warnings::WarningBuffer;
//! buffer.push(format!("Failed to parse stylesheets from '{}': {}", path, err));
//! ```
//!
//! The CLI layer takes the buffer at the end of `App::run_with` and returns
//! the messages on the run result; `App::run` then renders the batch through
//! the theme using stderr color capability from [`crate::TargetProperties`].
//! [`render_block_for_target`] exposes that same layout to a caller with no
//! stderr of its own — the in-process test harness reconstructing what the
//! error channel carried, with the app's theme and the run's output mode —
//! so the two cannot drift apart.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use crate::output::OutputMode;
use crate::theme::Theme;
use crate::TargetProperties;

/// Per-run collector of framework warnings.
///
/// Cheap to clone (`Rc`): glue creates one at the run edge, stores it on
/// the command context and the render request, and takes the messages onto
/// the run result. It is not a thread-local and not a process global.
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
    /// Creates an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a framework warning.
    ///
    /// The warning is stored verbatim — callers should format a complete,
    /// self-contained message (no trailing newline). The CLI layer adds the
    /// tab indent and banner when flushing.
    pub fn push(&self, message: impl Into<String>) {
        self.inner.borrow_mut().push(message.into());
    }

    /// Appends a framework warning unless the same message is already pending.
    pub fn push_once(&self, message: impl Into<String>) {
        let message = message.into();
        let mut warnings = self.inner.borrow_mut();
        if !warnings.contains(&message) {
            warnings.push(message);
        }
    }

    /// Removes and returns all collected warnings.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.inner.borrow_mut())
    }

    /// Returns a snapshot of pending warnings without draining.
    pub fn snapshot(&self) -> Vec<String> {
        self.inner.borrow().clone()
    }

    /// Returns `true` if no warnings are pending.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }
}

/// Appends a framework warning to `buffer`.
pub fn push_warning(buffer: &WarningBuffer, message: impl Into<String>) {
    buffer.push(message);
}

/// Style name for the "Standout :: Warnings" banner, looked up in the theme.
pub const WARNING_BANNER_STYLE: &str = "standout_warning_banner";

/// Style name for each individual warning line, looked up in the theme.
pub const WARNING_ITEM_STYLE: &str = "standout_warning_item";

/// Literal banner text. Leading/trailing spaces give the background color
/// room to breathe when the banner is styled with a bg fill.
const BANNER_TEXT: &str = " Standout :: Warnings ";

/// Renders the warning block for a destination, applying stderr color from
/// `target`.
///
/// Used by tests that need to assert the piped-stdout / TTY-stderr case
/// without writing to a real stderr.
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

/// Renders the warning block — blank line, banner, one tab-indented line per
/// warning — with each styled span passed through `style`.
///
/// The layout lives here alone so every writer of the block shares it:
/// [`flush_to_stderr`] and the in-process test harness both go through
/// [`render_block_for_target`] so theme and `--output=text` cannot drift.
/// [`render_block_plain`] is the unstyled form of the same layout. Returns `""`
/// for an empty batch, which is what makes "no warnings" add nothing to the
/// error channel.
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

/// Renders the warning block unstyled, exactly as [`flush_to_stderr`] writes it
/// when the theme carries no warning styles or stderr cannot take color.
///
/// Intended for callers that want the layout without theme or destination
/// styling. The in-process test harness reconstructs stderr through
/// [`render_block_for_target`] so it stays aligned with `App::run`. Returns
/// `""` for an empty batch.
pub fn render_block_plain(warnings: &[String]) -> String {
    render_block(warnings, |_, text| text.to_string())
}

/// Emits `warnings` to stderr.
///
/// Called by the CLI layer at the end of `App::run`, *after* the command
/// output has been written to stdout, so the banner is the last thing the
/// user sees. Does nothing if `warnings` is empty.
///
/// # Styling
///
/// Styling is applied when stderr color capability on `target` is true and
/// `output_mode` does not explicitly forbid ANSI output (`Text` mode). Piped
/// stdout does not strip stderr: primary render reads stdout capability,
/// warnings read stderr capability. The banner pulls its style from
/// [`WARNING_BANNER_STYLE`] in `theme`; each warning line pulls from
/// [`WARNING_ITEM_STYLE`]. Themes that don't define these styles fall back
/// to unstyled text.
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

    // Write everything through a single stderr lock so the banner and its
    // items cannot be interleaved with other output on a shared stream.
    let stderr = std::io::stderr();
    let mut out = stderr.lock();
    let _ = write!(out, "{}", block).and_then(|()| out.flush());
}

/// Applies `style_name` to `text`, forcing ANSI on/off based on `use_color`
/// rather than the crate-wide `console::colors_enabled()` (which tracks
/// stdout). This matters when stdout is piped but stderr is still a TTY:
/// `Styles::apply` would see the global flag and strip codes we actually
/// want to keep for stderr.
///
/// Falls back to unstyled text when the style is absent or `use_color` is
/// false, rather than applying the "missing style" indicator — a warning
/// with a stray `?` in front of it would be a worse UX than a plain one.
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

/// Decides whether the warnings block should use ANSI styling.
///
/// `OutputMode::Text` explicitly opts out of color. Structured modes
/// (`Json`/`Yaml`/`Xml`/`Csv`) target stdout, not stderr, so they don't
/// constrain our styling choices here — stderr color capability on
/// [`TargetProperties`] is what matters. Piped stdout does not strip
/// stderr. `TermDebug` emits bracket tags instead of ANSI in the main
/// output, but the warnings banner isn't subject to that contract.
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
        // the flush helper silently emits plain text, so bake the presence of
        // the style names into a test.
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
