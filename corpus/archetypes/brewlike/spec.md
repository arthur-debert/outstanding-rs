# `brewlike` — behavioral spec

`brewlike` is a package-manager query client in the homebrew mold: it reads
one frozen, built-in cellar and answers questions about it — what is
installed, what one formula is, what it depends on, and what is out of date.
There are no mutating commands. Its machine surface is `--output json`, and
every machine payload is stamped with a schema version.

**On this spec's provenance.** The 2026-08-16 survey's C10 sketch is lost
(`docs/spec/robustness-corpus-completion.md`, "Risks And Rabbit Holes"), so
this archetype is reconstructed from the survey's capability matrix and the
parity Specs that name it. It stresses four interactions the rest of the
roster does not: **nested data through one render pipeline** (a dependency
tree, human-indented and JSON-nested from one handler); **the empty
collection** (a query whose answer is legitimately nothing, in both a human
and a machine mode); **one catalog in four data shapes** (table, detail
record, tree, empty list) under the same `--output` axis; and **a versioned
machine contract**, which `docs/spec/parity-machine-contract.md` names as
one of its external gates. The schema-version criteria are the archetype's
gap group (`acceptance.toml`, `expected = "fail"`, gap `PAR02`); everything
else is criteria the roster proper owns. Fidelity to the lost sketch is not
claimed.

## The frozen cellar

| formula | installed | latest | depends on |
| --- | --- | --- | --- |
| `basalt` | 2.1.0 | 2.1.0 | `pebble`, `quartz` |
| `granite` | 1.4.2 | 1.5.0 | `pebble` |
| `pebble` | 0.9.0 | 0.9.0 | — |
| `quartz` | 3.0.1 | 3.2.0 | `pebble` |

`granite` and `quartz` are therefore out of date; `basalt` and `pebble` are
current. Formulae are always listed in the name order of this table.

## The commands

```text
brewlike
├── list
├── info <formula>
├── deps [--tree] <formula>
└── outdated [<formula>...]
```

`brewlike` with no subcommand is a usage error: exit 2, guidance on stderr,
nothing on stdout. `--help` works at every level and exits 0 with help on
stdout.

Tables follow one layout rule: columns separated by a single space, each
column but the last padded with trailing spaces to the width of its widest
rendered cell, header included.

### `brewlike list`

Every installed formula and the version installed:

```text
FORMULA VERSION
basalt  2.1.0
granite 1.4.2
pebble  0.9.0
quartz  3.0.1
```

### `brewlike info <formula>`

```text
basalt: stable 2.1.0
installed: 2.1.0
depends on: pebble, quartz
```

`stable` is the latest version, `installed` the installed one; when they
differ the installed line is marked:

```text
granite: stable 1.5.0
installed: 1.4.2 (outdated)
depends on: pebble
```

A formula with no dependencies says so rather than trailing an empty list:

```text
pebble: stable 0.9.0
installed: 0.9.0
depends on: none
```

### `brewlike deps <formula>`

The transitive dependency closure, deduplicated, one name per line, in name
order — `brewlike deps basalt`:

```text
pebble
quartz
```

A formula with no dependencies prints nothing and exits 0.

### `brewlike deps --tree <formula>`

The same closure as a tree instead of a set: the queried formula on the
first line, each dependency indented two spaces per level under the formula
that depends on it, direct dependencies in name order. A formula reached
through more than one path is printed in full at each occurrence —
`brewlike deps --tree basalt`:

```text
basalt
  pebble
  quartz
    pebble
```

### `brewlike outdated [<formula>...]`

The installed formulae a newer version exists for, both versions shown:

```text
FORMULA INSTALLED LATEST
granite 1.4.2     1.5.0
quartz  3.0.1     3.2.0
```

Formula names restrict the query to those formulae; a name that is not in
the cellar is an error rather than a silently narrower query. When the
result is empty — every named formula is current — the answer is a
sentence, not a headerless table, and the exit code is still 0:

```text
No outdated formulae.
```

## Machine output

`--output json` answers the same questions as data: one JSON document on
stdout, nothing on stderr. The root of that document is part of the contract,
not an implementation detail:

| command | document |
| --- | --- |
| `list`, `outdated` | an array of formula records, in the human order |
| `info <formula>` | one formula record, with `dependencies` added: the direct dependency names |
| `deps <formula>` | an array of the dependency names, in the human order |
| `deps --tree <formula>` | one node — `name` and `dependencies`, its child nodes, recursively |

A formula record carries `name`, `installed` and `latest` as strings and
`outdated` as a boolean. Records bind their own values: a formula's
installed and latest versions belong to that formula's record, never to a
parallel array the caller has to zip. So `brewlike list --output json` is

```json
[{"name":"basalt","installed":"2.1.0","latest":"2.1.0","outdated":false},
 {"name":"granite","installed":"1.4.2","latest":"1.5.0","outdated":true},
 {"name":"pebble","installed":"0.9.0","latest":"0.9.0","outdated":false},
 {"name":"quartz","installed":"3.0.1","latest":"3.2.0","outdated":true}]
```

and the nesting survives serialization — `brewlike deps --tree basalt
--output json` is a node whose children hang under the formula that depends
on them, not a flattened list beside it:

```json
{"name":"basalt","dependencies":[
  {"name":"pebble","dependencies":[]},
  {"name":"quartz","dependencies":[{"name":"pebble","dependencies":[]}]}]}
```

**An empty result is an empty list**: `[]`, present and empty — never
`null`, never an absent key, and never the human sentence
`No outdated formulae.` smuggled into the data.

### The stamped form

A consumer has to be able to tell a shape change from a data change, so the
payload is stamped with the version of the contract it satisfies: the
document above travels under `data`, beside `schema_version`, currently the
number `1`.

```json
{"schema_version":1,"data":[{"name":"basalt","installed":"2.1.0","latest":"2.1.0","outdated":false}]}
```

Help is data in this mode too: `brewlike deps --help --output json` is the
help of `deps` as a stamped document — its flags among the fields — rather
than the rendered prose page.

Stamping is the one part of this spec written past what standout can do
today (`acceptance.toml`, the `schema-version` group, gap `PAR02`). It
replaces the unstamped root rather than extending it, which is why the two
shapes are specified apart: the acceptance suite pins the unstamped
documents as what an implementation must produce now, and the stamped ones
as what `PAR02` has to deliver. `docs/spec/parity-machine-contract.md` plans
for exactly that break — versioning the envelope is "the last unversioned
one".

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success, including help and an empty result |
| 1 | domain errors: unknown formula |
| 2 | usage errors: no subcommand, unknown subcommand, unknown flag |

A domain error leaves stdout empty and puts prose on stderr:

```text
brewlike: no formula found: obsidian
```
