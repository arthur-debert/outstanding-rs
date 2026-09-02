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
  `docs/spec/parity-machine-contract.md`). All seven assertions are promoted and the
  gate reads `closed`; the group runs green against the in-repo fixture.
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

## The in-repo tflike fixture

`src/bin/tflike.rs` is the tflike binary the suites run against under plain
`pixi run test`: the workspace's `.cargo/config.toml` sets `CORPUS_TFLIKE_BIN` to
the fixture cargo builds beside the suites (`target/debug/tflike`), and a value
already in the environment wins, so pointing the variable at another produced
binary still works. Under a custom `CARGO_TARGET_DIR` export the variable yourself.
The harness library links nothing; the fixture is the one target in this package
built on standout.

The fixture carries exactly the capability its epics have landed, so the assertions
still wrapped keep failing against it: `apply` emits no lifecycle events and reports
its steps as stderr prose in every mode (PAR03). A promoted assertion resolves the binary with
`corpus_gap_suites::required_binary`, which panics — a broken suite — rather than
skipping when the variable names nothing.
