# Typed Command Output

## Problem

An application author writing a Standout handler returns one serializable value, and
Standout turns it into a rendered template or a JSON, YAML or CSV document according to
`--output`. That covers the command that computes an answer and prints it. It does not
cover the command whose result accrues while it runs: an `apply` that changes ten
resources, a `sync` that touches every remote, a `list` that walks a large tree. That
author has three options, all bad.

- Write the increments to stderr as prose or a spinner. A script reading `--output json`
  never sees them, the redraws interleave with the document under capture, and the test
  harness cannot assert on them as data.
- Emit them with `ctx.stream()`, which writes a line only under `--output ndjson` and
  discards the value everywhere else, so the human run of the same command shows nothing
  until the end and the author writes a second, prose path by hand.
- Buffer everything and return one value when the command finishes, which is the command
  the author did not want to write.

The flag compounds this. `term`, `text`, `auto` and `term-debug` sit next to `json`,
`yaml`, `csv` and `ndjson` as if all eight were formats, when the first four are one
representation, a rendered template, under different ANSI settings, and the last four are
encodings of the handler's value. `term` versus `text` is the only way a CLI user can ask
for or refuse color, so color cannot be requested without also naming a format. The one
pager Standout runs is the help pager: a `log` command that wants its rendered output paged
on a terminal spawns the pager, reads `PAGER` and parses `--no-pager` itself, which is the
workaround the blind corpus runs recorded more than any other.

## Proposed behavior

Application logic returns typed serializable values and nothing else. A command produces
either one batch value or a sequence of typed incremental events; both are results.
Standout, not the handler, chooses how a result is represented, whether ANSI decoration is
applied, and where the rendered bytes are delivered. Five distinctions carry the design.

**Results are not logs.** A command result is the value the command exists to produce and
is what a consumer parses. An operational message (a warning, a trace) is about the run,
not part of it, and never enters the result. Operational verbosity, `-q` and `-v` deciding
how much tracing or logging reaches stderr, is a separate future feature; whatever it
decides, it never changes a command's result values.

**Incremental results are not progress rendering.** An `apply_start` or `apply_complete`
event is a typed result value the handler emits before the command finishes; a consumer of
a structured encoding receives each one as a record. A spinner or a `2/5` counter is
transient human rendering that a human representation may derive from those events and
that no structured encoding carries. The handler produces events; it never draws progress.

**Representation is independent of production.** A batch value and an event sequence can
each be shown as human text or encoded as data. For human text Standout renders the
command's template: once for a batch value; per event, then once for the closing summary,
for a sequence. For data Standout encodes each value with a supported structured encoding.
NDJSON is not a separate format: it is the JSON record encoding plus line framing, which
is what a sequence of values needs to be read before the command ends. A batch value under
line framing is one record; a sequence under an encoding without line framing is a
complete array of records, written when the command ends.

A command that fails after emitting events has still produced those events: the human
representation has rendered them, line framing has written them, and the failure
diagnostic follows them in the shape
[the machine contract](../topics/execution-outcomes.md) already gives it. An encoding without line framing
writes nothing before the command ends, so a run that fails first delivers the diagnostic
in place of the array, as it does today for a batch value. An event that cannot be
serialized, or bytes that cannot be delivered, fail the run the way the machine contract
fails a final write.

**ANSI presentation is separate from representation.** Decoration applies only to
templated human text. `--output` names a structured encoding, `json`, `yaml`, `csv` or
`ndjson`; with no `--output` the representation is the human template, which has no
`--output` name. A separate `--color` setting, `auto`, `always` or `never`, decides whether
that human text carries escape sequences; `auto` is the default and resolves as a terminal
setting does, flag, then environment, then configuration, then detection: an explicit
`--color`, then the environment (an application-named variable, then `NO_COLOR`), then the
application's color key under [Config Layering](./parity-config-layering.md), then
terminal detection; configuration never overrides `NO_COLOR`. `term`, `text`, `auto` and
`term-debug` are not peer data formats: the first three are the human representation under
a presentation setting and are retired; `term-debug` stays as `--output term-debug`, a
diagnostic view of the template's style tags outside the stability contract, as today. A
structured encoding never carries ANSI, whatever `--color` says.

**Rendering is separate from delivery.** Rendering produces bytes; delivery places them on
stdout, in a file the user names, or, for complete human output on a terminal, in an
external pager. The application author declares which commands may page; the CLI user
declines paging per run with `--no-pager`. That declaration only makes a command eligible;
the pager command is a terminal setting resolved like color: `--no-pager`, then the
environment (an application-named variable, then `PAGER`), then the application's pager
key in configuration; with none set, nothing pages. A named output file wins over paging. A pager that cannot start delivers to stdout unpaged without changing
the run's status; a pager that stops reading ends delivery without failing the run.
Structured encodings, incremental human output and a stdout that is not a terminal never
page. Progress rendering, when the human representation derives it, goes to stderr and is
absent when stderr is not a terminal or the representation is structured.

The principal combinations, with `apply` as the incremental command and `list` as the
batch one:

| Command | Human representation                      | Structured encoding                         |
| ------- | ----------------------------------------- | ------------------------------------------- |
| `list`  | one rendered page, pageable if declared   | one document; one record under line framing |
| `apply` | one rendered line per event, then summary | an array of records, or one record per line |

### The app author

A handler returns a typed value, or emits typed events through the command's result
channel and then returns the summary; both paths use the serializable types the author
already writes. The author declares a template for the human representation and, for an
incremental command, one template for an event and one for the summary. The author marks a
command whose complete human output may page. The author never inspects the output mode,
never writes progress to stderr, and never branches on whether a stream is a terminal.

### The CLI user

```text
myapp apply                            # human text: one line as each event happens, then the summary
myapp apply --output json              # a JSON array of events and the summary; no ANSI, no progress
myapp apply --output ndjson            # one JSON record per line, written as each event happens
myapp list --color never               # human text without escape sequences, on any terminal
myapp list --color always | tee log    # escape sequences into the pipe
myapp log                              # human text through the pager when stdout is a terminal
myapp log --no-pager                   # the same text straight to stdout
myapp log --output-file-path out.txt   # rendered bytes into the file; stdout carries nothing
```

Under a structured encoding stdout carries the encoded result, or the failure diagnostic
that replaces it, and never ANSI or progress; failure and warning diagnostics keep the
channels and shapes the machine contract gives them, which this feature does not change.
A structured run never differs in its values from the human run of the same command: the
same events and the same summary, up to the point the run ended.

### The test author

The harness returns the result as data, the batch value or the ordered event sequence,
independently of the representation the run selected, and separately returns the rendered
stdout bytes and the delivery decision (stdout, file or pager). A test asserts on the
values once and on the presentation separately. A test that needs a real terminal, or
proof that there is none, runs the compiled binary as a process.

## Non-goals

Backwards compatibility with the current public output model: no aliases for the retired
mode names, no migration shim, and no period in which both models are accepted. The
implementation of operational verbosity. A TUI layer, theme or styling changes,
progress-bar styling, auto-paging heuristics, and any structured encoding beyond the
supported set.

## Further Notes
