# Robustness: One Blessed Surface

Fourth Spec in the **Robustness program**. Depends on composition contracts (the final
shapes must exist before idioms are blessed); closes the program's API and documentation
debt.

## Context

The assessment's DX audit measured the public surface a downstream author faces: ~15
concepts before the first line of business logic; **10 ways to register a handler, 9 ways
to provide a template, 7 ways to set a theme, 6 entry points, 60 public `AppBuilder`
methods**; `App::new()`, `App::builder()`, and `AppBuilder::default()` as three names for
one constructor; ~90 items re-exported from `standout-render` verbatim including
internals like `rgb_to_ansi256` and `walk_dir`.

The three canonical starting points — `crates/todo-example/tdoo` (the README's "canonical
worked application"), the bootstrap wizard's generated output, and
`docs/guides/minimal-single-crate.md` — teach three mutually exclusive idioms for input
handling and two for handler registration; the wizard, which the README tells new users
to run first, is the *least* representative (no `InputChain`, no typed param attributes,
no `--version`, no default command). **No reference artifact enables
`help_handling(true)`** — the framework's flagship themed-help feature is off in every
example, both quick-starts, and the wizard.

Docs promise behavior the code doesn't deliver (the audit's catalogue: the README
ordering example, `template_dir`, the harness TTY methods, `--output` documented as 4 of
8 values, `standout = "7"` pins at 8.1.0, dead links) — the loud-failures and
composition-contracts epics fix the *behavior* side; this epic fixes the *choice* and
*teaching* side. Separately, `docs/SUMMARY.md` (the mdBook nav) omits the best material:
`standout-help.md` (514 lines), `topics-system.md`, the piping topic; ListView, seeker,
`#[command]`, passthrough, and struct handlers have no docs at all.

Repo policy: no backwards compatibility required; the users are the maintainer and two
friendly downstreams who prefer porting breaking changes over carrying weight.

## Problem

Redundant near-equivalent paths are where agent-generated drift accumulates: each
session picks (or invents) a variant, reference clients diverge, tests cover the
diagonal of a growing matrix, and every additional path multiplies the interaction
surface the robustness program exists to shrink. A framework whose own examples disagree
with each other, and whose flagship feature is off everywhere, cannot be adopted from its
documentation — which the corpus experiment (`docs/spec/robustness-corpus.md`) will measure
directly.

## Goals

- **One blessed idiom per concern**, chosen deliberately and recorded: one primary
  handler-registration path (expected: derive-based `#[derive(Dispatch)]` +
  `#[handler]` with the ergonomic parameter style as default), one template mechanism,
  one theme mechanism, two entry points (run for binaries, a capture form for
  tests/embedding). Secondary paths survive only with a stated reason (e.g. the builder
  closure path as the dynamic-registration escape hatch); everything else is deleted.
- **`AppBuilder` shrinks to roughly half its 60 methods**; the prelude/re-export surface
  is curated down to what the blessed idioms need; internals lose their `pub`.
- **`help_handling(true)` becomes the default.** The flagship feature is on unless
  explicitly disabled; the loud-failures validation story already guards its edge cases.
- **The three reference clients converge on the blessed idioms**, and the wizard's
  generated project *is* the canonical example — compiled and run in CI (the
  wizard-output drift of #244/#250 becomes structurally impossible to miss).
- **Docs tell the truth and are reachable**: the audit's catalogue of doc/code
  divergences is zeroed; orphaned topics enter `SUMMARY.md`; undocumented shipped
  features get at least a topic or are deleted with their code (the same no-dead-switches
  rule as loud failures, applied to features).
- **A stability statement exists** for what remains: which surfaces are contract
  (blessed idioms, output of `--output` modes at the structural level), which are
  internal. (The full machine-readable schema contract is parity-program scope; the
  *policy* that a contract exists starts here.)

## Non-Goals

- New capabilities or new idioms — this epic chooses among what exists post-contracts.
- Machine-output schema versioning (parity: machine contract).
- Building the corpus (own Spec). Its **pilot** is a predecessor of this epic, not part of
  it: the pilot's scorecard is an input to the choosing below.
- Preserving source compatibility for deleted paths.

## Proposed Shape

**1. The choosing.** A short ADR round settles the blessed set — informed by the DX
audit's redundancy catalogue, the corpus pilot's friction reports, and the post-contracts
shapes. Each surviving secondary path gets a one-line justification in the ADR; unlisted =
deleted.

**2. The pruning.** Delete unblessed paths and their tests; shrink re-exports; visibility
sweep. Mechanical once (1) is fixed.

**3. The teaching.** Rewrite `tdoo` and the minimal guide on the blessed idioms; make the
wizard emit them; wire wizard-output compile+run into CI; fix the doc-truth catalogue;
complete `SUMMARY.md`; write the missing topics for surviving features. The README leads
with the strongest verified pitch (per the earlier docs feedback: testability — handlers
return data, not strings — now actually demonstrated by the testing guide against the
final harness).

## User / Agent Stories

1. As a new adopter, I want the README, the wizard output, and the example app to show me
   the same idiom, so that I learn one way that works instead of three that disagree.
2. As an application author, I want themed help on by default, so that the framework's
   distinguishing feature is what my users see without me discovering a flag.
3. As an agent implementing a downstream app, I want one obvious registration/template/
   theme path, so that I cannot pick a deprecated variant and produce drifting code.
4. As a maintainer, I want the wizard's generated project compiled and run in CI, so that
   generator rot (#244) is caught at commit time.
5. As a maintainer reviewing PRs, I want a stability statement, so that "is this change
   breaking?" has a written answer instead of a judgment call.

## Risks And Rabbit Holes

- **Premature blessing.** Without the corpus pilot's scorecard, idiom choices lean on
  taste. The dependency graph makes the pilot a predecessor for exactly this reason; if it
  is ever short-circuited, low-confidence choices must be marked revisitable in the ADR
  rather than presented as settled.
- **Deletion cascade.** Removing a path can strand a real capability that only it
  exposed (e.g. struct handlers for stateful commands). The redundancy catalogue lists
  paths, not capabilities; the ADR round must map capabilities → surviving paths before
  deleting.
- **Docs scope explosion.** "Write the missing topics" is unbounded; the boundary is:
  surviving features get *a* topic (accurate, reachable), not a book. Depth follows
  adoption pain, later.
- **Default-flip fallout.** `help_handling(true)` by default changes every existing app's
  help output on upgrade; the changelog and the collision `SetupError`s (already good)
  carry the migration.

## Cross-Cutting Concerns

- This epic is the largest deliberate compatibility break of the program; it should land
  as one major version with the whole program's migration notes consolidated.
- CI gains the wizard-output job and doc-link/doc-example checks (compile-tested
  examples; mdbook link check) so doc truth is enforced, not audited.
- Downstream apps (the two friendly users + the corpus) are the acceptance environment:
  each ports to the blessed surface during the epic, and their friction reports are
  review input.

## Testing / Verification

Wizard output compiles, runs, and passes its own generated tests in CI; doc examples
compile; the matrix snapshots hold for surviving paths; a surface census
(public-item count, builder-method count) is recorded before/after as the epic's metric;
each deleted path's tests are removed in the same commit that deletes the path (no
orphaned green).

## Workstream Hints

(1) ADR round (blessed set + capability map); (2) prune + visibility sweep;
(3) reference-client convergence + wizard CI; (4) doc-truth sweep + SUMMARY + missing
topics + README repositioning. (3) and (4) parallelize after (2).

## Out Of Scope

New features, schema versioning, corpus construction, crate reorganization.

## Further Notes

Expected ADRs: the blessed-idiom set; the stability statement; the help-on-by-default
decision. The DX audit's redundancy catalogue (session record 2026-08-15/16) is the
working inventory for step 1. The corpus pilot's scorecard — the observed-friction
input to the choosing — is `corpus/pilot/scorecard.md` (ROB03-WS04). Links to be
added by the grill.
