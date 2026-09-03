# PAR04: The Corpus Runner as a Pipeline

> **Implemented** by PAR04 (#478): WS01 #492, WS02 #493, WS03 #494 (ADR-0024
> amended), WS04 #495, and a convergence stage. As built, where the text below
> differs:
>
> - D25's post-run file assertions read a single inventory of the whole
>   sandbox, taken once the case's process group is confirmed dead, rather
>   than opening each asserted path on demand — a path an inventory has no
>   regular-file entry for is the same failure as a path the inventory
>   never saw.
> - `batch` takes one directory, `--out`; there is no separate `--runs-dir`
>   for it, unlike `run`.
> - D26's tag archive extracts `docs` (and each crate's `docs/` symlink
>   target) from the tag with `git archive`, never the tag's whole tree.
> - D17's evidence check runs on any case that carries a `gap` naming an
>   evidence-bearing manifest `[gaps]` entry, independent of `expected`: a
>   case already flipped to `expected = "pass"` once its epic closes the gap
>   keeps reporting `hand-rolled-pass` instead of a silent ordinary pass.
>   `scorecard.py`'s acceptance column reports required and gap cases as two
>   counts for the same reason — mixing their denominators misread a pass
>   rate no framework failure produced.
> - `validity`'s two `--output term`, `NO_COLOR`-set cases asserted no ANSI;
>   the framework's contract is the opposite (`docs/topics/output-modes.md`:
>   an explicit `term` request is unconditional). Corrected in the
>   convergence stage and re-evaluated against the preserved WS04 workspace
>   rather than re-run blind, since the produced app did not change.
> - `reevaluate` carries a schema-4-or-later source's own `blindness` block
>   forward unchanged, rather than always substituting the historical-partial
>   narrative: that narrative is accurate for a pre-instrumentation source
>   (which never recorded its own agent-phase isolation) and false for one
>   that did.
> - ghlike and dockerlike's suite edits (#460) replay through `reevaluate`
>   in the `standout-corpus` repository, not here; this repository has no
>   ghlike or dockerlike implementation to reevaluate against (the roster's
>   structural test forbids one).

Runs beside PAR01 (config layering) after PAR02 (the machine contract), and must finish
before PAR01's blind runs: two of its items (D17's evidence check and #455's file
assertions) are what make PAR01's exit criterion measurable. It touches
`crates/corpus-runner`, `corpus/scorecard.py` and the acceptance suites, never the
framework crates, so it shares no files with PAR02 or PAR01. Its exit criterion is one
command that runs every roster archetype against a published version and writes a
scorecard outside git, with the eight runner and suite defects below fixed.

## Problem

The corpus program (`docs/spec/implemented/robustness-corpus.md` and its two
successors) works: eleven blind runs against 8.1.1 and 9.0.0 produced comparable
reports, and `standout-corpus` replays four accepted apps on every PR. Four things about
how it runs make it expensive to use again.

**Run evidence lives in git and dominates review.** `git ls-files corpus` is 89 files
and 17.3 MB. The 17 `transcript.jsonl` files are 15.95 MB of that, one Claude Code
stream-json session each; the 17 `report.json` files are 838 KB. Run evidence is 73%
of every tracked byte in the repository. PR #470, the ROB07 umbrella, added 34,168
lines in 79 files, and a reviewer cannot read a transcript. Two tests pin the committed
runs in place: `crates/corpus-runner/tests/scorecard.rs` asserts `scorecard.py`
reproduces the pilot figures from `corpus/pilot/runs`, and
`crates/corpus-runner/tests/sanitize_pilot.rs` secret-scans every committed run.

**One agent does everything in one night.** The six completion evidence PRs were
opened between 00:37Z and 08:47Z on 2026-09-01 by the session that also fixed the
runner, ran the sessions and wrote the scorecards. `corpus-runner run` handles one
archetype; nothing runs a set, sanitizes, scores and writes a summary, so the agent
scripts that by hand each time and the steps drift.

**The scoring vocabulary cannot say "does not apply".** Three defects turned
framework-neutral choices into failures: ghlike's invariant score fell from 70/70 to
30/70 because its app called `no_output_flag()` and the matrix appends `--output` to
every cell (#461); kubelike scored 8 failures on commands declared `rendered` before an
app existed that chose artifact output (#467); cargolike scored 2 failures on
`config list`, a command whose content is the output mode (#465). The scorecard's
workaround and friction counter reads 0 for four of six completion runs because agents
listed as `(1)` and `(a)`, which its regex does not match. Ten of eleven 9.0
questionnaires read `collected: false` because `confidence` is a one-of scalar and
every agent justified its answer (#462).

**The suites and pins have authoring defects.** validity asserts `Usage` where standout
writes `USAGE` (#450, 16 cases) and asks for the `help` word at a leaf, which standout
installs at the root by design (#454); ghlike and dockerlike have the same header
assertions (#460, closed without the suite change). A run pinned to a published version
copies docs from the checkout, so the agent reads documentation for a different
release than its dependency (#451). A case is one invocation with no way to read the
sandbox afterwards, so gcloudlike's `config set` and `create` write contracts cannot be
asserted (#455).

## What the user gets

### The coordinator (a person or an agent)

```bash
corpus-runner batch gitlike cargolike --framework-version 10.0.0 --out ~/corpus-runs/par01
```

The batch runs each archetype serially with `run`, sanitizes each run in place, writes
`<out>/<archetype>-<run-id>/report.json` and `transcript.jsonl`, and writes
`<out>/scorecard.json` and `<out>/scorecard.md` from `scorecard.py`. The exit status is
non-zero if any run failed to complete. Committing evidence is then one copy of the
report files into `corpus/<set>/runs/`, nothing else.

```bash
corpus-runner run brewlike --framework-version 9.0.0
```

resolves docs from the `v9.0.0` tag of the checkout's repository with `git archive`
when the version differs from the checkout's `CARGO_PKG_VERSION`, and refuses to run if
that tag does not exist. `report.json` records `pins.docs_source` as `tag` or
`checkout`.

### The suite author

```toml
[case.run]
argv = ["config", "set", "core/project", "alpha"]
env = { GCLOUDLIKE_CONFIG_DIR = "conf" }
[case.expect]
exit_code = 0
stderr = "Updated property [core/project].\n"
files_absent = ["conf/configurations/config_staging"]
[case.expect.files]
"conf/configurations/config_default" = "[core]\nproject = alpha\n"

[[invariants.command]]
argv = ["build"]
contract = "either"           # rendered or opaque-bytes, whichever the binary does, consistently

[[invariants.command]]
argv = ["config", "list"]
contract = "rendered"
equal_across_modes = false    # the content names the output mode
```

Help assertions follow one rule across every suite: a case asserts the command path
and option names it expects, never the header's casing or the usage line's layout.

### The scorecard reader

`scorecard.json` rows carry the same columns as today plus `hand_rolled_passes`: the
count of gap cases that passed while the produced app's `Cargo.toml` lacks the crate
the archetype's manifest names as evidence (D17). The workaround and friction counts
come from the questionnaire's list fields, whichever list marker the agent used.

## Decisions

**D23. Invariant vocabulary.** Before the matrix runs, the runner invokes the binary
with `--help` and, when no `--output` argument appears, marks every mode cell
`not-applicable` with reason `no output flag` (#461); no manifest declaration, because
the app's choice is the fact being measured. `contract = "either"` accepts whichever
of `rendered` or `opaque-bytes` the binary satisfies on the first cell and holds it to
that for the rest (#467). `equal_across_modes = false` on a command skips the
term-versus-text byte comparison for it (#465). All three land in
`crates/corpus-runner/src/archetype.rs` and `acceptance.rs`.

**D24. One help-assertion rule.** Suites assert `stdout_contains` of the command path
and option names only. validity's 16 `Usage` cases, ghlike's one and dockerlike's two
`stdout_row_contains` cases are rewritten to that rule; validity's spec drops the
leaf-word sentence (#454). validity re-runs blind because its `acceptance_sha256`
changes; ghlike and dockerlike replay through `reevaluate` in `standout-corpus`.

**D25. Post-run file assertions.** `CaseExpect` gains `files` (path to exact content)
and `files_absent` (paths that must not exist), read from the case sandbox after the
process exits. A case stays one invocation; a multi-step scenario seeds its
precondition with `[case.run.files]`. This closes #455 without an ordered-case schema.

**D26. Docs from the tag.** When `--framework-version` differs from the runner's own
version, `provision` runs `git archive <tag> docs` into the workspace and records
`pins.docs_source = "tag"`; a missing tag is an error before the agent starts. Report
schema goes to 5 for the new field (ADR-0024 amended in the same PR).

**D27. Questionnaire and counter.** `confidence` becomes two fields: `confidence`
(one of low, medium, high) and `confidence_reason` (prose). A rejected field records a
diagnostic and leaves `collected: true`; `collected: false` means no sheet was found.
`LISTED_ITEM` in `scorecard.py` also matches `(1)` and `(a)` markers. The three
committed scorecards regenerate, and the test that pins the pilot figures updates to
the corrected counts. Changing the definition changes `questionnaire_fingerprint`, so
later runs compare against the pilot as `not comparable: questionnaire`, which the
scorecard already states per row.

**D28. Reports in git, transcripts out.** `corpus/<set>/runs/<run-id>/` holds
`report.json` only; `report.json` gains `session.transcript_sha256`. The batch command
leaves transcripts under `--out`. The 17 committed transcripts are deleted from the
tree in one commit, history is not rewritten, and `corpus/README.md` states the rule.
`tests/scorecard.rs` and `tests/sanitize_pilot.rs` read synthetic fixtures under
`crates/corpus-runner/tests/fixtures/` instead of the committed runs. Langfuse is not
adopted here: shipit's ADR-0013 rejects hosted eval stores, and nothing consumes the
transcripts today. When a re-run needs one, an uploader over the Langfuse SDK maps one
run to one trace, each assistant message to a generation and each acceptance ratio to a
score, and that is a separate decision.

**D29. The batch command.** `corpus-runner batch <archetype>... --framework-version
<v> --out <dir>` as described above, in `crates/corpus-runner/src/main.rs` and a new
`batch.rs`. Serial, because two sandboxed sessions share one host credential broker.
`sanitize-run.py` moves from `corpus/pilot/` to `corpus/` and the batch calls it.

**D17 (shared with PAR01). Evidence-checked gap passes.** An archetype's manifest
`[gaps]` entry may name evidence: `PAR01 = { text = "...", evidence = "uses-crate:clapfig" }`.
A gap case that passes in a workspace whose `Cargo.toml` does not depend on that crate
is reported as `hand-rolled-pass` instead of `unexpected-pass`, and the scorecard counts
it in `hand_rolled_passes`. This is the only way a black-box case can distinguish
"the framework closed the gap" from "the agent rebuilt the feature", which the
completion run showed happens 45 times out of 46.

## Workstreams

**WS01: Evidence out of git.** D28: delete the 17 transcripts, add the sha256 field,
fixture the two tests, move `sanitize-run.py`, rewrite `corpus/README.md`'s layout
section, update `.shipit.toml`'s exemption list. Done when `pixi run test` passes with
no `transcript.jsonl` tracked.

**WS02: Scoring vocabulary.** D23 and D27: the `--help` probe, `contract = "either"`,
`equal_across_modes`, the split `confidence`, tri-state collection, the counter regex,
regenerated scorecards. Done when re-evaluating the committed ghlike, kubelike and
cargolike workspaces (if preserved) or the fixture reports yields no invariant failure
whose reason is one of the three vocabulary defects.

**WS03: Suites and pins.** D24, D25, D26 and D17: the help-assertion rewrite across
validity, ghlike and dockerlike; `files` and `files_absent`; docs from the tag; the
evidence field and `hand-rolled-pass` outcome; report schema 5. Done when validity's
blind re-run passes its rewritten cases and gcloudlike's `config set` case asserts the
written file.

**WS04: The batch command.** D29, plus a `docs/dev` page on running a set. Done when
`corpus-runner batch smoke --out <tmp>` produces the two scorecard files.

WS01 and WS02 can start together; WS03 follows WS02 (shared parser); WS04 follows WS01
and WS03.

## Exit criteria

- One `batch` invocation runs gitlike and cargolike against a published version and
  writes both scorecards outside the checkout.
- No `transcript.jsonl` is tracked; `pixi run test` passes on fixtures.
- Issues #450, #451, #454, #455, #461, #462, #465, #467 closed by their PRs, and the
  ghlike and dockerlike suite edits #460 asked for landed.
- validity re-run committed as report only, with the corrected scorecard.

## Issues

Closes #450, #451, #454, #455, #461, #462, #465, #467. Finishes #460.

## Out of scope

A friction-theme judge (themes stay authored prose), a Langfuse exporter, running blind
sessions in CI (ADR-0036 forbids the host credential there), a "first render" metric,
and every framework-side fix the runs found (those belong to PAR02, PAR01 and PAR03).
