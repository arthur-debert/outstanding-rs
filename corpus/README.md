# The Downstream Corpus — Archetype Roster

This directory is the roster half of the corpus pilot
(`docs/spec/robustness-corpus.md`): synthetic CLI archetypes in repository
form, each one a package of

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

## Acceptance case format

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
# gap    = "PAR01"                   # the program epic that closes the gap
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

### Run semantics (what the WS01 runner must provide)

- **Scrubbed baseline env.** The process starts from a minimal environment:
  `PATH`, `HOME` pointing into the sandbox, `LANG`/`LC_ALL` = `C.UTF-8`.
  Everything that steers output — `TERM`, `NO_COLOR`, `CLICOLOR`,
  `CLICOLOR_FORCE`, `FORCE_COLOR`, `PAGER`, and any tool-specific variable —
  is **unset unless the case sets it**. A case's env is therefore complete,
  not a delta against whatever the CI host exports.
- **stdin.** Omitted `stdin` means piped-and-at-EOF (the adversarial
  non-interactive default). A string value is piped content followed by EOF.
  `"stdin"` in `tty` means an attended terminal instead.
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

## Manifest format

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

## The pilot roster

| Archetype | Survey | Shape |
| --- | --- | --- |
| `gitlike` | C1 | porcelain/plumbing split, config layering by cwd walk-up, pager |
| `systemdlike` | C5 | naked default command, `--plain`/`--no-legend`, color/pager env discipline |
| `formlike` | C12 | questionnaire-driven provisioning under full non-interactivity |
| `ghlike` | C2 | deep command nesting with machine JSON and field selection |

Out of scope here: the runner (WS01), the gap-spec suites `tflike`/`jjlike`
(WS03), and pilot execution (WS04).
