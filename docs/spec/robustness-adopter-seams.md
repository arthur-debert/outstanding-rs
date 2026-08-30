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
transcript-anchored finding verified still present on `main` at 7cb4152 (2026-08-30);
there is no design ambiguity in *what* is wrong, only small choices in *how* to expose
each seam.

| Finding | Runs | Still on `main` at |
| --- | --- | --- |
| #356 no programmatic output-mode selection; a second `--output` is refused; ROB04 left `output_mode_fallback()` hard-coded to `Auto` "for a later workstream" | systemdlike | `cli/builder/mod.rs:1023-1031`, `config.rs:307-315` |
| #357 no seam for an app-owned exit status plus a verbatim stderr line; `ExitStatus` fixed to 0/1/2; the "handler diagnostic framing" is never shown | gitlike, ghlike | `standout-dispatch/src/handler.rs:359-373, 459-471, 491` |
| #354 confirmation gate accepts only a literal `yes`; prompts write to `io::stdout()`; the injected `--answers`/`--yes` ids are `pub(crate)` | formlike | `cli/questionnaire.rs:29,34,122-126,318,354` |
| #351 the answer-sheet parser requires the framework preamble; an app cannot supply its own sheet format | formlike | `standout-input/src/questionnaire/parse.rs:175,283` |
| #359 `tabular()` sizes one row at a time; a whole-table resolver exists (`resolve_widths_from_data`) and help uses it, templates cannot | ghlike | `standout-render/src/tabular/filters.rs:268-296`, `resolve.rs:90-99` |
| #334 themed help renders no `-h/--help` / `-V/--version` rows: the extractor reads an unbuilt `clap::Command`; the clap-parity differential carries it as its first `DELIBERATE_OMISSIONS` entry | test net | `cli/help/data.rs:316-321,381`, `standout-test/src/clap_parity.rs:271-283` |
| #353 hook failures print `Hook error: hook error (pre-dispatch): …` | formlike | `cli/dispatch.rs:327` + `standout-dispatch/src/hooks.rs:206` |
| #352 (remainder) the displacement is now a loud `SetupError`, but pre-dispatch hooks still receive root matches, and hook order relative to `.questionnaire::<T>()` inside `CommandConfig` is positional and undocumented | formlike | `cli/builder/execution.rs:212-221`, `docs/guides/derived-questionnaires.md` |

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
  ROB04 stubbed — the mode used when the flag is absent — and an app-side override that
  wins over the flag only when the app says so (env-driven color policy is the use
  case). Appending the app's own `--output` to the user's argv is no longer the way.
- **An app-owned status and diagnostic** (#357): a handler can return a domain error
  carrying an exit status (any `u8`, not 0/1/2) and a verbatim stderr payload, and the
  framework emits exactly that — no framing, no doubled prefix. `ExternalFailure` keeps
  its documented meaning (another process owns the contract). The human-mode form is
  this epic's; the machine-mode form (structured error envelope) stays with the parity
  machine contract, which versions the envelope this seam will feed.
- **Hook diagnostics are framed once** (#353), and the framing is documented as the
  "handler diagnostic framing" `error-handling.md` names.
- **The confirmation gate is configurable** (#354): acceptance rule (exact word, `y/yes`
  case-insensitive, or disabled), prompt wording, and the stream it writes to (stderr by
  default, since stdout is the data channel); the injected argument ids are public
  constants.
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

1. **Dispatch seams** — #356, #357, #353, #352: `AppBuilder` output-mode fallback and
   override; the exit-status + payload error form and its single emission point; hook
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
- **#356 reopening ADR-0018.** The app-side override sets the fallback and, at most,
  a builder-declared precedence; it does not re-parse argv.

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
