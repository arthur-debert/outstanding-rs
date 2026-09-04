# Incremental Commands

A command whose result accrues while it runs — an `apply` that changes ten
resources, a `sync` that touches every remote, a `list` that walks a large tree
— emits typed values as it goes and returns a summary when it finishes. Both
are results: a person watching the terminal sees a rendered line per event, and
a script reading `--output ndjson` sees the same values as records.

## Writing one

The command declares an event type and takes the run's results channel as a
third handler parameter:

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    ApplyStart { resource: String },
    ApplyComplete { resource: String },
}

fn apply(
    _matches: &ArgMatches,
    _ctx: &CommandContext,
    results: &mut Results<Event>,
) -> HandlerResult<Summary> {
    for change in plan()? {
        results.emit(Event::ApplyStart { resource: change.name.clone() })?;
        change.apply()?;                  // a failure here follows the events
        results.emit(Event::ApplyComplete { resource: change.name.clone() })?;
    }
    Ok(Output::Render(summary))           // the summary, after the last event
}
```

Register it with `EventsFnHandler::new(apply)`, or write the same three
parameters under `#[handler]`, which reads the `Results` parameter and derives
the command's event type from it.

`emit` takes the event by value and returns once the framework has rendered,
written or retained it, so the handler's next statement runs after the event
has left the handler for good. It fails when the value does not serialize, does
not render, or cannot be written; propagate with `?` and the run fails with it. Standout reports that
failure as a render error whether or not the handler propagates it. The event
type is `'static`, so an event owns what it carries rather than borrowing from
the invocation.

`Results` exposes `emit` and nothing else. A handler cannot ask which
representation is running or where the bytes go, and emits the same events
under every representation. Standout serializes and renders an event without
interpreting it: its shape, including whether it carries a `type` key, is the
application's contract with its consumers. The Rust contract — the signatures, who owns `Results<E>`, and what
a failure after an emitted event means — is
[ADR-0041](../adr/0041-hand-the-handler-a-typed-results-sink.md).

## What a person sees

Standout renders each event from the command's template name with an `.event`
suffix — `apply.event` beside `apply` — resolved through the same directories
and theme. The template receives the event as `event` and branches on the
application's own discriminator, so one template covers every kind:

```jinja
{% if event.type == "apply_start" %}starting {{ event.resource }}…
{% elif event.type == "apply_complete" %}{{ "✔" | style("ok") }} {{ event.resource }}
{% endif %}
```

Each rendered event is one flushed write, on a terminal and in a pipe alike;
the summary follows from the command's own template.

```text
$ myapp apply
starting web…
✔ web
starting db…
✔ db
2 added, 0 removed
```

`.build()` requires the `.event` template of every command that declares an
event type, so a missing one is a setup error rather than a failure on the
first event. A command whose events are its whole result returns
`Output::Silent` as its summary and needs only the `.event` template: the
summary template is the one `.build()` lets it skip, because which `Output`
variant a handler returns is not something the build can read.

Incremental human output never goes through the pager, whatever the command
declares: the pager takes complete output, and this output arrives while the
command runs ([Paging](./output-modes.md#paging)).

## Under a structured encoding

`--output ndjson` is the JSON record encoding plus line framing: each event is
written as the handler produced it, compact, on its own line and flushed, and
the summary is the `result` record the machine contract gives a batch value.
Standout adds no header, so a `version` line an application writes first is an
event like any other.

```text
$ myapp apply --output ndjson
{"type":"version","format_version":1}
{"type":"apply_start","resource":"web"}
{"type":"apply_complete","resource":"web"}
{"type":"result","data":{"applied":1}}
```

`json` and `yaml` have no line framing, so nothing reaches stdout until the
command ends and the whole run arrives as one array — exactly the records line
framing would have written, in the same order: the events, the summary's
`result` record, then the warning entries. `myapp apply --output json` is what
`jq -s` makes of the `ndjson` run of the same command.

`csv` takes the events as its rows under the flat-record rule it applies to any
CSV document ([CSV Output](./output-modes.md#csv-output)), written when the
command ends. The summary has a different shape and is not encoded, so an
application that wants totals in CSV carries them in its last event; a warning
stays prose on stderr rather than becoming a row. A command that declares a
[`CsvProjection`](./output-modes.md#csv-output) has it applied to the events.

## Failure, and a reader that leaves

A failure after emitted events keeps them — the human representation has
rendered them, line framing has written them — and the diagnostic follows in
the shape [Execution
Outcomes](./execution-outcomes.md#failures-under-a-structured-mode) gives it.
Under `json`, `yaml` and `csv` nothing has been written yet, so the diagnostic
takes the place of the array or the rows.

A reader that goes away is not one of those failures. When stdout or the file
stops reading (`myapp apply --output ndjson | head -1`), Standout discards what
follows, lets the handler run to completion, and reports the command's own
status, so a command that changes things is never left half done by a pipe.

## Binary and artifact output cannot follow events

A command whose event type is not `NoEvents` carries `Output::Render` and
`Output::Silent` only, so either payload is a render error under every
representation — on the run that emitted nothing too, since the refusal follows
the type rather than the count. Under `ndjson` a payload is a render error
whether or not the command declares events: a stream of JSON lines has no room
for one.

## Where the bytes go

`--output-file-path` takes the whole run and stdout carries nothing. The
rendered lines and the `ndjson` records reach the file as each event is
emitted; `json`, `yaml` and `csv` reach it as the one document written when the
command ends.
