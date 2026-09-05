//! Structured records of what the style-tag pass could not resolve,
//! captured for callers that need more than the rendered output.
//!
//! [`resolve_tags`] is the render path's replacement for building a
//! [`BBParser`] directly: same output, but unresolved-tag warnings go to an
//! optional [`crate::warnings::WarningBuffer`] and a [`TagResolution`] is
//! recorded for every pass while a capture window is open.
//!
//! Capture is off by default. [`resolve_tags`] runs on every render,
//! including a standalone [`Renderer`](crate::Renderer) that may render
//! millions of times in a long-lived process, so nothing is recorded unless
//! [`begin_capture`] has opened a [`CaptureWindow`]. The window is a guard
//! rather than a begin/end pair so that a handler panicking mid-render still
//! closes it (via unwind) instead of leaking an ever-growing collector.
//!
//! Windows nest: a handler may drive another app through `run_to_string`,
//! opening an inner window inside the outer one. Closing the inner window
//! publishes its own batch to [`take_captured`] *and* folds it into the
//! enclosing window one level deeper on [`TagResolution::nesting_depth`] —
//! unconditionally, because this layer sees only the returned `String` and
//! cannot tell whether the caller embedded it or discarded it, and
//! under-reporting a page that does embed it would leave no evidence at all
//! in `Text` mode. A caller that wants only its own run's passes filters on
//! `nesting_depth() == 0`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;

use console::Style;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior, UnknownTagError};

thread_local! {
    static WINDOWS: RefCell<Vec<Vec<TagResolution>>> = const { RefCell::new(Vec::new()) };
    static CAPTURED: RefCell<Vec<TagResolution>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagResolution {
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
    unresolved: Vec<UnknownTagError>,
    malformed: Vec<UnknownTagError>,
    defined_tags: Vec<String>,
    nesting_depth: usize,
}

impl TagResolution {
    pub fn transform(&self) -> TagTransform {
        self.transform
    }

    pub fn unknown_behavior(&self) -> UnknownTagBehavior {
        self.unknown_behavior
    }

    pub fn unresolved(&self) -> &[UnknownTagError] {
        &self.unresolved
    }

    pub fn malformed(&self) -> &[UnknownTagError] {
        &self.malformed
    }

    pub fn unresolved_tag_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for error in &self.unresolved {
            let name = error.tag.as_str();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    pub fn defined_tags(&self) -> &[String] {
        &self.defined_tags
    }

    pub fn nesting_depth(&self) -> usize {
        self.nesting_depth
    }

    pub fn is_clean(&self) -> bool {
        self.unresolved.is_empty()
    }
}

pub fn resolve_tags(
    input: &str,
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
) -> String {
    resolve_tags_with(input, styles, transform, unknown_behavior, None)
}

pub fn resolve_tags_with(
    input: &str,
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
    warnings: Option<&crate::warnings::WarningBuffer>,
) -> String {
    let parser = BBParser::new(styles, transform).unknown_behavior(unknown_behavior);
    let (output, errors) = parser.parse_with_diagnostics(input);

    let (unresolved, malformed): (Vec<UnknownTagError>, Vec<UnknownTagError>) = errors
        .errors
        .into_iter()
        .partition(|error| !parser.styles().contains_key(&error.tag));

    if !unresolved.is_empty() {
        warn_unresolved_tags(&unresolved, warnings);
    }

    if !is_capturing() {
        return output;
    }

    let defined_tags = if unresolved.is_empty() {
        Vec::new()
    } else {
        let mut names: Vec<String> = parser.styles().keys().cloned().collect();
        names.sort();
        names
    };

    record(TagResolution {
        transform,
        unknown_behavior,
        unresolved,
        malformed,
        defined_tags,
        nesting_depth: 0,
    });

    output
}

/// Prefix of the degraded-to-unstyled-text warning, so a strict-mode caller
/// can drop it after escalating the same tags to an error.
pub const UNRESOLVED_DEGRADATION_PREFIX: &str =
    "Unresolved style tag(s) degraded to unstyled text: ";

fn warn_unresolved_tags(
    unresolved: &[UnknownTagError],
    warnings: Option<&crate::warnings::WarningBuffer>,
) {
    let Some(warnings) = warnings else {
        return;
    };
    let mut names: Vec<&str> = unresolved.iter().map(|error| error.tag.as_str()).collect();
    names.sort_unstable();
    names.dedup();

    if !names.is_empty() {
        warnings.push_once(format!(
            "{UNRESOLVED_DEGRADATION_PREFIX}{}",
            names.join(", ")
        ));
    }
}

/// Unresolved tag names from the current window's own (depth-zero) passes,
/// sorted and deduplicated, read without draining; empty when no window is open.
pub fn unresolved_in_current_window() -> Vec<String> {
    WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let Some(current) = windows.last() else {
            return Vec::new();
        };
        let mut names: Vec<String> = current
            .iter()
            .filter(|pass| pass.nesting_depth() == 0)
            .flat_map(TagResolution::unresolved_tag_names)
            .map(str::to_string)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    })
}

#[must_use = "the window closes when the guard drops; bind it across the run it bounds"]
pub struct CaptureWindow {
    _not_send: PhantomData<*const ()>,
}

impl Drop for CaptureWindow {
    fn drop(&mut self) {
        let batch = WINDOWS.with(|windows| windows.borrow_mut().pop().unwrap_or_default());

        WINDOWS.with(|windows| {
            if let Some(enclosing) = windows.borrow_mut().last_mut() {
                enclosing.extend(batch.iter().cloned().map(|mut pass| {
                    pass.nesting_depth += 1;
                    pass
                }));
            }
        });

        CAPTURED.with(|captured| {
            *captured.borrow_mut() = batch;
        });
    }
}

pub fn begin_capture() -> CaptureWindow {
    WINDOWS.with(|windows| windows.borrow_mut().push(Vec::new()));
    CaptureWindow {
        _not_send: PhantomData,
    }
}

fn is_capturing() -> bool {
    WINDOWS.with(|windows| !windows.borrow().is_empty())
}

fn record(resolution: TagResolution) {
    WINDOWS.with(|windows| {
        if let Some(current) = windows.borrow_mut().last_mut() {
            current.push(resolution);
        }
    });
}

#[cfg(test)]
fn drain() -> Vec<TagResolution> {
    WINDOWS.with(|windows| {
        windows
            .borrow_mut()
            .last_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

#[cfg(any(test, feature = "test-support"))]
pub fn take_captured() -> Vec<TagResolution> {
    CAPTURED.with(|captured| std::mem::take(&mut *captured.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_with(tag: &str) -> HashMap<String, Style> {
        let mut styles = HashMap::new();
        styles.insert(tag.to_string(), Style::new().bold());
        styles
    }

    fn reset() -> CaptureWindow {
        WINDOWS.with(|windows| windows.borrow_mut().clear());
        take_captured();
        begin_capture()
    }

    fn unresolved_across(batch: &[TagResolution]) -> Vec<&str> {
        batch
            .iter()
            .flat_map(TagResolution::unresolved_tag_names)
            .collect()
    }

    fn depths(batch: &[TagResolution]) -> Vec<usize> {
        batch.iter().map(TagResolution::nesting_depth).collect()
    }

    #[test]
    fn a_clean_pass_is_recorded_and_names_nothing() {
        let _window = reset();
        let output = resolve_tags(
            "[ok]done[/ok]",
            theme_with("ok"),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );

        assert_eq!(output, "done");
        let passes = drain();
        assert_eq!(passes.len(), 1);
        assert!(passes[0].is_clean());
        assert!(passes[0].unresolved_tag_names().is_empty());
        assert!(passes[0].defined_tags().is_empty());
    }

    #[test]
    fn an_unresolved_tag_is_named_in_every_transform() {
        for transform in [
            TagTransform::Apply,
            TagTransform::Keep,
            TagTransform::Remove,
        ] {
            let _window = reset();
            let output = resolve_tags(
                "[nope]hi[/nope]",
                theme_with("ok"),
                transform,
                UnknownTagBehavior::Passthrough,
            );

            let passes = drain();
            assert_eq!(passes[0].unresolved_tag_names(), ["nope"], "{transform:?}");
            assert_eq!(passes[0].transform(), transform);
            assert!(passes[0].defined_tags().contains(&"ok".to_string()));
            if transform == TagTransform::Remove {
                assert_eq!(output, "hi");
            }
        }
    }

    #[test]
    fn the_strip_behavior_is_recorded_alongside_the_tags() {
        let _window = reset();
        resolve_tags(
            "[nope]hi[/nope]",
            HashMap::new(),
            TagTransform::Apply,
            UnknownTagBehavior::Strip,
        );

        let passes = drain();
        assert_eq!(passes[0].unknown_behavior(), UnknownTagBehavior::Strip);
    }

    #[test]
    fn closing_the_window_ends_the_batch_so_runs_cannot_bleed_together() {
        let window = reset();
        resolve_tags(
            "[nope]hi[/nope]",
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );
        drop(window);

        let first = take_captured();
        assert_eq!(first.len(), 1);
        assert!(take_captured().is_empty());

        drop(begin_capture());
        assert!(take_captured().is_empty());
    }

    #[test]
    fn renders_outside_a_capture_window_accumulate_nothing() {
        drop(reset());
        take_captured();

        for _ in 0..1000 {
            resolve_tags(
                "[nope]hi[/nope]",
                HashMap::new(),
                TagTransform::Remove,
                UnknownTagBehavior::Passthrough,
            );
        }

        assert!(!is_capturing());
        assert!(drain().is_empty());
    }

    #[test]
    fn a_render_before_a_run_cannot_contaminate_it() {
        drop(reset());
        take_captured();

        let stray = crate::warnings::WarningBuffer::new();
        resolve_tags_with(
            "[stray]before the run[/stray]",
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
            Some(&stray),
        );

        let window = begin_capture();
        let run = crate::warnings::WarningBuffer::new();
        resolve_tags_with(
            "[ok]during the run[/ok]",
            theme_with("ok"),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
            Some(&run),
        );
        drop(window);

        let captured = take_captured();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].is_clean());
        assert_eq!(
            stray.take(),
            vec!["Unresolved style tag(s) degraded to unstyled text: stray".to_string()]
        );
        assert!(run.is_empty());
    }

    #[test]
    fn malformed_markup_on_a_defined_tag_is_not_an_unresolved_tag() {
        for input in ["[ok]unbalanced", "closed but never opened[/ok]"] {
            let _window = reset();
            resolve_tags(
                input,
                theme_with("ok"),
                TagTransform::Remove,
                UnknownTagBehavior::Passthrough,
            );

            let passes = drain();
            assert!(passes[0].is_clean(), "{input:?}");
            assert!(passes[0].defined_tags().is_empty(), "{input:?}");
            assert_eq!(
                passes[0]
                    .malformed()
                    .iter()
                    .map(|error| error.tag.as_str())
                    .collect::<Vec<_>>(),
                ["ok"],
                "{input:?}"
            );
        }
    }

    #[test]
    fn an_undefined_tag_is_unresolved_even_when_its_markup_is_broken() {
        let _window = reset();
        resolve_tags(
            "[nope]unbalanced",
            theme_with("ok"),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );

        let passes = drain();
        assert_eq!(passes[0].unresolved_tag_names(), ["nope"]);
        assert!(passes[0].malformed().is_empty());
    }

    fn render_unresolvable(tag: &str) {
        resolve_tags(
            &format!("[{tag}]x[/{tag}]"),
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );
    }

    #[test]
    fn an_inner_window_neither_clears_nor_closes_the_outer_one() {
        let outer = reset();
        render_unresolvable("before_inner");

        let inner = begin_capture();
        render_unresolvable("inner");
        drop(inner);

        let inner_batch = take_captured();
        assert_eq!(unresolved_across(&inner_batch), ["inner"]);
        assert!(is_capturing());

        render_unresolvable("after_inner");
        drop(outer);

        assert_eq!(
            unresolved_across(&take_captured()),
            ["before_inner", "inner", "after_inner"]
        );
    }

    #[test]
    fn windows_nest_to_any_depth() {
        let outer = reset();
        render_unresolvable("depth_one");

        let middle = begin_capture();
        render_unresolvable("depth_two");

        let inner = begin_capture();
        render_unresolvable("depth_three");
        drop(inner);
        assert_eq!(unresolved_across(&take_captured()), ["depth_three"]);

        drop(middle);
        assert_eq!(
            unresolved_across(&take_captured()),
            ["depth_two", "depth_three"]
        );

        drop(outer);
        assert_eq!(
            unresolved_across(&take_captured()),
            ["depth_one", "depth_two", "depth_three"]
        );
        assert!(!is_capturing());
    }

    #[test]
    fn a_panic_inside_a_window_still_closes_it() {
        drop(reset());
        take_captured();

        let outcome = std::panic::catch_unwind(|| {
            let _window = begin_capture();
            render_unresolvable("during_the_panicking_run");
            panic!("the handler blew up");
        });

        assert!(outcome.is_err());
        assert!(!is_capturing());
        assert_eq!(
            unresolved_across(&take_captured()),
            ["during_the_panicking_run"]
        );
    }

    #[test]
    fn the_fold_records_how_far_a_pass_travelled() {
        let outer = reset();
        render_unresolvable("outer_own");

        let middle = begin_capture();
        render_unresolvable("middle_own");

        let inner = begin_capture();
        render_unresolvable("inner_own");
        drop(inner);
        assert_eq!(depths(&take_captured()), [0]);

        drop(middle);
        assert_eq!(depths(&take_captured()), [0, 1]);

        drop(outer);
        assert_eq!(depths(&take_captured()), [0, 1, 2]);
    }

    #[test]
    fn a_discarded_nested_run_is_reported_by_the_enclosing_one_at_depth_one() {
        let outer = reset();

        let discarded = begin_capture();
        render_unresolvable("never_embedded");
        drop(discarded);
        take_captured();

        render_unresolvable("outer_own");
        drop(outer);

        let batch = take_captured();
        assert_eq!(unresolved_across(&batch), ["never_embedded", "outer_own"]);
        assert_eq!(depths(&batch), [1, 0]);
        assert_eq!(
            unresolved_across(
                &batch
                    .iter()
                    .filter(|pass| pass.nesting_depth() == 0)
                    .cloned()
                    .collect::<Vec<_>>()
            ),
            ["outer_own"]
        );
    }

    #[test]
    fn the_current_window_reader_names_unresolved_tags_without_draining() {
        let _window = reset();
        render_unresolvable("beta");
        render_unresolvable("alpha");
        render_unresolvable("beta");

        assert_eq!(unresolved_in_current_window(), ["alpha", "beta"]);
        assert_eq!(drain().len(), 3);
    }

    #[test]
    fn the_current_window_reader_ignores_folded_nested_passes() {
        let outer = reset();

        let discarded = begin_capture();
        render_unresolvable("nested_only");
        drop(discarded);
        take_captured();

        assert!(
            unresolved_in_current_window().is_empty(),
            "the folded nested pass must not surface as the outer run's own"
        );

        render_unresolvable("outer_own");
        assert_eq!(unresolved_in_current_window(), ["outer_own"]);
        drop(outer);
    }

    #[test]
    fn the_current_window_reader_is_empty_for_a_clean_run() {
        let _window = reset();
        resolve_tags(
            "[ok]done[/ok]",
            theme_with("ok"),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );

        assert!(unresolved_in_current_window().is_empty());
    }

    #[test]
    fn the_current_window_reader_is_empty_with_no_window_open() {
        WINDOWS.with(|windows| windows.borrow_mut().clear());
        assert!(unresolved_in_current_window().is_empty());
    }

    #[test]
    fn the_guard_is_neither_send_nor_sync() {
        struct Probe<T>(PhantomData<T>);

        trait NotSend {
            fn is_send(&self) -> bool {
                false
            }
        }
        impl<T> NotSend for Probe<T> {}
        impl<T: Send> Probe<T> {
            fn is_send(&self) -> bool {
                true
            }
        }

        trait NotSync {
            fn is_sync(&self) -> bool {
                false
            }
        }
        impl<T> NotSync for Probe<T> {}
        impl<T: Sync> Probe<T> {
            fn is_sync(&self) -> bool {
                true
            }
        }

        assert!(Probe::<String>(PhantomData).is_send());
        assert!(Probe::<String>(PhantomData).is_sync());

        assert!(!Probe::<CaptureWindow>(PhantomData).is_send());
        assert!(!Probe::<CaptureWindow>(PhantomData).is_sync());
    }
}
