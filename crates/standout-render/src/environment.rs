//! Injectable environment detection.
//!
//! This module centralizes process-global detection of terminal properties
//! — width and ANSI color capability — behind overridable function pointers
//! so tests can force specific values without touching real environment
//! state.
//!
//! There is deliberately no TTY detector. One existed and was removed: it
//! reported only on stdout, which is the wrong shape for the callers that
//! would want it (a pager gates on stdout, a progress display writes to
//! stderr), and the one in-repo caller that needed a terminal fact went
//! around it. See `docs/adr/0019-delete-the-in-process-tty-seam.md`.
//!
//! It follows the same pattern used by
//! [`set_theme_detector`](crate::set_theme_detector) and
//! [`set_icon_detector`](crate::set_icon_detector).
//!
//! # Usage
//!
//! In application code, call the `detect_*` functions. They resolve to real
//! process and terminal state by default:
//!
//! ```rust
//! use standout_render::{detect_terminal_width, detect_color_capability};
//!
//! let _width = detect_terminal_width();
//! let _color = detect_color_capability();
//! ```
//!
//! In tests, override any of them with a function pointer or a non-capturing
//! closure (both coerce to `fn(...) -> T`):
//!
//! ```rust
//! use standout_render::{set_terminal_width_detector, detect_terminal_width};
//!
//! set_terminal_width_detector(|| Some(80));
//! assert_eq!(detect_terminal_width(), Some(80));
//! ```
//!
//! Capturing closures are not supported — if you need per-test state, route
//! it through a thread-local or a static the detector reads from.
//!
//! Overrides are process-global, so tests that set them should be annotated
//! with `#[serial]` (via the `serial_test` crate) and should use
//! [`DetectorGuard`] to guarantee cleanup even when the test panics.

use crate::AmbiguousWidth;
use console::Term;
use once_cell::sync::Lazy;
use std::sync::Mutex;

type WidthDetector = fn() -> Option<usize>;
type ColorDetector = fn() -> bool;
type AmbiguousWidthDetector = fn() -> Option<AmbiguousWidth>;

static WIDTH_DETECTOR: Lazy<Mutex<WidthDetector>> =
    Lazy::new(|| Mutex::new(default_width_detector));
static COLOR_DETECTOR: Lazy<Mutex<ColorDetector>> =
    Lazy::new(|| Mutex::new(default_color_detector));
static AMBIGUOUS_WIDTH_DETECTOR: Lazy<Mutex<AmbiguousWidthDetector>> =
    Lazy::new(|| Mutex::new(default_ambiguous_width_detector));

/// Overrides the detector used to resolve terminal width.
///
/// The default detector consults a valid positive `$COLUMNS` value before
/// probing the terminal. An override replaces that entire resolution, returning
/// `Some(cols)` when a width should be used and `None` when it is unavailable.
/// Accepts a `fn` pointer or a non-capturing closure; useful to force a fixed
/// width in tests.
pub fn set_terminal_width_detector(detector: WidthDetector) {
    *WIDTH_DETECTOR.lock().unwrap() = detector;
}

/// Overrides the detector used to check whether ANSI color is supported on
/// stdout.
///
/// Accepts a `fn` pointer or a non-capturing closure. This is what
/// [`OutputMode::Auto`](crate::OutputMode::Auto) consults to decide between
/// applying and stripping style tags.
pub fn set_color_capability_detector(detector: ColorDetector) {
    *COLOR_DETECTOR.lock().unwrap() = detector;
}

/// Overrides the explicit ambiguous-width policy for rendering tests.
///
/// Returning `None` leaves the application or renderer configuration in
/// control. Standout never guesses this policy from locale settings.
pub fn set_ambiguous_width_detector(detector: AmbiguousWidthDetector) {
    *AMBIGUOUS_WIDTH_DETECTOR.lock().unwrap() = detector;
}

/// Resolves the current terminal width in columns.
///
/// By default, a valid positive `$COLUMNS` value takes precedence over probing
/// the terminal. Returns `None` when neither source provides a width. Layout
/// helpers may apply their own documented fallback when width is unavailable.
/// A detector installed with [`set_terminal_width_detector`] replaces this
/// default resolution so tests and applications can control the result.
pub fn detect_terminal_width() -> Option<usize> {
    // Copy the fn pointer out and release the lock before invoking the
    // detector. Holding the mutex across the call would poison it on panic
    // and deadlock if the detector re-entered `set_*`/`reset_*`.
    let detector = *WIDTH_DETECTOR.lock().unwrap();
    detector()
}

/// Returns `true` when ANSI color output is supported on stdout.
pub fn detect_color_capability() -> bool {
    let detector = *COLOR_DETECTOR.lock().unwrap();
    detector()
}

/// Returns a test override for ambiguous width, if one is installed.
pub fn detect_ambiguous_width_override() -> Option<AmbiguousWidth> {
    let detector = *AMBIGUOUS_WIDTH_DETECTOR.lock().unwrap();
    detector()
}

fn default_width_detector() -> Option<usize> {
    resolve_terminal_width(std::env::var_os("COLUMNS").as_deref(), || {
        terminal_size::terminal_size().map(|(width, _)| width.0 as usize)
    })
}

fn resolve_terminal_width(
    columns: Option<&std::ffi::OsStr>,
    probe_terminal: impl FnOnce() -> Option<usize>,
) -> Option<usize> {
    columns
        .and_then(std::ffi::OsStr::to_str)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&width| width > 0)
        .or_else(probe_terminal)
}

fn default_color_detector() -> bool {
    Term::stdout().features().colors_supported()
}

fn default_ambiguous_width_detector() -> Option<AmbiguousWidth> {
    None
}

/// Resets every environment detector in this module to its default
/// (real process-environment and terminal) implementation.
///
/// Tests that installed overrides should call this in teardown to avoid
/// leaking state into sibling tests. For panic-safe cleanup, prefer
/// [`DetectorGuard`] instead of calling this manually.
pub fn reset_detectors() {
    set_terminal_width_detector(default_width_detector);
    set_color_capability_detector(default_color_detector);
    set_ambiguous_width_detector(default_ambiguous_width_detector);
}

/// RAII guard that calls [`reset_detectors`] when dropped.
///
/// Install at the start of a test to guarantee the overrides are torn down
/// on normal exit *and* on panic-induced unwind, so a failing assertion
/// doesn't leak state into the next serial test.
///
/// ```rust
/// use standout_render::environment::{DetectorGuard, set_terminal_width_detector, detect_terminal_width};
///
/// let _guard = DetectorGuard::new();
/// set_terminal_width_detector(|| Some(80));
/// assert_eq!(detect_terminal_width(), Some(80));
/// // `_guard` resets everything when it goes out of scope.
/// ```
#[must_use = "the guard only resets detectors when dropped; bind it to a variable"]
pub struct DetectorGuard {
    _private: (),
}

impl DetectorGuard {
    /// Creates a guard that will reset all environment detectors on drop.
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for DetectorGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DetectorGuard {
    fn drop(&mut self) {
        reset_detectors();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serial_test::serial;
    use std::ffi::{OsStr, OsString};

    struct ColumnsGuard(Option<OsString>);

    impl ColumnsGuard {
        fn set(value: &str) -> Self {
            let original = std::env::var_os("COLUMNS");
            std::env::set_var("COLUMNS", value);
            Self(original)
        }
    }

    impl Drop for ColumnsGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("COLUMNS", value),
                None => std::env::remove_var("COLUMNS"),
            }
        }
    }

    #[test]
    #[serial]
    fn width_override_is_honored() {
        let _guard = DetectorGuard::new();
        set_terminal_width_detector(|| Some(42));
        assert_eq!(detect_terminal_width(), Some(42));
        set_terminal_width_detector(|| None);
        assert_eq!(detect_terminal_width(), None);
    }

    #[test]
    #[serial]
    fn default_width_resolution_honors_columns() {
        let _guard = DetectorGuard::new();
        let _columns = ColumnsGuard::set("47");

        reset_detectors();

        assert_eq!(detect_terminal_width(), Some(47));
    }

    #[test]
    #[serial]
    fn color_override_is_honored() {
        let _guard = DetectorGuard::new();
        set_color_capability_detector(|| true);
        assert!(detect_color_capability());
        set_color_capability_detector(|| false);
        assert!(!detect_color_capability());
    }

    #[test]
    #[serial]
    fn reset_replaces_panicking_overrides() {
        let _guard = DetectorGuard::new();

        fn boom_width() -> Option<usize> {
            panic!("width detector must not be called after reset")
        }
        fn boom_bool() -> bool {
            panic!("bool detector must not be called after reset")
        }

        set_terminal_width_detector(boom_width);
        set_color_capability_detector(boom_bool);

        reset_detectors();

        // If reset were a no-op the panicking detectors would still be
        // installed and these calls would unwind.
        let _ = detect_terminal_width();
        let _ = detect_color_capability();
    }

    #[test]
    #[serial]
    fn guard_restores_on_drop() {
        {
            let _guard = DetectorGuard::new();
            set_terminal_width_detector(|| Some(1));
            set_color_capability_detector(|| true);
            assert_eq!(detect_terminal_width(), Some(1));
        }

        // Guard dropped — a fresh panicking detector should be reachable
        // again (i.e. the override is gone) via reset_detectors. We verify
        // reset was effective by installing panicking detectors, dropping a
        // new guard, and confirming calls don't panic.
        fn boom() -> Option<usize> {
            panic!("override leaked past guard drop")
        }
        set_terminal_width_detector(boom);
        drop(DetectorGuard::new());
        let _ = detect_terminal_width();
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn columns_values_are_resolved_without_panicking(value in any::<String>()) {
            let expected = value
                .parse::<usize>()
                .ok()
                .filter(|&width| width > 0)
                .or(Some(73));
            prop_assert_eq!(
                resolve_terminal_width(Some(OsStr::new(&value)), || Some(73)),
                expected,
            );
        }

        #[test]
        fn every_positive_columns_width_precedes_the_terminal_probe(width in 1usize..) {
            prop_assert_eq!(
                resolve_terminal_width(Some(OsStr::new(&width.to_string())), || {
                    panic!("valid COLUMNS must prevent terminal probing")
                }),
                Some(width),
            );
        }
    }
}
