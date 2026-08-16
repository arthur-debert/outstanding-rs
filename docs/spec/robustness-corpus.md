# Robustness: The Downstream Corpus

Fifth Spec in the **Robustness program**. Depends on the test net (its invariants are the
corpus's oracles); runs alongside loud failures and composition contracts as their
acceptance environment; informs one-blessed-surface. Long-lived: the corpus outlives the
program.

## Context

The assessment's single strongest empirical finding: standout's real test suite is its
downstream apps. Nearly every bug in the tracker's history was filed from a consuming
app acting as an accidental fuzzer — lookma (#292–#303), padz (#215, #220, #223, #224),
dodot (#120, #141), rustloc (#98, #200, #201) — and almost none of the broken behavior
was asserted in-repo before the fix. Framework-side development is structurally biased:
the maintainer-plus-agent loop knows standout's point of view, so it exercises the paths
the framework expects, not the paths an uninitiated adopter takes. A real user base would
correct this; there is a chicken-and-egg problem in acquiring one.

A survey of eleven best-of-breed complex CLIs (gh, git, jj, cargo, docker, kubectl,
gcloud, terraform, systemctl, brew, pnpm — session record 2026-08-16) produced a
twelve-archetype roster of synthetic CLI shapes with objective acceptance criteria, from
`gitlike` (porcelain/plumbing split, config walk-up, pager) to `formlike` (questionnaires
under full non-interactivity). Ten archetypes are within standout's current intended
capability; two (`tflike` NDJSON diagnostics, `jjlike` user-supplied runtime templates)
are deliberate *gap specifications* — specs written past current capability whose failing
acceptance tests become the executable definition-of-done for the parity program.

## Problem

Bug discovery currently costs a downstream release cycle plus a debugging session, and
its coverage is whatever the two friendly downstream apps happen to touch. There is no
way to (a) discover an adoption-blocking defect before a release, (b) measure whether the
framework's DX is improving, or (c) test the docs-and-API surface as a black box the way
an adopter meets it. The robustness program's later epics (contracts, blessed surface)
also need an acceptance gate that is shaped like reality rather than like the framework's
own fixtures.

## Goals

- **A corpus of small CLI applications built with standout exists as a repository built
  against standout `main` in CI** — breaking a corpus app is a red check on a framework
  PR, turning one-time simulation into a standing regression net.
- **Corpus apps are produced by blind agent implementers**: each agent receives an
  archetype spec (behavioral requirements + pre-written acceptance tests) and the
  *published* standout documentation and API only — no framework source, no maintainer
  context. The blindness is the bias-remover; it tests docs and API as a black box.
- **Every run is instrumented twice**: objectively (acceptance-test pass rate, retries,
  time/tokens to first render, workarounds detectable in the produced code) and
  subjectively (a structured exit questionnaire per agent: friction points, unexpected
  behaviors, places docs lied — collected with standout's own questionnaire
  infrastructure).
- **Produced apps are matrix-evaluated** with the test net's invariants: every corpus
  binary runs across output modes × TTY × theme where applicable; failures are framework
  findings by default, app findings only on inspection.
- **The pilot precedes the heavy program epics**: 3–4 in-capability archetypes
  (recommended: `gitlike`, `systemdlike`, `formlike`, plus one of `ghlike`/`gcloudlike`)
  run against the post-test-net framework, so contracts and blessed-surface choices are
  grounded in observed adopter friction rather than taste.
- **Re-runs are the DX metric**: the same archetype specs re-executed after each program
  epic; declining friction-report volume and rising unassisted pass rate are the
  program's longitudinal measure.
- The two real downstream apps (and any friendly others) join the corpus CI build as
  first-class members.

## Non-Goals

- Fixing what the corpus finds (findings become issues in the normal flow).
- Building the two gap-spec archetypes' missing capabilities (parity program); the corpus
  merely hosts their failing specs.
- Simulating end *users* of the produced CLIs (interactive UX studies); the corpus
  simulates *developers* adopting the framework.
- Publishing the corpus as a product or maintaining corpus apps as real tools.

## Proposed Shape

**1. The roster.** The twelve archetype specs from the survey, refined into repository
form: per archetype, a behavioral spec (what the CLI does, from its user's perspective),
objective acceptance tests written *before* any implementation (spec-first, so "did it
work" is never judged by the implementer), and a manifest of which standout features and
— more importantly — which feature *interactions* it stresses.

**2. The harness.** A runner that: provisions a blind agent workspace (docs snapshot +
crates.io deps only), executes the implementation session with instrumentation, collects
the exit questionnaire, runs the acceptance suite and the invariant matrix against the
produced binary, and files a structured run report. Runs are reproducible artifacts:
spec version + docs version + framework version + transcript.

**3. The corpus repository.** Accepted implementations live in a corpus repo with a CI
workflow building all members against standout `main` (and against framework PRs via a
trigger or scheduled job — grill decides the exact integration; cargo patch/workspace
pinning mechanics are an implementation detail to settle there). Gap-spec archetypes
live as specs + red acceptance suites, explicitly marked expected-fail.

**4. The feedback loop.** Run reports distill into framework issues (normal triage) and
into a per-epic DX scorecard; the blessed-surface epic's ADR round consumes the pilot's
scorecard.

## User / Agent Stories

1. As the framework maintainer, I want a PR that breaks any corpus app to fail CI, so
   that adopter-breaking changes are caught before release instead of by a friend's bug
   report.
2. As the maintainer, I want agents with no standout context to build real apps from my
   docs alone, so that I learn where the docs and API actually fail an uninitiated
   adopter.
3. As a planning session for a later epic, I want friction reports ranked by frequency
   across corpus runs, so that API choices are grounded in observed pain.
4. As the parity program, I want the gap archetypes' acceptance suites red in the corpus,
   so that "done" for a capability epic means an existing suite turns green.
5. As the maintainer over time, I want the same specs re-run per release, so that "the
   framework is getting easier to adopt" is a measured claim.

## Risks And Rabbit Holes

- **Blindness is fragile.** Agents will find the framework source if it is reachable
  (training data, web search, the repo itself). The workspace must exclude the source
  and the run report must record what the agent consulted; partial blindness is fine if
  it is *known*.
- **Questionnaire theater.** Subjective reports from agents can be confabulated
  pleasantries. Anchor every subjective claim to a transcript moment (the questionnaire
  asks for the command/error that caused the friction); weight objective signals higher.
- **Harness gold-plating.** The runner is a means; the pilot needs a workable script,
  not a product. Build the minimum that makes runs reproducible and comparable, then
  stop.
- **Corpus rot.** Corpus apps pin framework idioms that later epics deliberately break;
  each program epic budgets a corpus-porting pass (which is itself signal: the migration
  cost of the break, measured).
- **CI cost/flake.** Building N apps per framework PR is expensive; the grill decides the
  trigger policy (per-PR for a fast subset, scheduled for the full corpus).

## Cross-Cutting Concerns

- Token/agent cost: pilot runs are budgeted explicitly; re-runs are batched per epic, not
  continuous.
- Security: blind agents execute arbitrary generated code; runs happen in isolated
  workspaces, and corpus CI treats produced apps as untrusted (no secrets in the corpus
  environment).
- Licensing/ownership: corpus apps are throwaway by policy; no one adopts them as
  dependencies.
- Observability: run reports are the durable artifact; transcripts retained per run for
  friction-claim anchoring.

## Testing / Verification

The harness itself gets a smoke archetype (trivial CLI) proving the loop end to end:
blind workspace → implementation → questionnaire → acceptance run → report. Pilot exit
criteria: 3–4 archetypes implemented by blind agents, scorecard produced, at least the
known open issues rediscovered independently (a validity check on the method — if blind
agents don't hit the known sharp edges, the archetypes are too gentle).

## Workstream Hints

(1) Roster refinement: survey archetypes → repo-form specs with acceptance tests (the
in-capability ten first); (2) runner walking skeleton + smoke archetype; (3) pilot
execution + scorecard; (4) corpus repo + CI wiring + real downstreams joined; (5)
gap-spec archetypes landed as expected-fail suites. (1) and (2) parallelize; (3) gates
nothing downstream except the blessed-surface ADR round preferring its scorecard.

## Out Of Scope

Capability building, interactive user studies, corpus-app maintenance as products,
framework fixes.

## Further Notes

The twelve archetypes with acceptance-test sketches are in the session record of
2026-08-16 (survey Part C): C1 gitlike, C2 ghlike, C3 kubelike, C4 tflike (gap), C5
systemdlike, C6 cargolike, C7 gcloudlike, C8 dockerlike, C9 jjlike (gap), C10 brewlike,
C11 pnpmlike, C12 formlike. C1/C6/C7 deliberately triangulate config layering ahead of
the parity config epic. Expected ADRs: corpus/CI integration mechanics; blindness
protocol; run-report schema. Links to be added by the grill.
