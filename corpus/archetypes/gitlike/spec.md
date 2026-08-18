# `gitlike` — behavioral spec

`gitlike` is a tiny version-control inspector in the git mold: a
**porcelain** surface for humans (styled, paged, allowed to evolve) over a
**plumbing** surface for scripts (byte-stable, never styled, never paged).
It operates on one frozen, built-in repository state — there is no repository
on disk and no mutating command; the tool exists to *show* things.

## The frozen repository

Three commits, newest first:

| hash | author | message |
| --- | --- | --- |
| `a3f4` | alice | Fix off-by-one |
| `b7e2` | bob | Add parser |
| `c1a9` | alice | Initial import |

Two branches: `main` at `a3f4` (the current branch), `dev` at `b7e2`.

## Porcelain commands

### `gitlike log`

One line per commit, newest first:

```text
<hash> <message> (<author>)
```

Shows at most *N* commits where *N* is the effective `log.limit`
(see Configuration); with no configured limit it shows all commits.
`--limit <N>` overrides any configured value for this invocation.

In a terminal the line may be styled per the active theme; piped output is
plain. Long output goes through the pager (see Paging).

### `gitlike status`

Exactly:

```text
On branch main
HEAD at a3f4 Fix off-by-one
```

## Plumbing commands

Plumbing output is a stability contract: byte-identical regardless of theme,
`--output` mode, terminal vs pipe, or environment. Plumbing never pages.

### `gitlike ref-list`

One line per branch, sorted by ref name:

```text
b7e2 refs/heads/dev
a3f4 refs/heads/main
```

### `gitlike cat-object <hash>`

The object body for a known hash:

```text
author <author>
message <message>
```

For an unknown hash: nothing on stdout, exit status **3**, and on stderr:

```text
gitlike: no such object: <hash>
```

## Configuration

Two keys:

- `log.limit` — positive integer; caps `gitlike log` output. Default: unset
  (no cap).
- `user.name` — string. Default: unset.

Config files are TOML (`log.limit = 2` or the `[log]` table form). Sources,
highest precedence first:

1. Command-line flags (`--limit` for `log.limit`).
2. The nearest `.gitlike.toml` found by walking up from the current working
   directory toward the filesystem root (the first file found wins; files
   further up are not merged).
3. The user config file `$GITLIKE_CONFIG_HOME/config.toml`
   (`$HOME/.config/gitlike/config.toml` when `GITLIKE_CONFIG_HOME` is unset).
4. Built-in defaults.

### `gitlike config get <key>`

Prints the effective value of the key followed by a newline, exit 0. For a
key with no effective value: prints nothing (both streams), exit status **1**.

## Paging

Porcelain commands page their output when stdout is an attended terminal.
The pager command comes from `GITLIKE_PAGER`, else `PAGER`; with neither set,
output is not paged. `--no-pager` disables paging for the invocation. Piped
stdout is never paged. Plumbing is never paged.

## Theming and color

`GITLIKE_THEME` selects a built-in theme: `default` or `mono` (an unstyled
theme). Themes affect porcelain only. `NO_COLOR` set (to anything) disables
styling in porcelain output. Neither has any effect on plumbing bytes.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | `config get` on an unset key |
| 2 | usage errors (unknown command, unknown flag, bad flag value) |
| 3 | unknown object hash |
