# `gcloudlike` — behavioral spec

`gcloudlike` is a tiny cloud-inventory CLI in the gcloud mold. Its distinctive
trait is not the inventory — that is three frozen rows — but how it decides
*which settings apply*: a **named configuration set**. Settings live in named
sets, exactly one set is active per invocation, and a set applies **whole**.
Nothing from a set that is not active reaches the invocation, not even from the
set called `default`.

Everything below is written from the CLI user's perspective and is asserted
black-box against a produced binary: argv in, stdout/stderr/exit status out.

## The frozen inventory

Three instances, listed in ascending name order:

| name | project | zone | machine_type | status |
| --- | --- | --- | --- | --- |
| `db-1` | `beta-proj` | `us-a1` | `large` | `RUNNING` |
| `web-1` | `alpha-proj` | `us-a1` | `small` | `RUNNING` |
| `web-2` | `alpha-proj` | `us-b2` | `small` | `TERMINATED` |

## Properties

A property is named `<section>/<name>`. There are exactly three:

| property | meaning |
| --- | --- |
| `core/project` | the project whose instances are visible |
| `core/account` | the acting account; carried, never filters |
| `compute/zone` | when set, narrows `instances list` to that zone |

None has a built-in default: with nothing supplying it, a property is **unset**.

A key with no `/`, or with an empty section or name, is a **usage error**. A
well-formed key naming a property outside the table above is a **domain error**.

### Effective value

For each property, the first source that supplies a value wins:

1. Its per-invocation flag: `--project` for `core/project`, `--account` for
   `core/account`, `--zone` for `compute/zone`.
2. Its environment variable, spelled `GCLOUDLIKE_<SECTION>_<NAME>` uppercased —
   `GCLOUDLIKE_CORE_PROJECT`, `GCLOUDLIKE_CORE_ACCOUNT`,
   `GCLOUDLIKE_COMPUTE_ZONE`. A variable set to the empty string is treated as
   unset.
3. The active configuration set's stored value for that property.

Sets do not merge. A property the active set does not store is unset at step 3,
whatever any other set stores for it.

## Configuration sets

Sets are persisted under the **configuration directory**: `$GCLOUDLIKE_CONFIG_DIR`
when that variable is set and non-empty, otherwise `$HOME/.config/gcloudlike`.
It holds:

- `active_config` — one line, the name of the persisted active set. Absent or
  empty means `default`.
- `configurations/config_<name>` — one file per set.

A set file is INI: `[section]` header lines, `name = value` lines, blank lines
ignored, whitespace around `name` and `value` trimmed. There are no comments
and no quoting; a value is the rest of the line after the first `=`.

The set named `default` always exists; with no file it is empty. Any other name
exists only if `configurations/config_<name>` does.

### Which set is active

The first source that names a set wins:

1. `--configuration <name>`.
2. `GCLOUDLIKE_ACTIVE_CONFIG`.
3. The `active_config` file.
4. `default`.

Naming a set that does not exist is a domain error, whichever source named it.

## Commands

```text
gcloudlike compute instances list
gcloudlike compute instances describe <name>
gcloudlike config get-value <section>/<name>
gcloudlike config set <section>/<name> <value>
gcloudlike config list
gcloudlike config configurations list
gcloudlike config configurations create <name>
gcloudlike config configurations activate <name>
```

Global flags — `--configuration`, `--project`, `--account`, `--zone`,
`--format`, `--quiet` — are accepted after the command path.

### `compute instances list`

Requires an effective `core/project`; unset is a domain error. Lists the
instances of that project, narrowed to the effective `compute/zone` when one is
set, in ascending name order. A header line always precedes the rows, and
fields are joined by exactly one space with no padding:

```text
NAME ZONE MACHINE_TYPE STATUS
web-1 us-a1 small RUNNING
web-2 us-b2 small TERMINATED
```

### `compute instances describe <name>`

Requires an effective `core/project`. The named instance must belong to it;
otherwise it is a domain error, whether or not the name exists in another
project. Five lines, in this order:

```text
name: web-1
project: alpha-proj
zone: us-a1
machine_type: small
status: RUNNING
```

### `config get-value <section>/<name>`

Prints the effective value followed by a newline, exit 0. An unset property is
a domain error.

### `config set <section>/<name> <value>`

Stores the value in the **active set**, creating the configuration directory
and the set file when they do not exist. Prints nothing on stdout; the notice
goes to stderr:

```text
Updated property [core/project].
```

The property must be one of the three; otherwise a domain error, and nothing is
written.

### `config list`

The effective properties of this invocation, INI-shaped: sections in ascending
order, properties in ascending order within a section, and only properties that
have an effective value. A section with no such property is omitted.

```text
[compute]
zone = us-a1
[core]
project = alpha-proj
```

### `config configurations list`

Every existing set, ascending by name, with `True` for the active one:

```text
NAME IS_ACTIVE
default False
prod True
```

### `config configurations create <name>`

Creates an empty set. It does not become active. Stdout stays empty; stderr:

```text
Created [staging].
```

### `config configurations activate <name>`

Writes the name into `active_config`. The set must exist. Stdout stays empty;
stderr:

```text
Activated [prod].
```

## `--format`

`--format text` (the default) selects the human forms above. `--format json`
selects a machine projection instead:

- `instances list` → `{"instances":[{"name":…,"zone":…,"machine_type":…,"status":…}]}`
- `instances describe` → `{"name":…,"project":…,"zone":…,"machine_type":…,"status":…}`

JSON output is the same content whether stdout is a pipe or an attended
terminal: no styling, no prose, no framing may enter it.

## `--quiet`

Suppresses the stderr notices of `config set`, `configurations create` and
`configurations activate`. It never suppresses errors and never changes stdout.

## Errors

Domain errors print nothing on stdout, one line on stderr, and exit **1**:

```text
gcloudlike: unset property: core/project
gcloudlike: unknown property: core/nope
gcloudlike: no such instance: nope
gcloudlike: unknown configuration: nope
```

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success |
| 1 | domain error (unset or unknown property, unknown instance, unknown configuration) |
| 2 | usage error (unknown command, unknown flag, malformed property key) |
