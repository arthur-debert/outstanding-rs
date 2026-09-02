# Version the document and mark the surface with a trait

A structured document carries the version of its own shape as a `schema_version` key, and the type behind it says so through the `ContractSurface` trait. This records decisions D4 and D9 of `docs/spec/implemented/parity-machine-contract.md`, whose reasoning is not repeated here, and retires the hold of [ADR-0029](0029-hold-structured-help-back-in-glue.md): structured help was held back until it could be versioned, and it is versioned here.

## The marker (D4)

`ContractSurface` is a trait with `const SCHEMA_VERSION: u32`, derived by `#[derive(ContractSurface)]` with `#[contract(schema_version = N)]`. `Envelope<T: ContractSurface>` serializes as `{"schema_version": N, "data": <T>}` and `T::envelope(self)` constructs it; a handler that returns `Output::Render(view.envelope())` has stamped its document. The trait lives in `standout-dispatch` beside the diagnostic, the derive in `standout-macros`, and both reach an application through the `standout` crate root.

A framework-owned document — `ListViewResult`, the help document, the diagnostic — implements the trait too but carries `schema_version` as a top-level key beside its own fields, because the framework owns the whole shape and has nothing to wrap. The diagnostic's version, which ADR-0037 landed as a plain constant, now comes from the trait, so each document's version has one owner. Adding the key to `ListViewResult` is the one breaking change.

There is no `--format-version` flag: the key is in the document, and a key needs no parsing.

## The help document (D9)

Under `json` and `yaml`, `--help`, `-h` and the `help` word answer with a `HelpDocument` instead of the page: `schema_version`, `name`, `path`, `usage`, `about`, `args` (each `name`, `short`, `long`, `value_name`, `required`, `help`, `default`, `possible_values`) and `subcommands` (each `name`, `about`). `short` and `long` are the tokens as typed, dashes included; `name` is the clap id. `usage` and `path` name the full command path from the root, which is what fixes the human page's usage line for a nested leaf as well (#453): the model is built from the root tree after clap's `build()`, where a subcommand knows its parents, rather than from a subcommand plucked out of the tree.

`csv` has no help projection, so `--help --output csv` is a render error, emitted as the diagnostic of kind `render` that ADR-0037 gives every structured-mode failure. `xml` keeps the human page until D7 deletes the mode. Topic pages (`help topics`, `help <topic>`) are not part of the document and stay prose.

## The snapshot (D4)

`standout-test` gains `TestResult::assert_schema_snapshot(name)`, which reduces the stdout document to its key names and JSON value types and compares that against `tests/schemas/<name>` in the crate under test. A missing file is recorded and the assertion fails, so a first run never passes on nothing; `STANDOUT_UPDATE_SNAPSHOTS=1` accepts a changed schema. The convention is stated in `docs/topics/stability.md`.
