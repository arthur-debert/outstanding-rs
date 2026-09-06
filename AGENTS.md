# Development

Run `scripts/install-quality-tools` once to install the repository's tool versions.
Run `scripts/quality` before committing; Lefthook and GitHub Actions run the same
command. Use native Git and GitHub CLI for branches and pull requests.

Keep changes in isolated worktrees. Open draft pull requests with a short
`## Context` section describing the change and its verification. For a feature
branch, target that branch and integrate reviewed changes there. A human approves
merges into `main` unless they explicitly authorize the operation.

Review behavioral defects: wrong results, lost errors, unsafe file or process
handling, races, and violations of documented behavior. Formatting belongs to the
configured linters. Keep reviews and implementation separate when delegating work.

## Comments and rustdoc: one owner, near zero volume

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
