# smoketable

A tiny star-catalog CLI. This spec describes observable behavior only; how you
structure the code is up to you, but the application must be built with the
`standout` framework dependency already declared in the provided `app/`
scaffold.

## Binary

The produced binary is named `smoketable` (the scaffold's package name already
matches — build it with `cargo build` inside `app/`).

## Commands

### `smoketable list`

Renders the fixed star catalog below as a table: one row per star, with the
star name, constellation, and apparent magnitude in aligned columns, under a
"Star Catalog" heading.

| name      | constellation | magnitude |
| --------- | ------------- | --------- |
| Aldebaran | Taurus        | 0.86      |
| Rigel     | Orion         | 0.13      |
| Vega      | Lyra          | 0.03      |

The catalog is hard-coded; there is no storage and no way to modify it.

### `smoketable about`

Prints a single descriptive line containing the tool name `smoketable` and a
short statement of purpose.

## Output modes

Both commands must honor the standard standout `--output` flag:

- `--output text` renders plain text (no ANSI escapes).
- `--output term` may add color/styling, but styling must change nothing about
  the layout or textual content relative to `text`.
- `--output json` for `list` emits the catalog as JSON carrying every star's
  name, constellation, and magnitude.

## Exit codes

Every successful invocation exits `0`. Unknown commands or flags may fail
however clap fails them.
