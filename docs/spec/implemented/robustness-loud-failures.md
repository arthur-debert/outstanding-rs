# Robustness: Loud Failures

Second Spec in the **Robustness program**. Depends on the test net
(`docs/spec/robustness-test-net.md`); precedes composition contracts.

## Context

The August 2026 assessment found that silence is the cost multiplier across standout's bug
history: nearly every expensive debugging session traces to a wrong-but-quiet default
rather than a hard failure. #31 and #89 (theme silently replaced by `Theme::new()` /
order-dependently ignored — "[header?] output, took ~1 hour to debug"), #141 (handler
`Err` collapsed into the success variant: error prints, process exits 0), #106
(`--output-file-path` silently ignored on the text path), #215 (silent 80-column
fallback), #120 (`opacity` parses and does nothing), #303 (app theme silently replaces
`default_help_theme`; unresolved `[about?]` tags reach end users).

The January 2026 remediation commit (`021fc20`) fixed part of this — actionable
"here's the builder call you're missing" error messages — and the next commit 28 minutes
later (`28e127b`) reverted them during the AppCore merge. The messages exist in git
history.

The worst family is templates, standout's core value surface. A command's template is a
`String` that is *either* registry content, *or* a file path, *or* `""`, disambiguated at
render time by `engine.has_template()` guessing (`crates/standout/src/cli/builder/
commands.rs:161-190`, `crates/standout-render/src/template/functions.rs:1011`). Four silent
failure faces follow from this one decision: no template → command prints nothing and
exits 0; a typo'd name → the name itself is rendered as MiniJinja source; `.templates()`
called after `.commands()` → resolution already ran, empty template (the README's own
Quick Example has this ordering); `.template_dir()` → the literal file path is rendered as
template source, because nothing ever loads the files. Adjacent: `.build()` is optional at
the type level — `run()` and friends are `&self` methods on `AppBuilder`, so an unbuilt
app runs with no templates loaded and the theme unresolved; hooks registered via
`.hooks()` and via `CommandConfig` silently clobber each other (last writer wins); and
several public surfaces are dead (`include_framework_styles()` sets a field nothing reads,
`AppBuilder::output_mode()` unconditionally returns `Auto`,
`standout_dispatch::RenderFn/from_fn` has zero call sites, the harness TTY methods are
handled by the test-net Spec).

## Problem

An application author who makes any of the most likely configuration mistakes — wrong
builder-call order, a typo'd template name, a missing template, an incomplete theme, an
unbuilt app — gets a program that runs, exits 0, and produces empty or corrupted output.
The framework's reflex on misconfiguration is to degrade quietly; every such quiet path
has already cost a real debugging session downstream. The loud paths that do exist (help
configuration `SetupError`s, `ThemeNotFound`) prove the codebase knows how to do this
well; the behavior is inconsistent, not unknown.

## Goals

- **Every misconfiguration the assessment catalogued fails loudly at the earliest possible
  moment** — build time where the information exists, dispatch time otherwise — with an
  actionable message naming the fix (restoring and extending the reverted `021fc20`
  messages).
- **The template surface is typed.** A command's template is an enum (named registry
  reference / inline source / explicitly absent), never a guessed-at `String`. `build()`
  validates that every named reference resolves and that a command without a template is
  an explicit, declared state (silent-command or structured-only), not an accident.
  `.template_dir()` either works (files loaded, path resolution defined) or is removed.
- **Ordering independence**: template resolution late-binds the way theme resolution was
  made to late-bind for #89 — documented builder-call orders all work, or misordering is a
  build error. The README example, as written today, either works or fails loudly.
- **Themes merge, never replace**: an app theme overlays the framework defaults
  (help/topic tag vocabulary included); an unresolved tag in a framework-shipped template
  is a build-time error, and in an app template degrades to inner text rather than
  emitting `[tag?]` markup to end users.
- **`build()` is the gate**: run/dispatch/parse entry points exist only on a built `App`
  distinct from `AppBuilder`, making "ran an unconfigured app" unrepresentable.
- **Conflicting configuration is rejected**: double-registered hooks (builder + config
  paths) error instead of last-writer-wins.
- **No dead switches**: every public builder method and exported item either does what its
  docs say or is deleted this epic (`include_framework_styles` wired to the existing
  `FRAMEWORK_STYLES` constant or both removed; `output_mode()` stub removed; `RenderFn`
  removed or adopted — adoption belongs to composition contracts, so default is removal).

## Non-Goals

- Pipeline consolidation, `ResolvedConfig`, or global-detector removal — composition
  contracts.
- API-surface reduction (choosing the blessed idioms) — one-blessed-surface Spec. This
  Spec makes existing surfaces honest; it does not decide which survive.
- New capabilities (structured errors in machine modes, config, pager) — parity program.
- Backwards compatibility. Breaking changes are explicitly acceptable (repo policy); every
  break lands with the loud error that explains the migration.

## Proposed Shape

Three layers of the same principle — make the illegal state unrepresentable where types
can, a build error where they cannot, and a loud runtime error last.

**1. Type layer.** The template enum; the `AppBuilder`→`App` split (builder methods
consume/configure, only `App` executes); hooks as a set-once slot per (path, phase).

**2. Build-time validation layer.** `build()` becomes the single choke point that already
half-exists: named templates resolve; every tag emitted by registered framework templates
exists in the *merged* theme (the bbparser `validate` primitive already exists and is used
for templates in isolation — this applies it to the composed artifact); declared-but-dead
config rejected. Error messages follow the restored `021fc20` style: state the missing
piece and the exact builder call that supplies it.

**3. Runtime layer.** Where only runtime knows (an app-template tag missing from the
theme, terminal-width detection failing), the behavior is defined and visible: degrade to
unstyled inner text (never `[tag?]`), and route warnings through the existing warning
channel rather than dropping them.

The test net gates the whole epic: mode×TTY×theme snapshots pin what "no behavior change
for correctly-configured apps" means, and each newly-loud path lands with a test that the
misconfiguration now fails with the intended message.

## User / Agent Stories

1. As an application author who typos a template name, I want `build()` to fail naming the
   typo and listing near-matches, so that I never ship a CLI that prints its template's
   filename.
2. As an application author who calls builder methods in the "wrong" order, I want order
   not to matter (or to be told immediately), so that copying the README can never produce
   a silently empty app.
3. As an application author with a partial theme, I want framework surfaces (help, topics,
   list-view) to keep their default styling for tags I didn't define, so that theming one
   part of my app can't corrupt help output (#303).
4. As an end user of a standout app, I want to never see `[tag?]` markup, so that template
   internals don't leak into my terminal.
5. As an agent building on the framework, I want every public builder method to do what
   its rustdoc says, so that I don't wire `include_framework_styles(true)` and chase the
   resulting no-op.
6. As a maintainer, I want misconfiguration failures to carry the fixing builder call in
   the message, so that downstream issue reports about setup mistakes become
   self-resolving.

## Risks And Rabbit Holes

- **The strictness dial.** Some quiet behavior is intentional flexibility (e.g. a
  structured-only command legitimately has no template). The rule is: quiet is fine when
  *declared*, never when *defaulted into*. Each newly-loud path needs the explicit escape
  hatch identified before it lands, or adoption pain will masquerade as robustness.
- **`AppBuilder`→`App` split blast radius.** Every test and example calls the current
  merged type; the split is mechanical but wide. Keep it a rename-shaped change: same
  methods, moved between two types — resist redesigning signatures here (that is
  one-blessed-surface work).
- **Theme merge semantics.** "Overlay" needs one definition (app wins per-tag over
  framework default) and one implementation point — if merging is implemented per call
  site it recreates the five-theme-defaults problem this program exists to kill. If the
  single implementation point proves to require the `ResolvedConfig` from composition
  contracts, take only the minimal seam here and leave consolidation there.
- **Restoring old messages verbatim.** `021fc20`'s messages are the style guide, not the
  content: builder API has drifted since January; each message is re-derived against
  today's methods.

## Cross-Cutting Concerns

- Every intentional behavior change updates the matrix snapshots — reviewers see exactly
  which cells changed, with reasons in the PR body.
- Docs: the README ordering example and the `app-configuration` topic get corrected in the
  same epic that makes them loud (docs asserting broken behavior are part of the defect).
- Release: this epic is a breaking release by design; changelog enumerates each
  newly-loud path with its migration line.

## Testing / Verification

Per-path pairs: (misconfigured fixture → asserted error message) and (correct fixture →
snapshot unchanged). The four template faces each get a named regression test. Tests for
the #303 and #141 classes flip from red (net) to green here. The property test's theme strategy
gains incomplete-theme cases and must stay green under the degrade-to-text rule.

## Workstream Hints

(1) Template enum + build-time resolution (walking skeleton — kills all four faces);
(2) theme merge + unresolved-tag degradation; (3) `AppBuilder`→`App` split + hook
conflict rejection; (4) dead-switch sweep + restored error-message style + doc
corrections. (2)–(4) parallelize after (1).

## Out Of Scope

Pipeline unification, defaults consolidation, detector globals, API pruning, machine-mode
error output, new capabilities.

## Further Notes

Bends the usual planning flow: Specs for the whole program are authored together; the
grill/ADR round for this epic ran after its tickets were minted (ROB02 is #316), so the
workstream issues predate these ADRs and must be read together with them.

The grill produced three ADRs, which are authoritative where they sharpen this Spec:

- [`docs/adr/0019-carry-a-template-as-a-typed-reference-resolved-at-render.md`](../adr/0019-carry-a-template-as-a-typed-reference-resolved-at-render.md) — the
  `TemplateRef` shape (a named reference survives to render, resolved through the retained
  registry so file-backed entries still reread; `build()` validates it), typed absence
  carrying its reason, and the removal of `.template_dir()`. `StructuredOnly` defaults to
  JSON, honors explicit structured modes, and *rejects* explicit presentation modes
  (`term`, `text`, `term-debug`) — note this supersedes the looser "serializes in human
  modes" phrasing used in this Spec's Goals. Ordering independence is a consequence of
  late resolution rather than a separate mechanism, which narrows WS01: the Spec offered
  "late-bind or make misordering a build error", and the decision is late-bind.
- [`docs/adr/0020-resolve-one-theme-at-build-over-a-single-framework-base.md`](../adr/0020-resolve-one-theme-at-build-over-a-single-framework-base.md) — one
  resolved theme computed in `build()`, replacing the five scattered defaults; every
  registered template validated against it at build; hot-reloaded latecomers degrading to
  unstyled inner text with a warning. This also folds `FRAMEWORK_STYLES` into the base,
  which resolves part of WS04 inside WS02.
- [`docs/adr/0021-split-the-configuring-builder-from-the-executable-app.md`](../adr/0021-split-the-configuring-builder-from-the-executable-app.md) — `AppBuilder`
  configures, `build(self)` returns `App`, which executes; the four uniqueness panics
  become structural.

Two findings from the grill's codebase exploration, recorded so they are not
re-researched: `Theme::merge` already exists with the wanted semantics
(`crates/standout-render/src/theme/theme.rs`, the argument wins per tag), so WS02 is a wiring change rather than new merge machinery; and
hot reload is delivered by the template registry's file entries plus `embed_templates!`
storing absolute source paths, not by `.template_dir()`, which is why removing the latter
costs no capability.
