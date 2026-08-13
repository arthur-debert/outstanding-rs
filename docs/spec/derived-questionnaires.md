# Derived Questionnaires

## Context

WIZ02 delivered questionnaire answer sheets: an application declares a `Questionnaire` through a builder API (`ScalarField`, `Group`, constraints, conditions, validators), and `standout-input` renders a prose answer sheet, parses and decodes submissions from interactive prompts, files, or stdin through one shared validation pipeline, guarded by a semantic fingerprint (ADR 0014, `docs/spec/questionnaire-answer-sheets.md`).

The first consumer — the bootstrap wizard in the `standout` binary — exposed the cost of the builder-level API. A deep review of the epic confirmed that the wizard encodes its questionnaire twice (a hand-rolled interactive prompt flow alongside the declarative definition, already divergent on context-dependent defaults), writes its answer vocabularies three times bridged by `.expect` calls that turn drift into runtime panics, and hand-converts decoded `Answers` back into domain structs through a second `.expect` chain. None of these are wizard bugs; they are the boilerplate any application pays at this API altitude.

Standout is a declarative framework: `standout-macros` already derives dispatch configuration from clap enums (`Dispatch`), tabular specs from struct annotations (`Tabular`), and query accessors (`Seekable`). The sibling project clapfig (built on confique) proves the adjacent pattern this Spec adopts: a plain Rust struct with doc comments and field attributes as the single source of truth, lowered by a derive macro.

The WIZ02 correction workstreams (WS06 tag-format recognition, WS07 dynamic defaults and collection robustness) are prerequisites and land before this feature; review findings in WIZ02 whose proper fix is this feature (vocabulary drift, the dual wizard) were deliberately deferred to it.

## Problem

Declaring a questionnaire today means writing four parallel artifacts that must agree: the definition (builder calls), the domain struct the answers become, the conversion between them, and — if the application wants interactive parity — a prompt flow. The wizard, at ~10 fields, needed ~170 lines of definition, ~120 lines of conversion, three copies of its choice vocabularies, and a duplicate interactive flow. Every agreement between these artifacts is maintained by hand and checked at runtime, if at all; the deep review found the divergences.

An application author should write the data struct for the information they collect and get everything else — sheet rendering, parsing, decoding into that struct, interactive collection, and the CLI surface (`--answers`, `questions`, `--yes`, confirmation) — from the framework.

## Goals

- A plain Rust struct with `///` doc comments and `#[question(...)]` attributes declares the ordinary static questionnaire shapes; a derive macro lowers it to the existing runtime model while the hand-built builder remains available for dynamic or unusual definitions.
- Decoded submissions materialize as the struct itself, typed; no hand-written conversion layer, no stringly-typed access in application code.
- Choice vocabularies are Rust enums with their own derive: one source for the declared choices, parsing, display, and the sheet's rendered hint.
- The `standout` framework offers declarative command wiring: a command declares its questionnaire input struct and receives the complete answer-sheet CLI surface, including the attended confirmation gate, without application code.
- The bootstrap wizard becomes a client of all of the above: one struct, no hand-rolled prompt flow, no conversion layer, no duplicated vocabulary.
- Definition equivalence is provable: a derived questionnaire and its hand-built builder equivalent produce identical fingerprints.

## Non-Goals

- Replacing or bypassing the runtime model. `definition.rs`, `decode.rs`, `fingerprint.rs`, and the collection adapters remain the semantic core and the compatibility contract; the derive is lowering, not a second engine.
- Compile-time verification of cross-field references (`active_when` controllers). These stay construction-time checks, as today.
- Answer-sheet format changes, migration, or versioning work. The WS06 tag format is fixed input to this feature.
- General-purpose form/survey features (branching pages, scoring, i18n of prompts).
- Backwards compatibility for the builder API's current consumers beyond keeping the builder itself working — it remains the lowering target and stays public.

## Proposed Shape

Four pieces, in dependency order.

**1. `#[derive(Questionnaire)]`** in `standout-macros`. The struct's shape is the definition: field order is presentation order; `///` doc comments are prompts - the first paragraph, unwrapped to one line, is the prompt, and later paragraphs are reserved (ignored today; rustfmt re-wrapping can never change a rendered sheet); field paths through nested structs are the stable IDs, and parent `#[question(id = "...")]` remapping is inherited by child paths. Types map: `String`/`PathBuf`/`bool` to the scalar kinds, `Option<T>` to optional scalar or choice fields, a nested struct to a group answered once, `Vec<NestedStruct>` with `#[question(min = ..., max = ...)]` to a repeatable group, and an enum field marked `#[question(choice)]` to a `one_of` constraint. Attributes carry what types cannot or must disambiguate: `id = "..."` on containers and fields (the final syntax; no `questionnaire_id` attribute exists), `default = "..."` (static), `default_with = path, revision = "..."` (dynamic, WS07 model), `validate = path, revision = "..."`, `active_when(field = "...", is = "...")` on `Option<T>` fields, `choice` (enum choice rather than nested questionnaire struct), `prose` (multi-line opt-out - single-line is the default contract of a scalar answer; prose fields are the marked exception), and `repeated` for scalar-vector block form. The derive generates a function producing the lowered `Questionnaire` via the existing builder - every construction-time validation invariant stays load-bearing.

The supported hook signatures are `fn(&EarlierAnswers<'_>) -> String` for `default_with` and `fn(&AnswerValue) -> Result<(), String>` for `validate`. A non-empty `revision` is mandatory for either hook because closure behavior cannot be fingerprinted directly; when both hooks are attached to one field, the same field-level revision identifies both hook contracts and must change when either accepted or supplied answers change. `active_when` is intentionally bounded to `Option<T>` fields controlled by an earlier scalar or choice field in the same group or an enclosing group. The attribute may name the controller by Rust field name or explicit ID, and the derive resolves that through inherited parent ID remapping before the runtime builder checks scope and order.

**2. Typed decode** in `standout-input` and the derive. The derive emits the filling code that materializes decoded `Answers` (occurrence-path keyed) as the struct — direct per-field construction over the closed questionnaire type universe, with no serde involvement in the contract (ADR 0015). The struct is the result; field-level `.expect` bridges cannot exist because there is nothing to bridge, and "decoded but unfillable" is unrepresentable. Scalar `Vec<String>`-style fields default to one single-line, comma-separated list answer, with a `#[question(repeated)]` opt-in to block-form repetition.

**3. `#[derive(QuestionnaireChoices)]`** in `standout-macros`, with its trait in `standout-input`. On a plain enum, generates the declared choice list (kebab-case variant names by default, `#[question(rename = "…")]` per variant), `FromStr`/`Display`, and wires into `#[question(choice)]` field lowering so the enum is the single vocabulary source for constraint, parse, and rendered hint. Renaming a variant's user-facing spelling changes the choice list and therefore the fingerprint, as a semantic change should.

**4. Declarative command wiring** in the `standout` framework crate. A command declares its input type (`#[dispatch(questionnaire = …)]` or the builder equivalent) and the framework *injects* the complete answer-sheet surface into that command's clap definition (ADR 0017): `--answers FILE` and `--answers -`, the `questions` subcommand (stdout or `--file`), and `--yes` — following the reserved `--output` flag-insertion and clapfig `config`-subcommand precedents — while resolution rides the existing `InputChain`/`InputCollector` seam (sheet, stdin, or interactive fallback resolving pre-dispatch, the handler receiving the typed struct) and the attended confirmation gate — the controlling-terminal confirmation, its no-terminal guidance, and the debug-only test seam — is hoisted from the wizard binary into the framework. Sources never merge; submitting a sheet never implies consent; the gate's invariants carry over unchanged.

The bootstrap wizard is then rewritten as the reference client: its questionnaire becomes one struct + two choice enums, its hand-rolled prompt flow and conversion layer are deleted, and it adopts `collect_interactive` with WS07 dynamic defaults.

## Implementation Status

The implemented wizard is the reference client of the final shape: a top-level `NewProjectAnswers` questionnaire with four nested derived structs (`ProjectAnswers`, `CommandAnswers`, `CommandInputAnswers`, and `ResultAnswers`) plus three `QuestionnaireChoices` enums for value type, cardinality, and result shape. Its command uses the framework-injected `questions`, `--answers`, and `--yes` surface, with app-side whole-form rules and a pre-confirmation review callback. The binary-local prompt loop, stringly conversion layer, local vocabulary parsers, and local attended gate were removed from the production path.

The `new_project_answer_sheets` integration tests changed mechanically around the new framework-owned surface and now cover terminal-seam behavior, Review-then-Confirm sequencing, parse warnings, BrokenPipe on `questions` stdout, stale fingerprints, batch diagnostics, and no-partial-write failures. Unit coverage pins derived-vs-builder fingerprint equivalence and interactive/file/stdin parity for nested, repeated, dynamic-default, prose, choice, and whole-form validation paths.

## User / Agent Stories

1. As an application author, I want to declare a questionnaire as a Rust struct with doc comments, so that one artifact defines the sheet, the validation, and the result type.
2. As an application author, I want decoded answers delivered as my struct, so that I never write conversion code or stringly-typed lookups.
3. As an application author, I want my choice fields to be Rust enums, so that adding a variant updates the sheet, the parser, and the constraint together and cannot drift into a panic.
4. As an application author, I want to declare a command's questionnaire input declaratively, so that `--answers`, `questions`, `--yes`, interactive fallback, and confirmation arrive without me writing CLI plumbing.
5. As an application author with context-dependent defaults or custom rules, I want function-path hooks (`default_with`, `validate`) with declared revisions, so that dynamic behavior composes with the fingerprint contract.
6. As a user of a generated CLI, I want sheet and interactive submissions to accept exactly the same values with the same diagnostics, so that the collection mode is a convenience choice, not a semantic one.
7. As an agent driving a generated CLI unattended, I want `--answers - --yes` to behave exactly as it does in the hand-built wizard today, so that automation contracts survive the rewrite.
8. As a framework maintainer, I want derived and hand-built definitions to fingerprint identically, so that adopting the derive (or leaving it) is never a compatibility event.

## Risks And Rabbit Holes

- **Macro scope creep.** The derive should lower to builder calls and stop; any temptation to give it its own validation, its own error rendering, or compile-time cross-field analysis duplicates the runtime model. The construction-time errors are already good; the derive's job is to surface them at startup/test time, not to re-implement them.
- **Filling-code totality.** The generated filling must cover every model feature (omitted optionals, inactive conditional `Option<T>` fields, repeated groups, defaults already resolved) for the closed type universe; a fill failure after successful decode is a contract bug, not a user error, and the derive's job is to make it unrepresentable (ADR 0015).
- **Doc-comment fidelity.** Prompts come from doc comments; rustfmt wrapping and multi-paragraph docs need a defined normalization (first paragraph is the prompt is the likely rule — grill decides).
- **The wiring layer meeting `Dispatch`.** Command wiring must compose with the existing declarative dispatch, not fork a second command-declaration mechanism.
- **Attended-gate regression.** The confirmation gate's security posture (no stdin byte confirms, absent terminal is an error, seam is debug-only) must move into the framework without weakening; its integration tests move with it.
- **Sequencing.** Piece 4 depends on 1–3; the wizard rewrite depends on all four plus WS07. Attempting the wizard rewrite first inverts the dependency and rebuilds scaffolding.

## Cross-Cutting Concerns

- **Fingerprint semantics.** Derived definitions must state their fingerprint surface exactly: field paths, kinds, optionality, defaults (or dynamic-default revisions), choice lists, conditions, validator revisions — and nothing cosmetic (doc-comment wording, attribute order). Equivalence tests against hand-built definitions are the enforcement.
- **Security.** The confirmation gate invariants are load-bearing and documented in WIZ02; hoisting them into the framework must preserve the debug-only seam and the never-from-stdin rule. Sheets may contain sensitive answers; the no-echo diagnostic policy carries into every derived surface.
- **MSRV / dependency policy.** The derive adds proc-macro surface to `standout-macros` (existing crate, existing pattern); typed decode is derive-generated with no serde in the contract (ADR 0015). No new heavyweight dependencies.
- **Compatibility.** The builder API remains public and unchanged; the derive is additive. Nothing in WIZ02/WIZ03 has shipped in a release, so no migration surface exists.

## Testing / Verification

- **Equivalence property**: for the wizard questionnaire and representative fixtures, derived definition ≡ hand-built definition (structural equality and fingerprint equality).
- **Round trip**: struct → rendered sheet → filled sheet → parse → decode → struct, across scalars, groups, repeatable groups, conditions, dynamic defaults, enums, prose fields.
- **Compile-fail suite** (trybuild-style) for every attribute misuse: both defaults declared, empty revision, `active_when` on unknown/later field surfaced at construction, non-struct/enum targets, unsupported field types.
- **Wiring integration**: a fixture app declaring one questionnaire command; assert the full CLI surface (`--answers` file and stdin, `questions`, `--yes`, interactive fallback, attended gate) against the behavior the wizard's integration tests pin today.
- **Wizard migration**: the existing `new_project_answer_sheets` integration tests pass unchanged against the rewritten wizard (the automation contract of story 7).

## Workstream Hints

Natural seams, in dependency order: (1) choices derive + trait; (2) questionnaire derive lowering to the builder; (3) derive-generated typed filling; (4) command wiring hoisting the gate; (5) wizard rewrite as the reference client + deletion of the duplicated flows. The deferred WIZ02 findings (vocabulary drift, dual wizard) close in (5).

## Out Of Scope

- Changes to the answer-sheet format, preamble, or fingerprint algorithm.
- A derive for whole-form rules (they remain a closure at the decode call site).
- Non-derive declarative frontends (config-file questionnaire definitions, runtime schema loading).
- Localization or theming of rendered sheets.

## Further Notes

Feature planned in the WIZ03 planning session following the WIZ02 deep review; the review findings and their disposition (fix in WS06/WS07 vs defer here) are recorded in issues #265–#267 and the epic issue #258's history. The grill (`/grill-me-with-docs`) that follows this Spec writes the ADRs for the durable decisions it crystallizes; each links back here.

- [ADR 0015 — Fill derived questionnaire structs without serde](../adr/0015-fill-derived-questionnaire-structs-without-serde.md)
- [ADR 0016 — Default scalar answers to single lines](../adr/0016-default-scalar-answers-to-single-lines.md)
- [ADR 0017 — Inject the answer-sheet CLI surface per questionnaire command](../adr/0017-inject-the-answer-sheet-cli-surface-per-questionnaire-command.md)

Decided at Spec level (no ADR: cosmetic or readily reversible): doc-comment prompts are the first paragraph unwrapped, later paragraphs reserved; `Vec<scalar>` lowers to one single-line, comma-separated list answer, with `#[question(repeated)]` opting into block form; choice enum fields use `#[question(choice)]` because otherwise a plain named type is ambiguous with a nested questionnaire struct; choice enums render kebab-case variant names with `#[question(rename = "…")]` per variant; `#[question(...)]` is the single attribute namespace in all positions (container, field, enum variant), following the serde precedent.

All grill branches are resolved; the decisions live in the ADRs above and the Spec-level notes.
