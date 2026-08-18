# The downstream corpus

The corpus pilot (`docs/spec/robustness-corpus.md`) in repository form: the
**archetype roster** — synthetic CLI archetypes with spec-first acceptance
suites — and the **runner** (`crates/corpus-runner`) that has blind agents
implement them from the archetype spec and the published standout
documentation alone, producing one structured, reproducible report per run.
The runner is a means, not a product — it is deliberately the minimum that
makes runs reproducible and comparable.

## Layout

- `archetypes/<name>/` — one archetype package: `spec.md`, `manifest.toml`,
  `acceptance.toml` (formats under "The archetype roster" below). One
  exception: `smoke` is not a roster member — it is the harness's own
  walking-skeleton archetype (spec, "Testing / Verification": "the harness
  itself gets a smoke archetype"), carrying `spec.md` plus an
  `acceptance.toml` in the runner's check schema, and its binary name
  (`smoketable`) deliberately differs from its directory name. The roster's
  structural test exempts it by name.
- `runs/<run-id>/` — one directory per run: the provisioned `workspace/`, the
  per-case `cases/` sandboxes, the session `transcript.jsonl`, and the
  durable `report.json`. Runs are artifacts, not source: `runs/` is
  gitignored. Deliberately kept demonstration runs live under `demo/`
  instead (report + transcript only, never the workspace). Demo transcripts
  are sanitized before committing: host paths become placeholders, session
  ids are zeroed, and the host's tool/plugin/connector inventory is removed
  from the init event.
- `pilot/` — the pilot execution's committed artifacts (ROB03-WS04): one
  `runs/<run-id>/` per pilot run (report + sanitized transcript, demo
  rules), and `scorecard.md` — the per-archetype signals, ranked friction
  themes, and validity verdict the blessed-surface (ROB05) ADR round
  consumes.

## The runner

The runner is `crates/corpus-runner`. One command runs the full loop:

```bash
cargo run -p corpus-runner -- run smoke
```

Provision → agent session → questionnaire collection → acceptance suite +
invariant matrix → report. `--agent-cmd` swaps the real agent for anything
else (the walking-skeleton test uses a scripted agent) and
`--framework-version` overrides the crates.io pin; see `corpus-runner run
--help` for the rest.

Every external process (agent, cargo build, produced binary) runs under a
per-phase deadline (`--agent-timeout`, `--build-timeout`, `--check-timeout`,
seconds) — an overrun is killed (whole process group) and recorded in the
report as a finding, so a prompting or looping produced CLI can never
prevent `report.json` from being written.

The runner executes both acceptance schemas: the roster's `[[case]]` suites
below with their full run semantics (per-case sandboxes, the scrubbed
baseline env, pty attachment, scripted stdin, per-case deadlines,
expected-fail mapping), and its own simpler check schema, which only the
`smoke` archetype still speaks.

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
   Symlinks in the docs source are dereferenced only when their target stays
   inside the published docs surface (`docs/` or a crate's `docs/`, which is
   how the mdbook mounts crate docs); any other link is a provisioning
   error, never a silent follow.
2. **Every untrusted-side process runs with a scrubbed environment.** The
   agent session, the cargo build of the produced app, and every produced-
   binary invocation get `env_clear()` plus a small recorded allowlist
   (PATH, HOME, cargo/rustup homes, locale, TERM, TMPDIR) — no repo secrets
   reach the produced code, which is treated as untrusted. The allowlist is
   written into the report. The default Claude session is additionally
   hardened: `--setting-sources ''` keeps host settings and plugins from
   loading and `--strict-mcp-config` keeps MCP servers/connectors from
   attaching. Known residue, recorded rather than eliminated: HOME grants
   the agent its own credentials and caches (what makes the session runnable
   at all, and what keeps cargo caches shared), and neither the agent nor
   the produced code runs inside an OS sandbox — full container isolation is
   deliberately out of the pilot's scope (harness gold-plating); run the
   pilot from a checkout without repo secrets.
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
  scaffold pinned, the git commit the docs snapshot came from, the sha256 of
  the snapshot's actual bytes (the content-true pin a commit alone cannot
  give when the tree is dirty), and the exit questionnaire's semantic
  fingerprint.
- `blindness` — the protocol statement, the environment allowlist, and the
  agent's own account of what it consulted (from the questionnaire).
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
- `invariants` — objective: the ROB01 invariant matrix cells (per command ×
  output mode: exit status, unresolved-tag markers, styling-preserves-layout,
  JSON well-formedness).
- `questionnaire` — subjective: whether a valid sheet was collected, its
  diagnostics, and the decoded answers keyed by stable field id.

A run that completes the loop always writes a report, even when every check
fails — failing checks are findings, not runner errors.

## The archetype roster

Synthetic CLI archetypes in repository form, each one a package of

- **`spec.md`** — the behavioral spec, written from the CLI *user's*
  perspective. This is the only description of the tool a blind implementer
  receives (together with the published standout documentation).
- **`manifest.toml`** — which standout features the archetype uses and, more
  importantly, which feature *interactions* it stresses. The interactions are
  the point: single features are covered by the framework's own suite; the
  corpus exists for the combinations an adopter meets.
- **`acceptance.toml`** — the objective acceptance suite, written **before any
  implementation exists**. Spec-first is a hard rule: if the criteria are
  written after seeing a result, "did it work" stops being an objective
  measure. A structural test (`crates/standout-test/tests/corpus_roster.rs`)
  enforces that no implementation lives under `corpus/archetypes/`.

Archetype names double as binary names: the implementer of `gitlike` produces
a binary called `gitlike`, so acceptance assertions can name their subject
without a per-run indirection.

### Acceptance case format

Every assertion is **black-box against the produced binary** — argv, env,
stdin in; stdout, stderr, exit status, wall time out. Nothing may inspect the
produced source, link against it, or depend on standout internals, so the
suites survive both idiom changes (ROB05) and any framework refactor.

An `acceptance.toml` is:

```toml
schema = 1
archetype = "gitlike"

[[case]]
name = "unique-kebab-case-name"
group = "optional-group-name"        # optional: milestone/topic grouping
stresses = "one line naming the interaction under test"
expected = "pass"                    # "pass" | "fail"
# When expected = "fail" (specced past current capability), both are required:
# gap    = "PAR01"                   # the epic that closes the gap — must be a key of the manifest's [gaps] table
# reason = "why this fails today"

[case.run]
argv = ["log", "--limit", "2"]       # arguments after the binary name
env = { GITLIKE_PAGER = "sed -n 1p" }  # explicit env on top of the scrubbed baseline
tty = []                             # streams attached to a pty: any of "stdin", "stdout", "stderr"
stdin = "piped content"              # omitted = stdin is piped and already at EOF
cwd = "."                            # working directory, relative to the sandbox root
timeout_seconds = 10                 # hard bound; exceeding it fails the case

[case.run.files]                     # sandbox files created before the run
"sub/dir/.gitlike.toml" = "log.limit = 2\n"

[case.expect]                        # at least one assertion is required
exit_code = 0
stdout = "exact bytes\n"             # exact match, LF-normalized
stderr = ""
```

Besides the cases, a suite may carry an `[invariants]` table — the read-only
commands the ROB01 invariant matrix sweeps across output modes
(text/term/json, piped) against the produced binary, on top of the cases:

```toml
[invariants]
commands = [["log"], ["status"], []]   # [] is the naked invocation
```

### Run semantics (what the runner must provide)

- **Scrubbed baseline env.** The process starts from a minimal environment:
  `PATH`, `HOME` pointing into the sandbox, `LANG`/`LC_ALL` = `C.UTF-8`.
  Everything that steers output — `TERM`, `NO_COLOR`, `CLICOLOR`,
  `CLICOLOR_FORCE`, `FORCE_COLOR`, `PAGER`, and any tool-specific variable —
  is **unset unless the case sets it**. A case's env is therefore complete,
  not a delta against whatever the CI host exports.
- **stdin.** Omitted `stdin` (with `"stdin"` not in `tty`) means
  piped-and-at-EOF — the adversarial non-interactive default. A string value
  is scripted input, and `tty` decides its transport: on a pipe it is the
  piped content followed by EOF; when `tty` includes `"stdin"` it is written
  to the pty as keystrokes (as if typed, already newline-terminated), after
  which terminal EOF is sent. The pty form is how attended interactive flows —
  prompt answers, confirmations — are driven. `"stdin"` in `tty` with no
  `stdin` string is an attended terminal that never sends anything.
- **tty.** Streams listed in `tty` are attached to a pseudo-terminal (the
  ROB01 harness's pty seam); all others are pipes. Pty captures are normalized
  to LF before comparison.
- **timeout_seconds** is mandatory and is itself an assertion: a run that
  exceeds it fails the case. This is how "must never hang" is expressed.

### Assertion vocabulary (all objective, nothing subjective)

| Key | Meaning |
| --- | --- |
| `exit_code` | exact process exit status |
| `stdout`, `stderr` | exact stream contents, LF-normalized |
| `stdout_json` | stdout parses as JSON and is *semantically* equal to this JSON string (key order and whitespace irrelevant) |
| `stdout_contains`, `stderr_contains` | every listed substring occurs in the stream |
| `stdout_not_contains`, `stderr_not_contains` | no listed substring occurs in the stream |

Prefer `stdout` (exact). Use `stdout_json` for machine output, where byte
layout is an implementation detail but content is not. Use the `contains`
family only where exactness would pin something the spec deliberately leaves
to the implementer — e.g. asserting *that* ANSI styling is present
(the two-byte CSI introducer, `ESC` `[`, written `\u001b[` in TOML) without pinning a theme's exact colors.

### Expected-fail cases

`expected = "fail"` marks a criterion **specced past current framework
capability**: the case is written as if the capability existed, and its
failure today reads as a framework gap — signal for the named parity epic —
rather than a spec defect. The runner reports these as *expected-fail* (and an
unexpected pass as news), never as suite errors. Gap-only archetypes
(`tflike`, `jjlike`, landed by ROB03-WS03) use the same marker per case.

### Manifest format

```toml
[archetype]
name = "gitlike"          # must match the directory name
survey = "C1"             # roster entry in the 2026-08-16 survey (Part C)
summary = "one line"
status = "in-capability"  # or "partially-past-capability" (gaps table says where)

[features]
used = ["dispatch.subcommands", "output-modes", "..."]

[[interactions]]
id = "kebab-case-id"
stresses = ["feature A", "feature B"]
description = "why this combination is where bugs live"
cases = ["case-name", "..."]   # acceptance cases that exercise it

[gaps]                          # only for partially-past-capability archetypes
PAR01 = "what is specced past current capability, and why on purpose"
```

### The pilot roster

| Archetype | Survey | Shape |
| --- | --- | --- |
| `gitlike` | C1 | porcelain/plumbing split, config layering by cwd walk-up, pager |
| `systemdlike` | C5 | naked default command, `--plain`/`--no-legend`, color/pager env discipline |
| `formlike` | C12 | questionnaire-driven provisioning under full non-interactivity |
| `ghlike` | C2 | deep command nesting with machine JSON and field selection |

Two gap-only archetypes (WS03) sit beside the pilot four, every acceptance
case `expected = "fail"`:

| Archetype | Survey | Shape |
| --- | --- | --- |
| `tflike` | C4 | plan/apply: NDJSON diagnostic/event stream, detailed exit codes, progress suppression |
| `jjlike` | C9 | user-supplied runtime templates as untrusted input |

Their byte-precise, runnable-today suites live in `corpus/gap-suites/` (see
its README for the expected-fail semantics under plain `pixi run test`).

The pilot's execution artifacts — committed run reports and the scorecard —
live under `pilot/` (see Layout above).
