# Robustness: Composition Contracts

Third Spec in the **Robustness program** — the core redesign. Depends on the test net;
benefits from loud failures; feeds one-blessed-surface. This is the epic with genuine
design risk: its grill/ADR round is expected to be the program's most substantial.

## Context

Standout is deliberately a set of standalone leaf crates (`standout-render`,
`standout-input`, `standout-dispatch`, `standout-pipe`, `standout-bbparser`,
`standout-seeker`, macros) integrated by the `standout` crate as glue. The assessment
verified this architecture is *not* a root cause of the robustness problems: the
dependency graph is clean (leaves independent, `standout` the sole aggregator), and the
design-class bugs concentrate in the ~5–6k-line composition layer
(`crates/standout/src/cli`), not the leaves.

What failed is that the glue routes *around* the seams instead of through them:

- `standout-dispatch` exports `RenderFn`/`from_fn` — the designed pluggable-render seam —
  with **zero call sites**; the glue built its own `DispatchFn` instead.
- The glue re-implements leaf capability: `cli/builder/rendering.rs` carries byte-identical
  copies of `standout-render`'s structured serialization and combined-context assembly,
  with its own `serde_yaml`/`csv`/`quick-xml` dependencies.
- There are **four independent rendering pipelines** (dispatch via `Presentation::render`;
  public `render_inline`; help; topics). Help and topics construct a fresh
  `MiniJinjaEngine::new()` per call, have no structured-mode branch at all (`myapp help
  --output=json` renders stripped text), and carry hardcoded private themes. Adding an
  output mode touches ~9 files.
- Cross-crate coordination happens through **7+ process globals** (TTY, color, width,
  ambiguous-width, theme, icon, stdin/clipboard/responder overrides). Ambient state is an
  interface — untyped and untestable: 150 `#[serial]` annotations serialize the rendering
  test suite, and the ambiguous-width global silently *wins over* the value threaded
  through the dispatch signature (`cli/dispatch.rs:80-81`).
- Defaults are defined at point of use, repeatedly: "the theme when none is set" has five
  different answers; "the output mode when none is given" about ten.
- The recipe/config trait split in `cli/group.rs` maintains four near-identical ~30-line
  dispatch closures — the surviving form of the old App/LocalApp duplication.

The pattern reads as agent-session drift: when a seam was inconvenient or unknown to a
session, the session duplicated or reached for a global, and nothing structural stopped
it. The crates forced decoupling of code; nothing forced decoupling of behavior through
contracts.

## Problem

Because the seams are informal, every cross-cutting concern (a new output mode, a color
decision, a theme default) must be implemented in several places that can — and do —
drift; features interact through ambient state whose combinations cannot be enumerated or
tested; and standalone-crate use and framework use are different code paths with different
bugs. The combinatorial testing problem the framework faces is unsolvable as stated; it
becomes tractable only if each leaf is a pure function of an explicit input and the glue
is thin enough to only *build* those inputs.

## Goals

- **One boundary object per seam.** Each leaf crate's entry point takes an explicit
  request/context value and behaves as a pure function of it. For rendering, the shape to
  design (grill decides the exact form): data, typed template reference, resolved theme,
  output mode, and target facts (tty, width, color, ambiguous-width policy). No leaf reads
  a process global to answer a question its input can carry.
- **One rendering pipeline.** Help and topics become ordinary registry templates rendered
  through the same path as dispatch — the app's engine and filters, theme merging, and the
  structured-mode branch all become reachable for help/topics instead of being absent by
  construction. `render_inline` becomes a convenience wrapper over the same path.
  **Structured help output stays unexposed here**: this epic removes the structural
  obstacle, but `help --output=json` is not published as a user-facing surface until the
  machine-contract Spec defines and versions the help envelope. Shipping it earlier would
  publish an unversioned public format that the very next program must break, contradicting
  that Spec's "framework-owned envelopes are versioned from day one." The grill picks the
  mechanism for holding it back (falling back to human rendering for help under structured
  modes, or an unstable-surface gate) and records it in the ownership table.
- **One resolution point.** `build()` produces a `ResolvedConfig` (working name): theme
  merged once, output-mode default defined once, template registry finalized once. The
  five theme defaults and ~ten output-mode defaults are deleted in favor of references to
  it.
- **Globals become injected context.** Detector overrides survive only as *inputs* to
  building the boundary object (a detection step at the edge of `run()`), not as ambient
  reads inside leaves. The `#[serial]` count drops accordingly; contradictions like the
  ambiguous-width global-beats-parameter bug become unrepresentable.
- **The glue is thin by rule, mechanically enforced**: `standout` carries no serializer
  dependencies, constructs no engine outside `build()`, defines no rendering defaults —
  each rule backed by a CI lint/test so a future session cannot quietly regress it (the
  `28e127b` lesson).
- **Standalone use and framework use converge**: a standalone `standout-render` user
  builds the boundary object by hand; the framework builds it from conventions; same
  object, same pipeline, one contract. Standalone usability is a stated deliverable, not
  a side effect.
- The recipe/config closure duplication collapses to one implementation.

## Non-Goals

- Reducing the public API surface or choosing blessed idioms — one-blessed-surface Spec.
  (This epic may *add* the boundary types; it deletes only internals.)
- New capabilities (color tri-state flag, structured errors, config) — parity program.
  **But**: the boundary object's target-facts design must not conflate format with color —
  the parity program's `--color` axis and machine-contract error emission plug into the
  seams this epic defines, and the grill must check the shapes against those Specs.
- Changing help's *semantics* or its human presentation. The unification changes internal
  reachability — help and topics gain access to the pipeline's capabilities — and
  externally observable help stays human-rendered under every mode until the
  machine-contract epic exposes the versioned structured envelope. Byte-level deltas from
  engine differences are *possible but not licensed*: the snapshot matrix surfaces each
  one, and every delta must be either justified in review as an improvement standing on
  its own or fixed so the bytes match. "It changed because the pipeline changed" is not an
  acceptance rationale. (The theme merge that loud-failures defines is the one delta
  already accounted for.)
- Renaming or splitting crates.

## Proposed Shape

Inside-out, in four moves:

**1. Define the contracts.** The grill/ADR round designs the boundary objects (render
request, input resolution context, dispatch presentation) and the ownership rule for each
fact (who detects TTY, who merges themes, who defaults the mode — each exactly once).
Written as ADRs before implementation; this is the program's heaviest design step.

**2. Make `standout-render` a pure function of its request.** Internalize the detector
reads into an explicit detection step that callers invoke at the edge; thread the request
through template functions, tabular, and width; delete the duplicate serializer/context
copies in the glue by making the leaf's the only one.

**3. Re-route the glue.** `build()` → `ResolvedConfig`; dispatch, `render_inline`, help,
and topics all construct render requests and call the one pipeline; the four pipelines
and the recipe/config closure duplicates collapse; `Box::leak`-per-parse and the
one-shot builder panics get resolved by the same restructuring where they fall out of it.
`standout-dispatch`'s dead seam (`RenderFn`) is either adopted as the real interface or
deleted in favor of the new one — the grill decides; two render seams may not survive.

**4. Enforce thinness.** The CI lints/tests from Goals land last and lock the shape.

Throughout, the test net's matrix snapshots gate each move; `#[serial]` removal is the
observable byproduct metric.

## User / Agent Stories

1. As a maintainer adding an output mode, I want the change to touch the mode enum and the
   serializer registry only, so that the current ~9-file blast radius becomes ~2.
2. As a standalone `standout-render` user, I want to render with an explicit request
   object and no ambient state, so that my GUI/server use of the crate is deterministic
   and test-parallel.
3. As a maintainer, I want help and topics to render through the same pipeline as every
   other command, so that a capability added to rendering reaches them by construction
   instead of needing a fourth implementation (structured help output itself ships,
   versioned, in the machine-contract epic).
4. As a framework test author, I want rendering tests to run in parallel without
   `#[serial]`, so that the suite is fast and cross-test contamination is impossible.
5. As a future agent session, I want a lint to reject a serializer dependency or a fresh
   engine construction in the glue, so that I cannot repeat the drift that created the
   four pipelines.
6. As the parity program's implementer, I want color/verbosity/error-emission decisions to
   have exactly one home in the boundary objects, so that those features land without
   re-opening this redesign.

## Risks And Rabbit Holes

- **This epic is the redesign** — the place where over-abstraction is most tempting. The
  boundary objects encode facts that already exist; they do not grow capability. Any
  field without a current consumer is cut.
- **Help-through-the-pipeline can perturb bytes.** Engine differences may shift help
  output even when semantics are preserved. The snapshot matrix must be in place *first*,
  and per the Non-Goal above each delta is then justified-or-fixed rather than waved
  through — otherwise this epic silently becomes a rendering change.
- **Globals have hidden clients.** `standout-input`'s overrides and the warning
  thread-locals serve the harness; removing globals must land with the harness's
  replacement injection path in the same workstream, or the test suite breaks unfixably
  mid-epic.
- **Sequencing pressure to skip the ADR round.** Steps 2–3 are large mechanical diffs
  that agents can produce quickly once shapes are fixed — and expensively wrong if shapes
  are fixed mid-flight. The grill is the cheapest part of this epic; do not let momentum
  skip it.
- **`ResolvedConfig` scope creep** into a god-object: it resolves what `build()` can know;
  per-invocation facts (argv-selected mode, detected width) stay in the request, not in it.

## Cross-Cutting Concerns

- Performance: request objects are built per dispatch — keep them cheap
  (`Rc`/borrowed views); the render lock (`width.rs`) should become unnecessary and be
  removed, not worked around.
- Concurrency posture: the framework is single-threaded by design (#84 history); the
  contracts should stop *pretending* otherwise (Arc/atomic globals guarding
  single-threaded state).
- Release: breaking for standalone-crate users (new entry-point signatures); changelog
  documents the by-hand construction path per crate.
- Docs: each leaf crate's standalone usage guide is updated to the request-object idiom —
  this is the "partial adoption" story the docs feedback asked to make prominent.

## Testing / Verification

The matrix snapshots pin behavior across the refactor. New: per-leaf contract tests
(same request → same output, no environment sensitivity — assertable by running the same
request under perturbed env/globals); a differential test that dispatch-path,
`render_inline`-path, and help-path rendering of the same request agree; the thinness
lints; `#[serial]` census before/after as a tracked metric.

## Workstream Hints

(1) ADR round (contracts + ownership table) — gate for everything; (2) render as pure
function + detection step + harness injection (walking skeleton: one command renders
through the new seam end to end); (3) `ResolvedConfig` + defaults deletion; (4) help +
topics onto the pipeline; (5) `render_inline` + recipe/config collapse + dead-seam
resolution; (6) thinness enforcement + `#[serial]` sweep. Roughly sequential; (4)/(5)
parallelize.

## Out Of Scope

API pruning, new user-facing capabilities, crate reorganization, multi-threaded App.

## Further Notes

The program's ADR-heavy epic; expected ADRs: boundary-object shapes; fact-ownership table
(one owner per fact); the fate of `standout-dispatch::RenderFn`; glue-thinness rules as
enforceable invariants. Links to be added by the grill. The parity Specs
(`parity-machine-contract.md`, `parity-terminal-citizenship.md`) name the seams they
expect this epic to leave open — read together before the grill.
