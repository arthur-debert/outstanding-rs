# Archetype: `tflike` (gap spec)

A plan/apply tool in the terraform mold: it reads a desired-state config, compares it
against recorded state, reports the difference as a *plan*, and *applies* it. Its machine
surface is an NDJSON diagnostic-and-event stream plus detailed exit codes.

**This is a gap specification.** It describes capability standout does not have (survey
Part C, archetype C4; `docs/spec/robustness-corpus.md`). Its acceptance suite
(`corpus/gap-suites/`) is red on arrival, deliberately, and is authored in two milestone
groups because two different epics own them:

- **Diagnostic milestone** — gates **PAR02** (`docs/spec/parity-machine-contract.md`).
- **Progress milestone** — gates **PAR03** (`docs/spec/parity-terminal-citizenship.md`).

Everything below is written from the CLI user's perspective and is asserted black-box
against a produced binary: argv in, stdout/stderr/exit status out.

## Inputs

**Config file** (`--config <path>`): line-oriented desired state. Each non-empty line is

```text
resource <name> <desired-state>
```

where `<desired-state>` is `present` or `absent`. Any line not matching this grammar is a
config error; diagnostics for it carry a range pointing at that line of that file.

**State file** (`--state <path>`, default `<config>.state`): the names of currently
present resources, one per line. A missing state file means empty state.

A resource whose name starts with `fail:` parses fine but fails when applied — the
black-box trigger for the handler-error case.

## Output modes

Default output is human-oriented and unspecified here. `--output ndjson` selects the
machine mode every assertion below runs in:

- **stdout is exclusively the NDJSON stream**: every line independently parses as a JSON
  object. No prose, no progress, no ANSI escapes may reach stdout.
- Progress reporting (spinners, step lines) is suppressed entirely in this mode: stderr
  carries no ANSI escape sequences and no progress redraw output.

## The stream

Every stream entry is a single-line JSON object with a `"type"` discriminator:

- `{"type":"version","format_version":<int>}` — first line of every stream.
- `{"type":"planned_change","resource":<name>,"action":"create"|"delete"}` — one per
  difference between desired state and recorded state.
- `{"type":"apply_start","resource":<name>}` / `{"type":"apply_complete","resource":<name>}`
  — apply-lifecycle events, one pair per changed resource, start preceding complete.
- `{"type":"change_summary","add":<int>,"remove":<int>}` — terminal entry of every
  successful plan or apply.
- `{"type":"diagnostic",...}` — see below.

## Diagnostics

Failures are stream entries, never prose:

```json
{"type":"diagnostic","severity":"error","summary":"...","detail":"...",
 "range":{"filename":"<config path as given>","start":{"line":<1-based>,"column":<1-based>}}}
```

`severity` is `"error"` or `"warning"`. `range` is present for config errors and points
at the offending line of the file as passed on the command line. A handler failure at
apply time (a `fail:` resource) yields a well-formed diagnostic entry — the stream stays
parseable; no error prose leaks into it.

## Exit codes

- `plan` / `apply`: 0 on success, 1 on error.
- `plan -detailed-exitcode`: **0** when the plan is empty (no changes), **2** when there
  are changes, **1** on error. The flag is spelled exactly `-detailed-exitcode`
  (single-dash long option, terraform's spelling) — if that spelling is inexpressible in
  the implementing framework, that is itself a finding.

## Commands

```text
tflike plan  --config <path> [--state <path>] [--output ndjson] [-detailed-exitcode]
tflike apply --config <path> [--state <path>] [--output ndjson]
```

`apply` performs the plan's changes and rewrites the state file to match desired state.
