# `formlike` — behavioral spec

`formlike` provisions a (pretend) site from a questionnaire. It is the
archetype no surveyed CLI matches: a multi-question form that must behave
perfectly with **no human attached** — piped stdin, CI, scripts. Interactive
collection is the fallback, not the design center.

There is exactly one real command, `provision`, plus the questionnaire's
answer-sheet surface around it.

## The questionnaire

Three questions, in order, with these stable IDs:

| id | prompt (cosmetic) | type | default | validation |
| --- | --- | --- | --- | --- |
| `name` | What is the site name? | string | *none — required* | lowercase letters, digits, hyphens only |
| `region` | Which region? | choice: `eu`, `us` | `eu` | must be a listed choice |
| `public` | Should the site be public? | bool (`yes`/`no`, `true`/`false`) | `no` | must be a bool spelling |

The `<id:...>` tags are the stable machine identity of the questions; all
prompt wording, numbering, and layout is cosmetic and may vary.

## `formlike provision`

On success prints exactly one line to stdout and exits 0:

```text
provisioned <name> in <region> (public: <yes|no>)
```

A fully non-interactive successful run writes **nothing to stderr**.

### Answer sources

- `--answers FILE` reads one completed answer sheet from a file;
  `--answers -` reads it from piped stdin.
- With no `--answers`, answers are collected interactively — which requires
  stdin to be an attended terminal.
- Sources never merge: a sheet replaces interactive collection entirely.
- In a sheet, a blank answer takes the question's default. A required
  question with no default (here: `name`) must carry an answer.
- `--yes` skips the confirmation gate. It does **not** invent answers:
  a missing required answer is still an error under `--yes`.

### Interactive collection

With no `--answers` and stdin an attended terminal, the three questions are
asked in order, one answer line each. A blank answer line takes the question's
default; a blank answer to a required question with no default re-asks, and
end-of-input during collection is an input error (exit 1) — collection never
silently proceeds without an answer. Prompt wording and layout are cosmetic
(the `<id:...>` tags
are not required interactively); the success line and exit codes are the same
as for sheet runs.

### Confirmation

Without `--yes`, provisioning asks for confirmation on an attended terminal
and reads one answer line: `yes` or `y` (case-insensitive) confirms; any other
answer declines, and a declined run exits 1 without provisioning. Piped stdin
never confirms; EOF never confirms.

### Non-interactive failure is bounded

Whenever required input is unavailable — no sheet and stdin is not an
attended terminal; a sheet missing a required answer; confirmation needed but
impossible — `formlike` must fail **promptly and cleanly**: exit status 1,
nothing on stdout, and a stderr message naming what was missing and how to
supply it (the offending question id, or the `--answers`/`--yes` flag that
would have unblocked the run). It must never block waiting for input that
cannot arrive.

## `formlike provision questions`

Prints the blank answer sheet to stdout (exit 0) and has no side effects.
The sheet contains a question line for each of the three IDs, each line
ending with its `<id:...>` tag.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | input errors: missing required answer, validation failure, impossible interactive collection or confirmation |
| 2 | usage errors (unknown command, unknown flag) |
