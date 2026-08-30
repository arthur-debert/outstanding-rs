# State which surfaces are contract

The repository has no stability statement, so "is this change breaking?" has had no written answer and every reviewer has answered it from memory. The statement below is that answer. It lives at `docs/topics/stability.md`, mounted in `docs/SUMMARY.md` under Framework Topics and linked from the README; the doc-truth workstream writes the page, this ADR fixes what it must say.

Contract does not mean permanent. The repository's policy is no backwards compatibility and no adapters; its users are the maintainer and downstreams who port. What contract buys is a rule about *cost*: changing a contract surface is a breaking change, so it needs a major version and a line in the migration notes, and it cannot ride along inside a refactor. Changing an internal surface needs neither, and a PR that stalls arguing about which one it touched is the failure this statement exists to prevent.

**Contract.** Five things, and nothing else:

1. **The blessed idioms of ADR-0032** — the blessed item on each axis and each surviving secondary path, by name, signature and meaning. A path that ADR-0032 kept with a stated reason is contract for exactly the capability that reason names.
2. **The structural shape of each `--output` mode's bytes.** For the structured modes (`json`, `yaml`, `csv`, `xml`), the document a handler's data produces — its field names and its nesting — is contract, and a change to it is a change to what the consuming script parses. For the human modes (`text`, `term`), the *bytes are not*: themes, wording, column widths and layout may change in any release. What is contract in the human modes is the pair of properties a script can rely on without reading words — `text` carries no ANSI and `term` does, and stdout carries data while stderr carries diagnostics and warnings.
3. **Exit statuses.** Zero means success; each documented nonzero status keeps its documented meaning. The adopter-seams epic's app-owned status (#357) is the application's to choose, and the framework emitting it verbatim is the contract.
4. **The two name mappings a user types on the command line** — `#[handler]`'s parameter name to clap id (#349), and `#[derive(Dispatch)]`'s variant name to command name (#350). These are contract because they are not source-level at all: they decide the words in a shell script, and a change to either breaks callers who never recompile.
5. **Re-export from the `standout` crate root.** An item's *availability* through `standout` is contract; its *location* is not. A type may move between leaf crates in any release as long as the root re-export still names it, and a leaf crate's own API is contract only where `standout` re-exports it. That rule is what makes the single-dependency claim (#360) true and keeps crate reorganization from being a breaking change on its own.

**Internal.** Everything else, explicitly including: any path ADR-0032 deleted; the internals that lose their `pub` in the visibility sweep; module paths within a crate; rendered help layout and the wording of diagnostics and warnings; the framework's own template and style names beyond the fact that `include_framework_templates(false)` and `include_framework_styles(false)` decline them; and the leaf crates' APIs where `standout` does not re-export them.

Two boundaries are worth stating because they will be asked about. The machine-readable schema — a versioned envelope that a consumer can validate against — is the parity program's, not this epic's; what starts here is that structured output *has* a contract shape at all, not that the shape is published as a schema. And `standout-test` is contract as an assertion API for the same reason the blessed idioms are: a downstream's suite depends on it. Its snapshot *contents* are not; a snapshot is evidence, and evidence changing is the point of a snapshot.

## Alternatives rejected

**Declaring the whole public API contract.** It is the honest reading of `pub` and it is what the repository behaves as if it had, which is why the census could reach roughly 99 root re-exports including `rgb_to_ansi256` and `walk_dir`. Under that rule every visibility sweep is a breaking change and the statement stops being usable the first time it is inconvenient.

**Declaring nothing contract and relying on the no-compatibility policy.** The policy is about *source* compatibility for the maintainer's own downstreams, who recompile. It says nothing about the shell script parsing `--output json`, which does not recompile and cannot be ported by the person who broke it. Items 2, 3 and 4 exist because the people they protect are not in this repository.

**Pinning the human-mode bytes.** It would make the ROB01 snapshot matrix the contract, and it is the wrong contract to sign: a framework whose distinguishing feature is themed output cannot promise its output does not change. The snapshots stay what they are — the record that a change was seen and intended, not a promise it will not happen.

## Consequences

The reviewer question has a written answer, and so does the changelog fragment's: a fragment describes a breaking change when it touched one of the five, and the consolidated migration notes carry every such line for this epic's major version.

Two of the five items are claims about behavior that no test asserts today, and stating them is what makes them testable: that `text` never carries ANSI and `term` always does, and that data goes to stdout while diagnostics go to stderr. The ROB01 matrix and `standout-test`'s stream accessors already have the machinery; that they now state a contract rather than record a behavior is the change.

The statement is also a boundary on this epic's own pruning. An item ADR-0032 kept with a stated reason cannot be quietly dropped later as "internal, nobody used it"; dropping it is a major version, which is the friction the map was drawn to create.
