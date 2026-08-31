# The corpus run-report schema

Every corpus run writes one `report.json` in which objective results and
agent self-assessment are deliberately separate sections — the reader must
never have to untangle what the harness measured from what the agent
claimed. This ADR transcribes the decision recorded in `corpus/README.md`
("Decision: the run-report schema") during the ROB03 corpus pilot, where it
was `schema_version: 2`; the corpus cleanup epic (ROC02) mints it in ADR
form and the amendment below records the `schema_version: 3` shape current
runs actually write. It is a transcription of decided material, not new
design; the serde structs in `crates/corpus-runner/src/report.rs` are the
schema's executable statement.

Amendment (ROC02-WS02, ROC02-WS03): `schema_version: 3` is one bump
carrying every shape change over version 2 — it replaced the single
isolation-backend word with a per-capability isolation record, dropped the
producerless `session.attempts` counter, and removed the retired check
schema's parallel `checks` vector; committed schema-2 evidence still loads,
unrewritten, through the typed historical-report path re-evaluation uses. The shape below is the version-2 transcription this ADR minted;
`report.rs` remains authoritative for the current shape.

The shape:

- `schema_version`, `run_id` — identity.
- `archetype` — name plus the sha256 of the exact spec text given to the
  agent.
- `pins` — what makes runs comparable: the crates.io framework version the
  scaffold pinned, the git commit the docs snapshot came from, the sha256 of
  the snapshot's actual bytes (the content-true pin a commit alone cannot
  give when the tree is dirty), the exact acceptance-suite hash, and the exit
  questionnaire's semantic fingerprint.
- `evaluation` — whether this was a full run or an isolated re-evaluation,
  the enforced backend, and the exact produced-binary hash.
- `blindness` — the protocol statement, environment key set, isolation
  backend, credential exceptions, and the agent's own account of what it
  consulted (from the questionnaire); the protocol itself is ADR-0023.
- `session` — instrumentation: the agent command, wall seconds, exit code,
  whether the session hit its deadline (`timed_out`), attempts, and
  turns/token counts when the transcript is Claude Code stream-json; plus
  the transcript path (always linked, relative to the run directory).
- `acceptance` — objective: whether the produced app built, and one entry
  per suite item — `checks` (pass/fail) for the check schema, `cases` for
  roster suites, each carrying the case's `expected` marker and its
  `outcome` (`pass`, `fail`, `expected-fail`, or `unexpected-pass`, the
  news of a gap silently closed) plus the authored `stresses`/`gap`/
  `reason` context so the report reads without the suite beside it.
- `invariants` — objective: the fixed ROB01 plan (command × output mode ×
  color × compiled theme × check). Every identity has `pass`, `fail`,
  `not-run`, or `not-applicable`; reports never improve a denominator by
  omitting a cell.
- `questionnaire` — subjective: whether a valid sheet was collected, its
  diagnostics, and the decoded answers keyed by stable field id.

A run that completes the loop always writes a report, even when every check
fails — failing checks are findings, not runner errors.

Amendment (ROB07-WS02): `schema_version: 4` makes the agent side of a run
recorded state, so comparisons can state their delta. The report gains a
provenance block carrying the backend, the executable version, the model id
(as requested, and as observed in the transcript), the session prompt, and
the runtime settings the runner passes. One bump to `schema_version: 4`
carries it; committed schema-2 and schema-3 evidence still loads,
unrewritten, through the typed historical-report path, and `report.rs`
remains authoritative for the current shape. Where a re-run cannot
reproduce the pilot's setup — a retired model, a changed backend — the
scorecard comparison states the delta beside every affected figure and the
result is reported as observational, not as evidence the framework
improved.
