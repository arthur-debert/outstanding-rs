# Robustness: Composition Contracts

Third Spec in the **Robustness program** — the core redesign. Depends on the test net
(`docs/spec/robustness-test-net.md`); benefits from loud failures
(`docs/spec/robustness-loud-failures.md`); feeds one-blessed-surface and the parity
program. ROB01, ROB02, ROB03, and ROC02 (Corpus Cleanup) have landed; this Spec is the
*why and what* against that code, not the 8.1.0 assessment.

Interface and behaviour live here. Internal type layouts, which function computes each
fact at the process edge, and how the crate invariants are checked are ADRs — listed in
Further Notes, written by the grill that follows.

## Context

Standout is a set of standalone leaf crates (`standout-render`, `standout-input`,
`standout-dispatch`, `standout-pipe`, `standout-bbparser`, `standout-seeker`, macros)
integrated by the `standout` crate as glue. The assessment verified this architecture is
*not* a root cause of the robustness problems: the dependency graph is clean (leaves
independent, `standout` the sole aggregator), and the design-class bugs concentrate in
the ~5–6k-line composition layer (`crates/standout/src/cli`), not the leaves.

Predecessors already closed several of the original defects:

- A command's template is a `TemplateRef` — named, inline, or typed absence — resolved
  at render through the retained registry ([ADR-0019](../adr/0019-carry-a-template-as-a-typed-reference-resolved-at-render.md)).
- `build()` computes one theme: framework base plus the application's merge
  ([ADR-0020](../adr/0020-resolve-one-theme-at-build-over-a-single-framework-base.md)).
- `AppBuilder` configures; `build(self)` returns `App`, which runs
  ([ADR-0021](../adr/0021-split-the-configuring-builder-from-the-executable-app.md)).
- The in-process TTY detector is gone. Terminal-dependent behaviour is evidence a real
  process produces; any later terminal facts must be stream-aware (stdout and stderr
  independently), because a single `is_tty` is the shape that already failed
  ([ADR-0022](../adr/0022-delete-the-in-process-tty-seam.md)).
- `standout-dispatch`'s unused `RenderFn` / `from_fn` render-callback API was deleted in
  ROB02 rather than adopted.

What remains is that the glue still routes *around* the leaves instead of through them:

- The glue re-implements leaf capability: `cli/builder/rendering.rs` copies
  `standout-render`'s structured serialization and combined-context assembly, and
  `crates/standout/Cargo.toml` depends on `serde_yaml`, `csv`, and `quick-xml` itself.
- There are **four independent rendering pipelines** (dispatch via `Presentation::render`;
  public `App::render` / `render_inline`; help; topics). Help and topics still call
  `render_with_output`, which constructs a fresh `MiniJinjaEngine` and has no structured
  branch (`myapp help --output=json` renders stripped text). Adding an output mode still
  touches many files.
- Cross-crate coordination still happens through process globals: color capability,
  width, ambiguous-width, color-scheme, icon mode, and `standout-input`'s
  stdin / clipboard / responder overrides. Help still re-merges a theme on its own path.
  `Presentation::render` still lets the ambiguous-width global win over the value already
  on the dispatch signature. In-process rendering tests take `#[serial]` because those
  globals are shared.
- Output-mode defaults are still defined at the point of use. The recipe/config split in
  `cli/group.rs` still maintains near-identical dispatch closures.

The pattern is unchanged: when a leaf entry was inconvenient or unknown to a session, the
session duplicated it or reached for a global, and nothing structural stopped it. The
crates forced decoupling of code; nothing forced decoupling of behaviour through
contracts.

## Problem

Because the contracts are informal, every cross-cutting concern (a new output mode, a
color decision, a theme default) must be implemented in several places that can — and do
— drift; features interact through ambient state whose combinations cannot be enumerated
or tested; and standalone-crate use and framework use are different code paths with
different bugs. The combinatorial testing problem is unsolvable as stated. It becomes
tractable only if each leaf is a pure function of an explicit input and the glue is thin
enough to only *build* those inputs.

## Goals

### Standalone crate author (`standout-render`, `standout-input`)

- **Render is a pure function of an explicit request.** The leaf does not read
  framework-owned detectors or process globals to answer a question the request can
  carry. Changing env, cwd, or those globals does not change the result. Hot-reloaded
  file-backed templates (ADR-0019) and application context-provider callbacks on a
  `ContextRegistry` are explicit external dependencies of the request: calling the leaf
  twice with the same request can yield different bytes if those dependencies changed.
  "Same request, same bytes" holds only when those external dependencies are held fixed.
- **The request carries these facts** (the facts a caller passes, not a Rust type):
  - data
  - template (already a `TemplateRef`)
  - resolved theme
  - **format** (`OutputMode`) and a **color policy** as separate facts — later `--color`
    must have a home that is not `--output`
  - **stdout color capability** and **stderr color capability** independently — primary
    render uses stdout; warnings and progress use stderr
  - **stdout-is-a-terminal** and **stderr-is-a-terminal** independently — later pager
    depends on one, progress on the other
  - width
  - ambiguous-width policy
  - color-scheme (light/dark)
  - icon mode
- **Detection lives at the caller's edge, not inside the leaf.** A detect helper may
  exist for callers who want "ask the process, then render." Template functions, tabular,
  and width never call it.
- **Today's convenience functions stay** (`render`, `render_with_output`, and siblings).
  Each is a thin wrapper at the crate edge: detect, build a request, call
  `render_request`. They do not share that function's name: Rust has no free-function
  overloading. They do not detect *inside* the leaf. Blessed-surface may later delete
  wrappers; this epic does not.
- **Input sources are explicit.** Stdin, clipboard, and the prompt responder are
  arguments to input collection, not process-global overrides. The same purity rule as
  render (no framework-owned detector or process-global reads).
- **Standalone help stays callable without an `App`.** `render_help` and `render_topic`
  remain public, as do `HelpConfig::template` and the topic template fields. They build
  a request (defaults for facts they are not given; custom template strings as
  `TemplateRef::Inline` with tag validation at construction) and call `render_request`.
  Blessed-surface may later delete them; this epic does not.

### App author (framework)

- **`run()` detects once at the process edge** and builds the request. App authors do not
  pass target facts in production.
- **After `build()`, each configured fallback has one answer** — theme (already,
  ADR-0020), output-mode fallback, template registry. These are build-time configured
  *fallbacks*, stored on `App`: the one place glue is allowed to have a default. They
  are not the final per-invocation value. Point-of-use fallbacks (`unwrap_or_default()`,
  silent 80-column width, "Auto because this call site forgot") go away. Per-invocation
  resolution is per setting family, not one ladder. Terminal settings (color, pager,
  stream facts) keep `flag > env > config > detection`, with the App fallback at the
  default end — config does not override env conventions such as `NO_COLOR`. Output
  mode and theme resolve as flag > later config > App fallback; they have no detection
  source. Resolved values land on the request. `App` does not hold per-invocation
  facts. This Spec does not require a public type for the fallback bundle.
- **`App::render` / `render_inline` are the same pipeline as dispatch.** All three call
  `render_request`.
- **Help and topics are ordinary renders** through that pipeline (the app's engine,
  filters, and merged theme). Framework defaults and build-time app overrides are named
  registry templates. Standalone `render_help` / `render_topic` and public
  `HelpConfig::template` / `TopicRenderConfig` template strings stay, carried as
  `TemplateRef::Inline` with equivalent tag validation at request construction. A
  rendering capability added later reaches them without a fourth implementation.
- **No new user-facing flags or modes.** `--output` meaning does not change. `--color`,
  pager, progress, verbosity, and config files belong to later Specs.

### Person running a standout CLI

- **Command output bytes do not change**, except where a delta is justified on its own.
  The ROB01 snapshot matrix records current output and fails on unexpected deltas. "It
  changed because the pipeline changed" is not an accepted reason. The theme merge from
  loud failures is the one delta already accounted for.
- **`help --output=json` (and yaml/csv/xml) still prints human help.** Structured help is
  not a public format until the machine-contract Spec defines and versions the envelope.
  Same for topics.
- **Errors stay prose.** There is one place later epics will emit from; leaves and help
  do not grow their own. This epic does not change the text or the stream.

### Test author

- **The harness injects facts; it does not install process globals** for width, color,
  ambiguous-width, stdin, clipboard, responder, or theme/icon detectors. The detector
  override APIs (`set_terminal_width_detector`, `set_color_capability_detector`,
  `set_theme_detector`, `set_icon_detector`, `set_default_stdin_reader`, and siblings)
  go away. Callers who need non-default facts pass them on the request; tests do that
  through `TestHarness`.
- **In-process rendering and input tests do not need `#[serial]`** for those reasons.
  `run_process` was already parallel; this is the in-process half.
- **Warning capture is part of the run result / harness API**, not a thread-local the
  test author has to know about.
- **Contract tests exist:** same request plus perturbed env/globals → identical output
  when file-backed templates and context providers are held fixed; dispatch vs
  `render_inline` vs help-path on the same request agree; piped-stdout / TTY-stderr
  leaves warning color intact.

### Later epics (sockets this work must leave)

These are not built here. The request and the one emission point must have a place for
them so those Specs do not reopen this redesign.

- **Color axis** reads the color-policy fact, not `OutputMode`. It composes with the
  per-stream color capabilities already on `TargetProperties` and keeps
  `flag > env > config > detection` (so `NO_COLOR` beats config). Primary render
  consumes stdout color capability / stdout-is-a-terminal; warnings and progress
  consume stderr color capability / stderr-is-a-terminal.
- **Pager** reads stdout-is-a-terminal; **progress** reads stderr-is-a-terminal and must
  be able to emit into a future machine-event channel without inventing a second one.
- **Config layering** is one input to per-invocation resolution, not a second path
  beside glue-invented defaults, and not a universal ladder. Terminal settings keep
  `flag > env > config > detection` (App fallback at the default end); env conventions
  such as `NO_COLOR` beat config. Output mode and theme resolve as flag > later config
  > App fallback, with no detection source. Resolved values land on the request;
  per-invocation facts do not land on `App`.
- **Machine-contract diagnostics** emit at the single emission point, including a
  parse-independent look at `--output` (that look is *their* Spec; this epic only must
  not scatter emission).

### Maintainer / future agent

- **`standout` (the glue crate) carries no serializer dependencies.** `serde_yaml`,
  `csv`, and `quick-xml` live in `standout-render` only.
- **No `MiniJinjaEngine::new()` outside `build()`.**
- **The glue does not invent a rendering default.** It builds the request from the
  build-time fallbacks on `App` plus this invocation's flags, env, later config (when
  those epics land), and detection (terminal facts only); it does not pick a theme,
  mode, or width of its own at the point of use. It does not apply one ladder to every
  setting.
- **Adding an output mode touches the mode enum and the serializer registry**, not nine
  files.

Those four are crate-level behaviour of this repository. How they are checked (a
dependency test, `cargo deny`, a compile-fail crate) is an ADR.

## Non-Goals

- Reducing the public API or choosing blessed idioms — one-blessed-surface Spec. This
  epic may *add* the request-taking entries; it deletes internals and the detector
  override APIs that the request replaces. It does not delete convenience wrappers,
  `render_help`, `render_topic`, or the public `HelpConfig::template` / topic template
  fields.
- New capabilities (color tri-state flag, structured errors, config, pager, progress,
  verbosity) — parity program.
- Changing help's *semantics* or its human presentation, other than the already-accounted
  theme merge.
- Publishing structured help or topics under `--output json|yaml|csv|xml`.
- Renaming or splitting crates.
- A public `ResolvedConfig` (or equivalent) type, unless a caller in the Goals above
  needs to name it. `App` after `build()` holds the configured fallbacks, not
  per-invocation resolved mode, theme, or target properties.
- Re-opening ADR-0019–0022 or restoring `RenderFn`.
- Multi-threaded `App`.

## Proposed Shape

Four moves, each an observable change rather than a type:

**1. Name the facts and the purity rule.** Every leaf entry takes an explicit request
carrying the facts listed above. The pure render entry is `render_request`. Detection,
when it happens, happens at the caller's edge: `run()` for a framework app, the
convenience wrapper or detect helper for a standalone user, `TestHarness` for a test.
Template functions, tabular, width, and input collectors never read a detector. The
leaf does not read framework-owned detectors or process globals; file-backed templates
and context-provider callbacks are external dependencies of the request.

**2. One pipeline in `standout-render`.** Serialization, combined-context assembly, and
engine construction live in the leaf. The glue's copies and serializer dependencies go
away. `App::render`, `render_inline`, help, and topics each build a request and call
`render_request`. Help and topics remain human-rendered under structured `--output`.

**3. Build-time fallbacks on `App`, per-invocation resolution on the request.** `App`
holds what `build()` can know as fallbacks (merged theme, output-mode fallback,
registry). Per-invocation facts (argv-selected mode, detected width, stream
terminal-ness and per-stream color capability) belong on the request, not on `App`.
Resolution is per setting family: terminal settings keep `flag > env > config >
detection` (App fallback at the default end); output mode and theme are flag >
later config > App fallback, with no detection source. Point-of-use defaults
disappear.

**4. Harness injection and crate invariants.** `TestHarness` puts facts on the request.
The detector override APIs are removed. The four maintainer invariants (no serializers
in glue, no engine outside `build()`, no glue-invented defaults, output-mode changes
touch two places) become checks that fail CI.

The ROB01 snapshot matrix is the oracle for byte-level deltas on every move.

## User / Agent Stories

1. As a standalone `standout-render` user, I want to render with an explicit request and
   no ambient state, so that GUI and server use does not read process detectors and is
   test-parallel.
2. As a standalone `standout-render` user, I still want `render(template, data, theme)`
   to work, so that the crate's documented quick start keeps compiling while it becomes a
   detect-then-call wrapper over `render_request`.
3. As an application author, I want `run()` to detect the process once, so that I do not
   pass terminal facts in production.
4. As an application author, I want `App::render`, dispatch, help, and topics to mean
   the same pipeline, so that a capability added to rendering reaches every one of them.
5. As a person running a standout CLI, I want `help --output=json` to keep printing human
   help, so that this epic does not publish an unversioned machine format the next
   program would have to break.
6. As a test author, I want to inject width, color, stdin, and warnings through
   `TestHarness` without process globals, so that in-process tests run in parallel and
   cannot contaminate each other.
7. As a maintainer adding an output mode, I want the change to touch the mode enum and
   the serializer registry only, so that the current many-file edit becomes two files.
8. As a future agent session, I want CI to reject a serializer dependency or a fresh
   engine construction in the glue, so that I cannot repeat the drift that created the
   four pipelines.
9. As the parity program's implementer, I want format, color policy, per-stream color
   capability, stdout-terminal, stderr-terminal, and a single error-emission point
   already named, so that `--color`, pager, progress, and structured diagnostics land
   without reopening this redesign.

## Risks And Rabbit Holes

- **This epic is the redesign** — the place where over-abstraction is most tempting. The
  request encodes facts that already exist; it does not grow capability. Any fact without
  a current consumer, or without a named later-epic consumer in Goals, is cut.
- **Help through the pipeline can change bytes.** Engine differences may shift help
  output even when semantics are preserved. The snapshot matrix must be in place first
  (it is, ROB01), and every delta is then justified on its own or fixed. Otherwise this
  epic silently becomes a rendering change.
- **Globals have hidden clients.** `standout-input`'s overrides and the warning
  thread-locals serve the harness. Removing globals must land with the harness's
  replacement injection in the same workstream, or the test suite breaks mid-epic with
  no way to fix it from the remaining tests.
- **Putting per-invocation facts on `App`.** `App` holds build-time configured
  fallbacks. Argv-selected mode, detected width, stream terminal-ness, and per-stream
  color capability belong on the request. Folding them into `App` recreates a
  god-object the next epic cannot extend.
- **Skipping the ADR round.** The mechanical diffs (one pipeline, help onto it, globals
  out) are cheap once the internal types and ownership are fixed, and expensive if those
  are invented mid-flight. The grill is the cheap part of this epic.

## Cross-Cutting Concerns

- Performance: a request is built per dispatch — keep it cheap. The width lock in
  `standout-render` should become unnecessary once width is on the request, and be
  removed rather than worked around. (How the request is stored — owned, borrowed, `Rc`
  — is an ADR.)
- Concurrency: the framework is single-threaded by design (#84). Contracts should stop
  pretending otherwise (Arc/atomic globals guarding single-threaded state).
- Release: breaking for callers of the detector override APIs and for standalone crate
  users who constructed ambient state. Changelog documents the request-taking path and
  the remaining convenience wrappers per crate.
- Docs: each leaf crate's standalone usage guide teaches `render_request` as the
  contract and the convenience wrappers as detect-then-call. This is the
  partial-adoption path the docs feedback asked to make prominent.

## Testing / Verification

The ROB01 snapshot matrix records behaviour across the refactor. New tests, by module:

- **`standout-render`:** same request with file-backed templates and context providers
  held fixed → same output under perturbed env and leftover globals; hot-reload and
  context-provider effects are documented as outside that contract; convenience
  wrappers detect at their edge then match `render_request`; no detector reads from
  template functions, tabular, or width; primary render uses stdout color/terminal
  facts and warnings/progress use stderr facts, including an asymmetric-stream case
  (piped stdout, TTY stderr).
- **`standout-input`:** stdin, clipboard, and responder as arguments; the same request
  plus perturbed process defaults → identical resolution.
- **`standout` glue:** dispatch, `render_inline`, and help-path rendering of the same
  request agree; `help --output=json` (and the other structured modes) still prints
  human help; no `serde_yaml` / `csv` / `quick-xml` in this crate; no
  `MiniJinjaEngine::new()` outside `build()`.
- **`standout-test`:** in-process runs inject facts without `#[serial]` for detector
  reasons; warnings are asserted through the harness API; `#[serial]` count before and
  after is a recorded metric, not an acceptance number.

No live or acceptance evidence beyond the in-repo suite and the existing corpus
invariants. Corpus apps are not rewritten in this epic.

## Workstream Hints

The internal types are the expensive thing to reverse, so they land first (interface
workstream, stubbed). Then a walking skeleton: one command renders end-to-end through
the new request. Then, in an order the tickets will set: `build()` holds configured
fallbacks and point-of-use defaults go away; help and topics join the pipeline;
convenience wrappers and recipe/config collapse; crate-invariant checks and the
`#[serial]` sweep. Help/topics and wrapper-collapse can run in parallel after the
skeleton.

## Out Of Scope

API pruning of blessed vs unblessed idioms, new user-facing capabilities, crate
reorganization, multi-threaded `App`, corpus completion, structured help as a public
format.

## Further Notes

Supersedes the 8.1.0-era draft of this Spec where it described `RenderFn` as live, TTY
as a global to inject, and theme defaults as five-way. Those are closed by ROB02 and
ADR-0019–0022.

ADRs from the grill, authoritative where they sharpen this Spec:

- [`docs/adr/0025-split-render-into-target-properties-and-render-request.md`](../adr/0025-split-render-into-target-properties-and-render-request.md)
  — two public types in `standout-render`: `TargetProperties` (destination of this
  invocation, `Copy`, per-stream color capability and terminal-ness) and
  `RenderRequest` (what to render, owned, engine/registry behind `Rc`). Primary
  render uses stdout facts; warnings/progress use stderr facts. `App` stays the
  build-time fallback bundle; `RenderContext` stays the provider view. No
  lifetime on the public API.
- [`docs/adr/0026-detect-target-properties-at-the-crate-edge.md`](../adr/0026-detect-target-properties-at-the-crate-edge.md)
  — `TargetProperties::detect()` is the one process probe, in `standout-render`,
  filling both streams. Convenience wrappers and `App::run` call it at their
  edge then pass the result into `render_request`; leaves and tests do not.
  Ambiguous-width defaults to `Narrow` and `App::run` then applies the app
  policy. The old `detect_*` / `set_*_detector` functions go away.
- [`docs/adr/0027-pass-target-properties-and-input-sources-into-run.md`](../adr/0027-pass-target-properties-and-input-sources-into-run.md)
  — inner public `run` takes `TargetProperties` and `InputSources` as two
  arguments, not a combined type. Production `run()` detects and uses real
  stdio; `TestHarness` constructs both and calls the inner method.
- [`docs/adr/0028-render-takes-one-owned-request.md`](../adr/0028-render-takes-one-owned-request.md)
  — `render_request` takes `&RenderRequest` (`RenderRequest` is owned: no
  lifetime). Existing `render(template, data, theme)` detects, builds a
  request, and delegates. `Presentation` is deleted. Render-time
  `TemplateRef` lives in `standout-render`; `Convention` stays glue-private.
  Framework help/topics are named registry templates; standalone
  `HelpConfig::template` and siblings stay as `TemplateRef::Inline` with
  equivalent tag validation at request construction.
- [`docs/adr/0029-hold-structured-help-back-in-glue.md`](../adr/0029-hold-structured-help-back-in-glue.md)
  — glue maps structured `--output` to `Auto` when building the help/topics
  `RenderRequest`. The leaf has no help flag. No structured help bytes.
- [`docs/adr/0030-apply-styles-from-the-request-not-process-globals.md`](../adr/0030-apply-styles-from-the-request-not-process-globals.md)
  — ANSI follows the request (`force_styling` from format + per-stream
  `TargetProperties` color capability), not `console`'s process-global
  switch. Width lock deleted. Warnings return on the run result, not a
  thread-local. `TestHarness::with_color()` does not call
  `set_colors_enabled`.
- [`docs/adr/0031-check-glue-invariants-with-tests.md`](../adr/0031-check-glue-invariants-with-tests.md)
  — the four glue invariants are tests in `standout` (`Cargo.toml` parse, source
  scans). Defaults are enforced by build-time fallbacks on `App` plus the
  snapshot matrix, not a grep. Not `cargo deny`.

No ADR: recipe/config dispatch closures collapse to one helper. There is no public
`GroupBuilder` change and no discarded alternative worth recording.

The parity Specs (`docs/spec/parity-machine-contract.md`,
`docs/spec/parity-terminal-citizenship.md`, `docs/spec/parity-config-layering.md`) name
the sockets they expect this epic to leave. Read them with this Spec before the grill.
