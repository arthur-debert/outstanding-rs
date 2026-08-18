# Robustness: Corpus Cleanup

Cleanup epic for the ROB03 corpus pilot delivery. ROB03 (#364) landed the corpus
runner, the archetype roster, the gap suites, and the pilot evidence; a deep review
(session 2026-08-18, four independent review passes: spec alignment, architecture,
test net, evidence) judged it sound and merge-worthy but carrying patchwork —
roughly 15% of the runner's non-test code is duplication or speculative surface, one
schema is maintained as two already-diverged parsers, and two robustness gaps are
silent by construction. This Spec is deliberately lean: every decision it needs was
already made in the review; there is no design ambiguity left to grill.

## Context

ROB03 shipped fast across four workstreams merged into an umbrella branch, and the
seams show. The runner (`crates/corpus-runner`) speaks two acceptance schemas
because the smoke archetype still uses the older one. The roster case schema is
parsed by the runner and, in parallel, by a second full serde struct set inside
`crates/standout-test/tests/corpus_roster.rs`. Integration tests copy-paste their
scaffolding. The review's findings are the authoritative inventory; this Spec turns
them into an epic rather than letting them rot as a follow-up list.

## Problem

Concrete defects and debt, as found by the review. File references throughout this
Spec are to the ROB03 delivery — PR #364's branch, which merges ahead of this epic —
so they may not exist on `main` until that merge lands:

1. **The roster case schema exists twice and has diverged.** The runner's parser
   (`corpus-runner/src/archetype.rs`) and the structural test's parallel structs
   (`standout-test/tests/corpus_roster.rs`) disagree today: the test forbids
   `stdout` + `stdout_json` together, enforces kebab-case case names, duplicate
   detection, and non-empty `gap`/`reason`; the runner enforces none of these. A
   suite can pass the runner and fail CI lint, or vice versa — the one defect class
   that can silently produce wrong pilot verdicts.
2. **A dual acceptance schema survives for one consumer.** `ChecksConfig`,
   `run_checks`, the parallel `checks`/`cases` report vectors, and the `Suite`
   branching exist solely because the smoke archetype speaks the check schema. The
   case schema lacks only two assertion kinds (`stdout_row_contains`,
   `stdout_json_rows`). Roughly 200 lines plus a documented concept ("two schemas
   exist") that every future reader must load.
3. **Duplicated orchestration and fragile report surgery.** `run()` and
   `reevaluate()` in `corpus-runner/src/lib.rs` duplicate the
   build → suite → invariants block nearly verbatim, and `reevaluate` edits the
   source report as raw `serde_json::Value` (panics on off-shape input; sets
   `pins.acceptance_sha256` twice through two different paths).
4. **Speculative surface with no consumer.** Per-command invariant axis overrides no
   archetype uses, a write-only `Archetype.dir` field, `SessionReport.attempts`
   hardcoded to 1, sha256 hex-encoding written three times, and
   `not_run_invariants` re-walking the same cell plan as `run_invariants`.
5. **Copy-pasted test scaffolding.** `fn script()`, the fake-cargo block, and the
   awk questionnaire agent are each duplicated across two to three integration test
   files (~100 lines).
6. **The gap suites are green-by-construction in CI.** Without
   `CORPUS_TFLIKE_BIN`/`CORPUS_JJLIKE_BIN` all 18 gap tests short-circuit, so the
   unexpected-pass tripwire — the parity program's definition-of-done signal — only
   fires when someone remembers to run them by hand. A silently closed gap goes
   unnoticed.
7. **Isolation claims overstate platform parity.** Linux Landlock silently ignores
   the `network=false` policy (filesystem only); macOS Seatbelt is allow-default
   with deny-listed roots while Landlock is default-deny — yet both are recorded
   under one `isolation_backend` word, so a report reads stronger than what was
   enforced. Separately, `session.rs` reads the whole transcript unbounded — a
   runaway agent balloons runner memory.
8. **Sanitizer guarantees are hand-verified.** The review's leak scan (home paths,
   usernames, hostnames, token patterns) was manual; `sanitize_pilot.rs` asserts the
   script's mechanics but no secret-shaped patterns over the committed artifacts.
9. **Two decisions await ADR form.** The blindness protocol and the run-report
   schema are recorded as "Decision:" sections in `corpus/README.md` with "an ADR
   may follow" noted; the ROB03 spec expected both as ADRs.

## Goals

- **One schema, one parser.** The roster case schema is defined once, in the runner;
  the structural roster test consumes those types. Where the two parsers disagreed,
  each divergence is resolved explicitly (default: keep the stricter rule) and the
  resolution is stated in the PR.
- **One acceptance schema.** The check schema is retired: its two missing assertion
  kinds ported into the case schema, smoke migrated, the dual path deleted from
  runner, report, and docs.
- **No duplicated orchestration, no untyped report surgery.** One
  evaluate-the-binary helper shared by `run()` and `reevaluate()`; historical-report
  rewriting goes through typed structs that fail with a diagnostic, never a panic.
- **Every field and option has a consumer.** The speculative surface in Problem 4 is
  deleted (or, where a field documents intent — e.g. `attempts` — it gains a real
  producer or goes).
- **Tests share scaffolding.** One shared helper module for the duplicated
  script/fake-cargo/questionnaire-agent blocks.
- **A closed gap is news, not silence.** Some CI-visible mechanism notices when a
  gap suite would pass — the mechanism is the implementer's choice (scheduled job,
  epic-close checklist enforced by test, or equivalent), but "nothing fires unless a
  human remembers" is no longer acceptable.
- **Reports state what was actually enforced.** Per-platform isolation capability
  (filesystem vs network, allow-default vs default-deny) is recorded distinctly;
  the Landlock network no-op is either enforced or loudly recorded. Transcript
  ingestion is bounded.
- **Sanitizer guarantees are asserted.** `sanitize_pilot.rs` gains pattern checks
  (home paths, usernames, email/token shapes) over all committed run artifacts, so
  the manual leak scan becomes a permanent regression test.
- **The two recorded decisions become ADRs**, transcribed from `corpus/README.md`
  (this is transcription of decided material, not new design work).

## Non-Goals

- No new runner capabilities: first-render token accounting as a report field stays
  a future item (self-flagged in the scorecard), and the completion-phase work
  (corpus repo, CI gate, real downstreams) stays with its own epic.
- No pilot rerun and no validity-check work — that is #365.
- No rewriting of committed pilot evidence (`corpus/pilot/`); historical reports
  stay byte-identical except where a sanitizer regression test finds a real leak.
- No gap-capability work (PAR02/PAR03) and no Windows support.

## Proposed Shape

Four thin workstreams, each independently reviewable, each leaving the 73-test
default net green:

1. **One roster schema** — unify on the runner's parser, move the structural test to
   the runner crate, reconcile the divergences, keep manifest-only types test-local.
2. **One acceptance schema** — port the two assertion kinds, migrate smoke, delete
   the check path end-to-end (code, report schema, README).
3. **Runner de-duplication and hardening** — shared evaluation helper, typed
   re-evaluation, bounded transcript read, speculative-surface removal, honest
   per-platform isolation recording.
4. **Test-net consolidation and tripwires** — shared test helpers, the closed-gap
   tripwire, sanitizer pattern assertions, the two ADRs.

## Risks And Rabbit Holes

- **Cleanup drifting into capability work.** The moment a change adds a feature (new
  report fields beyond isolation honesty, new assertion vocabulary beyond the two
  ported kinds), it has left this epic's scope.
- **Schema unification silently changing semantics.** Each parser divergence must be
  resolved as an explicit decision, not as whichever struct survived; existing
  roster files must still load, and any file the stricter rule now rejects is fixed
  in the same PR with the reason stated.
- **Tripwire overbuild.** The ROB03 spec's "workable script, not a product" warning
  still applies: the closed-gap mechanism should be the smallest thing that makes
  silence impossible, not a scheduling framework.
- **Deleting before porting.** The check schema's assertion kinds must land in the
  case schema (with tests) before the check path is removed, or smoke loses
  coverage mid-epic.

## Cross-Cutting Concerns

- The commit/push lint suite and `pixi run test` gate every WS; the 73 default
  tests must never go red between workstreams.
- The sanitizer work touches committed evidence paths — assertions only, no
  rewriting of pilot artifacts.
- Report `schema_version` bumps if the check-schema removal or isolation-honesty
  fields change the shape; historical reports must still deserialize (the
  re-evaluation path depends on it).

## Testing / Verification

- Schema unification is proven by the structural test importing and exercising the
  runner's parser, plus explicit tests for each reconciled divergence.
- Smoke's migration keeps the hermetic full-loop test meaningful (same phases, case
  schema); the ignored real-crates.io walking skeleton still passes when run.
- The closed-gap tripwire is demonstrated by simulation: point it at a binary that
  passes a gap case and show the visible failure.
- Sanitizer pattern assertions run over every committed report/transcript in
  `corpus/pilot/` and `corpus/demo/`.

## Workstream Hints

The four items under Proposed Shape map one-to-one to workstreams. WS2 depends on
WS1 (both rework the archetype schema surface); WS3 and WS4 are parallel to
everything.

## Out Of Scope

Everything under Non-Goals; framework fixes for the twelve pilot findings
(#349–#360); ROB03 completion-phase work.

## Further Notes

Source material: the ROB03 deep-review session (2026-08-18) over PR #364 — spec
alignment, architecture/duplication, roster/test-net, and evidence-credibility
passes. The two ADRs this epic mints transcribe the "Decision:" sections of
`corpus/README.md`. No grill round is planned: the decisions here are review
outcomes, already specific. Minted:
[ADR-0023 — the corpus blindness protocol](../adr/0023-the-corpus-blindness-protocol.md)
and
[ADR-0024 — the corpus run-report schema](../adr/0024-the-corpus-run-report-schema.md).
