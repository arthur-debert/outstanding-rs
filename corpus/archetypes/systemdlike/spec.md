# `systemdlike` — behavioral spec

`systemdlike` is a service-manager inspector in the systemctl mold: a naked
invocation that *is* a real command, tabular list output framed by a legend,
and strict environment discipline for color and paging. It reads one frozen,
built-in unit table; there are no mutating commands.

## The frozen unit table

| unit | load | active | sub |
| --- | --- | --- | --- |
| `core.service` | loaded | active | running |
| `web.service` | loaded | active | running |
| `cache.service` | loaded | inactive | dead |
| `backup.timer` | loaded | active | waiting |

Units are always listed in this order.

## `systemdlike list-units` — and the naked invocation

Running `systemdlike` with no subcommand is exactly `systemdlike list-units`,
including any flags on the naked line (`systemdlike --no-legend` behaves as
`systemdlike list-units --no-legend`).

Output is a table: a header row, one row per unit, then the legend — a blank
line followed by `<N> units listed.` where *N* counts the listed rows.

Layout rule: columns are separated by a single space; each column except the
last is padded with trailing spaces to the width of its widest cell among the
*rendered* rows, header row included when it is shown. The default output is
therefore exactly:

```text
UNIT          LOAD   ACTIVE   SUB
core.service  loaded active   running
web.service   loaded active   running
cache.service loaded inactive dead
backup.timer  loaded active   waiting

4 units listed.
```

### `--state <active|inactive>`

Filters the rows; order is preserved, the legend count reflects the filter,
and column widths are recomputed over what is rendered:

```text
UNIT         LOAD   ACTIVE SUB
core.service loaded active running
web.service  loaded active running
backup.timer loaded active waiting

3 units listed.
```

### `--no-legend`

Removes the header row and the legend (the blank line and the count line),
leaving only data rows.

### `--plain`

Forces undecorated output: no ANSI styling regardless of terminal, color
variables, or theme — but keeps the header and legend. On a piped stdout,
`--plain` output is byte-identical to default output.

## Color discipline

Whether table output is styled (ANSI) is decided by the first rule that
applies, top wins:

1. `--plain`, or an explicit `--output` mode (`term` forces ANSI, `text`
   forces plain).
2. `SYSTEMDLIKE_COLORS` — `1` forces ANSI (even piped, even under
   `NO_COLOR`); `0` forces plain (even on a capable terminal).
3. `NO_COLOR` set (to anything): plain.
4. Autodetect: ANSI on a color-capable attended terminal, plain otherwise
   (pipes are always plain here).

## Pager discipline

List output pages when stdout is an attended terminal. The pager command
comes from `SYSTEMDLIKE_PAGER`, else `PAGER`; with neither set, no pager.
`--no-pager` disables paging for the invocation. Piped stdout never pages.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 2 | usage errors (unknown subcommand, unknown flag, bad `--state` value) |
