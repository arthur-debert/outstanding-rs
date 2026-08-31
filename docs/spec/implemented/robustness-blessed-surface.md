# Robustness: One Blessed Surface

Fifth epic of the **Robustness program** (ROB05). Depends on composition contracts
(ROB04, merged as #394) for the final shapes and on the corpus pilot (ROB03) for the
observed-friction input: the [pilot scorecard](../../../corpus/pilot/scorecard.md) and its
findings #349–#361. Runs in parallel with the adopter-seams epic (ROB06,
`robustness-adopter-seams.md`); both precede corpus completion (ROB07), which re-runs the
archetypes against the surface this epic ships.

## Context

The August 2026 DX audit counted the surface a downstream author faces at 8.1.0. ROB02 and
ROB04 have since removed a share of it; the census below is of `main` at 7cb4152 and
replaces the audit's numbers as this epic's baseline.

- **`AppBuilder`: 35 public methods** (was 60), plus 27 on `App`, 25 on `CommandConfig`,
  12 on `GroupBuilder` — `crates/standout/src/cli/builder/{commands,config,execution,mod}.rs`,
  `cli/group.rs`.
- **Constructors: 2** (`App::builder()`, `AppBuilder::default()`; `App::new` is gone).
  **Run-family entry points: 6** (`run`, `run_with`, `run_to_string`, `dispatch`,
  `dispatch_from`, `run_command`) plus 4 render entries, 6 `parse`/`get_matches*`
  methods on `App`, and 2 free `cli::parse*` functions.
- **Wiring a command: 9 items across three axes**, which compose rather than
  substitute for each other. Registration (binding a name to a handler on the app): 7 —
  builder `command` / `command_with` / `command_handler` / `command_handler_with` /
  `command_passthrough` / `group(…)`, and `.commands(Commands::dispatch_config())` from
  `#[derive(Dispatch)]`. Handler adaptation (turning a fn into the dispatch signature):
  `#[handler]`. Adaptation plus declaration (the clap `Command` itself): `#[command]`, which
  generates the handler wrapper and the `Command` from one source but still needs
  registration. A working idiom takes one item from each axis (a `#[derive(Dispatch)]`
  enum registers, a `#[handler]` fn adapts, clap-derive declares), so the census counts
  items, not interchangeable alternatives.
- **Template provision: 9 paths** (inline third argument, `CommandConfig::template`,
  `template_name`, convention, `embed_templates!`, `templates_dir`, `template_ext`,
  `include_framework_templates`, `template_engine`). `.template_dir()` is gone from code
  but still named in `docs/information-lib.lex` and `docs/proposals/new-api.md`.
- **Theme provision: 5 builder methods + 7 `Theme` constructors** plus `merge` /
  `add_adaptive`.
- **Re-exports: ~99 items** in `crates/standout/src/lib.rs:241-340` (82 from
  `standout-render`), including internals: `rgb_to_ansi256`, `rgb_to_truecolor`,
  `flatten_json_for_csv`, `serialize_to_xml`, `walk_dir`, `walk_template_dir`,
  `extension_priority`, `strip_extension`, `build_embedded_registry`, `validate_template`,
  `render_auto_with_spec`; `cli/mod.rs:149-186` re-exports ~40 more, among them dispatch
  internals (`extract_command_path`, `get_deepest_matches`, `insert_default_command`).
- **`help_handling` defaults to `false`** (`builder/mod.rs:589`), and no reference
  artifact turns it on: not `tdoo`, not the wizard's generated project, not
  `docs/guides/minimal-single-crate.md`.
- **The three reference clients still teach different idioms.** `tdoo`:
  `command_with` + typed `#[handler]` params + `InputChain`. Wizard output:
  `command_with` + raw `ArgMatches` + `std::fs::read_to_string`, no `.version()`, no
  default command. Minimal guide: `#[derive(Dispatch)]` + `#[dispatch(pure)]`, no input
  handling at all. The guides pin `standout = "7"` at 8.1.1.
- **Wizard output is compile-tested** — `generated_project_matrix_formats_checks_tests_and_runs`
  in `crates/standout/src/bin/standout.rs:2823` runs `cargo fmt --check` / `check` /
  `test` on generated projects under `pixi run test`. The generator-rot risk (#244) is
  covered; what is not covered is that the generated project uses the *blessed* idiom.
- **`docs/SUMMARY.md` orphans**: `topics/standout-help.md`, `topics/topics-system.md`,
  both `index.md`s, two upgrade guides, `crates/input/design.md`, and
  `crates/standout-pipe/docs/topics/piping.md` — a piping topic exists but is mounted
  nowhere. No topic exists at all for ListView, seeker, passthrough, or `#[command]`.
- **No stability statement** exists anywhere in the repo.

The corpus pilot (four blind runs, scorecard themes 1, 3 and 5, all 4/4 or 2/4 in
frequency) confirmed the audit from the adopter side: every run hit a place where two
documented paths disagree with each other or with the macro. The findings still open on
`main` that belong to this epic, verified 2026-08-30:

| Finding | What the adopter hit | Where it lives |
| --- | --- | --- |
| #350 | `#[derive(Dispatch)]` names commands in snake_case, clap-derive in kebab-case; no per-variant rename; the miss is a silent `NoMatch` | `standout-macros/src/dispatch.rs:392,440` |
| #355 | a `#[handler]` fn under `#[dispatch(questionnaire = …)]` fails with an arity error from inside the derive unless `pure` is set | `dispatch.rs:451-453, 566-586` |
| #358 | generated `name__handler` / `name__expected_args` trip `non_snake_case` in every consuming crate | `standout-macros/src/handler.rs:505-511` |
| #360 | the questionnaire derives expand to `::standout_input::…`, so a single `standout` dependency does not compile despite the docs' claim | `standout-macros/src/questionnaire/mod.rs:282-684`, `standout/src/lib.rs:337` |
| #349 | `#[handler]` kebab-cases parameter names into clap ids (`handler.rs:388-391`); closed 2026-08-19 without a code change — the mapping is undocumented | `handler.rs:388` |
| #361 | the docs errata list; the items still wrong on `main` are enumerated under Goals | docs |

Repo policy: no backwards compatibility; the users are the maintainer and downstreams
who port breaking changes rather than carry weight.

## Problem

Redundant near-equivalent paths are where agent-generated drift accumulates: each
session picks (or invents) a variant, reference clients diverge, tests cover the
diagonal of a growing matrix, and every additional path multiplies the interaction
surface the program exists to shrink. The pilot measured the consequence: four blind
adopters, four independent collisions between a documented idiom and the macro that
implements it. A framework whose own examples disagree, whose flagship feature is off
everywhere, and whose derive macros reject each other cannot be adopted from its
documentation.

## Goals

- **One blessed idiom per concern, recorded in an ADR**: one primary
  handler-registration path (expected: `#[derive(Dispatch)]` + `#[handler]` with typed
  parameter attributes as the default style), one template mechanism, one theme mechanism,
  two entry points (a run form for binaries, a capture form for tests/embedding).
  Secondary paths survive only with a one-line stated reason (e.g. the builder closure
  as the dynamic-registration escape hatch; `run_command` as the manual-dispatch seam ROB04
  just repaired); unlisted paths are deleted with their tests.
- **The blessed idiom is internally consistent.** The pilot's macro collisions are fixed
  as part of blessing, not as separate bugs: `#[derive(Dispatch)]` emits kebab-case
  command names by default with a per-variant `name = "…"` rename, and a `NoMatch` on a
  name the derive registered is a loud error, not a `false` return (#350); a `#[handler]`
  fn works under every `#[dispatch(…)]` form, or the derive rejects the combination with
  a diagnostic that names it (#355); generated items carry their own lint allows (#358);
  the questionnaire derives route through `standout`'s re-export so one dependency is
  enough (#360); the `#[handler]` parameter-name → clap-id mapping is one documented rule
  the macro and the docs agree on (#349).
- **`AppBuilder` drops to roughly 20 methods**; `CommandConfig` and `GroupBuilder` are
  pruned on the same rule; the ~99 root re-exports plus ~40 `cli` re-exports are curated
  to what the blessed idioms need, and the internals listed in Context lose their `pub`.
  The before/after census is the epic's metric.
- **`help_handling(true)` becomes the default.** ROB02's build-time validation already
  guards the collision cases.
- **The three reference clients converge on the blessed idioms**, and the wizard's
  generated project *is* the canonical example. The existing generated-project test gains
  an assertion that the emitted source uses the blessed registration, template, theme and
  input idioms: a diff against a checked-in golden, or one positive structural assertion
  per required idiom (the blessed derive, the blessed template call, the blessed theme
  constructor, the blessed input entry are each present) with negative assertions for
  the unblessed forms on top. A negative-only grep passes on output that omits the
  concern, so it is not the check.
- **Docs tell the truth and are reachable.** The #361 items still wrong on `main` are
  fixed: `docs/crates/dispatch/topics/handler-contract.md:22,48` (generated return type,
  `Result` auto-wrapping), `docs/guides/intro-to-standout.md:687` (registers the
  un-suffixed fn), `docs/topics/app-configuration.md:172-178` (nested derive without
  `handlers = …`), `docs/guides/tldr-intro-to-standout.md:86` (`dispatch_config()?`),
  `docs/crates/input/topics/framework-integration.md:204` (single-dependency claim,
  closes with #360), `docs/crates/render/guides/intro-to-tabular.md:132` ("grows to fit
  content" — wording follows whatever the adopter-seams epic does with #359), the
  `standout = "7"` pins, the two stale `template_dir` mentions. The orphaned piping
  topic is reviewed for accuracy and mounted, not rewritten. The missing-but-load-bearing
  items get written: the `#[dispatch(…)]` attribute reference (`pure` is used in three
  guides and defined nowhere), the trailing-newline contract, the `#[handler]` id
  mapping, nested template-path resolution for derive commands, and the handler
  diagnostic framing (`docs/topics/error-handling.md:10-11` names it without showing
  it). Orphaned topics enter `SUMMARY.md`; each surviving undocumented feature (ListView,
  seeker, passthrough, `#[command]` if it survives) gets one accurate topic or is deleted
  with its code.
- **A stability statement exists**: which surfaces are contract (the blessed idioms; the
  structural shape of each `--output` mode's bytes), which are internal. The
  machine-readable schema contract is parity-program scope; the policy that a contract
  exists starts here.
- **The program's compatibility break ships as one major version**, with every
  `CHANGELOG/unreleased-*.md` fragment from ROB01–ROB05 consolidated into its migration
  notes. Corpus completion depends on this release: the runner pins crates.io versions
  by design (ADR-0023).

## Non-Goals

- New capabilities or new idioms — this epic chooses among what exists post-ROB04.
- The adopter escape-hatch findings (#351, #354, #356, #357, #359, #334, #353, #352) —
  `robustness-adopter-seams.md`. Where a docs fix here depends on one of those (#359's
  tabular wording, #357's diagnostic framing), the docs state the behavior that epic
  ships, and the two epics coordinate the wording in review.
- Machine-output schema versioning (parity: machine contract).
- Corpus completion, and porting the real downstreams (they are on 7.x; see
  `../robustness-corpus-completion.md`).
- Preserving source compatibility for deleted paths.

## Proposed Shape

**1. The choosing.** A short ADR round settles the blessed set, from three inputs: the
census above, the scorecard's ranked friction themes, and a capability → path map
(which capability does each of the 7 registration, 2 adaptation/declaration, 9 template
and 5+7 theme items uniquely expose? — stateful struct handlers, passthrough, dynamic
registration, custom engine). The map is drawn per axis: a complete idiom is one item
from each of registration, adaptation and declaration, and a layer is only deletable
when another item on the *same* axis covers its capability.
Each surviving secondary path gets its one-line reason; the map is what prevents a
deletion from stranding a capability.

**2. The pruning and the macro repairs.** Delete unblessed paths and their tests; shrink
re-exports; visibility sweep; the five macro items from Goals. Mechanical once (1) is
fixed, and the macro items ride here because the blessed idiom is the derive path — its
defects are the blessed surface's defects.

**3. The teaching.** Rewrite `tdoo` and the minimal guide on the blessed idioms; make the
wizard emit them and pin that with the golden assertion; fix the errata; write the
missing references; complete `SUMMARY.md`; the stability statement; the README leads
with the strongest verified pitch (testability: handlers return data, demonstrated by
the testing guide against the post-ROB04 harness).

**4. The release.** Consolidated migration notes; version bump; publish. The corpus
completion epic's first act is a re-run against this published version.

## User / Agent Stories

1. As a new adopter, I want the README, the wizard output and the example app to show me
   the same idiom, so that I learn one way that works instead of three that disagree.
2. As an agent implementing a downstream app from the docs alone, I want `#[derive(Dispatch)]`
   and `#[handler]` to compose in every documented combination, so that the arity error
   the formlike run hit cannot happen.
3. As an application author, I want themed help on by default, so that the framework's
   distinguishing feature is what my users see without me discovering a flag.
4. As a maintainer, I want the wizard's generated project asserted against the blessed
   idiom, so that generator drift is a red test and not a scorecard finding.
5. As a maintainer reviewing PRs, I want a stability statement, so that "is this change
   breaking?" has a written answer.

## Risks And Rabbit Holes

- **Deletion cascade.** The capability → path map in step 1 is the guard; a deletion
  that strands a capability reopens the ADR, not the code.
- **Macro repairs growing into macro redesign.** #350/#355/#358/#360/#349 are each a
  bounded change to `standout-macros`; a new attribute grammar or a second derive is out.
- **Docs scope explosion.** Surviving features get *a* topic (accurate, reachable), not
  a book. The list in Goals is the boundary.
- **Default-flip fallout.** `help_handling(true)` by default changes every existing app's
  help output on upgrade; the migration notes and the existing collision `SetupError`s
  carry it.
- **Pruning ahead of the seams epic.** The adopter-seams epic adds a small number of
  builder/`Output` entries (programmatic output mode, exit-status seam). Prune with that
  list in hand so the two epics do not fight over the same file; the ADR round reads that
  Spec before choosing.

## Cross-Cutting Concerns

- This epic is the program's one deliberate compatibility break; it lands as one major
  version with consolidated migration notes.
- CI gains doc-example compile checks and an mdbook link check so doc truth is enforced,
  not audited.
- The two friendly downstreams and the corpus archetypes are the acceptance environment:
  the corpus re-run (ROB07) and the lookma port measure the migration cost.

## Testing / Verification

Wizard output compiles, runs, passes its generated tests, and matches the blessed-idiom
assertion; doc examples compile; the ROB01 snapshot matrix holds for surviving paths;
the surface census (builder-method count, root re-export count, public-item count) is
recorded before/after; each deleted path's tests go in the commit that deletes the path;
each macro repair has a regression test in `standout-macros` (kebab-case default and
rename, the questionnaire + `#[handler]` combination, no `non_snake_case` warning under
`#![deny(warnings)]`, a standout-only consumer crate compiling the questionnaire derives).

## Workstream Hints

(1) ADR round: blessed set + capability map + stability statement, reading the
adopter-seams Spec; (2) prune + visibility sweep + macro repairs; (3) reference-client
convergence + wizard golden assertion; (4) doc-truth sweep + missing references +
SUMMARY + README; (5) release consolidation. (3) and (4) parallelize after (2); (5) closes.

## Out Of Scope

New features, schema versioning, corpus construction, crate reorganization, downstream
porting, the adopter-seams findings.

## Further Notes

The ADR round settled three decisions:
[bless one item per axis behind a capability map](../../adr/0032-bless-one-item-per-axis-behind-a-capability-map.md)
(the blessed-idiom set, each axis's keep-with-reason or delete verdict, the
capability → surviving-item map, and the adopter-seams additions blessed on arrival);
[state which surfaces are contract](../../adr/0033-state-which-surfaces-are-contract.md)
(the stability statement and where it lives); and
[handle help by default](../../adr/0034-handle-help-by-default.md).
Inputs: the census in Context (supersedes the 8.1.0 DX audit
numbers), the [pilot scorecard](../../../corpus/pilot/scorecard.md), issues #349–#361. The
audit's redundancy catalogue lived only in the 2026-08-15/16 session record; the census
here is its in-repo replacement and the ADR round should not need the original.
