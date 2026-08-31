# Robustness: Adopter Seams

Sixth epic of the **Robustness program** (ROB06), minted 2026-08-30 from the corpus pilot's
fallout. Depends on composition contracts (ROB04) — every item below sits on a seam that
epic defined. Runs in parallel with the blessed surface (ROB05), which reads this Spec
before pruning so the two do not fight over `AppBuilder` and `Output`. Precedes corpus
completion: the re-run's DX metric only moves if the workarounds the pilot recorded are
no longer needed.

## Context

The pilot scorecard's second-ranked theme, hit by 4/4 blind runs: *exact streams and
exit statuses require application-owned escape paths*. Every adopter reached a point
where the framework owned a decision (exit status, stderr bytes, output mode, the
confirmation prompt, the answer-sheet format, column widths) and exposed no way to
make it, so the app rewrote argv, set `CLICOLOR_FORCE`, wrote its own bytes outside
dispatch, or used `ExternalFailure` against its documented purpose. The
[ROB03 Spec](implemented/robustness-corpus.md) said findings would flow through normal
triage; two epics later, none has. This Spec is the home. Everything in it is a filed,
transcript-anchored finding re-verified on `main` at 1642d4e (2026-08-31, the 9.0.0
release commit — the anchors below survived the ROB05 prune); there is no design
ambiguity in *what* is wrong, only small choices in *how* to expose each seam.

| Finding | Runs | Still on `main` at |
| --- | --- | --- |
| #356 no programmatic output-mode selection; a second `--output` is refused (`Set` action, no `overrides_with`); ROB04 left `output_mode_fallback()` hard-coded to `Auto` "for a later workstream" | systemdlike | `cli/builder/mod.rs:691-693,1103-1110`, `cli/builder/execution.rs:446-466`, `cli/builder/config.rs:89-96` |
| #357 the emission mechanics exist (#208: `ExternalFailure::new` takes any nonzero `u8`, the diagnostic reaches stderr verbatim, the status rides to `process::exit`), but the only spelling is `ExternalFailure`, documented "only when another operation owns the status" — the app-owned form the adopters needed has no name | gitlike, ghlike | `standout-dispatch/src/handler.rs:156-188`, `docs/topics/error-handling.md:33` |
| #354 confirmation gate accepts only a literal `yes`; the review dump writes to `io::stdout()` (the prompt itself now goes to the controlling terminal); the injected `--answers`/`--yes` ids are `pub(crate)` | formlike | `cli/questionnaire.rs:18-23,109-118,336-352` |
| #351 the answer-sheet parser requires the framework preamble; an app cannot supply its own sheet format | formlike | `standout-input/src/questionnaire/parse.rs:126-128,303-340` |
| #359 `tabular()` resolves widths from the column spec alone; a whole-table resolver exists (`resolve_widths_from_data`) and help uses it, templates cannot | ghlike | `standout-render/src/tabular/filters.rs:198-227`, `resolve.rs:48-74` |
| #334 themed help renders no `-h/--help` / `-V/--version` rows: the extractor emits only declared args; the clap-parity differential carries it as its first `DELIBERATE_OMISSIONS` entry (ADR-0034 reworded the entry, the gap is unchanged) | test net | `cli/help/data.rs:284`, `standout-test/src/clap_parity.rs:137-151` |
| #353 hook failures print `Hook error: hook error (pre-dispatch): …` | formlike | `cli/dispatch.rs:272` + `standout-dispatch/src/hooks.rs:102-103` |
| #352 (remainder) the displacement is now a loud `SetupError`, but pre-dispatch hooks still receive root matches (the handler gets `get_deepest_matches` two lines later), and hook order relative to `.questionnaire::<T>()` is push order, positional and undocumented | formlike | `cli/builder/execution.rs:131-140`, `cli/group.rs:379-396`, `docs/guides/derived-questionnaires.md` |

## Problem

A framework that owns rendering, dispatch and input must let the app make the
decisions a CLI spec pins — the exact exit status for a domain error, the bytes on
stderr, whether ANSI goes through a pipe, the wording of a confirmation — or the app
routes around the framework, and every routed-around path is one the framework's tests
never see. The pilot recorded 19 workarounds across four apps; each is a place where
standout's guarantees stop applying to the app that adopted it.

## Goals

Each seam is one bounded change with its own acceptance test; none introduces a new
concept beyond the seam itself.

- **Output mode is app-selectable** (#356): `AppBuilder` gains the configured fallback
  ROB04 stubbed — the mode used when the flag is absent. Precedence stays the ROB04
  composition contract, `flag > later config > App fallback`: an explicit `--output`
  always wins, which is what systemdlike's suite asserts (`--output text`/`term`
  outranks `SYSTEMDLIKE_COLORS`, `NO_COLOR` and detection). The env-driven case the
  pilot hit is a fallback computed by the app at build time, not an override; forcing
  color regardless of mode is the color-policy axis (terminal citizenship), with its
  own precedence contract. Appending the app's own `--output` to the user's argv is
  no longer the way.
- **An app-owned status and diagnostic** (#357): a handler can return a domain error
  carrying an exit status (any nonzero `u8`) and a verbatim stderr payload under a name
  that *means* app-owned. The emission path already exists — `ExternalFailure` carries
  exactly these bytes and this status, rejects zero at construction, and skips all
  framing — so the seam is the spelling and its contract: an app-owned form sharing
  that emission path, a test proving a domain error can never report shell success,
  and `ExternalFailure` keeping its documented meaning (another process owns the
  contract) instead of being the name adopters misuse. The human-mode form is
  this epic's; the machine-mode form (structured error envelope) stays with the parity
  machine contract, which versions the envelope this seam will feed.
- **Hook diagnostics are framed once** (#353), and the framing is documented as the
  "handler diagnostic framing" `error-handling.md` names.
- **The confirmation gate is configurable** (#354): acceptance rule (exact word, `y/yes`
  case-insensitive, or disabled), prompt wording, and the stream the review dump writes
  to (stderr by default, since stdout is the data channel; the prompt itself already
  goes to the controlling terminal); the injected argument ids are public constants.
- **An app-defined answer-sheet format** (#351): the parser is a seam the app can
  replace or extend — the framework's preamble/fingerprint sheet is the default, not the
  only format `--answers` accepts.
- **Hooks see the command's matches and their order is a rule** (#352): pre-dispatch
  hooks receive the deepest matches; questionnaire resolution runs at a defined point
  relative to app hooks, and the guide states it.
- **`tabular()` can size a whole table** (#359): the template function reaches
  `resolve_widths_from_data`, so `{"min": n}` columns grow to fit the data as the guide
  promises.
- **Themed help lists `-h/--help` and `-V/--version`** (#334): the extractor reads a
  built command; the `DELIBERATE_OMISSIONS` entry goes.

## Non-Goals

- Choosing or pruning idioms (ROB05). Where this epic adds a builder method or an
  `Output` / error variant, it adds it to the blessed path only.
- The machine-mode error envelope, `--color` tri-state, pager, progress, verbosity
  (parity program). #356 gives the *app* a mode setter; the *user*-facing `--color` flag
  is terminal citizenship's.
- Fixing the corpus archetype implementations produced by the pilot (they are not
  committed) or re-running the pilot (corpus completion).
- #408/#409 (XML serializer) and #336 (lint hook) — ordinary maintenance, not adopter
  seams.

## Proposed Shape

Three thin workstreams, grouped by crate so each touches one seam family:

1. **Dispatch seams** — #356, #357, #353, #352: `AppBuilder` output-mode fallback; the
   exit-status + payload error form and its single emission point; hook
   matches and ordering rule.
2. **Questionnaire seams** — #354, #351: confirmation configuration; the sheet-format
   seam in `standout-input`.
3. **Render and help seams** — #359, #334.

Each workstream closes its issues with a test that reproduces the pilot transcript's
failing invocation and asserts the specified bytes/status.

## Risks And Rabbit Holes

- **#357 pre-empting the machine contract.** The seam carries status + bytes; it does
  not define a structured error type or a machine-mode shape. If the design needs an
  error *model*, stop and hand the item to PAR02.
- **#351 growing into a second questionnaire system.** The seam is "parse these bytes
  into answers"; the review flow, fingerprinting and derived structs stay as they are.
- **#356 reopening ADR-0018.** The seam sets the fallback and nothing else: no
  builder-declared precedence, no argv re-parse, no path for the app to beat the flag.

## Cross-Cutting Concerns

- Every new public entry is recorded in the ROB05 ADR's blessed set at the time it
  lands, so the prune does not remove it.
- The ROB01 snapshot matrix holds: no command-output bytes change except the ones a
  finding specifies (the doubled prefix, the help rows).
- `CHANGELOG/unreleased-*.md` fragments per workstream, consolidated by ROB05's release.

## Testing / Verification

Per finding: a regression test at the crate that owns the seam, plus one `TestHarness`
test in `standout` that replays the pilot invocation from the transcript link in the
issue. #334's fix deletes an allowlist entry in the clap-parity differential, which is
the proof. The systemdlike and formlike archetype suites (`corpus/archetypes/*/acceptance.toml`)
are unchanged — they are black-box and already assert the behavior these seams enable;
the corpus completion re-run is the end-to-end check.

## Workstream Hints

The three items under Proposed Shape are the workstreams; all parallel. Dispatch seams
first if serialized, since ROB05's ADR round wants its builder additions early.

## Out Of Scope

Everything under Non-Goals; docs beyond the topic paragraph each seam needs (ROB05 owns
the doc sweep and will pick up these paragraphs in its truth pass).

## Further Notes

Source: the [pilot scorecard](../../corpus/pilot/scorecard.md) theme 2, and
issues #351, #352, #353, #354, #356, #357, #359, #334, each verified against `main`
on 2026-08-30. No grill round is planned: each item is a review outcome with a filed
mechanism. One ADR is expected — the app-owned status/diagnostic seam (#357), because it
draws the line between this epic and the machine contract.
