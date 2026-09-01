# `kubelike` — behavioral spec

`kubelike` is a cluster resource client in the kubectl mold. A small fixed set
of verbs — `get`, `describe` — applies over an **open set of resource kinds**
named as an *argument*, not as a subcommand: `kubelike get pods`,
`kubelike describe node node-a`. Kinds carry metadata (aliases, whether they
are namespaced) that decides which flags are legal for a given invocation.
It reads one frozen, built-in cluster state; there are no mutating commands.

## What this archetype stresses

The behavioral sketch behind survey entry C3 is not in this repository, so
this spec does not reconstruct it. It states the interactions it is here to
stress and is written to those:

1. **The command surface where the type is data.** `ghlike` (C2) stresses
   depth in a fixed command tree. `kubelike` is that tree's transpose: two
   verbs over a registry of kinds, reached by plural name, singular name, or
   short alias, all three resolving to the same handler.
2. **Registry metadata deciding argument validity.** `nodes` is not
   namespaced, so `-n` and `-A` are usage errors on it and legal on `pods` —
   the same flag on the same verb, accepted or refused by a property looked up
   at run time.
3. **A format vocabulary wider than a render-mode enum.** `-o` selects among a
   default table, `wide` (same rows, more columns), `name` (pipe-shaped
   identifiers), `json`, and `custom-columns=` (a **caller-chosen column set**,
   headers and all). A user-supplied template *language* is deliberately
   absent: runtime templates as untrusted input are `jjlike`'s (C9) axis.
4. **Heterogeneous collections through one render.** `get pods,services`
   renders two column sets in one invocation, and one flat JSON list whose
   elements have different shapes.
5. **An ambient default overridden per invocation.** The namespace comes from
   a flag, an environment variable, a config file's recorded value, or a
   built-in default, in that order. This is deliberately one file and one
   precedence chain, not a merge rule: merge semantics across config layers
   are `gitlike`/`cargolike`/`gcloudlike`'s (C1/C6/C7) triangulation.
6. **The empty set is not an error.** A listing that matches nothing exits 0,
   and what it *emits* differs by format: human prose on stderr, an empty list
   on stdout under `json`.

Streaming and attachment (`logs -f`, `exec`), document input (`apply -f`), and
server warnings riding a successful command are kubectl traits this archetype
deliberately omits: they are past current framework capability, and this
archetype is authored inside it.

## The frozen cluster state

Namespaces are not enumerated anywhere: any name may be asked for, and one
holding no resources is an empty result, not an error.

Pods:

| namespace | name | ready | status | restarts | node | labels |
| --- | --- | --- | --- | --- | --- | --- |
| `default` | `web-1` | `1/1` | `Running` | 0 | `node-a` | `app=web` |
| `default` | `web-2` | `1/1` | `Running` | 2 | `node-b` | `app=web` |
| `default` | `db-0` | `1/1` | `Running` | 0 | `node-a` | `app=db` |
| `kube-system` | `dns-1` | `1/1` | `Running` | 0 | `node-a` | `app=dns` |

Services:

| namespace | name | type | clusterIP | port | selector |
| --- | --- | --- | --- | --- | --- |
| `default` | `web` | `ClusterIP` | `10.0.0.10` | `80/TCP` | `app=web` |
| `kube-system` | `dns` | `ClusterIP` | `10.0.0.53` | `53/UDP` | `app=dns` |

Nodes (cluster-scoped — they are in no namespace):

| name | status | role | version |
| --- | --- | --- | --- |
| `node-a` | `Ready` | `control-plane` | `v1.0.0` |
| `node-b` | `Ready` | `worker` | `v1.0.0` |

Resources always list in the order given above, filtered to the selected
scope.

## The kind registry

| name | short | namespaced | kind |
| --- | --- | --- | --- |
| `pods` | `po` | true | `Pod` |
| `services` | `svc` | true | `Service` |
| `nodes` | `no` | false | `Node` |

A kind argument may be the plural name, the singular (`pod`, `service`,
`node`), or the short name — `get pods`, `get pod`, and `get po` are the same
command. `get` also accepts a comma-separated list of kinds.

## The command tree

```text
kubelike
├── get <kind>[,<kind>...] [name] [-n <ns>] [-A] [-l <selector>] [-o <format>]
├── describe <kind> <name> [-n <ns>]
├── api-resources
└── config current-context
```

Tables follow one layout rule everywhere: columns separated by a single space,
each column but the last padded to its widest rendered cell, the header
included in that width.

## Namespace resolution

For a namespaced kind, the namespace in effect is the first of:

1. `-n <ns>` / `--namespace <ns>`
2. the `KUBELIKE_NAMESPACE` environment variable
3. the `namespace:` line of the kubeconfig file
4. `default`

The kubeconfig file is `$KUBECONFIG` when that variable is set, otherwise
`$HOME/.kubelike/config`. It is line-oriented:

```text
current-context: staging
namespace: kube-system
```

A missing file is not an error — resolution falls through it. `-A` /
`--all-namespaces` lists across every namespace instead and adds a leading
`NAMESPACE` column.

For a cluster-scoped kind, `-n` and `-A` are usage errors (exit 2, stdout
empty, stderr naming the kind and that it is not namespaced). Plain
`get nodes` needs neither.

A comma-separated list is judged as one invocation, not kind by kind: if any
kind in the list is cluster-scoped, `-n` and `-A` are that same usage error —
exit 2, stdout empty, stderr naming the cluster-scoped kind — and no block is
rendered, including the namespaced kinds' own. Without those flags a mixed list
is legal: each kind lists in its own scope, the namespaced ones in the resolved
namespace, and the blocks render in the order asked.

## `get` — output formats

`-o <format>` selects the rendering. Without it, the default table.

### default

`kubelike get pods` in `default`:

```text
NAME  READY STATUS  RESTARTS
web-1 1/1   Running 0
web-2 1/1   Running 2
db-0  1/1   Running 0
```

`kubelike get pods -A`:

```text
NAMESPACE   NAME  READY STATUS  RESTARTS
default     web-1 1/1   Running 0
default     web-2 1/1   Running 2
default     db-0  1/1   Running 0
kube-system dns-1 1/1   Running 0
```

Services and nodes carry their own column sets:

```text
NAME TYPE      CLUSTER-IP PORT
web  ClusterIP 10.0.0.10  80/TCP
```

```text
NAME   STATUS ROLE          VERSION
node-a Ready  control-plane v1.0.0
node-b Ready  worker        v1.0.0
```

`get pods,services` renders one block per kind in the order asked, separated
by one blank line.

### `-o wide`

The same rows with one more column — `NODE` for pods, and for services and
nodes nothing beyond the default set, so `wide` is identical to the default
there.

```text
NAME  READY STATUS  RESTARTS NODE
web-1 1/1   Running 0        node-a
web-2 1/1   Running 2        node-b
db-0  1/1   Running 0        node-a
```

### `-o name`

One `<singular-kind>/<name>` line per resource, no header, nothing else. This
is the surface scripts pipe, so it is byte-identical whatever the terminal or
the `--output` flag says:

```text
pod/web-1
pod/web-2
pod/db-0
```

### `-o json`

Pure JSON on stdout: never styled, never prefaced with prose, even on an
attended color-capable terminal. A listing emits `{"items":[...]}`; a get
naming one resource emits that resource's bare object.

```json
{"kind":"Pod","namespace":"default","name":"web-1","ready":"1/1",
 "status":"Running","restarts":0,"node":"node-a","labels":{"app":"web"}}
```

```json
{"kind":"Service","namespace":"default","name":"web","type":"ClusterIP",
 "clusterIP":"10.0.0.10","port":"80/TCP","selector":{"app":"web"}}
```

```json
{"kind":"Node","name":"node-a","status":"Ready","role":"control-plane",
 "version":"v1.0.0"}
```

`restarts` is a JSON number; `labels` and `selector` are objects; nodes carry
no `namespace`. Every field is always present regardless of `-o wide`, which
governs the table only. `get pods,services -o json` emits one `items` list
holding both shapes, in kind order.

### `-o custom-columns=<HEADER>:<field>[,...]`

The caller chooses the columns and their headers. Headers print verbatim, in
the order given; the layout rule is unchanged.

```console
$ kubelike get pods -o custom-columns=NAME:name,NODE:node
NAME  NODE
web-1 node-a
web-2 node-b
db-0  node-a
```

The field vocabulary of a kind is the scalar keys of its JSON object other than
`kind`, which names the shape rather than a value and is not selectable:
`name`, `namespace`, `ready`, `status`, `restarts`, `node` for pods; `name`,
`namespace`, `type`, `clusterIP`, `port` for services; `name`, `status`,
`role`, `version` for nodes. An unknown field is an error (exit 1, empty
stdout) whose stderr names the offender and lists the valid fields.

### `-o` and `--output`

`-o` is `kubelike`'s own format selector. `--output` is a separate global flag
that comes from the toolkit `kubelike` is built with and steers rendering
(`text`, `term`, `json`, …). They are different flags, and both may appear on
one command line: `get pods -o name --output text` emits exactly the `-o name`
bytes above.

## `-l <selector>`

`-l app=db` / `--selector app=db` keeps the resources whose labels carry that
exact key and value. Column widths are computed over the rows that survive:

```text
NAME READY STATUS  RESTARTS
db-0 1/1   Running 0
```

## The empty result

A listing matching nothing exits **0**. stdout carries the format's empty
form, and any explanation goes to stderr:

- default, `wide`, `name`, `custom-columns=`: stdout empty; stderr
  `No resources found in <ns> namespace.` — or `No resources found.` when the
  listing was `-A`.
- `json`: stdout `{"items":[]}`, stderr empty.

## `describe <kind> <name>`

A detail block: `<Key>:` padded to the widest key in the block, then a single
space, then the value. Each kind has its own key set, so the padding is
recomputed per block. All three kinds in the registry, in registry order —
pods, services, nodes:

```text
Name:      web-1
Namespace: default
Node:      node-a
Status:    Running
Restarts:  0
Labels:    app=web
```

```text
Name:      web
Namespace: default
Type:      ClusterIP
ClusterIP: 10.0.0.10
Port:      80/TCP
Selector:  app=web
```

```text
Name:    node-a
Status:  Ready
Role:    control-plane
Version: v1.0.0
```

`describe` resolves its namespace exactly as `get` does — the same flag > env >
kubeconfig > `default` chain for a namespaced kind, and `-n` a usage error on a
cluster-scoped one. A name matching nothing is `get`'s domain error unchanged
(exit 1, stdout empty, the kind's *plural* name in the message):
`kubelike: services "nope" not found in namespace "default"`.

## `api-resources`

The registry itself, as data:

```text
NAME     SHORTNAMES NAMESPACED KIND
pods     po         true       Pod
services svc        true       Service
nodes    no         false      Node
```

## `config current-context`

Prints the kubeconfig's `current-context` value and exits 0:

```text
staging
```

With no kubeconfig file, exit 1, empty stdout, stderr
`kubelike: no current context set`.

## Errors

Domain errors leave stdout untouched and put one prose line on stderr:

| situation | stderr |
| --- | --- |
| unknown kind | `kubelike: the server doesn't have a resource type "widgets"` |
| named resource absent | `kubelike: pods "nope" not found in namespace "default"` |
| unknown `custom-columns` field | `kubelike: unknown field "bogus" for pods; valid fields: name, namespace, ready, status, restarts, node` |
| no current context | `kubelike: no current context set` |

## Exit codes

| code | meaning |
| --- | --- |
| 0 | success, including a listing that matched nothing |
| 1 | domain errors: unknown kind, absent resource, unknown field, no current context |
| 2 | usage errors: namespace flags on a cluster-scoped kind, unknown flag or subcommand |
