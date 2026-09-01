# `cargolike` — behavioral spec

`cargolike` is a tiny build planner in the cargo mold: it reports what a
workspace contains and what building it would do, and every number it reports
comes out of a **layered configuration** — files found by walking up from the
current directory, environment variables mechanically derived from the config
keys, and per-invocation flags on top.

It operates on one frozen, built-in workspace. Nothing is compiled, no manifest
is read from disk, and no command writes anything: the tool exists to *report*
the effective plan. The only files it reads are configuration files.

## The frozen workspace

Three packages:

| package | version | dependencies |
| --- | --- | --- |
| `cli` | 1.0.0 | `core`, `util` |
| `core` | 0.3.1 | — |
| `util` | 0.2.0 | `core` |

Commands that list packages list all three in name order (`cli`, `core`,
`util`). `-p <name>` (long form `--package`) narrows a command to one package;
a name outside the workspace is an error (see Exit codes).

## Commands

### `cargolike metadata [-p <name>]`

One line per selected package:

```text
cli v1.0.0 deps: core util
core v0.3.1 deps: none
util v0.2.0 deps: core
```

Dependency names are space-separated in name order; a package with none prints
`none`. Under the JSON output mode the same data is an object:

```json
{"packages":[{"name":"cli","version":"1.0.0","dependencies":["core","util"]},
             {"name":"core","version":"0.3.1","dependencies":[]},
             {"name":"util","version":"0.2.0","dependencies":["core"]}]}
```

### `cargolike tree [-p <name>]`

The dependency tree rooted at the selected package, `cli` by default:

```text
cli v1.0.0
├── core v0.3.1
└── util v0.2.0
    └── core v0.3.1
```

Children are listed in name order. Each child's line starts with `├──` and a
space, except the last child's, which starts with `└──` and a space. The
subtree under a last child is indented by four spaces; under any other child
it is indented by `│` and three spaces. A package reached twice is printed
twice — there is no deduplication marker.

### `cargolike build [-p <name>] [--jobs <n>] [--features <list>]`

Reports the plan without building anything. One line per selected package, in
name order:

```text
compile cli v1.0.0 jobs=1 features=[]
```

`jobs` is the effective `build.jobs`; `features` is the effective
`build.features` rendered as `[` + the items joined by `,` + `]`. `--jobs`
sets `build.jobs` for this invocation. `--features <a,b>` **appends** its
comma-separated items to the effective feature list rather than replacing it
(see Configuration).

### `cargolike config get <key> [--show-origin]`

Prints the effective value of one config key followed by a newline, exit 0.
Values render as they do in `config list` below. With `--show-origin` the line
becomes `<value> (from <origin>)`, where origin names the layer the value came
from: `flag`, `override`, `environment`, `project`, `user`, `default`, or
`merged` when a list drew items from more than one layer.

A key of the known set (see Configuration) with no effective value prints
nothing on either stream and exits **1**. A key outside the known set is a
usage error (exit **2**).

Under the JSON output mode the command emits one object carrying the value in
its config type, with the origin always present regardless of `--show-origin`:

```json
{"key":"build.jobs","value":5,"origin":"environment"}
```

### `cargolike config list [--show-origin]`

Every key with an effective value, sorted by key, one `key = value` line each:

```text
build.features = []
build.jobs = 1
net.offline = false
term.color = auto
term.output = auto
```

Booleans render as `true`/`false`, integers as their decimal digits, strings
bare (no quotes), and lists as `[` + the items joined by `,` + `]`. With
`--show-origin` each line gains a space and `(from <origin>)`, exactly as
`config get` does. Under the JSON output mode
the listing is one object mapping each key to its value in the value's config
type.

## Configuration

The known keys, and nothing else:

| key | type | default |
| --- | --- | --- |
| `build.jobs` | positive integer | `1` |
| `build.features` | list of strings | `[]` |
| `build.target` | string | *unset* |
| `net.offline` | boolean | `false` |
| `term.color` | `auto` \| `always` \| `never` | `auto` |
| `term.output` | `auto` \| `text` \| `term` \| `json` | `auto` |

Config files are TOML, in either the table form or the dotted form:

```toml
[build]
jobs = 8
features = ["tls"]
```

### Where values come from

Highest precedence first:

1. **Command flags** that name a key: `--jobs` (`build.jobs`), `--features`
   (`build.features`), and the framework's own `--output` (`term.output`).
2. **`--config <key>=<value>`**, repeatable, applying to any known key. The
   text after the first `=` is the value, read as *Value text* below;
   repeating a key takes the last assignment on the command line. A
   `--config` argument without a `=` is a usage error.
3. **Environment variables**, one per key, spelled mechanically:
   `CARGOLIKE_` + the section + `_` + the key, uppercased —
   `CARGOLIKE_BUILD_JOBS`, `CARGOLIKE_BUILD_FEATURES`,
   `CARGOLIKE_BUILD_TARGET`, `CARGOLIKE_NET_OFFLINE`, `CARGOLIKE_TERM_COLOR`,
   `CARGOLIKE_TERM_OUTPUT` — carrying a value as *Value text* below.
4. **Project config files**: every `.cargolike/config.toml` found by walking up
   from the current working directory to the filesystem root.
5. **The user config file**: `$CARGOLIKE_CONFIG_HOME/config.toml`, or
   `$HOME/.config/cargolike/config.toml` when `CARGOLIKE_CONFIG_HOME` is unset.
6. **The built-in defaults** in the table above.

### How the layers merge

Every discovered project file contributes — the walk does not stop at the
first one — and how two contributions combine depends on the value's type:

- **Scalars** (integer, string, boolean, enum): the higher layer wins whole.
  Among project files, the one nearest the current directory is the higher
  layer; every project file outranks the user file.
- **Lists**: file layers **concatenate**, farthest ancestor first and the
  nearest last, with the user file's items ahead of every project file's.
  Repeated items collapse to their first occurrence, preserving order.
- **A list set from the environment or through `--config` replaces the merged
  file list** instead of appending to it, so an empty value there clears an
  inherited list. The `--features` flag appends to whatever that resolution
  produced.

An unknown key in a config file, a value of the wrong type, or a `build.jobs`
below 1 is a configuration error (exit **101**) naming the offending key.

### Value text

Every layer that carries a value as text — the `--jobs` and `--features` flags,
`--config`, and the environment variables — reads it against the key's type:

| type | text |
| --- | --- |
| integer | decimal digits |
| boolean | `true` or `false` |
| string, enum | the text as written, unquoted |
| list | the text split on `,`, items untrimmed; the empty string is the empty list |

Text that does not fit the key's type is a configuration error (exit **101**),
as a wrong-typed file value is.

### Color and output mode

`term.color` and `term.output` steer the framework's own rendering through the
same ladder as every other key, with one addition between the file layers and
the environment layer: `NO_COLOR`, set to anything, means `term.color = never`
— it outranks every config file and loses to `CARGOLIKE_TERM_COLOR`. `auto`
means what it means everywhere
else: style only for an attended terminal, plain text when stdout is captured.

When color is on, the package names and versions in `metadata`, `tree` and
`build` output are styled per the application's theme. When it is off, those
commands print the bytes shown above, on a terminal exactly as through a pipe.

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | `config get` on a known key with no effective value |
| 2 | usage errors (unknown command, unknown flag, unknown config key, a `--config` argument without `=`) |
| 101 | configuration and workspace errors (invalid config value, unknown package) |

Error prose goes to stderr and stdout stays empty:

```text
cargolike: no package named nope in the workspace
cargolike: unknown config key: build.nope
cargolike: malformed --config override: build.jobs
cargolike: invalid value for build.jobs: many
```

## What this archetype stresses

The behavioral sketches of the survey's unbuilt archetypes were lost with the
2026-08-16 session record; this spec is reconstructed from the survey's
capability matrix, and states its target rather than claiming fidelity to that
sketch. `cargolike` is the **per-type merge and key↔env mapping** corner of
the config-layering triangle (`docs/spec/parity-config-layering.md`), which
`gitlike` (walk-up, first file wins) and `gcloudlike` (named configuration
sets) complete:

- **Merging, not choosing.** `gitlike` picks one file; `cargolike` combines
  every file it finds, and the combination rule differs per value type. Scalar
  precedence and list concatenation disagreeing about which end of the walk is
  authoritative is where hand-rolled config layers break.
- **A mechanical key↔env mapping**, including the asymmetry that makes it
  usable: an environment list replaces rather than appends, so a caller can
  clear an inherited list without editing files.
- **Framework settings riding the config layer.** `term.color` and
  `term.output` are the framework's own knobs resolved through the same ladder
  as domain keys, sitting between the app's config files and the framework's
  `--output` flag and `NO_COLOR` handling.
- **Provenance as a first-class answer.** `--show-origin` and the JSON form of
  `config get` make "where did this value come from" assertable, which is the
  question a layered config makes expensive to answer by hand.
- **Config values reaching a rendered view.** `build`'s output is a projection
  of the effective config, so a merge bug shows up in a command's bytes and not
  only in `config get`.
