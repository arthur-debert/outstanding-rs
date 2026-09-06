- **Breaking:** configuration, render, final-write and event failures now carry the
  error underneath them. Downcast `source()` to `clapfig::ClapfigError`,
  `standout::RenderError`, `serde_json::Error` or `std::io::Error` instead of matching
  rendered prose (closes #572, #575).
- **Breaking:** `RunError::render`, `final_write`, `config` and `with_source` take an
  `Arc<dyn Error + Send + Sync>` cause — wrap yours in `Arc::new`. `RunError::new` refuses
  `RunErrorKind::Config`; build that one with `RunError::config(message, error)`.
- **Breaking:** `EmitError` is `Clone` and holds its cause behind an `Arc`:
  `Serialize(Arc<serde_json::Error>)`, `Write(Arc<std::io::Error>)` and
  `Render { message, cause }`. Build a render failure with the struct variant;
  `EmitError::from` still accepts a `serde_json::Error` or an `std::io::Error`.
