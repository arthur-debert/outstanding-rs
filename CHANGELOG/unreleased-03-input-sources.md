- **Breaking:** `InputCollector::bind_sources` is required. A collector that reads stdin,
  the clipboard or a prompt rebuilds itself over the run's `InputSources`; one that reads an
  argument, environment variable, config value or default returns `None`. A collector
  written against 12.0.1 that omits it no longer compiles, where before it inherited a
  `None` default and silently read the process's own streams. See
  [Input Backends](crates/standout-input/docs/topics/backends.md) (closes #550).
- **Breaking:** an unbound `StdinSource` fails instead of reading the process's stdin.
  `StdinSource::new()` carries no reader until the chain binds one through `bind_sources`;
  a source that never reaches that call fails with the new `InputError::StdinNotBound`.
  `StdinSource::with_reader` and `with_shared_reader` are unchanged.
