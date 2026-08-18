# The downstream corpus harness

The pilot-phase runner for the robustness corpus
(`docs/spec/robustness-corpus.md`): blind agents implement small CLIs from an
archetype spec and the published standout documentation alone, and every run
produces one structured, reproducible report. The runner is a means, not a
product — it is deliberately the minimum that makes runs reproducible and
comparable.

## Layout

- `archetypes/<name>/spec.md` — the agent-facing behavioral spec.
- `archetypes/<name>/acceptance.toml` — the pre-written acceptance suite
  (binary name, black-box checks, invariant-matrix commands), authored before
  any implementation exists.
- `runs/<run-id>/` — one directory per run: the provisioned `workspace/`, the
  session `transcript.jsonl`, and the durable `report.json`. Runs are
  artifacts, not source: `runs/` is gitignored. Deliberately kept
  demonstration runs live under `demo/` instead (report + transcript only,
  never the workspace).

The runner itself is `crates/corpus-runner`. One command runs the full loop:

```bash
cargo run -p corpus-runner -- run smoke
```

Provision → agent session → questionnaire collection → acceptance suite +
invariant matrix → report. `--agent-cmd` swaps the real agent for anything
else (the walking-skeleton test uses a scripted agent) and
`--framework-version` overrides the crates.io pin; see `corpus-runner run
--help` for the rest.

## Decision: the blindness protocol

Recorded here as a decision because an ADR may follow (spec: "blindness is
fragile"; partial blindness is acceptable if it is *known*).

1. **The workspace contains no framework source.** Provisioning materializes
   exactly: the archetype spec, an instructions file, the rendered exit
   questionnaire, a snapshot of the *published* documentation set (the mdbook
   sources: `index.md`, `intro.md`, `guides/`, `topics/`, `crates/` — never
   ADRs, internal specs, proposals, or dev notes), and a cargo scaffold whose
   standout dependencies are exact-version crates.io pins. No path or git
   dependencies, so cargo cannot resolve into a local checkout, and the
   scaffold declares its own empty `[workspace]` so cargo never adopts an
   enclosing checkout's workspace (a leak the first live smoke run exposed).
2. **The agent runs in an isolated workspace with a scrubbed environment.**
   The session process gets `env_clear()` plus a small recorded allowlist
   (PATH, HOME, cargo/rustup homes, locale, TERM, TMPDIR) — no repo secrets
   reach the produced code, which is treated as untrusted. The allowlist is
   written into the report. Known residue: HOME grants the agent its own
   credentials and caches; that is what makes the session runnable at all.
3. **Blindness is recorded, not assumed.** The exit questionnaire asks two
   dedicated questions — which provided docs were consulted, and what (if
   anything) beyond them: web search, prior knowledge of standout internals,
   other repositories. The answers land verbatim in the report's `blindness`
   section next to the transcript link, so a partially-blind run is a known
   partially-blind run, and runs remain comparable.

## Decision: the run-report schema

`report.json`, `schema_version: 1` (recorded here because an ADR may follow).
Objective results and agent self-assessment are deliberately separate
sections. The shape:

- `schema_version`, `run_id` — identity.
- `archetype` — name plus the sha256 of the exact spec text given to the
  agent.
- `pins` — what makes runs comparable: the crates.io framework version the
  scaffold pinned, the git commit the docs snapshot came from, and the exit
  questionnaire's semantic fingerprint.
- `blindness` — the protocol statement, the environment allowlist, and the
  agent's own account of what it consulted (from the questionnaire).
- `session` — instrumentation: the agent command, wall seconds, exit code,
  attempts, and turns/token counts when the transcript is Claude Code
  stream-json; plus the transcript path (always linked, relative to the run
  directory).
- `acceptance` — objective: whether the produced app built, and one
  pass/fail entry per acceptance check.
- `invariants` — objective: the ROB01 invariant matrix cells (per command ×
  output mode: exit status, unresolved-tag markers, styling-preserves-layout,
  JSON well-formedness).
- `questionnaire` — subjective: whether a valid sheet was collected, its
  diagnostics, and the decoded answers keyed by stable field id.

A run that completes the loop always writes a report, even when every check
fails — failing checks are findings, not runner errors.
