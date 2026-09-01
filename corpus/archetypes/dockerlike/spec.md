# `dockerlike` — behavioral spec

`dockerlike` is a container-engine client in the docker mold: one subject
reachable under two spellings — a legacy top-level verb (`ps`, `images`,
`inspect`) and a management-command path (`container ls`, `image ls`,
`container inspect`) — over one frozen, built-in engine state. Nothing
mutates.

Three conventions run through the whole tool:

- **`--quiet` decides what the data is**, not how it looks: the listing
  commands emit identifiers and nothing else. In the human-rendered modes
  (`auto`, `text`, `term`) that is one bare identifier per line, no header
  and no styling, on an attended terminal as much as on a pipe; in the
  structured modes the serialized value is the identifier array itself.
- **Truncation is a human-rendering rule.** Identifiers display shortened in
  the human-rendered output — the table's identifier column and the quiet
  list alike — and `--no-trunc` shows them whole in both. Structured output
  always carries the whole identifier, `--no-trunc` or not.
- **Selection is composable.** `--filter key=value` repeats and the
  conditions are ANDed; a filter that matches nothing is a success with no
  rows, not an error.

## The frozen engine state

Identifiers are 16 hexadecimal characters (a synthetic tool: shorter than a
real engine's, long enough that truncation is visible). Containers, in
listing order:

| id | name | image | state |
| --- | --- | --- | --- |
| `3f1a9c2b7d4e0a11` | `web` | `alpine:3.19` | running |
| `7b2e5a8f0c13d4f2` | `api` | `alpine:3.19` | running |
| `3f8d4e6a1b90c7a3` | `db` | `postgres:16` | exited |
| `c04b17e9a2f5e8d4` | `cache` | `redis:7` | exited |

Images, in listing order:

| id | repository | tag | size |
| --- | --- | --- | --- |
| `a1b2c3d4e5f60718` | `alpine` | `3.19` | `7.4MB` |
| `b9c8d7e6f5a40312` | `postgres` | `16` | `431MB` |
| `e5f4a3b2c1d09876` | `redis` | `7` | `41MB` |

## The command tree

```text
dockerlike
├── ps                 (alias of `container ls`)
├── images             (alias of `image ls`)
├── inspect <ref>…     (alias of `container inspect`)
├── container
│   ├── ls
│   └── inspect <ref>…
└── image
    └── ls
```

An alias and its management path are the **same command**: same flags, same
stdout bytes, same stderr bytes when they fail, same exit codes. Only the
words the user types differ, and `--help` is the one output where that
shows: the usage line names the path that was invoked, while the rest of the
help — the flags it documents above all — is the same at both.

Invoking a group (`dockerlike container`, `dockerlike image`) without a
subcommand is a usage error: exit 2, guidance on stderr, nothing on stdout.
So is `dockerlike` with no arguments at all, and so is an unknown command.
`--help` works at every level, exits 0, and writes help to stdout.

Tables follow one layout rule: columns separated by a single space, each
column but the last padded to its widest rendered cell, the header row
included. Widths are computed over the rows actually rendered — filtering
changes the geometry.

## `dockerlike ps` / `dockerlike container ls`

```text
CONTAINER ID NAME  IMAGE       STATE
3f1a9c2b7d4e web   alpine:3.19 running
7b2e5a8f0c13 api   alpine:3.19 running
3f8d4e6a1b90 db    postgres:16 exited
c04b17e9a2f5 cache redis:7     exited
```

The `CONTAINER ID` column shows the first 12 characters of each identifier.

### `--no-trunc`

Shows whole identifiers, so the column widens:

```text
CONTAINER ID     NAME  IMAGE       STATE
3f1a9c2b7d4e0a11 web   alpine:3.19 running
7b2e5a8f0c13d4f2 api   alpine:3.19 running
3f8d4e6a1b90c7a3 db    postgres:16 exited
c04b17e9a2f5e8d4 cache redis:7     exited
```

### `-q` / `--quiet`

Identifiers only, one per line — no header, no padding, no styling:

```text
3f1a9c2b7d4e
7b2e5a8f0c13
3f8d4e6a1b90
c04b17e9a2f5
```

`--quiet --no-trunc` prints the whole identifiers the same way. `--quiet`
output is never styled: not on an attended color-capable terminal, not under
`--output=term`.

### `--filter key=value`

Valid keys: `state`, `name`, `image`. Values are compared literally. The flag
repeats, and a container must match every filter to be listed:

```text
$ dockerlike ps --filter image=alpine:3.19
CONTAINER ID NAME IMAGE       STATE
3f1a9c2b7d4e web  alpine:3.19 running
7b2e5a8f0c13 api  alpine:3.19 running
```

```text
$ dockerlike ps --filter state=exited --filter image=redis:7
CONTAINER ID NAME  IMAGE   STATE
c04b17e9a2f5 cache redis:7 exited
```

A filter that matches nothing is a success: exit 0, the header alone (its
widths now computed over the header row only), nothing on stderr.

```text
$ dockerlike ps --filter state=paused
CONTAINER ID NAME IMAGE STATE
```

Under `--quiet` the same invocation prints nothing at all.

A filter naming a key outside `state`, `name`, `image`, or one carrying no
`=`, is a domain error — the flag parsed, its value was rejected: exit 1,
empty stdout, and on stderr the offending value as typed, followed by the
valid keys:

```text
$ dockerlike ps --filter status=running
dockerlike: invalid filter: status=running (valid keys: state, name, image)
$ dockerlike ps --filter running
dockerlike: invalid filter: running (valid keys: state, name, image)
```

## `dockerlike images` / `dockerlike image ls`

```text
IMAGE ID     REPOSITORY TAG  SIZE
a1b2c3d4e5f6 alpine     3.19 7.4MB
b9c8d7e6f5a4 postgres   16   431MB
e5f4a3b2c1d0 redis      7    41MB
```

`--no-trunc` and `-q` behave exactly as they do for containers — the whole
identifiers widen the `IMAGE ID` column, and quiet output is one identifier
per line:

```text
$ dockerlike images --no-trunc
IMAGE ID         REPOSITORY TAG  SIZE
a1b2c3d4e5f60718 alpine     3.19 7.4MB
b9c8d7e6f5a40312 postgres   16   431MB
e5f4a3b2c1d09876 redis      7    41MB
```

`--filter` is a container-listing flag only.

## Structured output

A listing command's structured modes (`--output=json` and friends) serialize
the records themselves, with the **whole** identifier. Containers carry `id`,
`name`, `image`, `state`:

```json
[{"id":"3f1a9c2b7d4e0a11","name":"web","image":"alpine:3.19","state":"running"}]
```

Images carry `id`, `repository`, `tag`, `size` — the same strings the table
shows, the size included:

```json
[{"id":"a1b2c3d4e5f60718","repository":"alpine","tag":"3.19","size":"7.4MB"}]
```

`--no-trunc` changes nothing here: it governs what the human-rendered output
shows, and the serialized identifier is whole either way. Under `--quiet` the
structured value is the identifier array itself, for both listings:

```json
["3f1a9c2b7d4e0a11","7b2e5a8f0c13d4f2","3f8d4e6a1b90c7a3","c04b17e9a2f5e8d4"]
```

## `dockerlike inspect <ref>…` / `dockerlike container inspect <ref>…`

`inspect` is the machine-only command in an otherwise human tool: it writes
**JSON on stdout in every output mode** — under `--output=text`, under
`--output=term`, on an attended color-capable terminal — always the same
bytes, never styled, never framed with prose.

The value is always a JSON array, one object per reference, in the order the
references were given, even for a single one. Object fields are `id` (whole),
`name`, `image`, `state`, in that order. Since the bytes are the contract,
they are fixed: compact JSON — no spaces, no indentation — on a single line
terminated by one newline.

```text
$ dockerlike inspect web
[{"id":"3f1a9c2b7d4e0a11","name":"web","image":"alpine:3.19","state":"running"}]
```

A reference is a container name or an identifier prefix. A prefix must
resolve to exactly one container:

- unique prefix (`7b2`) — resolves to `api`;
- ambiguous prefix — exit 1, empty stdout, and on stderr the prefix followed
  by every whole identifier it matched, in listing order:
  `dockerlike: ambiguous reference: 3f matches 3f1a9c2b7d4e0a11, 3f8d4e6a1b90c7a3`;
- no match — exit 1, empty stdout, and on stderr
  `dockerlike: no such object: <ref>`.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success, including a listing that matched nothing |
| 1 | domain errors: unknown or malformed filter, unknown reference, ambiguous prefix |
| 2 | usage errors: no command, group without subcommand, unknown command or flag |

## Provenance and stressed interactions

This archetype is entry C8 of the 2026-08-16 archetype survey. The survey's
behavioral sketch is not in the repository, so this spec is reconstructed
from the survey's capability matrix and the archetype's name rather than
restored; it aims at a coherent shape the roster does not already cover, not
at fidelity to the lost text.

What it exists to stress, stated so a reader can judge whether it earns its
place beside the rest of the roster:

- **One handler under two registered paths.** Legacy verbs and management
  commands share behavior: the same flags, the same errors, the same help but
  for the usage line naming the invoked path — and group nodes stay
  non-commands.
- **Quiet as a data-shape choice.** `--quiet` decides the value before the
  render pipeline sees it, so it must hold across templated modes,
  structured modes, forced color, and an attended terminal.
- **Display truncation against machine output.** Shortened identifiers are a
  human-rendering rule, in the table and the quiet list alike; structured
  output and `--quiet --no-trunc` carry whole ones.
- **A repeatable selection flag against table geometry.** Filters compose by
  AND, recompute column widths over the rendered rows, and reduce to a
  header-only success when nothing matches — while a bad filter *value* is a
  domain error, not a parse error.
- **A machine-only command inside a human application.** `inspect` emits the
  same JSON bytes in every output mode, with prefix-resolution failures
  leaving stdout empty.
