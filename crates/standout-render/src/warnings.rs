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
//! This module routes those messages through a process-local collector so
//! the CLI layer can render them *after* the command output, styled through
//! the active theme, with a clear banner separating them from the rest of
//! the terminal session.
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
//! Inside the framework, call [`push_warning`] instead of `eprintln!`:
//!
//! ```rust,ignore
//! use standout_render::warnings::push_warning;
//! push_warning(format!("Failed to parse stylesheets from '{}': {}", path, err));
//! ```
//!
//! The CLI layer drains the collector at the end of `App::run` and renders
//! the batch through the theme; see the `standout` crate for the flush
//! logic. [`render_block_plain`] exposes that same layout to a caller with no
//! stderr of its own — the in-process test harness reconstructing what the
//! error channel carried — so the two cannot drift apart.

use std::cell::RefCell;
use std::io::Write;

use crate::output::OutputMode;
use crate::theme::Theme;

thread_local! {
    /// Thread-local buffer of framework warnings collected during this run.
    ///
    /// A CLI process is effectively single-threaded for the duration of
    /// `App::run` (handlers themselves may spawn threads, but framework
    /// warnings come from the main-thread setup path), so a thread-local
    /// is sufficient and avoids the overhead of a mutex.
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };

    /// Warnings drained by the most recent capture-oriented run.
    ///
    /// `App::run_to_string` cannot print the warning block, but it still owns
    /// the run boundary. Stashing the drained batch here makes warnings
    /// observable to test/capture APIs without letting them leak into the next
    /// run on the same thread.
    static CAPTURED_WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Appends a framework warning to the thread-local collector.
///
/// The warning is stored verbatim — callers should format a complete,
/// self-contained message (no trailing newline). The CLI layer adds the
/// tab indent and banner when flushing.
pub fn push_warning(message: impl Into<String>) {
    WARNINGS.with(|w| w.borrow_mut().push(message.into()));
}

/// Removes and returns all collected warnings for the current thread.
///
/// After this call the collector is empty. The CLI layer calls this once
/// at the end of `App::run` to render the batch.
pub fn drain_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

/// Drains pending framework warnings into the captured-run buffer.
///
/// This is the capture-mode counterpart to [`flush_to_stderr`]: it ends the
/// current run's warning batch without writing to stderr. The next call
/// replaces the previous captured batch, so stale warnings cannot bleed across
/// consecutive in-process runs.
pub fn capture_warnings_for_run() {
    let warnings = drain_warnings();
    CAPTURED_WARNINGS.with(|captured| {
        *captured.borrow_mut() = warnings;
    });
}

/// Returns and clears the warning batch captured by the most recent
/// capture-oriented run on this thread.
pub fn take_captured_warnings() -> Vec<String> {
    CAPTURED_WARNINGS.with(|captured| std::mem::take(&mut *captured.borrow_mut()))
}

/// Returns `true` if any warnings are currently buffered for this thread.
///
/// Intended for hot-path checks that want to skip the rendering work when
/// there is nothing to emit.
pub fn has_warnings() -> bool {
    WARNINGS.with(|w| !w.borrow().is_empty())
}

/// Style name for the "Standout :: Warnings" banner, looked up in the theme.
pub const WARNING_BANNER_STYLE: &str = "standout_warning_banner";

/// Style name for each individual warning line, looked up in the theme.
pub const WARNING_ITEM_STYLE: &str = "standout_warning_item";

/// Literal banner text. Leading/trailing spaces give the background color
/// room to breathe when the banner is styled with a bg fill.
const BANNER_TEXT: &str = " Standout :: Warnings ";

/// Renders the warning block — blank line, banner, one tab-indented line per
/// warning — with each styled span passed through `style`.
///
/// The layout lives here alone so every writer of the block shares it:
/// [`flush_to_stderr`] passes a theme-aware styling closure, and a caller with
/// no real stderr to write to (the in-process test harness reconstructing the
/// error channel) passes the identity via [`render_block_plain`]. Returns `""`
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
/// Intended for callers that must reconstruct the error channel without owning
/// a stderr — chiefly `standout-test`, whose `TestResult::stderr()` would
/// otherwise have to duplicate this layout and drift from it. Returns `""` for
/// an empty batch.
pub fn render_block_plain(warnings: &[String]) -> String {
    render_block(warnings, |_, text| text.to_string())
}

/// Drains the collector and emits the warnings to stderr.
///
/// Called by the CLI layer at the end of `App::run`, *after* the command
/// output has been written to stdout, so the banner is the last thing the
/// user sees. Does nothing if no warnings have been collected.
///
/// # Styling
///
/// Styling is applied when stderr is a TTY that supports color and
/// `output_mode` does not explicitly forbid ANSI output (`Text` mode). The
/// banner pulls its style from [`WARNING_BANNER_STYLE`] in `theme`; each
/// warning line pulls from [`WARNING_ITEM_STYLE`]. Themes that don't define
/// these styles fall back to unstyled text.
pub fn flush_to_stderr(theme: &Theme, output_mode: OutputMode) {
    let warnings = drain_warnings();
    if warnings.is_empty() {
        return;
    }

    let use_color = should_style_stderr(output_mode);
    let styles = theme.resolve_styles(None);
    let block = render_block(&warnings, |style_name, text| {
        style_for_stderr(&styles, style_name, text, use_color)
    });

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
/// constrain our styling choices here — stderr TTY capability is what
/// matters. `TermDebug` emits bracket tags instead of ANSI in the main
/// output, but the warnings banner isn't subject to that contract, so we
/// still honor the stderr TTY signal.
fn should_style_stderr(output_mode: OutputMode) -> bool {
    if matches!(output_mode, OutputMode::Text) {
        return false;
    }
    console::Term::stderr().features().colors_supported()
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;

    fn reset() {
        let _ = drain_warnings();
    }

    #[test]
    fn push_and_drain_roundtrip() {
        reset();

        assert!(!has_warnings());
        push_warning("first");
        push_warning(String::from("second"));
        assert!(has_warnings());

        let drained = drain_warnings();
        assert_eq!(drained, vec!["first".to_string(), "second".to_string()]);
        assert!(!has_warnings());

        // Draining again yields nothing.
        assert!(drain_warnings().is_empty());
    }

    #[test]
    fn capture_replaces_previous_batch_and_drains_pending_warnings() {
        reset();
        push_warning("first");
        capture_warnings_for_run();
        assert_eq!(take_captured_warnings(), vec!["first".to_string()]);
        assert!(drain_warnings().is_empty());

        push_warning("second");
        capture_warnings_for_run();
        push_warning("pending");
        capture_warnings_for_run();
        assert_eq!(take_captured_warnings(), vec!["pending".to_string()]);
        assert!(drain_warnings().is_empty());
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
}
