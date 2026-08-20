# Standout

A CLI framework: leaf crates do one job (render, input, dispatch, …) and the `standout` crate composes them into an application.

## Language

**RenderRequest**:
The explicit input to rendering: data, template, theme, format, and **TargetProperties**. Same request, same bytes.
_Avoid_: RenderContext (that is the mid-render view for context providers)

**TargetProperties**:
Properties of the destination being rendered to for one invocation (width, color, whether stdout and stderr are terminals, and related). Detected, injected by tests, or later set by flags — not the whole run.
_Avoid_: RuntimeProperties, Capabilities, TargetFacts, environment globals

**RenderContext**:
The borrowed view passed to context providers while a render is in progress.
_Avoid_: using this as the leaf's public entry

**TemplateRef**:
Named, inline, or declared-absent template carried on a **RenderRequest**. A convention name exists only on the builder until `build()` turns it into a registry name.
_Avoid_: a `String` that might be source, a path, or empty

**InputSources**:
The stdin, clipboard, and prompt-responder used for one invocation. Constructed from the real process in production; constructed with mocks by tests. Passed into `App` next to **TargetProperties**, not bundled with them.
_Avoid_: process-global default readers, a combined run-environment type
