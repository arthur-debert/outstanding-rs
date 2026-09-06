- **Breaking:** an event whose serialization or template render fails now reaches the run
  carrying that failure, so `source()` downcasts to `serde_json::Error` or
  `standout::RenderError` instead of matching rendered prose (closes #575).
- **Breaking:** `EmitError` is `Clone` and carries each cause behind an `Arc`:
  `Serialize(Arc<serde_json::Error>)`, `Write(Arc<std::io::Error>)`, and
  `Render { message, cause }`, whose `cause` is an optional
  `Arc<dyn std::error::Error + Send + Sync>`. Construct a render failure with the struct
  variant; `EmitError::from` still accepts a `serde_json::Error` or an `std::io::Error`.
- **Breaking:** `RunError::render`, `RunError::final_write`, `RunError::config` and
  `RunError::with_source` take an `Arc<dyn std::error::Error + Send + Sync>` cause; wrap the
  error you pass in `Arc::new`.
