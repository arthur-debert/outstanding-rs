# The downstream corpus

The corpus pilot (`docs/spec/implemented/robustness-corpus.md`) in repository form: the
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
  `acceptance.toml` in the same case schema as every roster suite, but no
  `manifest.toml`. The roster's structural test exempts it by name.
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

A session that has to authenticate needs `--broker` (the run-credential
broker below); it reads the host Claude subscription's credential, so the
host CLI must be logged in, and the agent command must be spawnable without
a shell:

```bash
cargo run -p corpus-runner -- run validity --broker \
  --runs-dir /tmp/standout-corpus-runs-validity
```

Run workspaces default to the system temporary directory. `--runs-dir` may
override it, but the runner refuses a directory beneath the framework
checkout: a nested workspace would let parent traversal and Git discovery
cross the blindness boundary.

Every external process (agent, cargo build, produced binary) runs under a
deadline — an overrun is killed (whole process group) and recorded in the
report as a finding, so a prompting or looping produced CLI can never
prevent `report.json` from being written. `--agent-timeout` and
`--build-timeout` (seconds) bound their phases; `--check-timeout` bounds
each invariant invocation only — acceptance cases are deliberately outside
its reach, each governed by its own authored `timeout_seconds`.

The runner executes one acceptance schema: the `[[case]]` suites below with
their full run semantics (per-case sandboxes, the scrubbed baseline env,
pty attachment, scripted stdin, per-case deadlines, expected-fail mapping).
Every archetype — `smoke` included — speaks it.

## Decision: the blindness protocol

Recorded here as a decision and minted as
[ADR-0023](../docs/adr/0023-the-corpus-blindness-protocol.md) (spec:
"blindness is fragile"; partial blindness is acceptable if it is *known*).

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
   inside an exact published root (`docs/index.md`, `docs/intro.md`,
   `docs/guides`, `docs/topics`, or `crates/<name>/docs`, which is how the
   mdbook mounts crate docs). A guide cannot link into `docs/spec` and smuggle
   internal material into the snapshot; every other link is a provisioning
   error, never a silent follow.
2. **Every untrusted-side process has its own environment and kernel boundary.**
   The agent session, cargo build, and every produced-binary invocation get
   `env_clear()` plus a small recorded key set. HOME, CARGO_HOME, and TMPDIR
   point to separate disposable phase directories; their host values are
   never inherited. The processes and all descendants also run inside macOS
   Seatbelt or Linux Landlock. The policy admits the phase workspace,
   disposable home, system runtime, and selected toolchain paths while
   excluding source and host user-data roots; macOS Keychain brokers are
   denied too. A pre-run probe must prove that the checkout and host home are
   unreadable or the run refuses to start. The default Claude session is
   additionally hardened: `--setting-sources ''` keeps host settings and
   plugins from loading and `--strict-mcp-config` keeps MCP
   servers/connectors from attaching. It also gets `CLAUDE_CODE_TMPDIR`
   pointed at its disposable home, because that backend keeps its shell
   snapshots and tool sockets under `/tmp` by default: without it the write
   policy denies that directory and the session loses its shell, writing
   code it can never build. An agent backend that requires a host
   HOME, environment token, or Keychain item fails closed rather than
   exposing that credential to agent-invoked build scripts.

   The one exception, for the agent phase only, is the **run-credential
   broker** (`--broker`, ADR-0023's ROB07-WS01 amendment): a loopback
   forward proxy the runner holds on the host side, outside every sandbox,
   for as long as the agent session runs. It reads the host Claude
   subscription's OAuth token from the host credential store — the runner
   is unsandboxed, so the Seatbelt Keychain denial above is untouched — and
   injects the authorization into each forwarded request. Where that token
   goes is fixed rather than configured: a credential read from the host
   store forwards to the Anthropic API and nowhere else, so no flag and no
   auth-failure retry can aim it at a destination somebody chose; a test
   double's upstream comes with a credential the test supplied. The agent's
   environment carries `ANTHROPIC_BASE_URL` and a placeholder token only,
   so the credential never enters the agent's process tree. Before reading
   a request byte the broker resolves the connection's owner from the OS
   socket tables and serves only the agent process itself, holding the
   connection on a close-on-exec descriptor; a build script's own
   connection resolves to the build script, and the exec that started it
   left it no descriptor to reuse. A brokered session is therefore spawned
   directly rather than through a shell, since the pid the runner spawns
   has to be the pid that connects. When the agent session ends, the broker
   closes the connections it is still serving and kills the upstream
   transports they started, so no request outlives the session holding the
   credential and an agent that timed out does not leave the runner waiting
   on an unresponsive API. What the run admitted is written into
   `blindness.credential_exceptions`.
3. **Blindness is recorded, not assumed.** The exit questionnaire asks two
   dedicated questions — which provided docs were consulted, and what (if
   anything) beyond them: web search, prior knowledge of standout internals,
   other repositories. The answers land verbatim in the report's `blindness`
   section next to the transcript link, so a partially-blind run is a known
   partially-blind run, and runs remain comparable.

## Decision: the run-report schema

`report.json`, `schema_version: 3` (recorded here and minted as
[ADR-0024](../docs/adr/0024-the-corpus-run-report-schema.md)). Version 3 is
one bump carrying every shape change over version 2: it replaced the single
`isolation_backend` word with a per-capability isolation record, dropped the
producerless `session.attempts` counter, and removed the retired check
schema's parallel `checks` vector. Committed schema-2 evidence still loads,
unrewritten, through the typed historical-report path re-evaluation uses.
Objective results and agent self-assessment are deliberately separate
sections. The shape:

- `schema_version`, `run_id` — identity.
- `archetype` — name plus the sha256 of the exact spec text given to the
  agent.
- `pins` — what makes runs comparable: the crates.io framework version the
  scaffold pinned, the git commit the docs snapshot came from, the sha256 of
  the snapshot's actual bytes (the content-true pin a commit alone cannot
  give when the tree is dirty), the exact acceptance-suite hash, and the exit
  questionnaire's semantic fingerprint.
- `evaluation` — whether this was a full run or an isolated re-evaluation,
  the isolation record of the check boundary (backend, filesystem model,
  and the policy-derived network state: `denied` when the requested denial
  is kernel-enforced, `denial-requested-but-unsupported` on Landlock ABI
  v1, which is filesystem-only — the report says so rather than reading
  stronger than the kernel guarantee), and the exact produced-binary hash.
- `blindness` — the protocol statement, environment key set, the isolation
  record for the agent/build boundary (its network state is
  `allowed-by-policy`: those phases fetch crates.io), credential
  exceptions, and the agent's own account of what it consulted (from the
  questionnaire).
- `session` — instrumentation: the agent command, wall seconds, exit code,
  whether the session hit its deadline (`timed_out`), and turns/token counts
  when the transcript is Claude Code stream-json; plus the transcript path
  (always linked, relative to the run directory).
- `acceptance` — objective: whether the produced app built, and one entry
  per suite case, each carrying the case's `expected` marker and its
  `outcome` (`pass`, `fail`, `expected-fail`, or `unexpected-pass`, the
  news of a gap silently closed) plus the authored `stresses`/`gap`/
  `reason` context so the report reads without the suite beside it.
- `invariants` — objective: the fixed ROB01 plan (command × output mode ×
  color × compiled theme × check). Every identity has `pass`, `fail`,
  `not-run`, or `not-applicable`; reports never improve a denominator by
  omitting a cell.
- `questionnaire` — subjective: whether a valid sheet was collected, its
  diagnostics, and the decoded answers keyed by stable field id.

A run that completes the loop always writes a report, even when every case
fails — failing cases are findings, not runner errors.

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
  measure. A structural test (`crates/corpus-runner/tests/corpus_roster.rs`)
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
stdout_lines_end_with_once = ["<id:name>"] # each suffix ends exactly one non-empty line
```

Besides the cases, a suite may carry a declarative ROB01 matrix. The global
axes are the whole plan: every command runs on every global axis
combination, and each command declares only whether its output is
framework-rendered or intentionally opaque bytes:

```toml
[invariants]
modes = ["text", "term", "json"]
colors = ["off", "on"]

[[invariants.theme]]
name = "application"                  # the theme compiled into this binary

[[invariants.command]]
argv = ["log"]
contract = "rendered"                 # markers, layout, and JSON apply

[[invariants.command]]
argv = ["ref-list"]
contract = "opaque-bytes"             # modes preserve text bytes
```

Color is explicit and deterministic: `off` sets no-color controls; `on` sets
terminal capability and force-color controls. A produced binary cannot swap
its compiled application theme, so suites name the applicable theme rather
than fabricating a runtime variant. Reports still record all five check
identities per axis cell and mark incompatible checks (for example, JSON
parsing of opaque plumbing bytes) `not-applicable`. If a build or required
baseline does not run, the planned identities remain present as `not-run`.

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
| `stdout_row_contains` | every value in each group co-occurs on one single stdout line (row association, e.g. a star with *its* constellation and magnitude) |
| `stdout_json_rows` | stdout parses as JSON and every value in each group co-occurs among the scalars of one single JSON array element (numbers match their decimal literal) |
| `stdout_not_contains`, `stderr_not_contains` | no listed substring occurs in the stream |
| `stdout_lines_end_with_once` | each suffix terminates exactly one non-empty stdout line |

Every listed string and every row group must be non-empty: an empty group or
empty substring matches any output, so it would silently assert nothing —
the parser rejects it at load time.

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

One method-coverage archetype sits beside the product roster. It is not a
survey Part C CLI; it exists so the known-edge validity check (#365) can
pin all three known-edge families (including the two the ROB03 pilot did
not independently rediscover). Its spec includes an implementer
construction contract (registration order, the single registered
template name, an incomplete app theme) because those edges are
invisible on a happy-path product spec.

| Archetype | Survey | Shape |
| --- | --- | --- |
| `validity` | validity | missing/mistyped template name, registration order, incomplete theme × framework help |
