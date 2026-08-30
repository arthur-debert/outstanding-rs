# Robustness: Corpus Completion

Last epic of the **Robustness program** (ROB07). The completion phase of the corpus
Spec whose pilot phase ROB03 (#322) and the corpus cleanup ROC02 (#367) delivered — the
pilot record is now `implemented/robustness-corpus.md`. Depends on the blessed surface
(ROB05) **as a published release**: the runner pins crates.io versions by design
(ADR-0023, no path or git dependencies), so nothing here can run until the 9.0 line is
on crates.io. Also depends on the adopter seams (`robustness-adopter-seams.md`), because
the re-run is the measurement of whether those seams removed the pilot's workarounds.
Long-lived: the corpus outlives the program.

## Context

What exists on `main` at 7cb4152:

- **The runner** (`crates/corpus-runner`): provision → blind agent session → exit
  questionnaire → acceptance suite + ROB01 invariant matrix → `report.json`
  (schema 3, ADR-0024), under macOS Seatbelt / Linux Landlock with the blindness
  protocol of ADR-0023. One acceptance schema, one roster parser (ROC02).
- **The roster** (`corpus/archetypes/`): the four pilot archetypes `gitlike`, `ghlike`,
  `systemdlike`, `formlike`; the two gap archetypes `tflike`, `jjlike`; the harness's
  `smoke`; and `validity`, a method-coverage archetype added by #365 that forces the
  three known-edge families the pilot did not independently rediscover (mistyped
  template names, registration order, incomplete app theme × framework help).
- **The gap suites** (`corpus/gap-suites/`): 18 `expect_gap` tripwires across
  `tflike/diagnostic` (PAR02), `tflike/progress` (PAR03) and `jjlike/runtime-templates`
  (no epic minted), pinned by `gaps.toml` and a ledger test under plain `pixi run test`.
- **The pilot evidence** (`corpus/pilot/`): four sanitized runs against 8.1.1 and the
  scorecard. Twelve framework findings (#349–#360) and a docs errata list (#361).

What does not exist:

- **Six of the survey's ten in-capability archetypes** — C3 kubelike, C6 cargolike,
  C7 gcloudlike, C8 dockerlike, C10 brewlike, C11 pnpmlike — have no repository form.
  Their behavioral sketches live only in the 2026-08-16 session record; their names
  appear in the parity Specs (config layering expects C1/C6/C7 to triangulate config
  layering).
- **A CI gate.** `corpus/` is exercised by the roster structural test and the gap
  ledger; no produced app is built anywhere. `.github/workflows/checks.yml` has no
  corpus lane.
- **A completed validity run.** #407: two attempts at running `validity` blind failed
  before the agent started — the isolated `PATH` lacked the agent binary, then the agent
  could not authenticate because the runner grants no credential exception (disposable
  HOME, Keychain denied, `ANTHROPIC_API_KEY` off the allowlist). The scorecard's verdict
  stands at "partial signal".
- **Any downstream on the 8.x line.** lookma pins 7.10.1, dodot 7.10.2, padz 7.9.1 (git
  tag), rustloc 7.6.2. "Real downstreams join the corpus CI" is a port across the 8.0 and
  9.0 breaks before it is a wiring task.
- **The DX metric's second data point.** The pilot is one measurement; the program's
  longitudinal claim ("adoption is getting easier") needs the same specs re-run against
  the post-ROB05 release.

## Problem

The pilot proved the method — four blind runs produced twelve auditable framework
findings in a day — and then the program's later epics changed the framework under it.
Without a re-run there is no evidence that ROB04, ROB05 and the seams epic made adoption
easier rather than merely different; without the remaining archetypes the corpus stresses
four interaction shapes, not ten; without a CI gate a framework PR can break every
produced app and no check goes red; and the validity check that would tell us whether
the archetypes are sharp enough to find *known* defects has never run.

## Goals

- **The validity run completes** (#407), under a decided credential policy: the runner
  gains one recorded credential exception — a dedicated, scoped agent credential
  supplied through an allowlisted environment variable, written into the report's
  existing `blindness.credential_exceptions` field — rather than either weakening
  isolation or leaving the run permanently blocked. The scorecard's validity section is
  replaced with the sanitized outcome.
- **The pilot archetypes re-run** against the ROB05 release, same specs, same
  questionnaire, producing the second scorecard: per-archetype acceptance and invariant
  ratios, workaround counts, and friction themes side by side with the pilot's. This is
  the program's measured claim; the re-run happens before any framework work responds to
  it.
- **The six remaining in-capability archetypes exist in repository form** — `spec.md`,
  `manifest.toml`, spec-first `acceptance.toml` — and run blind. Priority order: C6
  cargolike and C7 gcloudlike first (config layering's triangulation, alongside the
  existing C1), then C3 kubelike, C8 dockerlike, C10 brewlike, C11 pnpmlike.
- **Accepted implementations become a standing regression net.** Produced apps that
  pass their suites are committed to a corpus repository (separate from this one — they
  are throwaway by policy and must not enter this workspace, which the roster structural
  test forbids) and built against standout `main` on a schedule, with a fast subset on
  framework PRs; a red build is a framework finding by default.
- **One real downstream joins the net**: lookma, the newest and the app that filed the
  most bugs (#292–#303), is ported to the ROB05 release and added to the corpus build.
  Its port is itself the migration-cost measurement ROB05 promised. The other three
  downstreams are out of scope; their ports are their own repos' work.
- **The gap suites keep their tripwires**; when PAR02/PAR03 close them, the ledger flip
  and suite promotion happen in the parity epic, not here.

## Non-Goals

- Fixing what the re-run finds (issues in the normal flow — and, if a third theme
  emerges at 4/4, a follow-on Spec like the adopter seams).
- The two gap archetypes' capabilities (parity program).
- Porting padz, dodot or rustloc.
- Runner features beyond what the goals need: first-render token accounting stays
  future; no new isolation backends; no Windows.

## Proposed Shape

**1. Validity and credential policy.** Decide and implement the one credential
exception; complete the `validity` run; update the scorecard. Smallest workstream,
first, because it unblocks every later blind run.

**2. The re-run.** Once 9.0 is published: the four pilot archetypes plus `validity`,
each run once, scorecard v2 written by the same script and ranking as v1.

**3. Roster expansion.** The six archetypes, spec-first, in the priority order above;
each authored as one PR (spec + manifest + suite, no implementation), then run blind.

**4. The corpus repository and the gate.** A new repository holding accepted
implementations, a workflow that builds them against standout `main` (scheduled) and a
fast subset against framework PRs (triggered), the lookma port as its first real
member. The grill settles the cargo patch/workspace mechanics and the trigger.

## Risks And Rabbit Holes

- **Running before the release.** A re-run against 8.1.1 measures nothing new. The
  dependency on a published 9.0 is hard; do not substitute a git pin.
- **The credential exception widening.** One variable, one scoped credential, recorded
  in every report; if the agent backend needs more (host HOME, Keychain), the answer is
  a different backend invocation, not a wider policy.
- **Archetype authoring drifting from the survey.** The six sketches are in a session
  record, not the repo; the author reconstructs them from the survey's capability matrix
  (the parity Specs' Context sections carry most of it) and states in the spec which
  interactions each stresses, rather than aiming for fidelity to a lost document.
- **The corpus repo becoming a product.** Accepted apps are frozen artifacts with a
  build; no maintenance beyond porting passes budgeted by later epics.
- **CI cost.** Full corpus on a schedule, a fast subset per PR; the grill picks the
  subset (recommended: the four pilot archetypes plus lookma).

## Cross-Cutting Concerns

- Token budget: each blind run is ~55k generated tokens and ~12 minutes (pilot
  measurements); eleven runs for the re-run plus six new archetypes is bounded and
  batched, not continuous.
- Security: unchanged from ADR-0023 — produced apps are untrusted; the corpus repo's CI
  carries no secrets.
- The scorecard's sanitizer pattern tests (ROC02-WS04) run over every newly committed
  run before it lands.

## Testing / Verification

The validity run rediscovers all three known-edge families (that is the check). The
re-run's scorecard v2 is produced by the committed script from committed reports.
Every new archetype's suite passes the roster structural test and the runner's own
integration test (a scripted agent producing a trivially failing binary yields a
complete report). The corpus repository's build is green on a framework `main` commit
and demonstrably red on a commit that breaks one member (a deliberate break on a
branch, recorded in the PR).

## Workstream Hints

(1) validity + credential policy; (2) re-run + scorecard v2 — after the 9.0 publish;
(3) six archetypes — (3a) cargolike + gcloudlike, (3b) the other four; (4) corpus repo +
gate + lookma port. (1) and (3) start immediately; (2) waits on the release; (4) waits
on (2) so the first accepted implementations are post-ROB05 ones.

## Out Of Scope

Framework fixes, gap capabilities, the three other downstream ports, runner
gold-plating, interactive user studies.

## Further Notes

The pilot-phase Spec, its decisions (ADR-0023 blindness protocol, ADR-0024 report
schema) and the cleanup (`implemented/robustness-corpus-cleanup.md`) are the record this
Spec builds on. Expected ADRs from the grill: the credential-exception policy; corpus
repository and CI mechanics. The survey's archetype list: C1 gitlike, C2 ghlike, C3
kubelike, C4 tflike (gap), C5 systemdlike, C6 cargolike, C7 gcloudlike, C8 dockerlike, C9
jjlike (gap), C10 brewlike, C11 pnpmlike, C12 formlike.
