# What Is Contract, and What Is Internal

This page answers one question: **is this change breaking?**

Contract does not mean permanent. This repository does not preserve backwards
compatibility and ships no adapters; its users are the maintainer and
downstreams who port. What contract buys is a rule about *cost*. Changing a
contract surface is a breaking change: it takes a major version and a line in
the migration notes, and it cannot ride along inside a refactor. Changing an
internal surface takes neither.

## Contract

Six things, and nothing else.

### 1. The blessed idioms

Each axis of wiring an application — registration, adaptation, declaration,
template provision, theme provision, entry points — has exactly one blessed
item, and a small number of secondary paths that survive because they name a
capability nothing else on the same axis covers. All of them are contract, by
name, signature and meaning. A secondary path is contract for exactly the
capability its stated reason names.

The blessed set:

```rust,ignore
#[derive(Parser)]              // declaration: clap-derive
struct Cli { /* … */ }

#[derive(Subcommand, Dispatch)]   // registration
#[dispatch(handlers = handlers)]
enum Commands { /* … */ }

#[handler]                     // adaptation
fn list(#[flag] all: bool, #[ctx] ctx: &CommandContext) -> Result<Output<Listing>, anyhow::Error> {
    /* … */
}

App::builder()
    .templates(embed_templates!("src/templates"))   // template provision
    .styles(embed_styles!("src/styles"))            // theme provision
    .default_theme("myapp")
    .commands(Commands::dispatch_config())?         // registration
    .build()?
    .run(Cli::command(), std::env::args());         // entry point
```

Dropping a secondary path later is itself a major version. That friction is
deliberate: an item kept with a stated reason cannot be quietly removed as
"internal, nobody used it".

### 2. The structural shape of each `--output` mode's bytes

`--output` accepts `auto`, `term`, `text`, `term-debug`, `json`, `yaml`, `xml`
and `csv`. All eight are classified here.

**Structured modes** (`json`, `yaml`, `csv`, `xml`): the document a handler's
data produces — its field names and its nesting — is contract. Changing it
changes what a consuming script parses.

**Human modes** (`text`, `term`): the bytes are **not** contract. Themes,
wording, column widths and layout may change in any release. What is contract
is the pair of properties a script can rely on without reading words:

- *The style transformation.* `text` removes Standout's style tags and adds no
  ANSI of its own. `term` turns every resolved style tag into ANSI. Neither
  half reaches ANSI that a handler or a template writes literally — the
  framework does not sanitize those bytes and does not promise to, so a caller
  who needs them gone strips them itself.
- *The split between the streams.* Data goes to stdout; diagnostics and
  warnings go to stderr.

**`auto`** is contract as a *resolution rule* rather than as bytes: it resolves
to `term` when the destination reports color capability and to `text` when it
does not, and a `term` request under a never-color policy resolves to `text`.
What a caller may rely on is which of the two modes it lands in, and then that
mode's own contract.

**`term-debug`** is **internal**. It prints style tags unresolved, as evidence
for the framework's own snapshots; both that tag vocabulary and its spelling
may change in any release.

One more byte-level rule belongs here because a script can see it: the render
pipeline consumes the template's final newline and the process edge appends
exactly one. See [the trailing-newline
contract](../crates/render/topics/templating.md#the-trailing-newline-contract).

### 3. Exit statuses

Zero means success, and each documented nonzero status keeps its documented
meaning. An application-owned status is the application's to choose, and the
framework emitting it verbatim is the contract.

### 4. The two name mappings a user types on the command line

- `#[handler]`'s parameter name to clap argument id — underscores become
  hyphens, so `no_legend` reads the argument id `no-legend`.
- `#[derive(Dispatch)]`'s variant name to command name — kebab-case, so
  `ListUnits` registers `list-units`, and `#[dispatch(name = "…")]` renames one
  variant.

These are contract because they are not source-level at all. They decide the
words in a shell script, and a change to either breaks callers who never
recompile. Both rules are stated in full in the
[`#[dispatch(…)]` and `#[handler]` reference](dispatch-attributes.md).

### 5. Re-export from the `standout` crate root

An item's *availability* through `standout` is contract; its *location* is not.
A type may move between leaf crates in any release as long as the root
re-export still names it, and a leaf crate's own API is contract only where
`standout` re-exports it. That rule is what makes one `standout` dependency
enough, and what keeps reorganizing the crates from being a breaking change on
its own.

### 6. `standout-test`'s assertion API

A downstream's test suite depends on it, so a rename there breaks a build that
never touched the framework. Contract by name, signature and meaning:

- `TestHarness`, with its injection methods and its run forms — `run` and
  `run_process` on every supported platform, `run_pty` on Unix only, where the
  pseudo-terminal it opens exists.
- `TestResult`, which an in-process `run` returns: its `outcome`, its exit
  status and success and error kinds, its raw and plain streams and `binary`,
  its style-tag resolutions, its `warnings`, its artifact accessors and its
  `assert_*` methods.
- `ProcessResult`, which a spawned `run_process` or `run_pty` returns: its
  process status, code and success, its raw, plain and byte streams, its
  tempdir and its `assert_*` methods.
- `assert_page_snapshot!` with `SnapshotCase`, and `matrix` with `MatrixCell`.
- The `clap_parity` and `invariants` modules.

An item behind a `cfg` is contract on the platforms it compiles for, so
narrowing its `cfg` is a breaking change on the platforms it leaves. A
snapshot's *contents* are not contract — a snapshot is evidence, and evidence
changing is the point of a snapshot. The `serial` re-export is `serial_test`'s
API, not this repository's.

## Internal

Everything else, explicitly including:

- any path the blessing deleted;
- the internals that lost their `pub` in the visibility sweep;
- module paths within a crate;
- rendered help layout, and the wording of diagnostics and warnings;
- the framework's own template and style names, beyond the fact that
  `include_framework_templates(false)` and `include_framework_styles(false)`
  decline them;
- the leaf crates' APIs where `standout` does not re-export them.

## One boundary worth naming

A machine-readable schema — a versioned envelope a consumer can validate
against — is not part of this statement. What this statement establishes is
that structured output *has* a contract shape at all, not that the shape is
published as a schema.

## Where this comes from

[ADR-0033](../adr/0033-state-which-surfaces-are-contract.md) decides what this
page says, including the alternatives that were rejected:
declaring the whole public API contract, declaring nothing contract, and
declaring the human-mode bytes contract.
[ADR-0032](../adr/0032-bless-one-item-per-axis-behind-a-capability-map.md)
carries the blessed set and the capability map behind item 1.
