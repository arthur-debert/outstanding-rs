# `ghlike` — behavioral spec

`ghlike` is a forge client in the gh mold: a deep command tree (`noun verb`,
down to `noun noun verb`) where every listing command serves humans by
default and machines through `--json` with **field selection**. It reads one
frozen, built-in forge state; there are no mutating commands.

## The frozen forge state

Repositories:

| name | description | visibility |
| --- | --- | --- |
| `demo/alpha` | Alpha service | public |
| `demo/beta` | Beta tools | private |

Pull requests (all in `demo/alpha`), in number order:

| number | title | state | author | draft |
| --- | --- | --- | --- | --- |
| 1 | Add pager | open | alice | false |
| 2 | Fix theme | merged | bob | false |
| 3 | Docs pass | open | alice | true |

Checks: PR 1 has `build` = pass and `lint` = fail (in that order); PR 2 has
`build` = pass and `lint` = pass; PR 3 has no checks.

## The command tree

```text
ghlike
├── auth status
├── repo view <owner/name>
└── pr
    ├── list
    ├── view <number>
    └── checks
        └── list <number>
```

Invoking a group (`ghlike pr`, `ghlike pr checks`) without a subcommand is a
usage error: exit 2, guidance on stderr, nothing on stdout. `--help` works at
every level and exits 0 with help on stdout.

Tables follow one layout rule: columns separated by a single space, each
column but the last padded to its widest rendered cell (header included when
shown).

### `ghlike auth status`

```text
Logged in as corpus-bot
```

### `ghlike repo view <owner/name>`

```text
demo/alpha
Alpha service
visibility: public
```

Unknown repository: exit 1, empty stdout, and on stderr:

```text
ghlike: repository not found: <owner/name>
```

### `ghlike pr list`

```text
NUMBER TITLE     STATE
1      Add pager open
2      Fix theme merged
3      Docs pass open
```

### `ghlike pr view <number>`

```text
#2 Fix theme
state: merged
author: bob
```

Unknown number: exit 1, empty stdout, stderr
`ghlike: no pull request found: <number>`.

### `ghlike pr checks list <number>`

Headerless rows, `<name> <status>`, same padding rule:

```text
build pass
lint  fail
```

## `--json <fields>` — machine output with field selection

`pr list`, `pr view`, and `pr checks list` accept `--json` with a
comma-separated field list. Available fields: `number`, `title`, `state`,
`author`, `draft` for pull requests; `name`, `status` for checks.

- Output is pure JSON on stdout: `pr list` and `checks list` emit an array
  of objects, `pr view` a single object, each object carrying exactly the
  requested fields. `number` is a JSON number, `draft` a JSON boolean,
  everything else strings.
- JSON output is byte-boring by design: never styled, never paged, never
  framed with human prose — even on an attended color-capable terminal.
- An unknown field is an error: exit 1, empty stdout, and a stderr message
  naming the rejected field and listing the valid ones.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | domain errors: unknown repo, unknown PR, unknown `--json` field |
| 2 | usage errors: group without subcommand, unknown subcommand or flag |
