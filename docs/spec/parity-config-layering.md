# Parity: Config Layering via clapfig

First Spec of the **Capability-parity program** (config layering → machine contract →
terminal citizenship), which follows the Robustness program. Depends on composition
contracts (the input-resolution seam) and is exercised by three corpus archetypes.

## Context

A survey of eleven best-of-breed complex CLIs (gh, git, jj, cargo, docker, kubectl,
gcloud, terraform, systemctl, brew, pnpm) found layered configuration to be the single
most universal capability — present in all eleven — and the only such capability standout
entirely lacks. Standout today reads exactly `VISUAL`/`EDITOR`/`PAGER`/`COLUMNS`/
`NERD_FONT`; there is no config file story, no discovery, no key↔env mapping, no
`--config k=v`. Every adopter hand-rolls the layer differently, untested, outside the
`TestHarness` seams. Three corpus archetypes (`gitlike`, `cargolike`, `gcloudlike`)
triangulate exactly this gap because each surveyed tool's merge rule catches different
bugs (walk-up merge with per-type semantics; first-file-wins env lists; named
configuration sets).

The sibling project **clapfig** (<https://github.com/arthur-debert/clapfig>, built on confique) already is this
capability, mature and framework-agnostic: struct-as-source-of-truth with doc-comment
templates, layered sparse merge (defaults < files < env < overrides, customizable
precedence), multi-path search with ancestor walk and boundary control, tree-walk
`Resolver` with per-file caching, mechanical `MYAPP__SECTION__KEY` env mapping,
kebab-case key normalization, strict unknown-key errors with file/line, post-merge
semantic validation, structured errors with miette-style rendering, commented template
generation, JSON Schema generation, and a clap adapter contributing drop-in
`config gen|get|set|unset|list` subcommands with `--scope`. Standout already follows a
clapfig precedent elsewhere: the questionnaire surface's injected-subcommand design
(ADR 0017) cites clapfig's `config` subcommand injection.

Standout also already has a *second* precedence system: `standout-input`'s `InputChain`
resolves per-value precedence across arg/env/stdin/prompt/default sources. Config
layering and input chains answer the same question — "where does this value come from,
in what order" — in two vocabularies today.

## Problem

A standout app cannot ship the config behavior users of any serious CLI expect (project
config discovered by walking up, user config in the platform directory, env overrides,
a `config` subcommand) without its author building the entire layer, and the corpus's
config archetypes cannot pass. Meanwhile the framework's own input-precedence story is
split between a would-be config layer (absent) and `InputChain` (present), guaranteeing
that the two will disagree about precedence semantics the moment both exist — unless
integrated deliberately.

## Goals

- **Standout integrates clapfig as *the* configuration layer** — not a reimplementation,
  not a wrapper API that hides it: the app author defines a clapfig/confique `Config`
  struct, hands it to the `App` builder, and receives discovery, layered merge, env
  mapping, strict errors, and the injected `config` subcommand family, with standout
  contributing what only it knows (the clap `Command` for flag auto-matching, the
  output/rendering pipeline for `config list`/errors, the `TestHarness` seams).
- **One precedence story.** The grill defines how clapfig layers and `InputChain`
  sources compose into a single documented resolution order (flag > one-shot override >
  env > project file > user file > defaults, with per-command input sources positioned
  explicitly in that order); the two systems share vocabulary in docs and diagnostics.
- **Framework settings ride the same rails**: standout's own knobs that belong in config
  (default output mode, color preference, pager choice, theme selection — the set the
  grill confirms) become a reserved config section resolved through the same layer,
  replacing ad-hoc env reads.
- **Config resolution is testable in-process**: `TestHarness` gains config-layer controls
  (fixture files per layer, env, cwd anchoring for the ancestor walk) so precedence is
  assertable the way stdin/env already are.
- **Config errors render through standout**: clapfig's structured errors (key, path,
  line, snippet) surface via standout's rendering/warning channels — plain and, where
  the machine-contract Spec lands, structured.
- The corpus archetypes `gitlike` (walk-up + `-c k=v`), `cargolike` (per-type merge +
  key↔env mapping), and `gcloudlike` (named sets, if in clapfig's scope) pass their
  config acceptance tests.

## Non-Goals

- Building configuration machinery in standout. Gaps found in clapfig are fixed *in
  clapfig* (same maintainer); standout carries integration only.
- Migrating existing downstream apps' hand-rolled config (they port at their own pace;
  the corpus demonstrates the path).
- Secrets management beyond clapfig's existing posture (the pnpm-style
  no-env-expansion-for-sensitive-keys hardening is a clapfig concern to evaluate there).
- Structured error *emission modes* (machine-contract Spec) — this Spec routes config
  errors into whatever channel exists.

## Proposed Shape

**1. The seam.** Building on composition contracts: config resolution happens once,
early (around `build()`/first dispatch — grill decides relative to argv parsing, since
`--config k=v` and `--scope` come from argv), producing a resolved config value that
joins `ResolvedConfig` as an input to boundary objects. Detection-adjacent settings
(color, pager, verbosity) flow config → target facts, so later parity features read one
source.

**2. The builder surface.** `App::builder().config::<MyConfig>(clapfig_builder)` (exact
shape grilled): the app's struct, clapfig search/precedence options, and standout-side
choices (inject `config` subcommands? reserved section name?). The clap adapter's
subcommand family is injected following the questionnaire precedent (ADR 0017), and
collision rules mirror the help-word collision handling (loud `SetupError`).

**3. Precedence unification.** `InputChain`'s `EnvSource`/defaults and the config layer
are documented as one ladder; where a command input names a config key as a source, it
resolves through the layer rather than a parallel env read.

**4. Harness + docs.** `TestHarness` layer fixtures; a config topic teaching the one
ladder; the wizard offers config scaffolding per the blessed idiom.

## User / Agent Stories

1. As an application author, I want to define a config struct and get discovery, merge,
   env mapping, and `config` subcommands, so that my CLI behaves like git/cargo without
   me building a config system.
2. As an app user, I want project config found by walking up from cwd and overridable by
   env and flags in a documented order, so that my expectations from every other serious
   CLI hold.
3. As an application author, I want one documented precedence ladder covering config
   files and per-command input sources, so that "where did this value come from" has one
   answer.
4. As an app user who typos a config key, I want an error naming the file, line, and
   nearest valid key, so that misconfiguration is minutes, not archaeology (clapfig
   strict mode, rendered by standout).
5. As a test author, I want to fixture each config layer in `TestHarness` and assert the
   resolved value, so that precedence bugs are caught in-process.
6. As the framework, I want my own settings (output default, color, pager) in the same
   ladder, so that framework behavior is configurable without bespoke env vars.

## Risks And Rabbit Holes

- **Two-repo coupling.** Integration work will surface clapfig gaps (e.g. named
  configuration sets, first-file-wins env lists). The discipline: file and fix in
  clapfig, pin versions, keep standout's integration free of workarounds — a workaround
  here is clapfig API debt deferred.
- **Precedence unification scope.** Full `InputChain` re-plumbing is a trap; the goal is
  one *documented, non-contradictory* ladder and a config-key source type — not
  rewriting input collection (which WIZ03 just stabilized).
- **Wrapper temptation.** Hiding clapfig behind a standout façade "for cohesion"
  recreates the glue-reimplements-leaf disease composition contracts just cured. Expose
  clapfig's types; standout adds only what only it knows.
- **Argv chicken-and-egg.** `--config k=v`/`--scope` live in argv; config may influence
  parsing-adjacent behavior (e.g. default output mode). The grill must fix the
  resolution order (parse → resolve config → resolved facts for dispatch) and reject
  config keys that would need to precede parsing.

## Cross-Cutting Concerns

- Security: config files are untrusted input (git's protected-scopes lesson);
  execution-adjacent settings (pager command, editor) honored only from appropriate
  scopes — grill decides the scope policy with clapfig.
- Performance: discovery walk once per invocation; clapfig's caching covers repeated
  resolution.
- Compatibility: purely additive for apps that opt out; the framework reserved section
  is versioned by the machine-contract Spec's stability policy once that exists.
- Release: coordinated standout+clapfig releases; corpus archetypes gate.

## Testing / Verification

Corpus acceptance tests for `gitlike`/`cargolike`/`gcloudlike` config behavior are the
external oracle (walk-up merge order, scalar-vs-append semantics, env mapping, flag >
env > file). In-repo: harness-fixtured precedence table tests (one row per adjacent
layer pair), collision `SetupError` tests for injected subcommands, config-error
rendering snapshots. clapfig's own suite continues to own merge-engine correctness.

## Workstream Hints

(1) Seam + minimal end-to-end (one struct, file+env+flag, resolved value reaches a
handler) as walking skeleton; (2) injected `config` subcommand family + collision rules;
(3) precedence unification + framework reserved section; (4) harness controls + docs +
wizard scaffolding; clapfig-side fixes threaded throughout as their own PRs in that
repo.

## Out Of Scope

Config machinery in standout, secrets features, machine-mode error emission, downstream
migrations.

## Further Notes

The clapfig repository's README and docs are the capability inventory; standout ADR 0017
(injected questionnaire surface) is the injection precedent. Expected ADRs: the
config/argv resolution order; the precedence ladder; the reserved framework section;
scope policy for execution-adjacent settings. Links to be added by the grill.
