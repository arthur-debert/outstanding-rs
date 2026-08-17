# Robustness: The Test Net

First of five Specs in the **Robustness program**, produced by the August 2026 robustness
assessment. This Spec is the program's entry point: it changes no runtime behavior and
everything after it depends on it.

**Authoritative dependency graph for both programs.** Every other Spec's header states its
own dependencies; where any statement disagrees with this graph, this graph wins.

```text
test net
├── loud failures
├── corpus PILOT ────────────────────────────┐   (3–4 in-capability archetypes;
│   └── gap-spec suites ───────────────┐     │    also authors the red gap-spec suites)
└── composition contracts              │     │
    ├── one blessed surface ◄──────────│─────┘   (consumes the pilot's scorecard)
    │   └── corpus COMPLETION          │         (full roster on blessed idioms, CI gate,
    └── parity: config layering        │          real downstreams joined)
        ├── parity: machine contract ◄─┤         (gated by tflike's diagnostic milestone)
        │   └── parity: terminal citizenship
        └──────────────────────────────┘         (gated by tflike's full suite; also
                                                  depends on machine contract for the
                                                  event model its progress seam emits)
```

The corpus is deliberately split. Its **pilot** runs early (immediately after the test net,
alongside loud failures and composition contracts) so its findings reach the
blessed-surface decisions, and it also authors the **gap-spec acceptance suites**
(`tflike`, `jjlike`) as expected-fail — those are black-box assertions on a produced
binary's stdout and exit codes, so they neither wait on the blessed idioms nor on the
capabilities they describe, and each parity epic must have its executable
definition-of-done in hand before it starts. Corpus **completion** — the full archetype
roster implemented on the blessed idioms, the CI gate, real downstreams joined — lands
after the blessed surface, since corpus apps pin idioms that epic deliberately breaks.
Terminal citizenship depends on machine contract, not merely on config layering: its
progress seam emits machine events into the model that epic defines. Ordering is by
dependency, not calendar: siblings under one parent may run concurrently.

## Context

An assessment of standout at 8.1.0 (issue-history mining, architecture verification, DX
audit, test-architecture audit) established how bugs reach releases. Of ~28 bug-type issues
in the tracker's history, only 3 were true regressions; the rest were latent gaps surfaced
by the first downstream adopter to exercise a path. The themed-help cluster (#292–#303) is
the type specimen: nine issues, three design causes, and none of the broken behavior was
asserted anywhere in-repo before the fix PRs.

The test suite is large (~1,600 unit, ~680 integration tests) but structurally unable to
see this class of defect:

- Every help test runs in a tag-erasing output mode (`Text`/`TermDebug`); the `[tag?]`
  corruption of #303 appears only under `TagTransform::Apply` (Term / Auto+color) — the one
  mode no help test uses.
- `TestHarness::is_tty()` and `with_color()` have zero call sites in the workspace; the
  TTY-positive branch of the harness is dead API, and `detect_is_tty()` has no production
  consumer.
- No fixture combines the properties that co-occur in real apps: `tdoo` has a custom theme
  but never enables `help_handling`; every help fixture enables help but has no theme.
- Assertions are existential (`contains "default: auto"`), while every escaped defect is a
  universal ("every value-taking option shows a metavar") or a negative ("no SetTrue flag
  shows possible-values"). The bogus row of #301 was rendered in the tests' own output on
  every run; no oracle rejected it.
- The one property test (`property_rendering.rs`) generates the exact shape of #303 —
  empty theme × styled template × Term — and passes it, because its only postcondition is
  "didn't panic".
- Three insta snapshots exist in the whole workspace, none of help. `TestResult` has no
  `stderr()`; there is no ANSI-strip helper, no matrix combinator, and `run()` takes the
  `App` and the `clap::Command` as two hand-synchronized arguments, so every test file
  rebuilds near-identical fixtures.

## Problem

Standout's tests assert the framework's own data model against itself, in the modes that
cannot show the failures, with fixtures too small to contain them. Consequently the de
facto test suite is the set of downstream apps (lookma, padz, dodot, rustloc), and defects
are discovered post-release at downstream-debugging cost. Before any redesign work can be
attempted safely (see the composition-contracts Spec), current behavior must be pinned by
tests that use external oracles and realistic fixtures — otherwise the redesign cannot be
distinguished from a regression.

## Goals

- **External oracles replace self-referential assertions** for the help surface: a
  clap-parity differential test walks `clap::Command` metadata over a shape matrix and
  asserts every user-facing fact clap's own formatter would render appears in themed help
  or is explicitly exempted.
- **Universal and negative invariants become reusable assertions** available to every test:
  no unresolved `?]` tag in any styled render; no possible-values row for arity-0 args; a
  metavar for every arity>0 arg; `strip_ansi(Term output) == Text output`; every tag a
  template emits is defined in the resolved theme; whole-page column alignment.
- **One shared downstream-shaped fixture** exists and is used by the help, output-mode, and
  rendering test suites: ≥3 commands, a positional with a value name, a `SetTrue` flag, an
  enum option with a default, a valued free-form option, an app theme that is deliberately
  *incomplete*, `help_handling(true)`, and topics registered. This single fixture reproduces
  #297, #298, #299, #301, #302, and #303.
- **The harness can express the whole matrix cheaply**: `stderr()` capture, an ANSI-strip
  accessor, a matrix combinator over (output mode × TTY × theme), insta snapshot
  integration, and a process-level escape hatch for the cases only a real pipe/pty proves.
- **The TTY axis is real or gone**: `.is_tty()`/`.with_color()` either work end to end
  through the help path with a worked example, or are deleted. *Resolved per method, not
  as one seam (see Further Notes): `.is_tty()` and the TTY axis it named are deleted;
  `.with_color()` stays and works end to end through the help path with a worked example,
  as the color axis it always was.*
- The still-open help bugs (#295 aside — it is a design decision) are caught by the new
  net: a test exists that fails on #302 and #303 before their fixes land.
- Existing environment-convention behavior that works by accident is pinned: `NO_COLOR`
  and `TERM=dumb` suppression currently arrive transitively through the `console` crate's
  API surface and are untested; tests must pin them so a dependency upgrade cannot
  silently change them.

## Non-Goals

- Fixing the bugs the net catches. This Spec pins and reveals; the loud-failures and
  composition-contracts Specs change behavior. (Trivial exceptions — a one-line
  postcondition fix — may ride along where separating them would be artificial.)
- Redesigning the test suite wholesale or migrating existing passing tests to the new
  style. New oracles are added beside what exists.
- Building the downstream corpus (own Spec).
- De-serializing the `#[serial]` test suite — that requires removing the process globals,
  which is composition-contracts work.

## Proposed Shape

Four pieces, in dependency order.

**1. Harness capabilities** in `standout-test`: `TestResult::stderr()`, a
`stdout_plain()` ANSI-stripping accessor, insta as a dev-dependency with matrix-keyed
snapshot helpers, a `matrix()` combinator yielding (OutputMode, tty, theme) cells, and a
thin `run_process()` that executes a real binary for stream/exit-code/pty evidence. The
TTY seam (`is_tty`, `with_color`) is wired end to end — including the `force_styling`
interaction documented only in a test comment today — or removed.

**2. The shared fixture** as a fixture module/crate consumable by every test crate — one
downstream-shaped app definition (shape described in Goals) exposed both as an `App` and
as its `clap::Command`, replacing the per-file hand-synced copies in
`themed_help_surfaces.rs`, `help_through_run.rs`, and `flat_command_help_word.rs`.

**3. Invariant assertions** as a small assertion library in `standout-test`: the
universal/negative checks from Goals, each usable against any `TestResult`. The clap-parity
differential is the flagship: it iterates the fixture's `clap::Command` (arguments,
subcommands, metavars, defaults, possible values with clap's own suppression rules,
about/long_about) and checks presence in the rendered page, with an explicit allowlist for
deliberate omissions so exemptions are visible in review.

**4. Coverage application**: the matrix (output mode × TTY × theme × help entry point:
`-h`, `--help`, help word) runs against the shared fixture with snapshots per cell; the
property test's postcondition is strengthened from "didn't panic" to the invariant set;
environment conventions (`NO_COLOR`, `TERM=dumb`) get pinning tests.

## User / Agent Stories

1. As a framework maintainer, I want help output checked against clap's own metadata, so
   that dropping a field clap knows about fails a test instead of surfacing in a
   downstream app.
2. As a maintainer fixing a help bug, I want a fixture that already contains valued
   options, boolean flags, an incomplete theme, and enabled help, so that writing the
   regression test is one assertion, not a new fixture file.
3. As an agent implementing a later redesign epic, I want the current rendered output
   pinned by snapshots across the mode×TTY×theme matrix, so that my refactor's diff shows
   me exactly which cells changed behavior.
4. As a test author, I want `stderr()`, ANSI-stripped stdout, and TTY control on the
   harness, so that stream routing and terminal-only defects are assertable in-process.
5. As a maintainer upgrading dependencies, I want `NO_COLOR`/`TERM=dumb` behavior pinned by
   tests, so that a `console` upgrade cannot silently change color semantics.

## Risks And Rabbit Holes

- **Snapshot sprawl.** Matrix × fixture snapshots must stay reviewable: one fixture, keyed
  cells, and no per-test ad-hoc fixtures — otherwise snapshot churn trains reviewers to
  rubber-stamp.
- **Differential-test overreach.** The clap-parity oracle asserts *presence of facts*, not
  layout equality with clap's renderer — chasing byte parity with clap's formatting is a
  trap; themed help exists to look different.
- **Wiring the TTY seam may reveal it cannot work** (the `console` force-styling problem —
  `default_help_theme` sets no `force_styling`, so a TTY-simulated render emits no ANSI).
  If so, the honest outcome is: `[tag?]`-class invariants run in Term mode (they do not
  need real ANSI), ANSI-positive checks run through `run_process()`, and the in-process
  TTY methods are deleted rather than left half-working.
- **Scope creep into fixes.** The net will immediately fail on #302/#303. Mark those
  `#[should_panic]`/`#[ignore]`-with-issue-reference rather than fixing them here, except
  where the fix is the one-line property-postcondition case.

## Cross-Cutting Concerns

- CI time: the matrix multiplies test count; keep cells cheap (in-process, one fixture)
  and reserve `run_process()` for the few PTY-necessary cases.
- The `#[serial]` constraint stands until composition-contracts lands; the matrix
  combinator must compose with `serial_test`.
- insta snapshot review becomes part of the PR review contract; reviewers accept snapshot
  diffs only with a stated reason.

## Testing / Verification

This Spec *is* testing; its own verification is: (a) the new suite fails on the unfixed
open bugs (#302, #303) and on reverts of the recent fixes (#297–#301 fix commits reverted
locally → red); (b) the shared fixture replaces at least the three known hand-synced
fixture copies; (c) `pixi run test` wall-time increase stays within an agreed budget.

## Workstream Hints

Natural slices: (1) harness capabilities + TTY seam decision as the walking skeleton;
(2) shared fixture + migration of the three help test files onto it; (3) invariant
assertion library + clap-parity differential; (4) matrix/snapshot application + property
postconditions + env-convention pins. (2) and (3) parallelize after (1).

## Out Of Scope

Behavioral fixes (loud-failures Spec), pipeline consolidation (composition-contracts
Spec), corpus construction (`docs/spec/robustness-corpus.md`), test de-serialization.

## Further Notes

Assessment evidence lives in the session record of 2026-08-15/16; key file:line citations:
help tests' modes (`crates/standout/tests/themed_help_surfaces.rs:10`,
`crates/standout-test/tests/help_through_run.rs:185`), dead TTY seam
(`crates/standout-test/src/lib.rs:160-179`), property postcondition
(`crates/standout/tests/property_rendering.rs:139-143`), fixture gap
(`crates/todo-example/tdoo/src/app.rs:19-51`). ADRs from the grill to be linked here.

The TTY-axis goal above resolved to *gone*: `docs/adr/0019-delete-the-in-process-tty-seam.md`
records why the seam was deleted rather than wired (a stdout-only global is the wrong shape
for its one named future consumer, which had already routed around it), how ANSI-positive
assertions became possible in-process anyway (`with_color()` now opens `console`'s color
gate as well as Standout's), and why the epic's tag-resolution invariants never depended on
the outcome.
