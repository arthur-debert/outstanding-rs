# Parity: Terminal Citizenship

Third Spec of the **Capability-parity program**. Depends on composition contracts (color,
pager, progress, and verbosity all read the target-facts/`ResolvedConfig` seams) and on
config layering (their settings ride the config ladder). Quiet/verbose was part of
standout's original goal and was missed; this Spec restores it alongside the terminal
conventions the survey found standout violating.

## Context

The eleven-tool survey measured standout against the conventions serious CLIs share for
being a good terminal citizen, and found one family of violations and two absences:

**Color.** The color decision is conflated into `--output`
(`auto|term|text|term-debug|json|yaml|xml|csv`): "human text with color forced into a
pipe" and "colorized JSON" are inexpressible, and there is no environment force-path.
Every surveyed tool with both axes keeps them separate (`--color=auto|always|never` in
git/cargo/jj plus force env vars `CLICOLOR_FORCE`/`GH_FORCE_TTY`/`HOMEBREW_COLOR`).
Worse, standout's compliance with the conventions it *does* meet is accidental: the
detector calls `console`'s `colors_supported()`, which happens to check `NO_COLOR` and
`TERM=dumb` but not `CLICOLOR`/`CLICOLOR_FORCE` (those live in a different `console`
API) — untested, undocumented, and one dependency upgrade from silently changing
(`crates/standout-render/src/environment.rs:150-152`; pinned by the test-net epic).

**Pager.** Paging exists only for help/topics behind `--page`, spawning a bare `less`:
no `LESS=FRX` discipline (git sets it so short output doesn't trap the user and ANSI
passes through), no `--no-pager` escape, no pager for normal command output (every CLI
with a `log`/`list`/`show` shape needs one), no secure-pager posture for elevated tools
(systemd's `SYSTEMD_PAGERSECURE` exists for a reason).

**Progress.** Absent entirely. Adopters bolt on `indicatif`, which knows nothing about
standout's output modes, writer seams, or `TestHarness` — so progress output corrupts
`--output json` streams and pollutes captured test output. Nine of eleven surveyed tools
have progress with piped-suppression; it is a capability only the framework can get
right, because correct suppression requires knowing the resolved output mode.

**Verbosity.** No quiet/verbose levels (11 of 11 surveyed tools have them). Standout
owns a warning channel (`flush_to_stderr`) that verbosity should govern; today warnings
are all-or-nothing and info-level narration has no home.

## Problem

A standout app is a worse terminal citizen than any of the CLIs its users already use:
they cannot force color in CI, cannot suppress or select a pager, get no progress on
long operations (or get adopter-bolted progress that breaks machine output), and cannot
say `-q` or `-v`. Each gap is individually small; together they mark the framework as
not-serious to exactly the adopters the corpus simulates — and each interacts with
output modes, which is why adopter-side fixes are reliably wrong.

## Goals

- **Color is its own axis**: a `--color auto|always|never` global (name/shape grilled
  against the blessed-surface conventions), composing with every output mode where
  color is meaningful; the env ladder honored deliberately (`NO_COLOR`, `CLICOLOR`,
  `CLICOLOR_FORCE`, an app-specific override var, `TERM=dumb`) with documented
  precedence (flag > app var > generic vars), all feeding the target-facts seam — one
  decision point, tested, no longer an accident of `console`'s API surface.
- **A real pager story**: opt-in paging for command output (per-command or by
  heuristic — grilled), `--no-pager`, the chain app-var → `PAGER` → `less` with
  `LESS=FRX` applied when unset, TTY-gated always, a secure-mode switch for elevated
  contexts, and config-ladder settings for all of it.
- **A framework progress seam**: an API handlers use for progress/spinner/step
  reporting that renders on the terminal path, degrades to plain sequential lines when
  piped, is *silent or structured* under machine modes (feeding the machine-contract
  diagnostic/event stream where present), and is capturable/assertable in
  `TestHarness`.
- **Verbosity levels**: `-q/-v` (count or leveled — grilled) governing the warning
  channel and a new info-level channel, with defined interaction per output mode
  (machine modes: verbosity governs the diagnostic stream's detail, never breaks the
  document).
- Corpus archetypes exercising these conventions (`systemdlike` color/pager env
  discipline, `gitlike` pager behavior, `pnpmlike` reporter/quiet matrix) pass — and
  `tflike`'s **full** acceptance suite closes here: the machine-contract epic gates its
  diagnostic milestone, while the progress/apply-lifecycle events that complete it are
  this epic's progress seam.

## Non-Goals

- A TUI or interactive rendering layer (progress is linear output, not screen
  management).
- Changing the theme/styling system (color axis decides *whether*, themes decide
  *what*).
- Log-file/tracing integration (verbosity is user-facing channel control; structured
  logging remains app territory).
- Building these before their seams exist: this epic does not start until composition
  contracts and config layering land.

## Proposed Shape

Four features, one pattern: each is a *decision* resolved once at the target-facts/
`ResolvedConfig` layer (from flag > env > config > detection), consumed by the single
rendering pipeline, controllable in the harness, and covered by matrix tests.

**1. Color axis**: the resolved tri-state + force ladder lands in target facts;
`--output` loses its color connotations (`term`/`text` become presentation-format
choices whose relationship to the color axis the grill defines carefully — this is the
one genuinely tricky compatibility surface); the accidental-compliance findings convert
into deliberate, tested behavior.

**2. Pager**: a pager decision in resolved config (enabled-for, command, secure mode),
applied at the output-emission point after rendering; help/topics `--page` rebased onto
it.

**3. Progress**: a handler-facing reporter handle (through `CommandContext`) whose
backend is selected by resolved mode/TTY facts: rich terminal, plain lines, silent, or
machine events; `TestHarness` captures emitted progress as data.

**4. Verbosity**: level in resolved config; warning channel and info channel filter by
it; machine modes map levels onto diagnostic detail.

## User / Agent Stories

1. As a CI user, I want to force color through a pipe with a flag or env var, so that my
   build logs are readable without pretending to be a TTY.
2. As an app user, I want `NO_COLOR` and `--color=never` to fully win, so that my
   accessibility choice is respected everywhere including help.
3. As an app user of a `log`-shaped command, I want paged output that doesn't trap me on
   short content and a `--no-pager` escape, so that the app behaves like git.
4. As an application author with a slow command, I want a progress API that renders a
   spinner on a TTY, prints plain steps when piped, and stays out of `--output json`,
   so that I never corrupt machine output with progress bars.
5. As an app user, I want `-q` to silence warnings and `-v` to show more, so that the
   universal CLI contract holds.
6. As a test author, I want progress and verbosity assertable in `TestHarness`, so that
   "this command reports three steps and warns once at default verbosity" is a test.
7. As an adopter shipping a sudo-invoked tool, I want a secure-pager mode, so that my
   elevated tool never hands control to an attacker-controlled `$PAGER`.

## Risks And Rabbit Holes

- **The `--output term/text` untangling.** Separating color from format touches the
  most-used flag in the framework; the grill must produce an explicit mapping table
  (old spelling → new axes) and the matrix snapshots arbitrate. This is the epic's
  highest-blast-radius decision — do it first, not last.
- **Progress API scope.** Handlers need "report step/spinner/percent" — not a widget
  toolkit. Resist configurable bar styles until a corpus app asks; the seam and the
  degradation policy are the product.
- **Pager heuristics.** Auto-paging (systemctl-style) is a UX opinion with sharp edges
  in tests and scripts; default to opt-in per command, let config flip defaults, and
  keep TTY-gating absolute.
- **Verbosity semantics sprawl.** Levels govern *channels* (warnings, info, diagnostic
  detail), never *content* of primary output; the moment `-v` changes a template's
  rendering, verbosity has become a second theming system.

## Cross-Cutting Concerns

- All four settings join the config ladder (config-layering Spec) and the target-facts
  seam (composition contracts) — this epic adds no new resolution mechanisms.
- Machine contract: progress events and verbosity map into the diagnostic/event model
  where structured modes are active.
- Security: pager/editor execution from config honored per the scope policy set in the
  config-layering grill.
- Docs: a "terminal behavior" topic documenting the full env/flag ladder (the survey's
  per-tool tables are the model); the corpus archetypes double as executable docs.

## Testing / Verification

Matrix tests over (color axis × mode × TTY) with the env ladder as table tests; pager
decision unit-tested with process-level spot checks (`run_process`) for the `LESS`/
short-content behavior; progress backend selection tested per mode; verbosity table
tests over channels. Corpus `systemdlike` env-discipline tests (tool var beats generic
var beats detection; flag beats all) are the external oracle.

## Workstream Hints

(1) Color axis + env ladder + `--output` untangling (walking skeleton and the risk
concentrator — first); (2) verbosity + channels; (3) pager; (4) progress seam +
harness capture. (2)–(4) parallelize after (1).

## Out Of Scope

TUI features, theming changes, logging frameworks, progress styling options.

## Further Notes

Survey evidence: gh/git/jj/cargo color and pager chains, systemd's env discipline and
`PAGERSECURE`, pnpm's reporter matrix (session record 2026-08-16); accidental-compliance
finding pinned by the test-net epic. Expected ADRs: the color/format axis mapping; the
pager decision model; the progress seam shape; verbosity semantics. Links to be added by
the grill.
