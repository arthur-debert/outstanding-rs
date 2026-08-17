# Delete the in-process TTY seam

Standout has no injectable TTY detector. `standout_render::detect_is_tty`, its
`set_tty_detector` override, and the `TestHarness::is_tty()` / `no_tty()` pair
that drove it are removed. Terminal-dependent behavior is evidence a real
process produces, and `TestHarness::run_process` is where a test goes to get
it. The color methods — `TestHarness::with_color()` / `no_color()` and
`detect_color_capability()` — are a different seam and stay: they resolve
`OutputMode::Auto`, they have production and downstream consumers, and nothing
here touches them.

The weak argument for removal is that the seam was dead: no production code in
the workspace ever called `detect_is_tty`, so the override could not change
what any run did. That argument alone would not justify the break, because the
seam had a named future consumer. The terminal-citizenship Spec
(`docs/spec/parity-terminal-citizenship.md`) wants a pager that is TTY-gated
always, a progress display that spins on a terminal and prints plain steps
when piped, a backend selected from resolved mode and TTY facts, and — line
127 — progress that is assertable in `TestHarness`, which is exactly a TTY
detector with a test override. Deleting a seam that an approved Spec plans to
use is churn unless something stronger is true.

Two things are. First, the shape is wrong for that consumer: `detect_is_tty`
answered for stdout alone (`Term::stdout().is_term()`), while terminal
citizenship needs stdout and stderr terminal facts independently — the pager
gates on one, progress writes to the other — and a single global cannot
answer both. Second, the seam already failed its first real customer in this
repo: `standout-render`'s warning renderer needed a stream-specific terminal
fact and called `console::Term::stderr().features()` directly, going around
the detector. Production code with precisely this need looked at the seam and
declined it. So terminal citizenship should design a stream-aware seam knowing
what it needs, rather than inherit a stdout-only global that has already been
refused once. The future consumer was known and weighed; this is not an
oversight to re-litigate, and re-adding a global `is_tty()` would reintroduce
the shape that failed.

The break is confined to `standout-render`. `detect_is_tty` and
`set_tty_detector` were never re-exported from the top-level `standout` crate,
so only direct `standout-render` dependents see the removal; the changelog
records it as breaking for that crate.

## What replaced the TTY axis in tests

Two things, because the seam was covering for two different needs.

For *ANSI-positive* assertions — the ones that made a TTY simulation look
necessary — the harness now opens both gates that stand between a styled
template and escape bytes. Standout's own color decision is only the first;
the second is `console`'s process-global color switch, read inside
`Style::apply_to`, which is off in a non-TTY process, and a test binary is
never a TTY. That is the `force_styling` trap the workstream was warned about,
stated precisely: `default_help_theme()` sets `force_styling` on none of its
styles, so forcing Standout's gate open left `console`'s gate shut and a
"TTY-simulated" help render emitted no ANSI at all. `with_color()` now sets
both and restores `console`'s switch on drop, so an unmodified theme — the
default help theme included — renders real escapes in-process. The worked
example is `crates/standout-test/tests/color_gates.rs`, which also pins the
negative case and the restoration. This is test-only; no production path sets
that switch. The alternative was to leave every ANSI-positive assertion to
`run_process`, and it was rejected because it would leave
`strip_ansi(Term) == Text` comparing a no-op against a no-op — an invariant
that passes on an implementation that never styles anything.

For the questions that genuinely need a terminal — or need to prove there
isn't one — `TestHarness::run_process` runs the compiled binary and returns
the two real pipes plus the process exit status. It refuses, loudly, any
harness setting a child cannot inherit (the detectors, stdin, clipboard,
prompts), because a `run_process` call that silently dropped
`terminal_width(80)` would read as a pinned terminal and in fact ask the CI
machine's.

## Consequence: environment conventions are pinned at the process boundary

Opening gate 2 forces a decision about *when* the harness reads `console`'s
switch, and the answer constrains what an in-process test can measure at all.

`console::colors_enabled()` initializes its process-global lazily, on the first
*read* in the process, from `CLICOLOR` / `CLICOLOR_FORCE`. The harness reads it
before applying `env_set` / `env_remove`, because reading after would take a
global that this run's own `.env("CLICOLOR_FORCE", "1")` had just latched, and
the value restored on drop would be the harness's own invention rather than the
real baseline.

That read happens on **every** run, not only on runs that override the switch.
The first read in a binary need not be the harness's: app code under test that
styles anything reads the same global, and it runs with the harness's temporary
environment installed — so a knobless run would latch `CLICOLOR_FORCE=1` from
an environment that exists for one run, with nothing recorded to put back. The
same holds for `console`'s other three color globals (stderr, and the two
true-color ones, which initialize from `COLORTERM` by way of `Term::features()`),
so all four are read up front. Only the stdout switch is ever *written*
(`with_color()` / `no_color()`), so it is the only one with a value to restore.

Reading early necessarily *forces* that lazy initialization under the
pre-test environment. That is the correct baseline, and it has a consequence
worth stating plainly: **a test's in-process `.env()` can no longer reach
`console`'s initialization.** By the time the run applies environment
variables, the harness has already latched the global; `console` will not read
`CLICOLOR_FORCE` again in that process.

So the environment conventions this epic wants pinned — `NO_COLOR`,
`TERM=dumb`, and the `CLICOLOR_FORCE` gap — **cannot be pinned in-process.**
An in-process test that sets `NO_COLOR` and asserts on suppression would be
measuring the harness's own gate-2 handling, not the framework's response to
the convention: it can pass on a build where the convention is entirely
unimplemented. That is a false negative of exactly the kind this Spec exists to
remove.

Those pins belong at the process boundary, through `TestHarness::run_process`.
A child process reads the real environment and performs its own lazy
initialization, so `NO_COLOR` and `TERM=dumb` are genuinely observable there
and nowhere else. This is a decision the environment-conventions workstream
(#315) inherits, not one for it to rediscover.

## What this does not affect

The epic's central invariant does not depend on any of this. The `[tag?]`
marker of #303 is a plain literal the parser emits under `TagTransform::Apply`
for an unknown tag; it never passes through `console`, so it appears with no
TTY, no color, and no `force_styling`. The underlying `UnknownTagError` is
recorded before the transform branch, so the diagnostics are identical under
`Apply`, `Keep`, and `Remove` — output-mode-independent. Unresolved-tag
invariants are therefore assertable structurally, whatever this ADR had
decided. A reader who finds the TTY axis gone should not conclude the test net
hinges on it.
