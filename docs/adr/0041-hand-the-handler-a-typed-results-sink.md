# Hand the handler a typed results sink

An incremental command produces its result while it runs. This records the Rust contract
that lets a handler produce those values — the signatures, who owns what, what order the
values reach a consumer in, and what a failure after the first value means — for
`docs/spec/typed-command-output.md`, which carries the reasoning and whose closed
decisions this restates rather than reopens. Nothing here changes production behavior;
the workstream that implements it does.

## The contract

```rust
pub trait Handler {
    type Event: Serialize + 'static;
    type Output: Serialize;

    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<Self::Event>,
    ) -> HandlerResult<Self::Output>;

    fn expected_args(&self) -> Vec<ExpectedArg> { Vec::new() }
}

pub struct Results<E: Serialize> { /* no lifetime parameter */ }

impl<E: Serialize> Results<E> {
    pub fn emit(&mut self, event: E) -> Result<(), EmitError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("event does not serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("{0}")]
    Render(String),
    #[error("event could not be written: {0}")]
    Write(#[from] std::io::Error),
}

/// The event type of a command that emits none: uninhabited, so `emit` has no argument
/// that can be constructed.
#[derive(Serialize)]
pub enum NoEvents {}
```

`NoEvents` derives `Serialize` because `Handler::Event` is bound by it; serde derives an
impl for an uninhabited enum, and nothing can construct a value to reach it. `EmitError`
derives `thiserror::Error` — the same shape as the `StreamError` it replaces — because
`HandlerResult`'s error is `anyhow::Error`, which converts only from a
`std::error::Error`, and `?` on an `emit` is how a handler propagates.

`Handler::Output` keeps its name and its meaning: the summary an incremental command
returns is the same `Output<S>` a batch command returns. `Handler::Event` is new. A batch
command sets it to `NoEvents` and ignores the parameter, so there is one trait rather
than a batch trait and an incremental one, and whether a command is incremental is read
from that type rather than declared beside it. It is `'static` because an associated type
carries no lifetime from `handle`'s parameters. Function handlers keep their current
signatures: the adapter behind a two-argument closure sets `Event = NoEvents`, and a
three-argument closure taking `&mut Results<E>` is the incremental adapter. `#[handler]`
picks the adapter from whether the function declares a `Results` parameter.

`emit` takes the event by value because the framework consumes it: it serializes or
renders the value during the call and keeps nothing the handler could observe. A handler
that needs the value afterwards clones before emitting.

`emit` returns once the value has been rendered, written or retained, and the handler's
next statement runs then. Production is synchronous throughout: there is no backpressure
and no cancellation between the framework and the handler, and nothing interrupts a
handler that is running.

## Ownership and lifetimes

`Results<E>` holds the run's one destination behind an `Rc`, the way `StreamSink` already
does, so the type carries no lifetime parameter and `Handler::handle` gains none. The
framework constructs it per run inside the dispatch closure and drops it when the handler
returns; the handler receives a borrow, so it cannot store, clone or outlive it.

The borrow is a third parameter rather than a member of `CommandContext` because
`CommandContext` is one non-generic type shared by every command and named in every hook
signature; a typed member would put `E` on the context and from there on the type-erased
dispatch closure. As a separate `&mut` it is disjoint from `&ArgMatches` and
`&CommandContext`, so a handler may hold borrows taken out of `matches` across every
`emit` and use them afterwards.

`E: Serialize` is the whole bound. No `Send`, no `Clone`, no `Deserialize`, so an event may
hold an `Rc` or any other data that does not cross threads.

## Ordering and retention

Events reach a consumer in emit order, each one written before `emit` returns; the
summary follows the handler's return. Under line framing that is the handler's event
lines, then the `result` record, then the warning entries ADR-0038 places last. Under an
encoding without line framing it is one array of exactly those records, the events in
order and the summary's record last, written when the command ends. The human
representation renders one line per event as it happens, from the command's template name
with an `.event` suffix, then the summary from the command's own template.

What `emit` keeps follows the representation: line framing and human text write and
retain nothing, and the encodings that produce one document retain the serialized record
until the command ends. A recorder, which the test harness and the in-process capture
entry points install, retains the records under any representation.

The summary is the `Output<S>` the handler returns, with the meanings it already has:
`Silent` produces no summary record and no summary render, leaving the events alone;
`with_exit_status` applies as it does to a batch value; `Binary` and `Artifact` from a
command that declares events are a render error decided before anything is written.

## Failure after emitted events

`emit` fails when the event does not serialize, does not render, or cannot be written. The
handler propagates with `?` and the run fails under the machine contract. Values already
written stand: nothing is retried and nothing is withdrawn.

A reader that went away is not one of those failures. The framework classifies a
`BrokenPipe` on the run's destination, discards everything that follows, and returns
`Ok(())` from every later `emit` without serializing, so a handler that changes things
runs to completion and the run reports the command's own status.

A handler that fails for its own reason after emitting returns `Err`, and the diagnostic
follows the events in the shape ADR-0037 gives it. Under an encoding without line framing
nothing has been written yet, so the diagnostic replaces the array.

## Hooks and the harness read values, not bytes

A post-dispatch hook sees the summary alone, as the `serde_json::Value` it sees today,
and may replace it. Events do not pass through it: a per-event hook would run inside the
handler's loop, after line framing had already written the value, so a replacement it
returned could not be applied. Post-output hooks keep seeing rendered bytes.

The records `emit` produces are what the harness reads. `TestResult::result()` returns the
ordered events and the summary as recorded values whatever representation the run
selected, and `TestResult::events::<E>()` deserializes them into the application's own
type — `Deserialize` is a requirement of the test, never of production. A test asserting
on values therefore never selects a representation, and the rendered bytes and the
delivery decision stay the separate assertions the Spec names.

## What the handler cannot reach

`Results<E>` exposes `emit` and nothing else. A handler cannot ask it which
representation is running, whether a destination is a terminal, where bytes go, or what
the renderer will do, and producing a result needs no filesystem or environment access.
That is what replaces `ctx.stream()`, whose `is_live()` exists because the same handler
had to return a value under some modes and entries under others; a handler emitting typed
events emits the same ones under every representation, so the question has no counterpart
here.

## The forms that were tried

Three forms were prototyped against the boundary in
`crates/standout-dispatch/src/handler.rs` and its erasure into the dispatch closure in
`crates/standout/src/cli/group.rs`, each with borrowed `ArgMatches`, a failure after
earlier events, a post-dispatch hook and in-process capture. The prototypes are not in
this diff; the PR reports what each compiled to.

A **returned iterator** cannot be `'static`, because the events are built from borrowed
`ArgMatches`; the compiler asks for a lifetime on the returned trait object, which puts
one on `Handler::handle`. That version compiles, and costs three further things: `&'a mut
self` tied to `&'a ArgMatches` means nothing may touch the handler while the framework
drains it, not even `expected_args`; the summary exists only after exhaustion, so the
return becomes a two-phase object with a `finish` method; and a fallible step becomes
`Item = Result<E, _>`, a second error path beside the handler's own. The application loop
has to be rewritten as a state machine, which is the code the Spec's problem statement
says the author did not want to write.

A **channel** drained after the handler returns writes nothing while the command runs,
which is the buffering the feature exists to remove: the prototype's trace shows all of
the handler's work before any write. Draining on a thread requires `Event: Send`, a bound
the application's own type would have to satisfy — the prototype's event holds an `Rc` and
does not compile — and the drain would write through the run's one destination, which is
`Rc<RefCell<dyn Write>>` and is not `Send` either. `Sender::send` also returns before
anything is serialized or written, so the serialization and write failures the Spec makes
`emit`'s only failures could not reach the `?` that caused them.

The **sink** keeps the object-safe erasure and the borrowed `ArgMatches`, adds no
lifetime, and hands the harness typed records without stdout, so it is the form recorded
above.
