# `validity` — known-edge method coverage

`validity` is not a product CLI. It exists so a blind implementer cannot
finish the suite without exercising two known framework edges the ROB03
pilot never independently rediscovered: the silent-template family, and
an application-owned incomplete theme combined with framework-rendered
help.

The construction contract below is part of the spec. Follow it exactly.
Workarounds that satisfy the user-facing lines while skipping the
named construction steps fail the purpose of this archetype.

## Binary

The produced binary is named `validity`.

## Construction contract

1. **One registered application template.** Register a template whose
   registry name is `ok`. Its body renders the inner text `ok` plus a
   trailing newline. Wrap that word in an application style tag the
   theme below defines (for example `[ok]ok[/ok]`). Do **not** register
   templates named `okk` or `nosuch`.
2. **`show` resolves a registry name.** `validity show <name>` treats
   `<name>` as a standout *template registry name* and renders through
   that named template. Do not `match` on the string in application
   code to pick canned output. The only name that must succeed is `ok`.
3. **Missing and mistyped names fail loudly.** `validity show okk` and
   `validity show nosuch` must not succeed. They must not print template
   source (`{{`, `{%`, or the requested name as MiniJinja body), must
   not print the success bytes `ok\n`, must not exit 0, and must finish
   inside the case timeout. Stderr names the requested template. Empty
   stdout. Exit status **1**.
4. **Registration order.** Register the `late` command (configured to
   use template `ok`) *before* application templates are loaded.
   Register the `early` command (also template `ok`) *after* templates
   are loaded. Both orders must produce the same success bytes as
   `validity show ok`. An order that silently renders empty output or
   the template name as source is a failure.
5. **`build()` is the gate.** Call `build()` before any
   run / dispatch / parse / render entry point. The shipped binary is a
   built `App`. An unbuilt builder must not render (the framework makes
   this unrepresentable; if a builder entry point still compiles and
   runs, it must fail loudly, never silently render).
6. **Incomplete application theme.** Ship one application theme that
   defines *only* the application tag used by the `ok` template. Do
   **not** define help or topic tags: `about`, `usage`, `header`,
   `item`, `metavar`, `desc`, `default`, `values`, `example`. Enable
   standout help handling (`.help_handling(true)`). Framework-rendered
   help at the root and at the deep leaf must still resolve every help
   tag and still present clap's facts.

## Command tree

```text
validity
├── show <name>
├── early
├── late
└── nest
    └── inner
        └── leaf
```

`nest` and `nest inner` are group nodes, not commands. Invoking a group
without a subcommand is a usage error (exit 2) — the suite does not pin
that path; it exists so help has a deep leaf.

### `validity show <name>`

`<name>` is a template registry name. `ok` is the only registered
application template.

Success (`show ok`) is exactly:

```text
ok
```

Piped / `--output text` is those exact bytes, no ANSI. `--output term`
with color forced on may style the word, but the visible text is still
`ok` and no unresolved `[tag?]` marker appears.

### `validity early` and `validity late`

Both render identically to `validity show ok` (same exact bytes under
`--output text`). They exist to force the two registration orders in
the construction contract.

### `validity nest inner leaf`

Renders identically to `validity show ok` under `--output text`. It
exists so framework help has a leaf three levels down.

## Help

Standout-rendered help is required. The `help` word, `--help`, and `-h`
all work at the root and at `nest inner leaf`.

- The `help` word honors `--output` (`validity help --output text` is
  text; `validity help --output term` is term).
- `-h` and `--help` are clap flags; they still render the same themed
  help page.
- Every help invocation exits 0, writes help on stdout, and leaves
  stderr empty.
- No unresolved style-tag marker reaches the page: none of
  `[about?]`, `[header?]`, `[usage?]`, `[item?]`, `[metavar?]`,
  `[desc?]`, `[default?]`, `[values?]`, `[example?]`.
- Clap facts are present: a `Usage` line; root help names `show`,
  `early`, `late`, and `nest`; leaf help names the leaf.
- `--output term` with color forced on still carries ANSI (the
  framework help theme must survive the incomplete app theme). Text
  mode and color-off carry no ANSI.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success, including help |
| 1 | missing or mistyped template name |
| 2 | usage errors (unknown command or flag, group without a subcommand) |
