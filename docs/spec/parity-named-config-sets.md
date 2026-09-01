# PAR05: Named Configuration Sets

Last epic of the capability-parity program, run in the order PAR02 (machine contract)
→ PAR01 (config layering) with PAR04 (corpus runner fixes) alongside → PAR03 (terminal
citizenship) → PAR05 (this). PAR05 depends on PAR01: it adds one more layer source to
the clapfig integration PAR01 ships, and it cannot start before `App::builder().config`
exists. The exit criterion is executable: the 24 `gcloudlike` cases tagged
`gap = "PAR05"` pass in a blind corpus run whose produced application depends on
clapfig, with no `hand-rolled-pass` outcome (PAR01 D17).

## Problem

gcloud keeps several complete configurations side by side and switches between them:
`gcloud config configurations create staging`, `activate prod`, `list`, plus
`--configuration <name>` and `CLOUDSDK_ACTIVE_CONFIG_NAME` to pick one per invocation.
Each set is a file in a directory (`configurations/config_<name>`), and one file names
the active set. Docker contexts and kubectl contexts are the same shape.

clapfig 0.24.0 has no such feature. Its search paths are `Cwd`, `Ancestors`, `Platform`
and explicit paths; nothing selects a file by a name resolved from a flag, an env var
and a pointer file. The `gcloudlike` archetype specifies the shape in full
(`corpus/archetypes/gcloudlike/spec.md`), and 24 of its 51 acceptance cases cannot be
answered without it: creating, activating and listing sets, resolving the active set
from `--configuration` then `GCLOUDLIKE_ACTIVE_CONFIG` then the pointer file then
`default`, and the two-chain rule in its manifest's
`selector-chain-vs-property-chain` interaction (the set selector and the property
values resolve on separate chains; a flag naming a set does not outrank an env var
naming a property inside it). In the completion run the blind agent hand-rolled all of
it by rewriting argv, which is the outcome D17 now reports as `hand-rolled-pass`.

## What the user gets

### The CLI user

```text
myapp config configurations create staging
myapp config configurations activate staging       # writes the active pointer
myapp config configurations list                   # NAME  ACTIVE
myapp --configuration prod deploy                  # one invocation, no pointer change
MYAPP_ACTIVE_CONFIG=prod myapp deploy
myapp config set core.project beta --scope set     # writes into the active set's file
```

Selector precedence: `--configuration`, then `MYAPP_ACTIVE_CONFIG`, then the pointer
file, then `default`. Property precedence inside the selected set stays PAR01's ladder.
Naming a set that does not exist is a domain error, exit 1, on stderr:
`myapp: unknown configuration: nope`.

### The app author

```rust
App::builder()
    .config(
        clapfig::Clapfig::typed::<MyConfig>()
            .app_name("myapp")
            .named_sets(clapfig::NamedSets {
                dir: clapfig::SearchPath::Platform.join("configurations"),
                file_prefix: "config_",
                pointer_file: "active_config",
                selector_env: "MYAPP_ACTIVE_CONFIG",
                default_name: "default",
            }),
    )
```

standout adds `--configuration <name>` as a global flag when `named_sets` is
configured, injects `config configurations create|activate|list|delete` beside the
PAR01 `config` family, and adds `set` to the `--scope` values. Everything else is
clapfig.

### The test author

```rust
let r = TestHarness::new(app)
    .config_set("prod", "[core]\nproject = \"beta\"\n")
    .active_set("prod")
    .run(&["config", "get", "core.project"]);
r.assert_stdout("beta\n");
r.assert_config_layer("core.project", ConfigLayer::Set("prod"));
```

## Decisions

**D31. Named sets are a clapfig feature.** `clapfig::NamedSets` is one more layer
source in clapfig's `layer_order`, positioned between the user file and env by default.
standout contributes the flag, the injected subcommands and the harness fixtures only.
Reason: PAR01 D11 (expose clapfig, do not wrap it) applies; a set store built in
standout would be the second config engine PAR01 exists to prevent.

**D32. Two chains, not one.** The set selector resolves on its own chain (flag, env,
pointer file, default) before any property resolves. Reason: `gcloudlike`'s manifest
records the collapse bug a single ladder produces (a selector flag answering for a
property env var), and its cross-chain cases exist to catch it.

**D33. A missing set is a domain error, not a config error.** `activate nope` and
`--configuration nope` exit 1 with the app's own message; they are not clapfig
`ConfigError`s with file and line. Reason: the user typed a name, not a key, and the
gcloudlike cases assert the message shape.

## Workstreams

**WS01. clapfig: `NamedSets` layer source.** In `arthur-debert/clapfig`: the type, the
selector chain, `Layer::Set` attribution in `config list` output, create/activate/
delete/list operations on the store, the `set` scope for writes. Done when clapfig's
own tests cover selector precedence and the two-chain rule, and a release is on
crates.io.

**WS02. standout exposure.** `--configuration`, the injected `configurations` family,
`ConfigLayer::Set` in the harness, docs. Done when the hermetic loop test for
`gcloudlike` passes every case.

## Exit criteria

- One blind `gcloudlike` run via `corpus-runner batch` with all 51 cases passing, the
  clapfig evidence present, and the questionnaire listing no config workaround.
- `gcloudlike`'s manifest `[gaps]` entry removed in the closing PR.

## Issues

None open. The completion-run `gcloudlike` report (`corpus/completion/runs/gcloudlike-*`)
is the evidence.

## Out of scope

Per-set encryption or secrets; remote or shared set stores; migrating docker- or
kubectl-style context files an app already has.
