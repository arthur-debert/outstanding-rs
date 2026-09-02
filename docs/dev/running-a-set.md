# Running a corpus set

`corpus-runner batch` runs several archetypes through the full blind-agent
loop in one command, sanitizes each run's evidence, and writes a scorecard
for the set:

```bash
cargo run -p corpus-runner -- batch gitlike cargolike \
  --framework-version 10.0.0 --broker --out ~/corpus-runs/par01
```

Archetypes run serially, in the order given — two sandboxed sessions cannot
share one host credential broker. Each archetype gets the same treatment
`corpus-runner run` gives one: provision, agent session, questionnaire,
acceptance suite and invariant matrix, `report.json`. `--broker`,
`--runs-dir`, `--agent-cmd`, `--agent-timeout`, `--build-timeout` and
`--check-timeout` all mean what they mean for `run` and apply to every
archetype in the set; see `corpus-runner run --help`.

A failed run is recorded and the batch moves on to the next archetype; the
command's own exit status is non-zero if any run failed to complete.

## What lands under `--out`

```text
<out>/
  <archetype>-<timestamp>/
    report.json
    transcript.jsonl
  scorecard.md
  scorecard.json
```

One directory per completed run, sanitized in place by
`corpus/sanitize-run.py`: host paths and session ids are scrubbed, and
`report.json`'s `session.transcript_sha256` is recomputed to match the
sanitized transcript. `transcript.jsonl` sits beside `report.json` under
`--out` and nowhere else — a run's transcript never enters the repository
(see `corpus/README.md`, Layout).

`scorecard.md` is the objective table `corpus/scorecard.py` renders by
default; `scorecard.json` is the same rows in the script's `--json` form,
carrying the pins and provenance a later run compares itself against. Both
come from the same sanitized reports under `--out`.

## Committing a run's evidence

Committing a set is one copy, nothing else — `report.json` alone, never the
transcript beside it:

```bash
mkdir -p corpus/<set>/runs/<archetype>-<timestamp>
cp ~/corpus-runs/par01/<archetype>-<timestamp>/report.json \
  corpus/<set>/runs/<archetype>-<timestamp>/
```

`corpus/<set>/runs/<run-id>/` holds `report.json` only — the transcript
stays out of the repository. Regenerate a scorecard for a committed set with
`corpus/scorecard.py <set>=corpus/<set>/runs`.
