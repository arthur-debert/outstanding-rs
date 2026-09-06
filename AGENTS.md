<!-- Managed by shipit; do not edit. Regenerate via shipit install. -->
## Development workflow (managed by shipit)

<!-- shipit-managed; edit the surrounding AGENTS.md, not this block — `shipit install` regenerates it. -->

Every change ships as an agent-driven PR. The shipit **PR engine is authoritative**:
it reads where a PR stands and emits the **single next action**. Don't carry the policy
(reviewers, waits, breakers) in your head — run the tool and do what it returns.

**Planning a new feature/epic?** Run `/planning` first — it walks overview → Spec → ADRs → issues, checking in with you at the overview and the docs PR.

### Commands

```text
pixi run lint     # the commit/push checks — multi-language, hard fail, never skips (CI runs the same)
pixi run lint --fix  # the same gate, same pinned toolchain, formatting what it can
pixi run test     # the test suite (a commit/push check)
shipit pr status  # where the PR stands + the next action (read-only)
shipit pr next    # DO the next action, then report — the verb you loop on
shipit pr ready   # guarded flip draft→ready (refuses early); --undo reverts
```

PR number is optional (resolves the current branch's PR). Also: `shipit pr review
request`; setup/ops `shipit gh-setup` / `verify-apps` / `install` / `lint` / `logs`.
To orient on what a session or epic already did, read the dev-cycle event log:
`shipit logs --flow --session current` (this session's story) or
`shipit logs --flow --epic <CODE>` — the same view `/shipit-session-status` wraps.

### The cycle: draft → address reviews → checks passing + mergeable → flip to ready

Open every change as a **DRAFT** PR. Loop `shipit pr next` — do the one thing it returns
(request a review, address threads, wait for CI) — until it reports **READY** and flips
draft→ready. **Stop at the flip**: the human verifies + merges; never auto-merge. A human
"changes needed" returns it to draft (`shipit pr ready --undo`); re-loop.

**Floor / ceiling:** committing, pushing, and opening the draft need no go-ahead; the
**only** step needing a human is the merge.

**Large work (epics):** the same cycle runs per workstream, but each workstream PR targets
the **epic branch**, not `main`. Subagents drive their WS PR to READY; the **coordinator
merges each READY WS PR into the epic branch** on its own authority — no human checkpoint for
intra-epic merges. The human checkpoint is the **umbrella PR** (epic branch → `main`), which the
coordinator shepherds to READY, then stops for the human to merge.

### Roles — always delegated, split so no one context carries the whole cycle

- **Coordinator** (the agent the human addresses): never implements. Delegates the work;
  owns every wait (`shipit pr wait`) and the flip; spawns ONE shepherd per PR and resumes
  it per round; in an epic, merges READY workstream PRs into the epic branch.
- **Implementer** (subagent): implements + tests, gets the tests green (`pixi run test`;
  the commit/push hooks run the lint suite), opens the DRAFT PR with a `## Context` handoff
  note (why this approach, what's out of scope), then **stops at PR-open** — never handles
  a review round.
- **Shepherd** (subagent, ONE per PR — parked between rounds, resumed with a one-line
  brief per round): triages open threads — the local agent has the final word, so
  fix-or-pushback and resolve each — sweeps the PR diff for other instances of each
  finding's class, pushes the round's commits at once, hands back and parks.

### Naming & references

Codes are **assigned by the human**, never invented mid-stream. Implementers use them in:

- **PR title** — epic work: `<identifier>: Epic: <Epic Name> - Workstream: <WS Name>`
  (e.g. `APP-GPU02-WS03: …`); a standalone PR: a plain summary.
- **Commit messages** — reference the GitHub issue (`#123`).
- **PR body** — `closes #123` (auto-closes the issue on merge to `main`) or `for #123`
  when it must not auto-close (e.g. a workstream PR landing on an epic branch).
<!-- End shipit-managed block. -->

### Comments and rustdoc: one owner, near zero volume

Code comments and rustdoc are kept deliberately sparse. The default for any new
`//`, `///`, or `//!` line is: don't write it.

- A **complex module** may carry ONE `//!` doc at the top of the file (≈70 lines
  max): what it is for, its model, the non-obvious tradeoffs. That doc is the
  single owner of that information; other places link to it, never restate it.
- A crate's `lib.rs` may carry a short `//!` saying what the crate is.
- A single-line `//` is fine for a genuinely surprising line (a workaround, an
  invisible invariant). Anything that narrates what the next lines do is noise.
- No history, dates, ticket numbers, "we decided", chain-of-thought, or
  task-execution notes in source. That belongs in the PR, the ADR, or `docs/`.

**Some `///` is code, not documentation.** A `///` on a `#[derive(Questionnaire)]`
field is the question prompt the derive reads (`doc_prompt` in `standout-macros`),
and a `///` in a clap derive tree is the help text a user sees. Deleting those
changes what the program says, and no test that does not assert on the prompt will
notice. Read what a `///` feeds before removing it.

Reviewers: a module doc that omits a permutation is **not** a finding; module docs
are orienting, not exhaustive. "Add a comment explaining X" is not a finding
unless X is a surprising line per the rule above. Shepherds: do not answer review
threads by adding comments.

This binds whoever is directing the work too. Asking an agent what facts a doc
should contain is the same request a reviewer makes when it files a documentation
finding, and it produces the same result: more prose, which draws the next
finding. Brief the cap and the ownership rule, then let the author choose the
content.

### Review rounds: code defects only

A review round is justified by a code defect and nothing else — not style, not
thoroughness, not a reviewer's preference. When no code defect remains, flip the
PR. Before briefing a round, read each thread's `severity=` and ask whether a user
would hit it; if not, it is one reply and a resolve.

Not actionable, ever: a finding about a comment, docstring, module doc or
documentation completeness; anything under `docs/**`; changelog wording. A
fragment over the word budget fails CI, and that is the only changelog feedback
that counts. These rules override any role prompt that says to update a docstring
in the same diff.

Reviewers are often right about the finding and wrong about the remedy. "Update
the docs to match" — decline the remedy, keep the finding: a broken contract
belongs in the CHANGELOG or an ADR. A stale ADR is a false record rather than
documentation lag, so revise it in the same PR.

One exception runs the other way. When a finding is marked `severity=major` but
the engine's breaker reports `no-major-finding`, the breaker's premise is false;
force the re-review with `shipit pr review request N --reviewer X`. A `round-cap`
breaker is different — it is cost control, so honour it and move the remainder to
its own issue.

Measure rather than exhort: `rustloc diff --type code,tests,docs,comments,total
main..<branch>` before and after. Numbers change behaviour; words about brevity
add words.

### Before you instruct someone, verify

- `gh api .../pulls/N/comments` does not report resolved state. Use the GraphQL
  `reviewThreads` filtered on `isResolved == false`, or let the shepherd read the
  threads itself.
- Re-measure before claiming a file lacks something; a push may be in flight.
- `gh run rerun --failed` reuses the same merge commit, so it will not pick up a
  fix that landed on `main`. Merge `main` into the branch instead.
- `pixi run test` without `--manifest-path <worktree>/pixi.toml` can execute
  against a different worktree and report a result that is not yours.
