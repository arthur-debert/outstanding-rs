# ROB03 corpus pilot scorecard

This scorecard records four partially blind implementations against Standout 8.1.1 and the
documentation snapshot at `c9d7198c173a986756876994431a3174366bdef6`. Each agent
received an archetype specification, the published documentation snapshot, and crates.io
dependencies. The provisioned workspaces omitted Standout source, but the run directories
were nested beneath a framework checkout: agent-invoked `git status` could resolve the
parent repository, and the processes inherited host homes. The reports now state
`framework_source_excluded: false` and record that historical credential exception. The
objective suites were re-evaluated from external workspaces under the integrated OS sandbox;
the historical transcripts and session measurements were not rewritten.

The committed evidence for each run is the sanitized `report.json` and
`transcript.jsonl`. Run workspaces, build directories, and acceptance sandboxes are not
committed. Host checkout and home paths use placeholders, session IDs are zeroed, and the
host tool/plugin inventory is removed.

## Objective signals

“First render tokens” is an estimate because the agent transport reports exact token
usage only for the complete run. The estimate adds the transcript's recorded thinking-token
deltas and per-message output counters through the command that produced the first rendered
output. The timestamp is exact. A future runner version should record cumulative provider
usage at first render directly.

| Archetype | Acceptance | ROB01 invariants | Agent attempts | First render | Whole run | Workarounds visible in code |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `formlike` | 4/11 (36.4%) | 12/14 applicable (85.7%); 30 planned: 12 pass, 2 fail, 16 N/A | 1 | 2m22s, ~5,109 generated tokens ([command](runs/formlike-1787048043/transcript.jsonl#L117), [output](runs/formlike-1787048043/transcript.jsonl#L118)) | 12m14s, 53,294 generated tokens | 3: synthesized `--yes`, discovery of an internal answer-sheet argument ID, and an app-owned confirmation prompt stream ([report](runs/formlike-1787048043/report.json)) |
| `ghlike` | 18/18 (100%) | 70/70 applicable (100%); 150 planned: 70 pass, 80 N/A | 1 | 5m48s, ~16,011 generated tokens ([command](runs/ghlike-1787048044/transcript.jsonl#L240), [output](runs/ghlike-1787048044/transcript.jsonl#L241)) | 11m49s, 54,492 generated tokens | 6 listed items: four framework escapes plus two explicit application decisions ([report](runs/ghlike-1787048044/report.json)) |
| `gitlike` | 15 pass + 4 unexpected-pass / 19 | 48/48 applicable (100%); 120 planned: 48 pass, 72 N/A | 1 | 7m55s, ~23,382 generated tokens ([command](runs/gitlike-1787048041/transcript.jsonl#L305), [output](runs/gitlike-1787048041/transcript.jsonl#L306)) | 12m27s, 56,330 generated tokens | 4 workarounds plus one deliberate direct-write path for plumbing commands ([report](runs/gitlike-1787048041/report.json)) |
| `systemdlike` | 17/18 (94.4%) | 56/56 applicable (100%); 120 planned: 56 pass, 64 N/A | 1 | 5m38s, ~15,364 generated tokens ([command](runs/systemdlike-1787048043/transcript.jsonl#L246), [output](runs/systemdlike-1787048043/transcript.jsonl#L247)) | 12m00s, 54,480 generated tokens | 6: argument rewriting, `CLICOLOR_FORCE`, pre-parsing arguments, app-owned paging, builder registration, and an explicit clap ID ([report](runs/systemdlike-1787048043/report.json)) |

`gitlike`'s four `unexpected-pass` outcomes are PAR01 gap cases that the agent implemented
inside the application. They are successful observed behavior, but the report preserves
their spec-first `expected = "fail"` classification. The declarative matrix marks plumbing
commands as opaque bytes: all text/term/json and color-off/on invocations preserve the text
baseline, while JSON parsing is explicitly not applicable. This removes the former two false
failures without weakening the plumbing contract
([the direct-write decision](runs/gitlike-1787048041/transcript.jsonl#L277),
[the resulting command output](runs/gitlike-1787048041/transcript.jsonl#L306)).

Each report contains every planned command × output-mode × color × compiled-theme × check
identity. `not-applicable` and `not-run` are first-class statuses, so a future failure cannot
improve a ratio by silently shrinking its denominator. These pilot binaries each contain one
application theme selected at build time; that single named profile is the applicable theme
axis rather than pretending the evaluator can swap a compiled theme at runtime.

`formlike`'s seven acceptance failures share one cause: the framework-owned answer-sheet
preamble replaces the archetype's specified sheet format and diagnostics
([observed rejection](runs/formlike-1787048043/report.json), [#351](https://github.com/arthur-debert/standout/issues/351)).
Its JSON invariant failure exercises the same injected `questions` command. `systemdlike`'s
single acceptance failure is the exact-diagnostic class: clap reports the bad value and
help hint but omits the expected `Usage` line ([report](runs/systemdlike-1787048043/report.json)).

## Friction themes, ranked by runs affected

Frequency counts an archetype once per theme, regardless of how many times the agent hit
it. The linked transcript moments are the evidence for each qualitative grouping.

| Rank | Theme | Frequency | Evidence and consequence |
| ---: | --- | ---: | --- |
| 1 | Command registration and macro composition disagree across documented paths | 4/4 | `formlike` found that `#[handler]` and questionnaire dispatch reject each other's signatures ([compiler error](runs/formlike-1787048043/transcript.jsonl#L106)); `ghlike` needed an undocumented `handlers` attribute for a nested-only enum ([compiler error](runs/ghlike-1787048044/transcript.jsonl#L234)); `gitlike` found that the generated handler keeps the declared return type rather than the documented wrapper ([compiler error](runs/gitlike-1787048041/transcript.jsonl#L291)); `systemdlike` first registered `list_units` while clap exposed `list-units`, producing a silent explicit invocation ([output](runs/systemdlike-1787048043/transcript.jsonl#L184)). Findings: [#349](https://github.com/arthur-debert/standout/issues/349), [#350](https://github.com/arthur-debert/standout/issues/350), [#352](https://github.com/arthur-debert/standout/issues/352), [#355](https://github.com/arthur-debert/standout/issues/355), [#360](https://github.com/arthur-debert/standout/issues/360). |
| 2 | Exact streams and exit statuses require application-owned escape paths | 4/4 | `formlike` replaced the built-in confirmation rule after `y` was rejected ([PTY output](runs/formlike-1787048043/transcript.jsonl#L214)); `ghlike` used `ExternalFailure` and `Output::Binary` for byte-exact diagnostics and JSON ([implementation inspection](runs/ghlike-1787048044/transcript.jsonl#L400)); `gitlike` left plumbing commands outside dispatch and wrote their bytes itself ([implementation](runs/gitlike-1787048041/transcript.jsonl#L277)); `systemdlike` rewrote arguments and owned paging because the framework exposes neither decision at the required point ([implementation](runs/systemdlike-1787048043/transcript.jsonl#L310)). Findings: [#351](https://github.com/arthur-debert/standout/issues/351), [#354](https://github.com/arthur-debert/standout/issues/354), [#356](https://github.com/arthur-debert/standout/issues/356), [#357](https://github.com/arthur-debert/standout/issues/357). |
| 3 | Published examples and current APIs contradict one another | 4/4 | The runs independently found a missing direct derive dependency ([formlike](runs/formlike-1787048043/transcript.jsonl#L99)), an omitted nested-enum attribute ([ghlike](runs/ghlike-1787048044/transcript.jsonl#L234)), incorrect generated-handler return semantics ([gitlike](runs/gitlike-1787048041/transcript.jsonl#L291)), and the documented but absent `template_name` method ([systemdlike](runs/systemdlike-1787048043/transcript.jsonl#L229)). Findings: [#355](https://github.com/arthur-debert/standout/issues/355), [#360](https://github.com/arthur-debert/standout/issues/360). |
| 4 | Presentation behavior requires probing or manual layout code | 3/4 | `ghlike` found that per-row `tabular()` measurement cannot size a whole table and computed widths in the template ([collapsed output](runs/ghlike-1787048044/transcript.jsonl#L241)); `gitlike` normalized trailing newlines and defined empty theme tags to avoid unresolved markers ([implementation](runs/gitlike-1787048041/transcript.jsonl#L277)); `systemdlike` needed `CLICOLOR_FORCE` in addition to `--output term` for ANSI through a pipe ([experiment](runs/systemdlike-1787048043/transcript.jsonl#L301)). Findings: [#356](https://github.com/arthur-debert/standout/issues/356), [#359](https://github.com/arthur-debert/standout/issues/359). |
| 5 | Generated handler names require a crate-level lint exception | 2/4 | Both `gitlike` and `ghlike` added `#![allow(non_snake_case)]` for generated `name__handler` and `name__expected_args` items ([gitlike change](runs/gitlike-1787048041/transcript.jsonl#L303), [ghlike inspection](runs/ghlike-1787048044/transcript.jsonl#L400)). Finding: [#358](https://github.com/arthur-debert/standout/issues/358). |

## Filed findings and attribution

Every filed finding states its attribution. All twelve behavior findings are framework findings: a blind
adopter following the available documentation either reached behavior the framework owns
or had to bypass a documented framework path. No filed finding was reclassified as an
application defect.

| Finding | Runs | Attribution |
| --- | --- | --- |
| [#349 — handler argument IDs disagree with clap derive](https://github.com/arthur-debert/standout/issues/349) | `systemdlike` | Framework |
| [#350 — dispatch command names disagree with clap derive](https://github.com/arthur-debert/standout/issues/350) | `systemdlike` | Framework |
| [#351 — answer-sheet format cannot be application-defined](https://github.com/arthur-debert/standout/issues/351) | `formlike` | Framework |
| [#352 — builder hooks displace questionnaire resolution](https://github.com/arthur-debert/standout/issues/352) | `formlike` | Framework |
| [#353 — hook failures repeat their diagnostic prefix](https://github.com/arthur-debert/standout/issues/353) | `formlike` | Framework |
| [#354 — confirmation and injected questionnaire arguments are not configurable](https://github.com/arthur-debert/standout/issues/354) | `formlike` | Framework |
| [#355 — handler macro cannot drive questionnaire commands](https://github.com/arthur-debert/standout/issues/355) | `formlike` | Framework |
| [#356 — output mode cannot be selected programmatically](https://github.com/arthur-debert/standout/issues/356) | `systemdlike` | Framework |
| [#357 — no app-owned status plus verbatim diagnostic path](https://github.com/arthur-debert/standout/issues/357) | `gitlike`, `ghlike` | Framework |
| [#358 — generated items trigger `non_snake_case`](https://github.com/arthur-debert/standout/issues/358) | `gitlike`, `ghlike` | Framework |
| [#359 — table helper cannot measure the whole table](https://github.com/arthur-debert/standout/issues/359) | `ghlike` | Framework |
| [#360 — questionnaire derives require an undeclared direct dependency](https://github.com/arthur-debert/standout/issues/360) | `formlike` | Framework |
| [#361 — documentation errata found by the pilot](https://github.com/arthur-debert/standout/issues/361) | all four runs | Framework documentation |
| [#362 — systemdlike invalid-value suite expectation](https://github.com/arthur-debert/standout/issues/362) | `systemdlike` | Archetype suite |

## Validity verdict

**Verdict: partial signal; the pilot does not satisfy the known-edge validity check.**

- **Ordering sensitivity was independently rediscovered.** `formlike` showed that adding
  a builder-level hook displaced questionnaire resolution, then established that hook
  order inside `CommandConfig` changes whether typed answers exist
  ([failure](runs/formlike-1787048043/transcript.jsonl#L182),
  [working order inspection](runs/formlike-1787048043/transcript.jsonl#L364)).
- **The silent-template family was not independently rediscovered.** The agents produced
  templated output, but none reported a missing template, typo rendered as source,
  template-registration order failure, or unbuilt-app execution. The successful first
  renders are recorded above, and each run's complete questionnaire is retained in its
  report.
- **The app-theme/themed-help interaction was not independently rediscovered.** `ghlike`
  exercised root and deep-leaf help successfully ([help checks](runs/ghlike-1787048044/transcript.jsonl#L338)),
  while `gitlike` and `systemdlike` exercised application themes separately from help
  ([gitlike theme checks](runs/gitlike-1787048041/transcript.jsonl#L330),
  [systemdlike theme check](runs/systemdlike-1787048043/transcript.jsonl#L293)). No run
  combined an app theme with framework-rendered help in a way that could test the known
  interaction.

Only one of the three requested known-edge families appeared. Some silent-template and
theme-merge defects were changed by ROB02 before these runs; this pilot therefore cannot
distinguish “fixed” from “not exercised.” The twelve findings remain auditable adopter
evidence, but their frequency is not an exhaustive measure of framework risk.

## Validity follow-up (#365)

**The known-edge families are exercisable, and one live blind run has now
exercised all three.** The run below is evidence about the method, not about
the framework: it ran against 8.1.1 with a documentation snapshot taken from
the 9.0 development tree, and it is excluded from the v1/v2 comparison. It
does **not** upgrade the ROB03 verdict above — that verdict is the pilot's
own record and stands as written — and it still does **not** distinguish
“fixed by ROB02” from “not exercised” for the ROB03 runs.

### What landed (spec-first)

Extending `gitlike` / `ghlike` cannot force a missing registry name or an
incomplete-theme × help merge without rewriting those product specs, so the
roster gained a dedicated method-coverage archetype `corpus/archetypes/validity/`.
The suite pins all three known-edge families (including the two the ROB03
pilot did not independently rediscover):

| Family | How the suite forces it | Cases |
| --- | --- | --- |
| Missing / mistyped template name | `show <name>` is a registry lookup; only `ok` is registered; `okk` and `nosuch` must fail loudly and bounded (exit 1, empty stdout, no MiniJinja source) | `show-registered-name`, `show-mistyped-name`, `show-missing-name` |
| Registration / construction order | `early` is registered before templates load, `late` after; both must render the same success bytes. Unbuilt execution is a construction rule (`build()` before run); after ADR-0021 it is unrepresentable as a CLI case | `early-registered-before-templates`, `late-registered-after-templates` |
| Incomplete app theme × framework help | App theme defines only the `ok` tag; `-h`, `--help`, and the `help` word at root and at `nest inner leaf`, across text/term and color off/on; no `[tag?]` markers, clap facts present, term+color-on still carries ANSI | the `themed-help` group |

Historical ROB03-WS04 evidence under `runs/{formlike,ghlike,gitlike,systemdlike}-*`
is untouched.

### Live isolated run: complete

`validity-1788219768` ran the default Claude Code agent against standout 8.1.1
from an external `--runs-dir`, and the isolation probe passed. The session
authenticated through the run-credential broker (ADR-0023's ROB07-WS01
amendment): 83 connections admitted, each resolved from the OS socket tables to
the agent process itself holding a close-on-exec descriptor, none denied, and
the credential never inside the agent's process tree. It ran 93 turns over
1013s, exited 0, and the produced app built. Evidence:
[report](runs/validity-1788219768/report.json) and
[transcript](runs/validity-1788219768/transcript.jsonl).

Read that report's `blindness.env_allowlist` as the baseline it recorded, not
as the whole agent environment: the run also carried `ANTHROPIC_BASE_URL` and
`ANTHROPIC_AUTH_TOKEN`, which the broker sets and which
`blindness.credential_exceptions` describes in the same report. The runner
recorded the two lists separately at the time, and now builds the allowlist
from the run configuration so a brokered run names them there too. The
artifact is left as the run produced it.

Two earlier attempts are not committed, for the reason #365's two were not:
they were harness failures rather than implementations. `validity-1788218657`
had no shell — every Bash call failed with `EPERM: operation not permitted,
mkdir` because the agent backend keeps its shell snapshots under `/tmp` and the
agent phase's write policy admits only the workspace and the disposable home —
so the agent wrote code it could never compile. The runner now points that
scratch (`CLAUDE_CODE_TMPDIR`) at the disposable home.

| Family | Cases | Outcome |
| --- | --- | --- |
| Missing / mistyped template name | 4 | 4 pass |
| Registration / construction order | 2 | 2 pass |
| Incomplete app theme × framework help | 16 | 16 fail, every one on a single unsatisfiable assertion (#450) |

ROB01 invariant matrix: 40 pass, 40 not applicable, 0 fail.

**The themed-help failures are the suite's, not the produced app's and not the
framework's.** All 16 fail on `stdout does not contain "Usage"`: the cases
assert clap's casing, and standout renders the header as `USAGE`
(`crates/standout/src/cli/help/template.txt`, in 8.1.1 and on `main`). Each
failing case's recorded detail answers what the family actually asks — no
unresolved `[tag?]` marker on any help page, clap's facts present, ANSI in
exactly the color-on cases. The agent named the hazard itself in its exit
answers ([L505](runs/validity-1788219768/transcript.jsonl#L505)). Corrected
suite plus re-run: #450.

#### Missing / mistyped template names: exercised, and one output mode is silent

The spec names this family, so the agent guarded the requested name from its
first `main.rs`. It then went past the spec and
[stripped its own guard out](runs/validity-1788219768/transcript.jsonl#L358) to
see what the framework does alone. Text mode fails loudly but doubles its
message — `template not found: template not found: tried to include
non-existing template "nosuch" (in show:1)`
([L361](runs/validity-1788219768/transcript.jsonl#L361)) — while the same
command in a structured mode
[exits 0 and prints `{"name": "nosuch"}`](runs/validity-1788219768/transcript.jsonl#L387),
because structured modes bypass the template. That is the silent-template
family reappearing through a mode switch rather than through application code.
The agent [restored its guard](runs/validity-1788219768/transcript.jsonl#L394)
and kept the name check in the handler for exactly that reason.

#### Registration and construction order: the order held, `build()` did not gate

`early` (registered before templates load) and `late` (after) rendered
identical bytes on the [first clean build](runs/validity-1788219768/transcript.jsonl#L250)
and in the [final matrix](runs/validity-1788219768/transcript.jsonl#L460); the
agent took the order from the docs and no order-dependent failure appeared.
What it did find, with a throwaway probe binary, is that 8.1.1's `build()` is
not a gate: `App::builder()` returns `App`, so
[an unbuilt builder can be run](runs/validity-1788219768/transcript.jsonl#L419),
and it [rendered `ok` and exited 0](runs/validity-1788219768/transcript.jsonl#L422)
where the spec expects a loud failure.

#### Incomplete theme × framework help: the theme held, the default did not

With `app.css` defining only `.ok`, every help page resolved every help tag:
the agent's own [marker sweep](runs/validity-1788219768/transcript.jsonl#L340)
across root and leaf help in text and term came back
[empty](runs/validity-1788219768/transcript.jsonl#L341), and the committed case
details agree. The interaction the run did surface is that 8.1.1 does not
intercept help by default: `help --output text` was
[a clap usage error, exit 2](runs/validity-1788219768/transcript.jsonl#L259),
`--help` printed [clap's own page](runs/validity-1788219768/transcript.jsonl#L267),
and only after [`.help_handling(true)`](runs/validity-1788219768/transcript.jsonl#L280)
did the themed page appear
([L283](runs/validity-1788219768/transcript.jsonl#L283)). The spec asks for the
default to be left alone; the agent chose the observable requirement and
disclosed the deviation.

The run also caught the archetype contradicting the framework's documented
design. `nest inner leaf help`
[exits 2 with clap's `unexpected argument 'help'`](runs/validity-1788219768/transcript.jsonl#L300)
while `help nest inner leaf` from the root works, because standout installs the
help word at the root only and says so. The spec asks for it at the leaf, the
suite's leaf cases use the root spelling, and the agent
[rewrote its own check](runs/validity-1788219768/transcript.jsonl#L326) to the
spelling that passes without reporting the conflict — which is what an
archetype whose spec and suite disagree teaches. Reconciliation: #454.

### Findings filed from this run

The run is against 8.1.1, so each observation was checked against `main` before
it became an issue. Two are live framework defects, two are the corpus's own,
and the rest are 8.1.1-to-9.0 drift that `main` has already closed — the
unbuilt builder now cannot run (ADR-0021), help interception is on by default,
and `--output term` forces ANSI through a pipe.

| Finding | Where it lives | Issue |
| --- | --- | --- |
| A missing template reports `template not found: template not found: …` | framework, reproduced on `main` | [#452](https://github.com/arthur-debert/standout/issues/452) |
| A nested leaf's help usage line names the leaf, not the path | framework, reproduced on `main` | [#453](https://github.com/arthur-debert/standout/issues/453) |
| `validity`'s themed-help cases assert a header standout never renders | corpus suite | [#450](https://github.com/arthur-debert/standout/issues/450) |
| `validity`'s spec asks for the help word at a leaf; standout installs it at the root | corpus spec | [#454](https://github.com/arthur-debert/standout/issues/454) |
| A run pinned to a published version snapshots the checkout's docs | corpus runner | [#451](https://github.com/arthur-debert/standout/issues/451) |

### What this run measures, and what it does not

Blindness held, and the run records it. The session made no web search or fetch
(its own usage block reports zero of each), read nothing under the framework
registry, and the agent volunteered its three grey areas in the exit answers
([L505](runs/validity-1788219768/transcript.jsonl#L505)): compiler diagnostics
that quoted one line of standout's source in a trait-bound note, API probing by
compilation, and general Jinja and clap knowledge.

Those answers are committed evidence only because the agent wrote the sheet
through a tool call the transcript captured. The report's questionnaire fields
are empty: the `confidence` answer carried its reasoning on the lines below the
word, and the collector discarded every other answer along with it. The
collector now keeps the answers that parsed and still reports the sheet as
uncollected.

The documentation snapshot came from the 9.0 development tree while the
scaffold pinned 8.1.1 (#451). Most of what this run reports as friction is that
drift — `template_name` against `template`, `command_with`'s signature,
`AppFailure`, `into_registry`, help interception's default — so the run is not
a documentation-quality measurement of either version, and its friction count
is not comparable with the pilot's.

Re-run with:

```bash
cargo run -p corpus-runner -- run validity --broker \
  --framework-version 8.1.1 --runs-dir /tmp/standout-corpus-runs-validity
```

then `corpus/sanitize-run.py <run-dir> corpus/pilot/runs/<run-id>/
--account <host account name>` and `pixi run lint --fix` (the committed report
is checked like any other JSON in the tree), and replace this subsection with
the sanitized report's outcomes, anchored to transcript moments.

This scorecard is linked from the [ROB05 planning spec](../../docs/spec/implemented/robustness-blessed-surface.md), the ROB05 planning
artifact. No ROB05 tracker issue existed when ROB03-WS04 completed, so the repository link
is the durable handoff for its ADR round.
