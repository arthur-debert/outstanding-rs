# `pnpmlike` — behavioral spec

`pnpmlike` is a workspace script runner in the pnpm mold. It reads a small package
manifest, "installs" the packages it declares (nothing is fetched — installing is
simulated work), and runs the scripts it declares as child processes. Its distinguishing
trait is that it speaks on three separate channels: **data** on stdout, **progress** on
stderr through a selectable *reporter*, and **logs** (warnings and info) on stderr
through a level. The two stderr channels are silenced by two different switches.

## What this archetype stresses

The survey's original sketch for C11 is lost with the 2026-08-16 session record. This
spec is reconstructed from the survey's capability matrix, where C11 `pnpmlike` is the
roster's **reporter/quiet matrix** (`docs/spec/parity-terminal-citizenship.md`, goals and
"Further Notes"), and it is written to stress five interactions rather than to reproduce
pnpm:

1. **Reporter selection against stream attendance.** The reporter writes to stderr, so
   it degrades on *stderr's* attendance — not stdout's. `pnpmlike install > out.txt` in
   a terminal still redraws; `pnpmlike install 2> log` does not.
2. **Progress against machine output.** A machine output mode must never carry progress:
   stdout stays exactly one document whichever reporter is running, and selecting a
   machine mode switches the reporter to a machine form of its own.
3. **Two silencers, two channels.** `--reporter silent` silences progress and keeps
   warnings; `-q` silences warnings and keeps progress; `--silent` silences both. A
   framework that routes both through one warning channel cannot express this.
4. **A child process against the output pipeline.** `run` hands the terminal to a script:
   its bytes reach stdout and stderr unchanged in every output mode, and its exit status
   becomes `pnpmlike`'s.
5. **Four ways of having no answer.** An absent manifest is an empty workspace (exit 0), a
   malformed one is a located domain error (exit 1), an undeclared script is exit 3, and a
   bad flag value is a usage error (exit 2). Collapsing any pair makes the tool
   unscriptable.

Everything below is written from the CLI user's perspective and is asserted black-box
against the produced binary: argv, environment and sandbox files in; stdout, stderr and
exit status out.

## The manifest

`pnpmlike.pkg` in the current directory, or the file named by `--manifest <path>`. Every
command that reads the manifest takes `--manifest`, `run` included: it selects the input
for the whole run, and when it is given the default `pnpmlike.pkg` is not read at all. It
is line-oriented. Blank lines and lines whose first non-space character is `#` are ignored.
Every other line is one directive:

```text
dep <name> <version>
script <name> <command...>
```

`<version>` is a single token; a `dep` whose version is exactly `*` is *unpinned* (see
"The log channel"). A `script` line's command is the rest of the line, verbatim.

A line matching neither directive is a manifest error: exit 1, nothing on stdout, and a
message on stderr naming the file as given and the 1-based line number, spelled
`<file> line <n>` (for example `error: pnpmlike.pkg line 2: unrecognized directive`).

**A missing manifest is not an error.** It means an empty workspace: no packages, no
scripts.

## Commands

```text
pnpmlike list    [--manifest <path>]
pnpmlike install [--manifest <path>]
pnpmlike run <script> [--manifest <path>] [-- <args>...]
```

### `pnpmlike list`

One `<name> <version>` line per declared package, in manifest order, and nothing else.
An empty workspace prints nothing at all and exits 0. Under `--output json` the document
is:

```json
{"packages":[{"name":"alpha","version":"1.0.0"},{"name":"beta","version":"2.3.1"}]}
```

`list` reports no progress: it is instantaneous by construction.

### `pnpmlike install`

Installs every declared package in manifest order — one *step* per package (see "The
reporter") — and then prints the summary, which is the whole of its stdout:

```text
2 packages installed.
```

Under `--output json`:

```json
{"installed":[{"name":"alpha","version":"1.0.0"},{"name":"beta","version":"2.3.1"}],"count":2}
```

An empty workspace installs nothing, reports no steps, and prints `0 packages installed.`

### `pnpmlike run <script> [-- <args>...]`

Runs the named script as `/bin/sh -c "<command>" pnpmlike <args>...`, so arguments after
`--` are the script's positional parameters (`$1`, `$2`, … and `$*`). Flags before `--`
belong to `pnpmlike`, not to the script.

- The child's stdout and stderr reach `pnpmlike`'s stdout and stderr **unchanged**, in
  every output mode. `--output json` neither wraps, frames, styles nor captures them.
- The child's exit status becomes `pnpmlike`'s exit status verbatim: a script ending in
  `exit 7` makes `pnpmlike run` exit 7. What `pnpmlike` itself writes to stderr about a
  failing script is deliberately unspecified; what is specified is that the script's own
  output arrives unaltered.
- A script name that is not declared (including every name in an empty workspace) is exit
  3, nothing on stdout, and a stderr message naming the script.

## The reporter

`--reporter <auto|default|append-only|ndjson|silent>`, default `auto`.

`auto` resolves at startup:

| condition | resolved reporter |
| --- | --- |
| the output mode is a machine mode (`json`, `yaml`, `xml`, `csv`) | `ndjson` |
| stderr is an attended terminal | `default` |
| otherwise | `append-only` |

An explicit `--reporter` outranks all three rows. A value outside the list is a usage
error (exit 2).

**The reporter never writes to stdout, in any mode, under any value.**

Steps carry the work being done:

- `install` reports one step per package, in manifest order; its step text is
  `<i>/<n> <name> <version>` (`1/2 alpha 1.0.0`), with `<version>` exactly as the
  manifest spells it.
- `run` reports exactly one step, `1/1 <script>`, written and flushed **before** the
  child starts, so it can never interleave with the child's own output.
- `list` reports no steps.

Each reporter renders those steps differently:

- **`append-only`** — one step per line, LF-terminated, no ANSI, no rewriting:

  ```text
  1/2 alpha 1.0.0
  2/2 beta 2.3.1
  ```

- **`default`** — the dynamic reporter: each step is a carriage return (`\r`), the step
  text, then the ANSI erase-to-end-of-line sequence `ESC [ K`, with no line feed between
  steps; one line feed follows the last step. For the two packages above, stderr is
  exactly `\r1/2 alpha 1.0.0ESC[K\r2/2 beta 2.3.1ESC[K\n`. With no steps to report it
  writes nothing at all.

- **`ndjson`** — one single-line JSON object per step, and, after the last step of a
  successful `install`, a terminal `done` entry:

  ```text
  {"event":"progress","step":1,"total":2,"name":"alpha","version":"1.0.0"}
  {"event":"progress","step":2,"total":2,"name":"beta","version":"2.3.1"}
  {"event":"done","installed":2}
  ```

  A `run` step is `{"event":"script","name":"build"}`. No ANSI ever appears in this form.
  The objects are exactly as spelled here, key order included.

- **`silent`** — nothing.

## The log channel

Warnings and info notes are a separate stderr channel, governed by a level:
`--loglevel <error|warn|info>`, default `warn`. `-q`/`--quiet` is `error` and
`-v`/`--verbose` is `info`; an explicit `--loglevel` outranks both.

- At `info`: one note before anything else, `info: using manifest <path>`, where
  `<path>` is the path as given on the command line or, when discovered, `pnpmlike.pkg`.
- At `warn` and above: one warning per unpinned package,
  `warning: <name> is unpinned (*)`.
- At `error`: neither.

The manifest is read and validated before any work starts, so **every log line is written
before the first reporter step**.

`--silent` is the both-channels switch: exactly `--reporter silent --loglevel error`.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | manifest error (a line matching no directive) |
| 2 | usage error (unknown flag or subcommand, bad `--reporter` or `--loglevel` value) |
| 3 | unknown script |
| *the script's own status* | `run` propagates a failing script's status verbatim |

The first four codes are `pnpmlike`'s own and stay distinct from each other. A propagated
child status is not: a script free to end in `exit 3` can land on any of them, and
verbatim propagation is worth more than a reserved range.
