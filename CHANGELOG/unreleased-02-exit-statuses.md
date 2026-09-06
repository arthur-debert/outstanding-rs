- **Breaking:** `ProcessOutcome` gains `final_write_failure: Option<RunError>` and is no
  longer `Copy`, `PartialEq` or `Eq`. Read it for the failure that turned a successful run
  into exit `1`: `kind()` says which write failed, `source()` is the `std::io::Error`.
  Struct-literal constructions add the field; comparisons compare `handled` and `status`
  (closes #564).
- `AppBuilder::usage_exit_status(u8)` names the status a run exits with when clap rejects
  the command line, for an application whose published contract already spends `2`. The
  default is still `2` and the error is still `RunErrorKind::ClapUsage`;
  `usage_exit_status(0)` fails `build()` (closes #545).
- `AppFailure::framed()` keeps the failure's exit status but takes the ordinary diagnostic
  path instead of writing your bytes verbatim: `Error: <message>` on stderr for humans, a
  stdout diagnostic document under `json`, `yaml`, `csv` and `ndjson`. A plain `AppFailure`
  is unchanged (closes #546).
