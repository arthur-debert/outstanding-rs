# Project map and API gotchas

## Ownership map

| Location | Responsibility |
| --- | --- |
| `crates/standout` | Framework facade, app builder, clap integration, help/topics |
| `crates/standout-dispatch` | Handler contract, context/state, hooks, dispatch primitives |
| `crates/standout-render` | MiniJinja, styles/themes, output modes, tabular rendering |
| `crates/standout-input` | Input chains, sources, readers, interactive responders |
| `crates/standout-pipe` | Post-output command and clipboard piping |
| `crates/standout-macros` | Handler, dispatch, embedding, tabular, and seeker macros |
| `crates/standout-bbparser` | Semantic style-tag parsing and transformation |
| `crates/standout-seeker` | In-memory typed filtering, ordering, and query parsing |
| `crates/standout-test` | In-process application test harness |
| `crates/todo-example` | Worked framework application and integration tests |

Start broad framework changes at `crates/todo-example/README.md` and `src/lib.rs`. Use `docs/SUMMARY.md` to locate guides and topics. Follow a claim into the owning crate's public types and integration tests before copying it.

## Drift checks

Common copied examples can target older APIs. Confirm these current contracts:

- `App::run(...) -> bool`; it does not return `Option<ArgMatches>`. Use `run_to_string` and `RunResult::NoMatch` when fallback needs matches.
- `CommandContext` has `command_path`, `app_state`, and `extensions`; no `output_mode` field.
- Binary handler output is `Output::Binary { data, filename }`, not a tuple variant.
- `#[handler]` preserves the typed function and generates `name__handler`; wire the wrapper and unit-test the original.
- `#[derive(Dispatch)]` maps to `handlers::name`; add `#[dispatch(pure)]` for a `#[handler]`-generated wrapper.
- `TestHarness` mutations are process-global; every harness test is serial.
- Structured output bypasses templates, so template fixes cannot change JSON/YAML/XML/CSV.
- Embedded paths are resolved at compile time; debug hot reload depends on the original path remaining available.

Do not infer a crate's role from its name. In particular, `standout-seeker` is a query engine, not a file/resource resolver.
