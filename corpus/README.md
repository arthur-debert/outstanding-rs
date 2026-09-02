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
  per-case `cases/` sandboxes, the session transcript, and the durable
  `report.json`. Runs are artifacts, not source: `runs/` is gitignored.
- `demo/`, `pilot/`, `rerun/`, `completion/` — committed run evidence: one
  `runs/<run-id>/` per run, holding `report.json` only. A run's transcript
  never enters the repository — it stays under `--runs-dir` or the system
  temporary directory (a set's batch invocation writes it under its own
  `--out` directory instead); `report.json`'s `session.transcript_sha256`
  is its fingerprint. `sanitize-run.py` is the sanitizer every committed
  report goes through: host paths become placeholders and session ids are
  zeroed. `demo/` is a deliberately kept demonstration set; `pilot/` is the
  pilot four's first runs against 8.1.1, plus `scorecard.md` — the
  per-archetype signals, ranked friction themes, and validity verdict;
  `rerun/` is the same four against 9.0.0, plus `scorecard.md` v2 — the
  re-run beside the pilot, with the agent delta between them stated;
  `completion/` is the completion six's **first** blind runs, against the
  published 9.0 line, plus `scorecard.md` as a first data point rather than
  a comparison.
- `scorecard.py` — computes a scorecard's objective table from committed
  reports (`scorecard.py pilot=corpus/pilot/runs rerun=corpus/rerun/runs completion=corpus/completion/runs`).
  Every scorecard's figures come from this one script under one set of
  counting rules; its own test checks that the pilot's reports still
  reproduce the pilot scorecard's published numbers. Every row also carries
  the pins and the agent provenance the comparison rests on: a row whose
  spec, acceptance suite, questionnaire or agent differs from the first row
  its archetype has is marked not comparable, and a note under the table
  states the difference, so a figure a changed question produced cannot read
  as a framework result. Reports written before schema 4 state no
  provenance, so the script recovers it from the run's own two sources — the
  recorded command and the transcript — and marks the cell `(recovered)`.

## Where accepted implementations go

An implementation that passes its acceptance suite is committed to
[arthur-debert/standout-corpus](https://github.com/arthur-debert/standout-corpus),
never here ([ADR-0036](../docs/adr/0036-freeze-accepted-implementations-in-a-built-corpus-repository.md);
the roster's structural test forbids implementation files under `archetypes/`). That repository holds them frozen, redirects their
standout dependencies onto a checked-out framework tree, and runs their suites:
the full roster on a schedule, and a fast subset on every PR here through
`.github/workflows/corpus.yml`. A red build there is a finding about the
framework by default — the members do not change.

The suite a member is checked against is the archetype's `acceptance.toml` in
*this* repository, replayed by `corpus-runner reevaluate` against the binary
built from the frozen sources. Editing a suite here therefore changes what the
corpus asserts, which is the intent: the implementation is frozen, the suite is
not.

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

`corpus-runner batch <archetype>...` runs a whole set through this loop in
one command, one archetype at a time, and sanitizes and scores the results —
see [`docs/dev/running-a-set.md`](../docs/dev/running-a-set.md).

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
   broker** (`--broker`, an amendment to ADR-0023): a loopback
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

`report.json`, `schema_version: 5`
([ADR-0024](../docs/adr/0024-the-corpus-run-report-schema.md)). Committed
schema-2 through schema-4 evidence still loads, unrewritten, through the typed
historical-report path re-evaluation uses. Objective results and agent
self-assessment are deliberately separate sections. The shape:

- `schema_version`, `run_id` — identity.
- `archetype` — name plus the sha256 of the exact spec text given to the
  agent.
- `pins` — what makes runs comparable: the crates.io framework version the
  scaffold pinned, the git commit the docs snapshot came from, the sha256 of
  the snapshot's actual bytes (the content-true pin a commit alone cannot
  give when the tree is dirty), where that snapshot came from
  (`docs_source`: `checkout` when the pin matches the runner's own version,
  `tag` when `--framework-version` names another published version and
  `provision` archived its docs from that version's git tag instead — a
  missing tag refuses the run before the agent starts), the exact
  acceptance-suite hash, and the exit questionnaire's semantic fingerprint.
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
  (always linked, relative to the run directory) and its sha256
  (`transcript_sha256`, absent on a report written before this field
  existed) — the transcript's fingerprint, since the file itself is not
  committed (see Layout above).
- `provenance` — who implemented the run: the backend the runner spawned
  (`backend`, the program's name), the version that backend announced in the
  transcript, the model the command asked for (`model_requested` — absent
  means the run took the backend's default) and the model the transcript
  shows answering (`model_observed`), the session prompt, and the remaining
  settings the runner passed. It is written from the spawned command and the
  transcript alone — the runner never runs the agent executable to ask it
  about itself, which would execute an unknown program on the host outside
  every boundary the run is built on. So the environment those phases carry
  is `blindness.env_allowlist`, the command's own text is
  `session.agent_cmd`, and a field neither source states is absent rather
  than guessed: a session spawned through a shell command that names no
  single program records nothing it cannot parse, a scripted agent announces
  no version or model, and a re-evaluation keeps a schema-4-or-later source's block
  or, for an older one, states only what the recorded command says. Two runs
  compare as evidence when these match; where they cannot, the comparison
  states the delta and reads as observational.
- `acceptance` — objective: whether the produced app built, and one entry
  per suite case, each carrying the case's `expected` marker and its
  `outcome` (`pass`, `fail`, `expected-fail`, `unexpected-pass` — the news
  of a gap silently closed — or `hand-rolled-pass`, the same news with the
  manifest's `[gaps]` evidence crate absent from the produced app's
  `Cargo.toml`, so the pass was rebuilt by hand rather than closed by the
  framework) plus the authored `stresses`/`gap`/`reason` context so the
  report reads without the suite beside it.
- `invariants` — objective: the fixed invariant plan (command × output mode ×
  color × compiled theme × check). Every identity has `pass`, `fail`,
  `not-run`, or `not-applicable`; reports never improve a denominator by
  omitting a cell.
- `questionnaire` — subjective: `collected` is false only when no sheet was
  found or its structure could not be parsed at all; a sheet that parsed but
  had a field rejected (a diagnostic, dropped from `answers`) still reads
  `collected: true` alongside every answer that did decode. `confidence`
  (`low`/`medium`/`high`) and its free-text `confidence_reason` are separate
  fields, keyed by stable field id like every other answer.

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
suites survive both idiom changes and any framework refactor.

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
files_absent = ["sub/dir/.gitlike.lock"]   # paths that must not exist after the run

[case.expect.files]                  # sandbox paths read back after the run, exact
"sub/dir/.gitlike.toml" = "log.limit = 3\n"  # content, LF-normalized like stdout/stderr
```

A case is one invocation; `[case.run.files]` seeds a precondition, and
`[case.expect.files]`/`files_absent` read the sandbox back afterwards — the
only way a store-mutating command's write path is checked rather than only
its streams. `[case.expect.files]` values are exact content; a key naming a
path the run never wrote is a failure (not silently unasserted), same as a
`files_absent` path that does exist.

Besides the cases, a suite may carry a declarative invariants matrix. The global
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

[[invariants.command]]
argv = ["build"]
contract = "either"           # rendered or opaque-bytes, whichever the binary does, consistently

[[invariants.command]]
argv = ["config", "list"]
contract = "rendered"
equal_across_modes = false    # the content names the output mode
```

A command's own choice can outrun what the suite could know spec-first.
`contract = "either"` accepts whichever of `rendered` or `opaque-bytes` the
produced binary actually satisfies — read from its first evaluated cell
(json-mode output that parses as JSON reads `rendered`; failing that, a
non-text mode whose bytes match the text baseline reads `opaque-bytes`) —
and holds that contract for the rest of the command's cells, rather than
failing an implementation the spec never ruled out. `equal_across_modes =
false` marks a command whose content is deliberately not the same across
modes — the resolved mode is part of what it prints, as `config list`'s
`term.output` row is — so the cross-mode content check (`styling preserves
text layout` for `rendered`, the byte-identity check for `opaque-bytes`)
reads `not-applicable` instead of failing on content the spec asked for.
Every other identity in the cell still runs.

Before the matrix runs at all, the runner invokes the produced binary with
`--help`. An application built with `no_output_flag()` (or
`no_output_file_flag()`) never has to say so in the manifest: the runner
reads the choice from the binary itself, and when `--help` never mentions
`--output`, every planned cell for every command reads `not-applicable`
with reason `no output flag` instead of failing an unknown-flag error on
each one.

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
- **tty.** Streams listed in `tty` are attached to a pseudo-terminal; all
  others are pipes. Pty captures are normalized to LF before comparison.
- **timeout_seconds** is mandatory and is itself an assertion: a run that
  exceeds it fails the case. This is how "must never hang" is expressed.

### Assertion vocabulary (all objective, nothing subjective)

| Key | Meaning |
| --- | --- |
| `exit_code` | exact process exit status |
| `stdout`, `stderr` | exact stream contents, LF-normalized |
| `stdout_json` | stdout parses as JSON and is *semantically* equal to this JSON string (key order and whitespace irrelevant) |
| `stdout_json_subset` | stdout parses as JSON and *carries* this JSON: objects may hold keys the expectation omits, while arrays and scalars must match |
| `stdout_contains`, `stderr_contains` | every listed substring occurs in the stream |
| `stdout_row_contains` | every value in each group co-occurs on one single stdout line (row association, e.g. a star with *its* constellation and magnitude) |
| `stdout_json_rows` | stdout parses as JSON and every value in each group co-occurs among the scalars of one single JSON array element (numbers match their decimal literal) |
| `stdout_not_contains`, `stderr_not_contains` | no listed substring occurs in the stream |
| `stdout_lines_end_with_once` | each suffix terminates exactly one non-empty stdout line |
| `files` | a table of sandbox path → exact content (LF-normalized), read after the process exits |
| `files_absent` | sandbox paths that must not exist after the process exits |

Every listed string and every row group must be non-empty: an empty group or
empty substring matches any output, so it would silently assert nothing —
the parser rejects it at load time.

Prefer `stdout` (exact). Use `stdout_json` for machine output, where byte
layout is an implementation detail but content is not. Reach for
`stdout_json_subset` only when part of a document is deliberately left open —
a framework-owned envelope whose payload another Spec defines — so the case can
require the part the archetype specifies without inventing the rest; asserting
the whole document would pin fields the archetype does not own, and a substring
would not require them to be fields at all. Use the `contains`
family only where exactness would pin something the spec deliberately leaves
to the implementer — e.g. asserting *that* ANSI styling is present
(the two-byte CSI introducer, `ESC` `[`, written `\u001b[` in TOML) without pinning a theme's exact colors.

### Expected-fail cases

`expected = "fail"` marks a criterion **specced past current framework
capability**: the case is written as if the capability existed, and its
failure today reads as a framework gap — signal for the named parity epic —
rather than a spec defect. The runner reports these as *expected-fail* (and an
unexpected pass as news), never as suite errors. Gap-only archetypes
(`tflike`, `jjlike`) use the same marker per case.

### Manifest format

```toml
[archetype]
name = "gitlike"          # must match the directory name
survey = "C1"             # roster entry in the archetype survey (Part C)
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
# or, with a black-box-checkable evidence claim (D17):
# PAR01 = { text = "...", evidence = "uses-crate:clapfig" }
```

A `[gaps]` entry is prose alone, or prose plus `evidence`: a claim the runner
can check against the produced workspace rather than trust at face value.
`uses-crate:<name>` is the only kind today — the runner reads the produced
app's `Cargo.toml` and, for a gap case that passes, reports `hand-rolled-pass`
instead of `unexpected-pass` when the named crate is absent from
`[dependencies]`. A black-box case cannot otherwise tell a framework-supplied
capability from one the agent rebuilt by hand; `scorecard.py` counts these as
`hand_rolled_passes`, separate from the ordinary pass/fail/expected-fail/
unexpected-pass tally.

### The roster

Every directory under `archetypes/` except `smoke` (see Layout) is a roster
member. *Survey* is the archetype's entry in the archetype survey (Part C);
*Shape* is one line — each archetype's own `spec.md` and `manifest.toml` carry
the rest.

**The pilot four** — the archetypes the pilot ran against 8.1.1 and the
re-run against 9.0.0:

| Archetype | Survey | Shape |
| --- | --- | --- |
| `gitlike` | C1 | porcelain/plumbing split, config layering by cwd walk-up, pager |
| `ghlike` | C2 | deep command nesting with machine JSON and field selection |
| `systemdlike` | C5 | naked default command, `--plain`/`--no-legend`, color/pager env discipline |
| `formlike` | C12 | questionnaire-driven provisioning under full non-interactivity |

Their execution artifacts — committed run reports and a scorecard — live under
`pilot/` for the first runs and `rerun/` for the 9.0.0 ones (see Layout above).

**The completion six** — the survey's remaining in-capability shapes,
authored spec-first after the pilot
(`docs/spec/implemented/robustness-corpus-completion.md`); each spec derives
the shape from the survey's capability matrix and states in prose which
interactions it stresses. Their first blind runs, against 9.0.0, live under
`completion/` (see Layout above).

| Archetype | Survey | Shape |
| --- | --- | --- |
| `kubelike` | C3 | verb-over-resource-kind dispatch, kinds as data with aliases and scope, an `-o` vocabulary wider than a render mode |
| `cargolike` | C6 | layered config: per-type merge across every discovered file, key↔env mapping, framework settings on the same ladder |
| `gcloudlike` | C7 | named configuration sets selected per invocation, layered under property env vars and flags |
| `dockerlike` | C8 | legacy verbs beside management commands, `--quiet` as a data-shape contract, display truncation vs machine output |
| `brewlike` | C10 | package-manager query client: nested dependency data, empty results, a versioned machine contract |
| `pnpmlike` | C11 | progress on stderr by stream attendance, two silencers for two channels, child-process passthrough |

`crates/corpus-runner/tests/hermetic_authored_roster_loop.rs` drives all six
through the full loop against a produced binary that builds and then fails
every invocation, so a suite that cannot execute is a red test rather than a
wasted blind run.

**Two gap-only archetypes**, every acceptance case `expected = "fail"`:

| Archetype | Survey | Shape |
| --- | --- | --- |
| `tflike` | C4 | plan/apply: NDJSON diagnostic/event stream, detailed exit codes, progress suppression |
| `jjlike` | C9 | user-supplied runtime templates as untrusted input |

Their byte-precise, runnable-today suites live in `corpus/gap-suites/` (see
its README for the expected-fail semantics under plain `pixi run test`).

**One method-coverage archetype.** `validity` is not a survey Part C CLI; it
exists so the known-edge validity check can pin all three known-edge
families. Its spec carries an implementer construction contract (registration order, the
single registered template name, an incomplete app theme) because those edges
are invisible on a happy-path product spec.

| Archetype | Survey | Shape |
| --- | --- | --- |
| `validity` | validity | missing/mistyped template name, registration order, incomplete theme × framework help |
