# Archetype: `jjlike` (gap spec)

A log-viewing tool in the jj mold: the user supplies a runtime template on the command
line, and that template is the tool's *only* stable output surface. Typed template
functions, and — the point of this archetype — templates treated as **untrusted input**:
a bad template must produce a diagnostic, never a panic or a hang.

**This is a gap specification.** It describes capability standout does not have (survey
Part C, archetype C9; `docs/spec/implemented/robustness-corpus.md`): user-supplied *runtime*
templates. Standout's templates are app-author artifacts resolved at build time; nothing
lets an end user hand the renderer a template string at invocation time, and the
untrusted-input direction (unknown names, hostile budgets) is the least-tested direction
of the core rendering feature.

**Owning epic: not yet minted.** No parity Spec currently covers user-supplied runtime
templates; the existing parity Specs are config layering (PAR01), the machine
contract (PAR02), and typed command output (`docs/spec/typed-command-output.md`).
Epic codes are assigned by the human, so this suite deliberately does not invent one — every assertion group below is
owned by the future runtime-templates parity epic, and assigning its code is flagged for
that epic's grill. Until then the expected-fail markers name the gap in prose: the
roster's `acceptance.toml` beside this spec keys its cases to the manifest's
`runtime-templates` gap slug (a slug, not a code), and the runnable-today suite in
`corpus/gap-suites/tests/jjlike.rs` prints the same ownership with every outcome.

Everything below is asserted black-box against a produced binary.

## Inputs

**Data file** (`--data <path>`): NDJSON, one record per line. Each record is a JSON
object with string fields `id`, `author`, and `message`.

**Template** (`-T <template>`): a runtime template string applied once per record, in
file order, each rendering followed by a newline on stdout.

## Template language

- `{{ <field> }}` interpolates a record field.
- `{{ <expr> | <filter> }}` applies a typed template function (filter). The stable
  built-in set for this archetype: `upper`, `lower`.
- `{% <tag> %}...{% end<tag> %}` block tags. The stable built-in set: `for` (with
  `range(<int>)`), as in `{% for i in range(3) %}...{% endfor %}`.

## Untrusted-input behavior (the assertions)

Diagnostics are single-line JSON objects on **stderr**:

```json
{"severity":"error","summary":"...","function":"<name>","offset":<int>}
```

`offset` is the 0-based byte offset of the first byte of the offending name within the
template string as passed to `-T`. The process must never print panic output (no
`panicked at`, no backtrace advice) for any template input.

- **Unknown filter**: `-T '{{ message | frobnicate }}'` exits 1 with one diagnostic
  whose `function` is `"frobnicate"` and whose `offset` points at it (offset 13 for that
  exact template). Nothing renders to stdout.
- **Unknown tag**: behavior is configured by `--unknown-tags <error|inner>`
  (default `error`):
  - `error`: `-T '{% frob %}X{% endfrob %}'` exits 1 with one diagnostic whose `tag` is
    `"frob"` (the diagnostic carries `tag` instead of `function`) and whose `offset`
    points at it (offset 3 for that exact template).
  - `inner`: the unknown tag pair degrades to its inner text — the same template renders
    `X` per record, exit 0, no diagnostic.
- **Render budget**: `--render-budget-ms <int>` caps total render time. A template that
  exceeds it (e.g. `{% for i in range(1000000000) %}{{ i }}{% endfor %}`) must
  *terminate* with exit 1 and one diagnostic whose `summary` contains
  `render budget exceeded` — promptly after the budget elapses, never hanging.

## Commands

```text
jjlike log --data <path> -T <template> [--unknown-tags <error|inner>] [--render-budget-ms <int>]
```
