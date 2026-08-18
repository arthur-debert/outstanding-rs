# Corpus gap-spec acceptance suites (expected-fail)

Executable definitions-of-done for the parity epics, authored before those epics start.
The two gap archetypes (`corpus/archetypes/tflike/`, `corpus/archetypes/jjlike/`)
describe capability standout does not have; their acceptance suites here are red on
arrival, deliberately.

The archetypes also sit in the roster proper: each carries the roster's three files
(`spec.md`, `manifest.toml`, `acceptance.toml`, all `expected = "fail"` cases — see
`corpus/README.md`) for the WS01 runner. The suites *here* are the byte-precise form
of the same criteria — per-line NDJSON parseability, exact byte offsets, state-file
rewrites — and the form that runs today under plain `pixi run test`.

**Gating note.** These suites gate the parity epics and must be green before those
epics close:

- `tests/tflike_diagnostic.rs` — gates **PAR02** (machine contract,
  `docs/spec/parity-machine-contract.md`). PAR02 is done when this group turns green.
- `tests/tflike_progress.rs` — gates **PAR03** (terminal citizenship,
  `docs/spec/parity-terminal-citizenship.md`). PAR03 is done when this group turns green.
- `tests/jjlike.rs` — gates the future runtime-templates parity epic, whose code is not
  yet minted (codes are human-assigned; see the ownership note in
  `corpus/archetypes/jjlike/spec.md`).

## Expected-fail semantics

Each assertion runs through `corpus_gap_suites::expect_gap`, which distinguishes
"gap not yet closed" from "suite broken":

- **No binary produced yet** (the archetype's `CORPUS_*_BIN` env var is unset, or points
  at nothing): the assertion reports **expected-fail** with its gate and reason, and the
  test passes. This is today's steady state — `pixi run test` runs the suites green,
  as expected-fail, not as errors.
- **Binary present, assertion fails**: still **expected-fail** — the gap is open.
- **Binary present, assertion passes**: the test **fails loudly** ("unexpected pass") so
  a closed gap is promoted — remove the `expect_gap` wrapper and the assertion becomes a
  plain requirement.
- **Suite broken** (an existing binary cannot be spawned, fixture IO fails): the harness
  panics — an *error*, distinct from expected-fail, so a rotten suite never hides as
  "gap not yet closed".

## The closed-gap tripwire (`gaps.toml` + `tests/tripwire.rs`)

Without a produced binary every assertion short-circuits as expected-fail, so a
plain green CI run says nothing about whether a gap quietly closed. Two pieces keep
that silence honest, both running under `pixi run test`:

- **The ledger** (`gaps.toml`) lists every milestone group with its owning epic,
  binary env var, test file, `status`, and `armed` assertion count.
  `tests/tripwire.rs` enforces it against the suite sources: the armed count must
  match the `expect_gap` call sites exactly (a tripwire cannot be deleted or added
  without the ledger recording it), an `open` gate must still carry its wrappers,
  and a `closed` gate must carry none. Closing an epic therefore *requires* the
  promotion edit — flip the gate to `closed` and remove the wrappers in the same
  change, or CI is red either way. This is the epic-close checklist, enforced by
  test rather than memory.
- **The simulation** (`tripwire.rs::a_gap_case_that_passes_fails_loudly`) runs the
  expected-fail machinery against a binary that already has a gap's behavior — a
  silently closed gap, manufactured on purpose — and asserts the run fails loudly
  with the gate name. The loud path itself is under test, permanently.

Detection still happens where it always did: run the suites against a produced
binary and an unexpected pass fails the run. The tripwire makes sure that signal
cannot be skipped at epic close and that the net's assertion inventory cannot
erode unnoticed.

## Running against a produced binary

```bash
CORPUS_TFLIKE_BIN=/path/to/tflike CORPUS_JJLIKE_BIN=/path/to/jjlike cargo nextest run -p corpus-gap-suites
```

Every assertion is black-box — argv in, stdout/stderr/exit status out — so the idiom
changes of ROB05 (blessed surface) cannot invalidate it.
