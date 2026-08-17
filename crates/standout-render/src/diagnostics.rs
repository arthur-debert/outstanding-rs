//! What the style-tag pass observed, carried outward instead of discarded.
//!
//! Every rendered page goes through a second pass in which [`BBParser`]
//! resolves `[tag]` markup against the theme's resolved styles. That pass
//! already computes which tags the theme could not resolve — the parser
//! records an [`UnknownTagError`] for each one *before* it decides what to
//! emit, so the diagnostic exists identically under
//! [`TagTransform::Apply`], [`Keep`](TagTransform::Keep), and
//! [`Remove`](TagTransform::Remove), with no dependency on output mode, ANSI,
//! or a TTY. Until this module existed the render path threw it away: the
//! framework computed "this page is corrupt" on every render and dropped it on
//! the floor.
//!
//! This module routes that result through a thread-local collector, mirroring
//! [`crate::warnings`]. The collector is the *only* thing it does:
//!
//! - **Nothing here changes a rendered byte.** [`resolve_tags`] returns exactly
//!   what `BBParser::parse` returned before; it simply keeps the error list
//!   that `parse` discarded.
//! - **Nothing here reacts.** The framework does not warn, error, or degrade on
//!   an unresolved tag because this module saw it. Surfacing is not reacting;
//!   what the framework should *do* about a corrupt page is the loud-failures
//!   Spec's decision.
//!
//! # Consumer
//!
//! The consumer is the invariant assertion library in `standout-test`, which
//! needs to state "every tag this page emitted is defined in the resolved
//! theme" as a fact about structured data rather than as a search for the
//! `[tag?]` marker in rendered text. The marker is a symptom: it only appears
//! under `TagTransform::Apply`, it is absent from the very modes most help
//! tests run in, and finding it means substring-matching a page instead of
//! naming a tag. A [`TagResolution`] names the tag in every mode.
//!
//! # Usage
//!
//! Render paths call [`resolve_tags`] in place of building a [`BBParser`] and
//! calling `parse`. The run boundary calls [`capture_for_run`], after which
//! [`take_captured`] hands the batch to whoever is observing the run:
//!
//! ```rust
//! use standout_bbparser::{TagTransform, UnknownTagBehavior};
//! use standout_render::diagnostics;
//! use std::collections::HashMap;
//!
//! let output = diagnostics::resolve_tags(
//!     "[nope]hi[/nope]",
//!     HashMap::new(),
//!     TagTransform::Remove,
//!     UnknownTagBehavior::Passthrough,
//! );
//! assert_eq!(output, "hi"); // Remove mode: no marker reaches the page…
//!
//! diagnostics::capture_for_run();
//! let passes = diagnostics::take_captured();
//! // …but the pass still names the tag the theme did not define.
//! assert_eq!(passes[0].unresolved_tag_names(), ["nope"]);
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use console::Style;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior, UnknownTagError};

thread_local! {
    /// Tag resolutions recorded by the current run on this thread.
    ///
    /// Thread-local for the same reason [`crate::warnings`] is: a CLI process
    /// is effectively single-threaded across the run boundary, so this avoids
    /// a mutex on the render path.
    static RESOLUTIONS: RefCell<Vec<TagResolution>> = const { RefCell::new(Vec::new()) };

    /// The batch ended by the most recent [`capture_for_run`] call.
    static CAPTURED: RefCell<Vec<TagResolution>> = const { RefCell::new(Vec::new()) };
}

/// What one style-tag pass resolved, and what it could not.
///
/// One of these is recorded per rendered page — or per rendered fragment, for a
/// render that composes several. The permutation that produced it is part of
/// the record, because "no unresolved tag" means something different under each
/// combination: [`TagTransform`] says whether tags became ANSI, stayed as
/// brackets, or vanished, and [`UnknownTagBehavior`] says what an unresolved
/// tag did to the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagResolution {
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
    unresolved: Vec<UnknownTagError>,
    defined_tags: Vec<String>,
}

impl TagResolution {
    /// The tag transform this pass applied.
    pub fn transform(&self) -> TagTransform {
        self.transform
    }

    /// What this pass did with a tag the theme did not define.
    pub fn unknown_behavior(&self) -> UnknownTagBehavior {
        self.unknown_behavior
    }

    /// Every tag this pass could not resolve, in the order encountered.
    ///
    /// A tag used as a matched pair and unresolved appears twice — once for
    /// the open, once for the close — since each carries its own position.
    pub fn unresolved(&self) -> &[UnknownTagError] {
        &self.unresolved
    }

    /// The distinct names of the tags this pass could not resolve.
    ///
    /// This is the failure-message form: one entry per offending tag, open and
    /// close collapsed, in first-seen order.
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

    /// The tag vocabulary the resolved theme offered this pass, sorted.
    ///
    /// Recorded **only when the pass left something unresolved**, because that
    /// is the only time anyone reads it: a failure message that says which tag
    /// was missing is much more useful when it can also say what was on offer.
    /// A clean pass returns an empty slice rather than paying to clone the
    /// theme's key set on every render.
    pub fn defined_tags(&self) -> &[String] {
        &self.defined_tags
    }

    /// Whether every tag this pass met was defined in the resolved theme.
    pub fn is_clean(&self) -> bool {
        self.unresolved.is_empty()
    }
}

/// Runs a style-tag pass, records what it resolved, and returns the output.
///
/// This is the render path's replacement for building a [`BBParser`] and
/// calling `parse`: byte-for-byte the same output, with the error list `parse`
/// discarded kept in the thread-local collector instead.
pub fn resolve_tags(
    input: &str,
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
) -> String {
    let parser = BBParser::new(styles, transform).unknown_behavior(unknown_behavior);
    let (output, errors) = parser.parse_with_diagnostics(input);

    let unresolved = errors.errors;
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
        defined_tags,
    });

    output
}

/// Appends a tag resolution to the thread-local collector.
pub fn record(resolution: TagResolution) {
    RESOLUTIONS.with(|r| r.borrow_mut().push(resolution));
}

/// Removes and returns the tag resolutions collected on this thread.
pub fn drain() -> Vec<TagResolution> {
    RESOLUTIONS.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// Ends the current run's batch, moving it to the captured slot.
///
/// The run boundary calls this — both the printing and the capturing entry
/// point — so a run's resolutions are bounded by that run and cannot bleed
/// into the next one on the same thread. The next call replaces the previous
/// captured batch.
pub fn capture_for_run() {
    let resolutions = drain();
    CAPTURED.with(|captured| {
        *captured.borrow_mut() = resolutions;
    });
}

/// Returns and clears the batch ended by the most recent [`capture_for_run`].
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

    /// Clears both slots so a test starts from a known state regardless of
    /// what ran before it on this thread.
    fn reset() {
        drain();
        take_captured();
    }

    #[test]
    fn a_clean_pass_is_recorded_and_names_nothing() {
        reset();
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
        assert!(
            passes[0].defined_tags().is_empty(),
            "a clean pass does not pay to clone the theme's vocabulary"
        );
    }

    /// The whole point of the structured channel: the tag is named in the modes
    /// where no marker reaches the page.
    #[test]
    fn an_unresolved_tag_is_named_in_every_transform() {
        for transform in [
            TagTransform::Apply,
            TagTransform::Keep,
            TagTransform::Remove,
        ] {
            reset();
            let output = resolve_tags(
                "[nope]hi[/nope]",
                theme_with("ok"),
                transform,
                UnknownTagBehavior::Passthrough,
            );

            let passes = drain();
            assert_eq!(passes[0].unresolved_tag_names(), ["nope"], "{transform:?}");
            assert_eq!(passes[0].transform(), transform);
            assert!(
                passes[0].defined_tags().contains(&"ok".to_string()),
                "a failing pass reports the vocabulary that was on offer"
            );
            if transform == TagTransform::Remove {
                assert_eq!(output, "hi", "no marker reaches the page in Remove mode");
            }
        }
    }

    #[test]
    fn the_strip_behavior_is_recorded_alongside_the_tags() {
        reset();
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
    fn capture_ends_the_batch_so_runs_cannot_bleed_together() {
        reset();
        resolve_tags(
            "[nope]hi[/nope]",
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );
        capture_for_run();

        let first = take_captured();
        assert_eq!(first.len(), 1);
        assert!(
            take_captured().is_empty(),
            "taking a captured batch clears it"
        );

        capture_for_run();
        assert!(
            take_captured().is_empty(),
            "a run that rendered nothing captures nothing"
        );
    }
}
