# Declare success statuses, delete XML, take flat CSV records

Three decisions of `docs/spec/parity-machine-contract.md` — D5, D7 and D8 — recorded together because they land in one workstream (PAR02-WS04) and each is a rule about what the machine contract does and does not carry. The spec holds the reasoning; this records the shape.

## D5: a successful run declares its own status

`ExitStatus` keeps `0`, `1` and `2` as the only statuses the framework assigns. A handler that succeeded and wants the shell to know more returns `Output::with_exit_status(ExitStatus)`, and the run exits with that status verbatim, the way an `AppFailure`'s status already reaches the shell. The framework names no code for "found nothing" or "has changes": there is no ecosystem convention to follow, and the two adopters that needed one (tflike's `plan -detailed-exitcode`, dodot's findings) needed different codes.

A declared status is success-with-signal, never an error. The outcome stays `DispatchResult::Handled`, the document goes to stdout, stderr carries nothing, no diagnostic is produced, and `RunOutput::exit_status()` is where the status rides. It applies to `Output::Render` and `Output::Silent`; declaring one on `Output::Binary` or `Output::Artifact` is a render error, because those outcomes have no carrier for it and a dropped status would be a lie. `ListViewBuilder::empty_exit_status(n)` is the list case: `ListViewResult::into_output` applies it when `items` is empty.

`Output` gains the variant `WithStatus { output, status }` rather than a field on every variant, so the existing `Output::Render(x)` patterns in handlers and macros keep compiling; `map_render` and the `is_*` predicates reach through it.

## D7: XML is deleted

`OutputMode::Xml`, `serialize_to_xml`, `sanitize_xml_keys` and the `quick-xml` dependency are gone, with no shim: `--output xml` is a clap usage error, exit `2`. No client project used the mode; its serializer silently dropped rows whose keys collided after element-name sanitization (#409) and rode a version with two advisories (#408). The deletion closes #107, #408 and #409.

## D8: CSV takes flat records only

A flat record is a map whose values are scalars. `--output csv` accepts one flat record or an array of flat records — one row each, columns in first-seen key order — and anything else is a `RenderError` that names the offending value and `CsvProjection` as the way to declare columns. The indexed `items.0.name` flattening is deleted; the loud error replaces the silent reshaping that #108 reported.

The framework's own documents obey the rule. The diagnostic (ADR-0037) is expressed as a `CsvProjection` over the document itself: `CsvProjection::builder(".")`, one row, with `range` as the three columns `range_filename`, `range_line` and `range_column`, `null_repr("")` so they are empty when no range is set. To make that possible a projection's row source may be `.`, and a record at the row source is one row where an array is one row per element.

## Consequences

`csv_records` and `write_csv` replace `flatten_json_for_csv` in `standout-render`, and the glue-invariant test bans them from the glue crate as it banned their predecessors. CSV column order now follows the handler's declared order, the same rule JSON and YAML follow since #464. The 10.0 release notes carry D7 and D8 as breaking lines.
