# ROB03 corpus pilot scorecard

This scorecard records four blind implementations against Standout 8.1.1 and the
documentation snapshot at `c9d7198c173a986756876994431a3174366bdef6`. Each agent
received an archetype specification, the published documentation snapshot, and crates.io
dependencies. The runner excluded the Standout source tree.

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
| `formlike` | 4/11 (36.4%) | 6/7 (85.7%) | 1 | 2m22s, ~5,109 generated tokens ([command](runs/formlike-1787048043/transcript.jsonl#L117), [output](runs/formlike-1787048043/transcript.jsonl#L118)) | 12m14s, 53,294 generated tokens | 3: synthesized `--yes`, discovery of an internal answer-sheet argument ID, and an app-owned confirmation prompt stream ([report](runs/formlike-1787048043/report.json)) |
| `ghlike` | 18/18 (100%) | 35/35 (100%) | 1 | 5m48s, ~16,011 generated tokens ([command](runs/ghlike-1787048044/transcript.jsonl#L240), [output](runs/ghlike-1787048044/transcript.jsonl#L241)) | 11m49s, 54,492 generated tokens | 6 listed items: four framework escapes plus two explicit application decisions ([report](runs/ghlike-1787048044/report.json)) |
| `gitlike` | 15 pass + 4 unexpected-pass / 19 | 26/28 (92.9%) | 1 | 7m55s, ~23,382 generated tokens ([command](runs/gitlike-1787048041/transcript.jsonl#L305), [output](runs/gitlike-1787048041/transcript.jsonl#L306)) | 12m27s, 56,330 generated tokens | 4 workarounds plus one deliberate direct-write path for plumbing commands ([report](runs/gitlike-1787048041/report.json)) |
| `systemdlike` | 17/18 (94.4%) | 28/28 (100%) | 1 | 5m38s, ~15,364 generated tokens ([command](runs/systemdlike-1787048043/transcript.jsonl#L246), [output](runs/systemdlike-1787048043/transcript.jsonl#L247)) | 12m00s, 54,480 generated tokens | 6: argument rewriting, `CLICOLOR_FORCE`, pre-parsing arguments, app-owned paging, builder registration, and an explicit clap ID ([report](runs/systemdlike-1787048043/report.json)) |

`gitlike`'s four `unexpected-pass` outcomes are PAR01 gap cases that the agent implemented
inside the application. They are successful observed behavior, but the report preserves
their spec-first `expected = "fail"` classification. Its two invariant failures apply the
JSON-parse check to plumbing commands whose archetype contract requires identical plain
bytes under every output mode; they are an invariant-applicability mismatch, not an app
failure ([the direct-write decision](runs/gitlike-1787048041/transcript.jsonl#L277),
[the resulting command output](runs/gitlike-1787048041/transcript.jsonl#L306)).

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

Every filed finding states its attribution. All twelve are framework findings: a blind
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
evidence, but their frequency is not an exhaustive measure of framework risk. Before the
method is used as a regression detector, add one scenario that combines an app theme with
framework-rendered help and one that deliberately varies template registration order and
name resolution, then rerun the pilot.

This scorecard is linked from `docs/spec/robustness-blessed-surface.md`, the ROB05 planning
artifact. No ROB05 tracker issue existed when ROB03-WS04 completed, so the repository link
is the durable handoff for its ADR round.
