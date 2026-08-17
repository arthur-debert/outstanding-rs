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
//! - **Nothing is collected unless someone asked.** Recording happens only
//!   inside a capture window — see below.
//!
//! # The capture window
//!
//! [`resolve_tags`] sits on every render path, including the standalone
//! [`Renderer`](crate::Renderer) and `render*` helpers a long-lived embedding
//! may call millions of times. So the collector is **off by default**:
//! [`record`] keeps nothing until [`begin_capture`] opens a window, and the
//! [`CaptureWindow`] guard it returns closes that window when it drops. A run
//! boundary holds one across dispatch, which both bounds the collector by one
//! run and stops one run's passes from contaminating the next. A caller outside
//! a window renders exactly as before, storing nothing.
//!
//! The window is a guard rather than a `begin`/`end` pair because the thing it
//! bounds can fail: a handler that panics between the two calls would leave the
//! window open forever, and every later run on that thread would then record
//! into a collector nothing ever closes — the unbounded growth the window
//! exists to prevent. Unwinding drops the guard.
//!
//! # Nesting
//!
//! Windows **nest, because runs do**: a handler is free to drive another app
//! through `run_to_string`, and that inner run opens a window of its own inside
//! the outer one. Each open window owns its own batch, so an inner run neither
//! clears what the outer has already recorded nor stops the outer from
//! recording once it closes.
//!
//! Closing an inner window publishes its batch to [`take_captured`] — that run
//! observed itself — *and* folds it into the enclosing window, one deeper on
//! [`TagResolution::nesting_depth`].
//!
//! The fold is **unconditional**, which is a choice worth stating because the
//! obvious alternative — fold only when the inner run's output actually reaches
//! the outer page — is not implementable here and would be the wrong default if
//! it were:
//!
//! - **This layer cannot see the difference.** An inner run hands its handler a
//!   `String`. Whether that string is rendered into the outer page, logged,
//!   compared and thrown away, or replaced is a fact about the handler's
//!   control flow, and nothing about the string carries it. Distinguishing the
//!   two would mean either tainting the returned value — a breaking change to
//!   the run entry points, to sharpen an oracle in a rare shape — or asking the
//!   app under test to declare when a child batch counts, which would make the
//!   oracle's correctness depend on the subject's cooperation. An oracle the
//!   code under test can opt out of is not an oracle.
//! - **The two errors are not symmetric.** Folding a discarded run's batch in
//!   over-reports: the failure names a tag that genuinely was rendered during
//!   the run, from a theme that genuinely lacks it, and it goes away by fixing
//!   that. Leaving an embedded run's batch out under-reports: the outer page
//!   carries corruption and the oracle says nothing. In `Text` mode — the mode
//!   this structured channel exists for — the embedded page keeps no evidence
//!   at all, so nothing downstream can recover the miss.
//!
//! A caller that genuinely wants only the observing run's own renders has the
//! provenance to do it: [`TagResolution::nesting_depth`] is `0` for those.
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
//! calling `parse`. The run boundary opens a window with [`begin_capture`]
//! before dispatch and holds the guard across it; dropping the guard ends that
//! run's batch, and [`take_captured`] then hands it to whoever is observing the
//! run:
//!
//! ```rust
//! use standout_bbparser::{TagTransform, UnknownTagBehavior};
//! use standout_render::diagnostics;
//! use std::collections::HashMap;
//!
//! let window = diagnostics::begin_capture();
//! let output = diagnostics::resolve_tags(
//!     "[nope]hi[/nope]",
//!     HashMap::new(),
//!     TagTransform::Remove,
//!     UnknownTagBehavior::Passthrough,
//! );
//! assert_eq!(output, "hi"); // Remove mode: no marker reaches the page…
//!
//! drop(window); // the run boundary ends the batch
//! let passes = diagnostics::take_captured();
//! // …but the pass still names the tag the theme did not define.
//! assert_eq!(passes[0].unresolved_tag_names(), ["nope"]);
//! ```
//!
//! Bind the guard to a name. `let _ = diagnostics::begin_capture();` drops it
//! on the spot and closes the window before anything renders into it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;

use console::Style;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior, UnknownTagError};

thread_local! {
    /// The capture windows open on this thread, innermost last — one batch of
    /// tag resolutions per window.
    ///
    /// Empty by default, and an empty stack *is* "not capturing": [`resolve_tags`]
    /// runs on every render, so a collector that recorded unconditionally would
    /// grow without bound in any embedding that renders outside a run boundary.
    /// A stack rather than a single batch because runs nest — see the module's
    /// nesting section.
    ///
    /// Thread-local for the same reason [`crate::warnings`] is: a CLI process
    /// is effectively single-threaded across the run boundary, so this avoids
    /// a mutex on the render path.
    static WINDOWS: RefCell<Vec<Vec<TagResolution>>> = const { RefCell::new(Vec::new()) };

    /// The batch ended by the most recently closed [`CaptureWindow`].
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
///
/// The parser's diagnostics are split in two, because they answer different
/// questions and only one of them is about the theme: a diagnostic whose tag
/// the theme does not define is [`unresolved`](Self::unresolved), and one
/// naming a tag the theme *does* define is [`malformed`](Self::malformed) —
/// markup the template got wrong (an unbalanced open, an unexpected close)
/// rather than a hole in the theme's vocabulary.
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
    ///
    /// These are exactly the diagnostics whose tag is absent from
    /// [`defined_tags`](Self::defined_tags); markup errors on a tag the theme
    /// does define are [`malformed`](Self::malformed) instead, so a caller
    /// reporting "the theme does not define this tag" can never name a tag it
    /// does.
    pub fn unresolved(&self) -> &[UnknownTagError] {
        &self.unresolved
    }

    /// Every markup error this pass met on a tag the theme *does* define.
    ///
    /// An unbalanced open (`[b]` with no `[/b]`) or an unexpected close
    /// (`[/b]` with no `[b]`) is a defect in the template, not in the theme.
    /// The parser reports both alongside unknown tags; this is where they are
    /// kept, so [`unresolved`](Self::unresolved) means one thing only.
    pub fn malformed(&self) -> &[UnknownTagError] {
        &self.malformed
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

    /// How many run boundaries this pass was folded outward across to reach the
    /// batch it is being read from.
    ///
    /// `0` means the run reading this batch rendered the pass itself. `1` means
    /// a run its handler drove rendered it, `2` a run that one drove, and so on.
    ///
    /// Every pass a run met is reported, at whatever depth it happened —
    /// including one from a nested run whose output the handler discarded, for
    /// the reasons in the module's nesting section. This is what to filter on
    /// when only the observing run's own renders are wanted:
    ///
    /// ```rust
    /// # use standout_render::TagResolution;
    /// # fn f(passes: &[TagResolution]) {
    /// let own = passes.iter().filter(|pass| pass.nesting_depth() == 0);
    /// # let _ = own;
    /// # }
    /// ```
    pub fn nesting_depth(&self) -> usize {
        self.nesting_depth
    }

    /// Whether every tag this pass met was defined in the resolved theme.
    ///
    /// This is a statement about the theme's vocabulary only. A pass that met
    /// [`malformed`](Self::malformed) markup on defined tags is clean by this
    /// measure, because the theme is not what is wrong with it.
    pub fn is_clean(&self) -> bool {
        self.unresolved.is_empty()
    }
}

/// Runs a style-tag pass, records what it resolved, and returns the output.
///
/// This is the render path's replacement for building a [`BBParser`] and
/// calling `parse`: byte-for-byte the same output, with the error list `parse`
/// discarded kept in the thread-local collector instead — and kept only while
/// a capture window is open, so a standalone render costs no memory.
pub fn resolve_tags(
    input: &str,
    styles: HashMap<String, Style>,
    transform: TagTransform,
    unknown_behavior: UnknownTagBehavior,
) -> String {
    let parser = BBParser::new(styles, transform).unknown_behavior(unknown_behavior);
    let (output, errors) = parser.parse_with_diagnostics(input);

    if !is_capturing() {
        return output;
    }

    // The parser reports two different failures through one vector: a tag the
    // theme has no style for, and markup it could not balance. Only the first
    // is what "unresolved" means, so the split is by whether the theme defines
    // the tag — otherwise a malformed `[b]` on a theme that *does* define `b`
    // would be reported as a missing tag, and listed under `defined_tags` in
    // the same breath.
    let (unresolved, malformed): (Vec<UnknownTagError>, Vec<UnknownTagError>) = errors
        .errors
        .into_iter()
        .partition(|error| !parser.styles().contains_key(&error.tag));

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
        // The window recording it is the one that rendered it; the fold
        // deepens it on the way out.
        nesting_depth: 0,
    });

    output
}

/// An open capture window, owning the batch recorded inside it.
///
/// Held by a run boundary across dispatch and closed by dropping it. Closing is
/// the whole of its behaviour, so there is no method to call: see [`Drop`] for
/// what closing does with the batch, and the module's nesting section for what
/// it does with the enclosing window.
///
/// Deliberately neither `Send` nor `Sync`. The window stack is thread-local, so
/// a guard dropped on another thread would pop a window it never opened. The
/// `PhantomData<*const ()>` field is what withholds both: a raw pointer is
/// itself `!Send` and `!Sync`, and an auto trait holds for a struct only when
/// it holds for every field. `the_guard_is_neither_send_nor_sync` pins it, so
/// this stays a compiler-checked property rather than a comment.
#[must_use = "the window closes when the guard drops; bind it across the run it bounds"]
pub struct CaptureWindow {
    _not_send: PhantomData<*const ()>,
}

impl Drop for CaptureWindow {
    /// Closes this window: its batch becomes the captured batch, and — if this
    /// window was nested inside another — is also folded into the enclosing
    /// one, a level deeper on [`TagResolution::nesting_depth`]. The enclosing
    /// window stays open and keeps recording.
    fn drop(&mut self) {
        let batch = WINDOWS.with(|windows| windows.borrow_mut().pop().unwrap_or_default());

        // Unconditionally, because this layer cannot tell an embedded child
        // render from a discarded one and the two errors are not symmetric —
        // see the module's nesting section. The depth is what a caller reads to
        // tell them apart afterwards. Cloning is paid only when runs actually
        // nest, which no ordinary CLI run does.
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

/// Opens a capture window on this thread, returning the guard that closes it.
///
/// The run boundary calls this before dispatch and holds the guard across it.
/// Outside a window nothing is recorded at all, which is what keeps
/// [`resolve_tags`] — a function on every render path, including the standalone
/// [`Renderer`](crate::Renderer) — from accumulating a record per render
/// forever in a long-lived process.
///
/// Opening a window inside another nests rather than replaces: the new window
/// starts empty, and the enclosing one keeps everything it has recorded so far.
pub fn begin_capture() -> CaptureWindow {
    WINDOWS.with(|windows| windows.borrow_mut().push(Vec::new()));
    CaptureWindow {
        _not_send: PhantomData,
    }
}

/// Whether a capture window is currently open on this thread.
pub fn is_capturing() -> bool {
    WINDOWS.with(|windows| !windows.borrow().is_empty())
}

/// Appends a tag resolution to the innermost open capture window.
///
/// A no-op outside a window opened by [`begin_capture`].
pub fn record(resolution: TagResolution) {
    WINDOWS.with(|windows| {
        if let Some(current) = windows.borrow_mut().last_mut() {
            current.push(resolution);
        }
    });
}

/// Removes and returns the tag resolutions recorded in the innermost open
/// window, leaving that window open.
///
/// Empty when no window is open. This is the observe-mid-run form; closing a
/// window is [`CaptureWindow`]'s job.
pub fn drain() -> Vec<TagResolution> {
    WINDOWS.with(|windows| {
        windows
            .borrow_mut()
            .last_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

/// Returns and clears the batch ended by the most recently closed
/// [`CaptureWindow`] on this thread.
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

    /// Abandons any window left open on this thread, clears the captured slot,
    /// and opens a fresh window, so a test starts from a known state regardless
    /// of what ran before it.
    fn reset() -> CaptureWindow {
        WINDOWS.with(|windows| windows.borrow_mut().clear());
        take_captured();
        begin_capture()
    }

    /// Every unresolved tag named by `batch`, pass by pass, in order.
    fn unresolved_across(batch: &[TagResolution]) -> Vec<&str> {
        batch
            .iter()
            .flat_map(TagResolution::unresolved_tag_names)
            .collect()
    }

    /// How far each pass in `batch` travelled to get there, in order.
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
        assert!(
            take_captured().is_empty(),
            "taking a captured batch clears it"
        );

        drop(begin_capture());
        assert!(
            take_captured().is_empty(),
            "a run that rendered nothing captures nothing"
        );
    }

    /// The regression the capture window exists for: `resolve_tags` is on every
    /// render path, so a standalone renderer outside a run would otherwise add
    /// a record per render, forever, in a long-lived process.
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

        assert!(!is_capturing(), "the window closed with the run");
        assert!(
            drain().is_empty(),
            "a render outside a capture window records nothing"
        );
    }

    /// …and the second half of that defect: those renders must not turn up in
    /// whatever run captures next on the same thread.
    #[test]
    fn a_render_before_a_run_cannot_contaminate_it() {
        drop(reset());
        take_captured();

        resolve_tags(
            "[stray]before the run[/stray]",
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );

        let window = begin_capture();
        resolve_tags(
            "[ok]during the run[/ok]",
            theme_with("ok"),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );
        drop(window);

        let captured = take_captured();
        assert_eq!(captured.len(), 1, "only the run's own pass is captured");
        assert!(captured[0].is_clean());
    }

    /// Markup the theme *does* define is a template defect, not a hole in the
    /// theme — and reporting it as one would blame a tag the same record lists
    /// as defined.
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
            assert!(
                passes[0].is_clean(),
                "{input:?}: the theme defines `ok`, so nothing is unresolved"
            );
            assert!(
                passes[0].defined_tags().is_empty(),
                "{input:?}: a pass with nothing unresolved names no vocabulary"
            );
            assert_eq!(
                passes[0]
                    .malformed()
                    .iter()
                    .map(|error| error.tag.as_str())
                    .collect::<Vec<_>>(),
                ["ok"],
                "{input:?}: the markup error is kept, under its own name"
            );
        }
    }

    /// The other side of the split: a tag the theme does not define is
    /// unresolved however malformed its markup also is.
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
        assert!(
            passes[0].malformed().is_empty(),
            "a tag the theme never defined is reported as missing, not as mis-nested"
        );
    }

    /// Renders one unresolvable tag by that name into the innermost window.
    fn render_unresolvable(tag: &str) {
        resolve_tags(
            &format!("[{tag}]x[/{tag}]"),
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Passthrough,
        );
    }

    /// Reentrancy, which a single on/off flag and one shared batch could not
    /// survive: a handler drives another app, so an inner window opens inside
    /// the outer one and closes while the outer run is still going. The inner
    /// window must not clear what the outer already recorded, must not turn
    /// recording off behind it, and must not overwrite the outer batch when the
    /// outer closes in turn.
    #[test]
    fn an_inner_window_neither_clears_nor_closes_the_outer_one() {
        let outer = reset();
        render_unresolvable("before_inner");

        let inner = begin_capture();
        render_unresolvable("inner");
        drop(inner);

        let inner_batch = take_captured();
        assert_eq!(
            unresolved_across(&inner_batch),
            ["inner"],
            "the inner run observes itself, and only itself"
        );
        assert!(
            is_capturing(),
            "the outer window is still open once the inner one closes"
        );

        render_unresolvable("after_inner");
        drop(outer);

        assert_eq!(
            unresolved_across(&take_captured()),
            ["before_inner", "inner", "after_inner"],
            "the outer batch keeps what it recorded on both sides of the nested \
             run, and accounts for the nested run's own passes in between"
        );
    }

    /// The same property at depth, because a handler that drives an app is not
    /// barred from driving one that does the same.
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
        assert!(!is_capturing(), "every window closed");
    }

    /// The window is a guard so that a handler panicking mid-run cannot leave
    /// it open: an abandoned window would keep collecting every later render on
    /// the thread, which is the unbounded growth the window exists to prevent.
    #[test]
    fn a_panic_inside_a_window_still_closes_it() {
        drop(reset());
        take_captured();

        let outcome = std::panic::catch_unwind(|| {
            let _window = begin_capture();
            render_unresolvable("during_the_panicking_run");
            panic!("the handler blew up");
        });

        assert!(outcome.is_err(), "the panic is not swallowed");
        assert!(!is_capturing(), "unwinding closed the window");
        assert_eq!(
            unresolved_across(&take_captured()),
            ["during_the_panicking_run"],
            "and the batch it had collected is still ended, not lost"
        );
    }

    /// Provenance survives the fold, which is what lets a caller that wants
    /// page scope rather than run scope filter for it.
    #[test]
    fn the_fold_records_how_far_a_pass_travelled() {
        let outer = reset();
        render_unresolvable("outer_own");

        let middle = begin_capture();
        render_unresolvable("middle_own");

        let inner = begin_capture();
        render_unresolvable("inner_own");
        drop(inner);
        assert_eq!(
            depths(&take_captured()),
            [0],
            "a run's own pass is at depth zero in its own batch"
        );

        drop(middle);
        assert_eq!(
            depths(&take_captured()),
            [0, 1],
            "the middle run's own pass, then the one it drove"
        );

        drop(outer);
        assert_eq!(
            depths(&take_captured()),
            [0, 1, 2],
            "depth accumulates with every boundary a pass is folded across"
        );
    }

    /// The fold is unconditional, and this is the case that costs: the handler
    /// throws the nested run's output away, so nothing it rendered reaches the
    /// enclosing page — and the enclosing batch reports it anyway.
    ///
    /// Deliberate, not an oversight. This layer sees a `String` handed back to a
    /// handler and cannot know whether it was embedded or dropped, so the choice
    /// is between two total policies. Reporting it names a tag that really was
    /// rendered from a theme that really lacks it; not reporting it would also
    /// silence the embedded case, where the enclosing page carries corruption
    /// and — in `Text` mode — keeps no evidence of it. The depth is how a caller
    /// that wants only its own page tells the two apart.
    #[test]
    fn a_discarded_nested_run_is_reported_by_the_enclosing_one_at_depth_one() {
        let outer = reset();

        let discarded = begin_capture();
        render_unresolvable("never_embedded");
        drop(discarded);
        take_captured(); // the handler throws the nested output away

        render_unresolvable("outer_own");
        drop(outer);

        let batch = take_captured();
        assert_eq!(
            unresolved_across(&batch),
            ["never_embedded", "outer_own"],
            "the enclosing run reports every pass rendered inside it"
        );
        assert_eq!(depths(&batch), [1, 0]);
        assert_eq!(
            unresolved_across(
                &batch
                    .iter()
                    .filter(|pass| pass.nesting_depth() == 0)
                    .cloned()
                    .collect::<Vec<_>>()
            ),
            ["outer_own"],
            "and page scope is one filter away for a caller that wants it"
        );
    }

    /// The guard's thread-affinity is a claim its docstring makes, so the
    /// compiler is made to check it: the window stack is thread-local, and a
    /// guard sent elsewhere would pop a window that thread never opened.
    ///
    /// `Probe`'s inherent method exists only when `T: Send`, and an inherent
    /// method wins over a trait method of the same name — so resolution lands on
    /// the trait's `false` exactly when the bound does not hold.
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

        assert!(
            Probe::<String>(PhantomData).is_send(),
            "the probe detects a Send type, so a false below means something"
        );
        assert!(Probe::<String>(PhantomData).is_sync());

        assert!(
            !Probe::<CaptureWindow>(PhantomData).is_send(),
            "a guard moved to another thread would pop a window it never opened"
        );
        assert!(!Probe::<CaptureWindow>(PhantomData).is_sync());
    }
}
