# Parity: The Machine Contract

Second Spec of the **Capability-parity program**. Depends on composition contracts (error
emission and serialization live at seams that epic defines) and on config layering; its
executable definition-of-done is the **diagnostic milestone** of the corpus gap archetype
`tflike`, authored as an expected-fail suite in the corpus pilot before this epic starts
(`tflike`'s progress criteria belong to terminal citizenship, which depends on this epic).

## Context

Structured, machine-consumable behavior was part of standout's original goal — the
framework's headline claim is that handlers return data and the framework renders it,
including `--output json|yaml|csv|xml` for machines. The assessment found the claim only
half-kept, and this Spec restores the missed half.

Three findings define the gap:

- **Errors are prose even in machine modes.** `emit_run_result` writes the error string
  to stderr with no branch on the resolved output mode
  (`crates/standout/src/cli/builder/execution.rs:842-846`): `myapp list --output json`
  plus a handler failure yields valid-JSON-or-nothing on stdout and a human sentence on
  stderr. Any automation wrapping a standout CLI must regex stderr — precisely what the
  framework exists to eliminate. The taxonomy already exists: `RunErrorKind`
  distinguishes seven causes; only the structured emission is missing. The survey's best
  practice is terraform's diagnostic shape — `severity`, `summary`, `detail`, plus
  locus fields — and cargo's discriminated NDJSON stream (`reason`-keyed messages) for
  long-running output.
- **No output-stability contract.** Structured modes serialize the handler's view struct
  directly via serde, so `--output json` field names are private Rust identifiers with
  no stability marking; renaming a view field silently breaks every downstream script.
  Surveyed tools version their machine surfaces (`git status --porcelain=v2`,
  `cargo metadata --format-version 1`, `brew --json=v2`). The blessed-surface epic
  establishes the *policy* that contract surfaces exist; this Spec builds the mechanism.
- **The exit-code vocabulary cannot express "succeeded, but the answer is no/empty."**
  `ExitStatus` is 0/1/2 plus external passthrough; `git diff --exit-code`,
  `terraform -detailed-exitcode`, and `systemctl is-active` all exist because a boolean
  exit cannot distinguish "nothing to do" from "failed" — and standout ships `ListView`,
  the exact command shape that needs it. The systemd lesson from the survey: one
  meaningful code applied consistently beats a broad taxonomy (whose inconsistencies
  became their own bug class).

Also latent in the existing surface: `--output=csv` on nested data silently flattens to
one garbage row (#108's family), XML crashes on top-level maps (#107) — the structured
modes need defined, documented semantics per data shape, not best-effort serialization.

## Problem

A machine driving a standout CLI cannot rely on the one thing a framework could
guarantee wholesale: that success output, failure output, and exit status are structured,
stable, and documented for every command of every adopting app. Individual CLIs earn
this per-tool (terraform, cargo); a framework can make it the default — and standout,
which owns the handler seam, the serializer, and the error path, is uniquely positioned
to. Today it does none of it, and the gap undermines the framework's core pitch.

## Goals

- **Structured failure in structured modes.** When the resolved output mode is
  structured, run failures emit a machine-readable diagnostic document/stream entry —
  severity, summary, detail, and the `RunErrorKind`-derived kind — on a defined stream,
  while human modes keep today's prose. One emission point (the composition-contracts
  seam), covering handler errors, hook errors, render errors, and usage errors
  consistently.
- **A defined bootstrap rule for pre-parse failures.** Usage errors are the hard case:
  clap returns `ClapUsage` before `ArgMatches` exists, so the emission seam cannot ask
  the parsed line which mode was requested — the same structural shape as the open #295
  (`--output` reaches the `help` word but not `--help`/`-h`, because clap short-circuits
  first). Without an explicit rule, an implementation can satisfy every other goal here
  and still emit prose for a malformed command line. This Spec therefore **requires a
  parse-independent mode-selection stage**: whenever raw argv contains a recognizable
  structured-mode request, that mode governs failure emission even when parsing never
  succeeds. A documentation-only exclusion is not an acceptable alternative — it would
  contradict the consistency the goal above claims. The ADR fixes the recognizer's
  precedence rules (repeated `--output` occurrences, position relative to the failing
  token, `--output=x` versus `--output x`, `--` termination, and a malformed mode value,
  which must itself produce a structured usage diagnostic when the surrounding request is
  recognizably structured) and the deliberately narrow scope of the recognizer, so it does
  not become a second parser (the One Parser pillar constrains this: it scans for one
  known flag, it does not classify the command line). Tested directly —
  `myapp --output json --bad-flag` and `myapp --bad-flag --output json` must both produce
  structured diagnostics — and #295 is resolved by the same decision.
- **A stability mechanism for structured output.** Apps can mark a view type (or the
  framework marks its own, e.g. `ListView`'s envelope) as a *contract surface* with a
  schema version; the version is addressable from the CLI (flag or key in the
  envelope — grill decides), and `standout-test` can snapshot the JSON shape so an
  unmarked breaking change fails the app's own tests. Framework-owned envelopes
  (diagnostics, `ListView`, help data) are versioned from day one.
- **`help --output=json` returns structured help data, and this is where it is first
  exposed.** The composition-contracts epic makes help ride the pipeline but deliberately
  withholds the machine surface; this Spec defines and versions the help document's schema
  and turns it on — so the format is public and versioned in the same release (also
  unblocking completions/manpage generation from standout's own model, per the
  blessed-surface stability policy).
- **One opt-in "empty/negative result" exit code**, consistently defined
  (`ExitStatus`-level, `ListView`-integrated, handler-signalable), documented in the
  execution-outcomes topic beside the existing 0/1/2 — deliberately *one* code, not a
  taxonomy.
- **Structured modes have defined semantics per data shape**: CSV declares what it does
  with nested data (project-or-error, not silent garbage); XML's mapping is defined or
  the mode is dropped (grill decides — an 8th untested mode may cost more than it
  serves); the decision closes #107/#108.
- The corpus `tflike` archetype's **diagnostic milestone** turns green — the NDJSON
  diagnostic stream, severity entries, and `-detailed-exitcode` behavior — and
  `brewlike`'s schema-version tests pass. `tflike`'s remaining acceptance criteria cover
  progress/apply-lifecycle events, which the terminal-citizenship epic owns; the archetype
  is therefore split into a diagnostic milestone (gated here) and a full-suite gate
  (terminal citizenship). The corpus Spec carries the same split, so neither epic claims a
  gate it cannot close.

## Non-Goals

- A general streaming/eventing system. `tflike` needs an NDJSON *diagnostic and
  progress* stream; whether full streaming render becomes a framework feature is its own
  future decision — this Spec covers the diagnostic stream and single-document modes.
- Changing human-mode error presentation (terminal-citizenship handles verbosity).
- Retroactive stability guarantees for existing app view structs (opt-in mechanism;
  apps adopt per surface).
- Machine-readable *warnings* beyond routing them into the diagnostic channel.

## Proposed Shape

**1. The diagnostic model.** One diagnostic type (severity, summary, detail, kind,
optional command path/source locus) produced from `DispatchResult`/`RunErrorKind` at the
single emission seam; serialized per structured mode; stream placement (stdout document
vs stderr, single-doc vs NDJSON) fixed by the grill informed by the survey precedents.

**2. The contract mechanism.** A marker (derive attribute or registration call — grill)
declaring a view type a contract surface with a version; the version travels in the
envelope; `standout-test` gains schema-snapshot assertions; framework envelopes
(diagnostics, ListView, help document) adopt it first as the worked examples.

**3. The exit code.** `ExitStatus::EMPTY` (working name) with one meaning; `ListView`
and handler API integration; documented interaction with the diagnostic model
(empty is success-with-signal, never an error).

**4. Mode semantics.** Per-mode data-shape rules written and enforced (loud per the
robustness posture): CSV projection requirements, XML fate, the existing
`CsvProjection` positioned within the rules.

## User / Agent Stories

1. As an automation author, I want `--output json` failures to be JSON diagnostics with
   a severity and kind, so that I branch on data instead of regexing stderr.
2. As an application author, I want to mark a view struct as a stable surface with a
   version, so that renaming a field is a caught, versioned event instead of a silent
   downstream break.
3. As a script author, I want a documented exit code meaning "ran fine, found nothing,"
   so that my pipeline distinguishes empty from failed without parsing output.
4. As a machine consumer, I want `myapp help --output=json` to describe the CLI's
   commands and options in a versioned schema, so that wrappers and completion tooling
   generate from ground truth.
5. As an application author using CSV mode, I want nested data to produce a loud error
   pointing at projections, so that I never ship one-row garbage (#108).
6. As the corpus's `tflike` implementer working its diagnostic milestone, I want the NDJSON
   diagnostic stream and detailed exit codes available from the framework, so that the
   milestone's acceptance tests pass without hand-rolled plumbing.

## Risks And Rabbit Holes

- **Schema-versioning gold-plating.** The mechanism is a marker, a version field, and a
  test helper — not a migration framework, not schema evolution tooling. Resist v1→v2
  transformation machinery until a real consumer needs it.
- **Stream-placement bikeshed.** Where diagnostics go (stdout vs stderr, mixed vs
  separate) has real precedents pulling both ways (terraform: stdout stream; general
  Unix: stderr). Pick per survey evidence in the ADR and stop; the contract is the
  shape, the placement just needs to be *documented and versioned*.
- **Exit-code creep.** The systemd finding is the guardrail: one new code, opt-in,
  consistent. Any proposal for a richer taxonomy defers to a future Spec with new
  evidence.
- **XML sunk-cost.** If the grill keeps XML, its mapping must be fully defined and
  tested (it is currently the crash-prone, least-used mode); dropping it is the cheaper
  honest option under the no-backwards-compat policy.

## Cross-Cutting Concerns

- Composition contracts must leave the emission seam and the mode-semantics decision
  point open — this Spec is named in that epic's grill inputs.
- Docs: `execution-outcomes` topic (already the repo's most precise doc) extends to the
  diagnostic model and the new code; the stability policy doc from blessed-surface gains
  the mechanism reference.
- Testing: schema snapshots join the harness; the external gates are `tflike`'s diagnostic
  milestone and `brewlike`'s schema-version tests. (`brewlike`'s remaining criteria belong
  to the corpus roster proper, which corpus completion owns.)
- Release: envelope changes to `ListView` output are breaking for scripts reading
  today's shape — coordinated with the version-stamping so the break is the last
  unversioned one.

## Testing / Verification

Per structured mode: failure-path snapshot (diagnostic shape), success-path schema
snapshot (versioned envelopes), exit-code table tests (success/empty/failure ×
human/structured). `tflike`'s **diagnostic milestone** green — not its full suite, whose
progress/apply-lifecycle criteria terminal citizenship owns; `brewlike`'s schema-version
tests green; issues #107/#108 closed by the mode-semantics rules with regression tests.

## Workstream Hints

(1) Diagnostic model + emission at the seam (walking skeleton: one failing command
emits a JSON diagnostic); (2) contract marker + schema snapshots + framework envelopes
versioned; (3) empty exit code + ListView integration; (4) mode semantics (CSV rules,
XML decision) + docs. (2)–(4) parallelize after (1).

## Out Of Scope

General streaming, human-mode presentation changes, warning taxonomy, schema migration
tooling.

## Further Notes

Survey evidence: terraform diagnostics and `-detailed-exitcode`, cargo
`--message-format=json` discriminated stream, versioned surfaces (`--porcelain=v2`,
`--format-version 1`, `--json=v2`), systemd's taxonomy-vs-consistency lesson (session
record 2026-08-16). Expected ADRs: diagnostic shape and stream placement; the contract
marker and version addressing; the empty-result code; XML's fate. Links to be added by
the grill.
