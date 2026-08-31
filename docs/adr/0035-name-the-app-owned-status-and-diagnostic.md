# Name the app-owned status and diagnostic

`AppFailure` is a handler-returnable error carrying a nonzero `u8` exit status and a stderr payload Standout writes verbatim. It reaches the shell through the path `ExternalFailure` already used — a `RunError` whose status the error declared, written to stderr with no `Error:` prefix and no trailing newline, the status riding to `process::exit` — and it differs from `ExternalFailure` in one respect: which party owns the contract those bytes state. A pre-dispatch guard reaches the same seam through `HookError::pre_dispatch_app`; post-dispatch and post-output hooks do not, for the reason they cannot declare an external failure either — by then the handler has already succeeded.

Two blind adopters needed this and neither found it. `ghlike` had to produce exit `1` and the exact line `ghlike: repository not found: demo/gamma`; it used `ExternalFailure` against the documented meaning and filed the workaround. `gitlike` needed exit `3` for an unknown object and kept its plumbing commands outside dispatch entirely, writing their bytes itself — which is the worse outcome, because every byte written outside dispatch is a byte the framework's snapshots, capture APIs and `TestHarness` never see.

## What the two names divide

The division is who reached the verdict, not what the bytes look like.

- `ExternalFailure` — the application is **relaying**. Another operation ran, decided a status and wrote a diagnostic, and the application is passing both through unaltered. A delegated `git` invocation is the case the name was written for.
- `AppFailure` — the application **reached the verdict itself**. Its own specification pins the status and the stderr line, usually because a test suite or a caller's shell script reads them.

Keeping these apart is what lets each name mean something. Widening `ExternalFailure` to cover both would leave the framework unable to say, at the one place it writes the bytes, whether a status it is about to exit with came from a process it launched or from a decision the handler made — and it would leave `docs/topics/error-handling.md` with an "only when" sentence that no longer excludes anything.

Both constructors reject status `0`. A domain error that reported shell success would be indistinguishable from a run that worked, and there is no case where an application wants that: an application whose domain outcome is "nothing to do, that is fine" returns `Output::Silent`, not a failure. This is the property ADR-0033's exit-status item rests on, and it is asserted directly rather than inferred from the type.

## Where this stops, and what picks it up

`AppFailure` carries a status and bytes. It is not an error model: there is no variant set, no error code vocabulary, no category, and no way to render the same failure differently per output mode. A handler that wants a structured failure the caller can parse is asking for the **machine-readable error envelope**, which belongs to the parity program's machine contract (PAR02) — the same place that will version the envelope and state its fields (ADR-0033 draws the same boundary for structured output generally: what starts here is that a failure *has* an application-owned shape, not that the shape is published as a schema).

The two are not alternatives, and the ordering is deliberate. The human-mode form is what an application needs to be usable from a shell today, and it is small enough to be a spelling rather than a design. When the machine contract arrives it feeds on this seam — a status the application chose and a payload it wrote — rather than replacing it, which is why this ADR fixes the seam's shape now and leaves the envelope unshaped.

## Alternatives rejected

**One `Failure` type with an owner field.** It halves the code and loses the property the code exists for. `ExternalFailure` and `AppFailure` are distinguished by `downcast` at the one place a handler error becomes a `RunError`; an owner *field* would be a runtime value a handler could set to either thing, so the documented meaning would stop being checkable by reading the call site. The duplication is roughly forty lines of a plain data type, which is the honest price.

**A trait, so applications declare their own failure types.** It is the shape an error model would take, and adopting it here would pre-empt PAR02 with a surface that has to be lived with. Two adopters needed a status and a line; neither needed a hierarchy.

**Renaming `ExternalFailure` to something that covers both.** The name is accurate for what it names, it is contract under ADR-0033, and renaming it would break every downstream that relays a delegated command's verdict correctly — to fix a problem those downstreams do not have.

**Leaving adopters to `anyhow` plus the handler diagnostic framing.** It is what they had. The framing is fixed at `Error: {error}` and the status at `1`, so an application whose spec says exit `3` with an exact line has no path through it. That is the finding.

## Consequences

`RunErrorKind` gains `App`, and `RunError::new` refuses it the way it already refuses `External` — both kinds are constructed from their failure type or not at all. The one place the shell adapter writes an error asks `RunError::writes_diagnostic_verbatim()` rather than naming a single kind, so the two owner-declared kinds cannot drift apart at the point that matters. `standout-test` reports `RunErrorKind::App` through `error_kind()` and reproduces the verbatim bytes through `stderr()`, so an application can assert its pinned line in a unit test instead of a process test.

`ExternalFailure` keeps its name, its meaning and its documentation, and `docs/topics/error-handling.md` now states the handler diagnostic framing it previously only referred to — one `Error:` prefix in front of the error's own `Display`, shared by handler and hook failures, which is what makes "no framing at all" a describable property of the two owner-declared kinds.
