# Standout

A CLI framework: leaf crates do one job (render, input, dispatch, …) and the `standout` crate composes them into an application.

## Language

**RenderRequest**:
The explicit input to the pure entry `render_request`: data, template, theme, format, **color policy**, **TargetProperties**, and optional extras forwarded to context providers. The type is owned (no lifetime); the function takes it by reference. Convenience `render(template, data, theme)` detects, builds a request, and delegates. The leaf does not read framework-owned detectors or process globals. File-backed templates and context-provider callbacks are external dependencies of the request.
_Avoid_: RenderContext (that is the mid-render view for context providers)

**TargetProperties**:
Properties of the destination being rendered to for one invocation (width, stdout and stderr color capability independently, whether stdout and stderr are terminals, color-scheme, icon mode, and ambiguous-width policy). Width, stream facts, color-scheme, and icon mode are detected, with no App fallback. Ambiguous-width is App-owned policy on the type (`detect()` defaults `Narrow`; `App::run` overwrites). Primary render uses stdout facts; warnings and progress use stderr facts.
_Avoid_: RuntimeProperties, Capabilities, TargetFacts, environment globals, a single color capability for both streams

**ColorPolicy**:
Resolved color axis on a **RenderRequest** (`Auto` / `Always` / `Never`). Independent of format (`OutputMode`) and of per-stream color capability on **TargetProperties**. Later `--color` and the env ladder resolve into this field; they are not `--output`.
_Avoid_: folding color into `--output`, a single capability bool, detecting color policy inside the leaf

**RenderContext**:
The borrowed view passed to context providers while a render is in progress.
_Avoid_: using this as the leaf's public entry

**TemplateRef**:
Named, inline, or declared-absent template carried on a **RenderRequest**. A convention name exists only on the builder until `build()` turns it into a registry name. Standalone `HelpConfig::template` and topic template strings stay as `Inline`.
_Avoid_: a `String` that might be source, a path, or empty

**InputSources**:
The stdin, clipboard, and prompt-responder used for one invocation. Constructed from the real process in production; constructed with mocks by tests. Passed into `App` next to **TargetProperties**, not bundled with them.
_Avoid_: process-global default readers, a combined run-environment type
