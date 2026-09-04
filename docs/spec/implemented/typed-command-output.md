# Typed Command Output

> **Implemented** by TERM01 (#511): WS01 #521 (ADR-0041), WS02 #522, WS03 #523,
> WS04 #526 and #527, WS05 #525, WS06 #531, WS07 #529. As built, where the text
> below differs:
>
> - The pager variable's `MYAPP` is the name the application gives
>   `AppBuilder::name`, upper-cased with every character outside `A-Z0-9` as
>   `_`. An application that never names itself reads `PAGER` alone, so the
>   framework never guesses a name from argv or the binary.
> - The human representation of an incremental command is one rendered line per
>   event and nothing else. No spinner, counter or transient line is derived
>   from the events, and no setting asks for one; that feature is still the
>   separate spec the Non-goals name.
> - The corpus `systemdlike` member asks for color with `--color never` and
>   `--color always` in place of the retired `--output text` and `--output
>   term`. Its produced binary predates the change, so it leaves the per-PR
>   corpus subset until #519 re-accepts it from a fresh blind run; an empty
>   subset passes that workflow rather than failing it.
> - #519 has not run.

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
transient human rendering, not a result, and no structured encoding carries one. This
feature writes none: the human representation of an incremental command is one rendered
line per event as it happens, which shows the user the command moving, and nothing beyond
results reaches stdout or stderr under any representation. A transient progress line
derived from events on a terminal is a later feature with its own spec. The handler
produces events; it never draws progress.

**Representation is independent of production.** A batch value and an event sequence can
each be shown as human text or encoded as data. For human text Standout renders the
command's template: once for a batch value; per event, then once for the closing summary,
for a sequence. For data Standout encodes each value with a supported structured encoding.
NDJSON is not a separate format: it is the JSON record encoding plus line framing, which
is what a sequence of values needs to be read before the command ends. Under line framing
each event is written as the handler's value, carrying whatever discriminator the
application gave it, and the summary is the `result` record
[the machine contract](../../topics/execution-outcomes.md) already gives a batch value; a
`version` line an application writes first is an event like any other, and Standout adds
no header. A sequence under an encoding without line framing is the array of exactly
those records, in that order, written when the command ends, so `--output json` is what
`jq -s` makes of the `ndjson` stream. CSV takes the events as its rows under the
flat-record rule the machine contract applies to any CSV document; the summary has a
different shape and is not encoded, so an application that wants totals in CSV carries
them in its last event. The summary is the same `Output` a batch handler returns:
`Silent` means no summary record and no summary render, an exit status declared on it
applies as it does today, and binary or artifact output from an incremental command is a
render error decided before anything is written.

A command that fails after emitting events has still produced those events: the human
representation has rendered them, line framing has written them, and the failure
diagnostic follows them in the shape the machine contract already gives it. An encoding
without line framing writes nothing before the command ends, so a run that fails first
delivers the diagnostic in place of the array, as it does today for a batch value. An
event that cannot be serialized, or bytes that cannot be delivered, fail the run the way
the machine contract fails a final write, with one exception: a reader that has gone
away. When stdout or the pager stops reading (`myapp apply --output ndjson | head -1`),
Standout discards what follows, lets the handler run to completion, and reports the
command's own status. A handler is never interrupted or informed because its consumer
left, so a command that changes things is never left half done by a pipe.

**ANSI presentation is separate from representation.** Decoration applies only to
templated human text. `--output` names a structured encoding, `json`, `yaml`, `csv` or
`ndjson`; with no `--output` the representation is the human template, which has no
`--output` name. A separate `--color` setting, `auto`, `always` or `never`, decides whether
that human text carries escape sequences; `auto` is the default. The application's own
setting is the `color` key, with the same three values, in the `[term]` section that
[Config Layering](./parity-config-layering.md) gives a home, from a file or the key's
environment spelling `MYAPP__TERM__COLOR`; the framework receives the resolved key, not
its source. Resolution is the terminal-setting order: an explicit `--color`, then
`NO_COLOR`, then the resolved `[term] color`, then whether the destination is a terminal,
where a named output file never is and a pager inherits stdout's answer. `term`, `text`,
`auto` and `term-debug` are not peer data formats: the first three are the human
representation under a presentation setting and are retired, from the flag and from
`[term] output`, which accepts the four structured encodings and nothing else;
`term-debug` stays as `--output term-debug`, a diagnostic view of the template's style
tags outside the stability contract, as today, and has no configuration spelling. A
structured encoding never carries ANSI, whatever `--color` says. `--color` and
`--no-pager` are on every command the way `--output` is, and an application renames or
removes them through the same seam.

**Rendering is separate from delivery.** Rendering produces bytes; delivery places them on
stdout, in a file the user names, or, for complete human output on a terminal, in an
external pager. The application author declares which commands may page; the CLI user
declines paging per run with `--no-pager`, which disables paging outright. That
declaration only makes a command eligible. The pager command is executed, so it never
comes from a configuration file, which a project may supply, and there is no `[term]`
pager key: it is read from the environment, `MYAPP_PAGER` (the application's name in upper
case, every character outside `A-Z0-9` as `_`), then `PAGER`. With neither set, or the
winning value empty, nothing pages. The value runs through `sh -c`, so `less -FRX` and
`sed -n 1p` both work as they do for git; on Windows nothing pages. Standout sets
`LESS=FRX` and `LV=-c` in the pager's environment when the user has not, so colored
text pages readably through a bare `less`. The help pager follows the same rule and the
same `--no-pager`, and drops its own fallback to `less`. A named output file wins over
paging. A pager that cannot start delivers to stdout unpaged without changing the run's
status. Structured encodings, incremental human output and a stdout that is not a
terminal never page.

The principal combinations, with `apply` as the incremental command and `list` as the
batch one:

| Command | Human representation                      | Structured encoding                         |
| ------- | ----------------------------------------- | ------------------------------------------- |
| `list`  | one rendered page, pageable if declared   | one document; one record under line framing |
| `apply` | one rendered line per event, then summary | an array of records, or one record per line |

### The app author

A handler returns a typed value, or emits typed events through the command's result
channel and then returns the summary; both paths use the serializable types the author
already writes, and an incremental command has one event type, an enum when its events
differ, and one summary type. The author declares a template for the human
representation. For an incremental command the summary uses the command's template and
the events use the same name with an `.event` suffix (`apply.event`), resolved through
the same directories and theme; that template receives the event as `event` and branches
on the application's own discriminator, so one template covers every event kind. Each
rendered event is one flushed write, on a terminal and in a pipe alike. The author marks
a command whose complete human output may page. The author never inspects the output
mode, never writes progress to stderr, and never branches on whether a stream is a
terminal.

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

The harness returns the result as data, `result()`, the batch value or the ordered events
and summary, independently of the representation the run selected, and separately
returns the rendered stdout bytes and the delivery decision, `delivery()`: stdout, the
file's path, or the pager command chosen. The harness takes the color policy and whether
stdout is a terminal as two separate settings, replacing today's `with_color`, `no_color`
and `text_output`, which decide both at once. A test asserts on the values once and on
the presentation separately. A test that needs a real terminal, or proof that there is
none, runs the compiled binary as a process.

## Non-goals

Backwards compatibility with the current public output model: no aliases for the retired
mode names, no migration shim, and no period in which both models are accepted. The
implementation of operational verbosity. A transient progress line derived from events
(a spinner, a counter), which gets its own spec. A TUI layer, theme or styling changes,
auto-paging heuristics, backpressure or cancellation between the framework and a
handler, and any structured encoding beyond the supported set.

## Further Notes

The handler contract for an incremental command — the types and signatures, who owns
`Results<E>`, the order events and the summary reach a consumer in, what a failure after
an emitted event means, and how hooks and the test harness read the values — is defined by
[ADR 0041 — Hand the handler a typed results sink](../../adr/0041-hand-the-handler-a-typed-results-sink.md),
which also records the returned iterator and the channel that were prototyped against the
handler boundary and rejected.
