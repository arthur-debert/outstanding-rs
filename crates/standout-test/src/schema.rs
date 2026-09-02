//! Schema snapshots: the key names and JSON value types of a structured
//! document, with every value ignored (`docs/topics/stability.md`).
//!
//! A schema is the document with each scalar replaced by its type name
//! (`"string"`, `"number"`, `"boolean"`, `"null"`), each object mapped key by
//! key, and each array reduced to its distinct element schemas in order of
//! first appearance. The stored file is that schema as pretty JSON under
//! `tests/schemas/<name>` in the crate under test (`CARGO_MANIFEST_DIR`). A
//! missing file is written and the assertion fails, so a first run never
//! passes on nothing; `STANDOUT_UPDATE_SNAPSHOTS=1` rewrites a stored file
//! to accept an intentional change.

use serde_json::Value;
use standout_render::OutputMode;
use std::path::{Path, PathBuf};

pub(crate) const UPDATE_ENV: &str = "STANDOUT_UPDATE_SNAPSHOTS";
pub(crate) const SCHEMA_DIR: &str = "tests/schemas";

pub(crate) fn schema_of(value: &Value) -> Value {
    match value {
        Value::Null => Value::from("null"),
        Value::Bool(_) => Value::from("boolean"),
        Value::Number(_) => Value::from("number"),
        Value::String(_) => Value::from("string"),
        Value::Array(items) => {
            let mut shapes: Vec<Value> = Vec::new();
            for item in items {
                let shape = schema_of(item);
                if !shapes.contains(&shape) {
                    shapes.push(shape);
                }
            }
            Value::Array(shapes)
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), schema_of(value)))
                .collect(),
        ),
    }
}

pub(crate) fn document_value(output_mode: OutputMode, stdout: &str) -> Result<Value, String> {
    standout_render::deserialize_document::<Value>(output_mode, stdout).map_err(|error| {
        format!(
            "stdout is not a {output_mode:?} document ({error}):\n--- stdout ---\n{stdout}\n--------------"
        )
    })
}

pub(crate) fn snapshot_path(manifest_dir: &Path, name: &str) -> PathBuf {
    manifest_dir.join(SCHEMA_DIR).join(name)
}

pub(crate) fn check_snapshot(path: &Path, actual: &Value, update: bool) -> Result<(), String> {
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(actual).expect("a schema is JSON")
    );
    let stored = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_snapshot(path, &rendered)?;
            return Err(format!(
                "no schema snapshot was stored; recorded this run's schema at {}",
                path.display()
            ));
        }
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let expected: Value = serde_json::from_str(&stored)
        .map_err(|error| format!("{} is not a JSON schema snapshot: {error}", path.display()))?;
    if expected == *actual {
        return Ok(());
    }
    if update {
        return write_snapshot(path, &rendered);
    }
    Err(format!(
        "schema mismatch against {}\n--- stored ---\n{}--- actual ---\n{}--------------\nrun with {UPDATE_ENV}=1 to accept the change",
        path.display(),
        serde_json::to_string_pretty(&expected).expect("a schema is JSON"),
        rendered
    ))
}

fn write_snapshot(path: &Path, rendered: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, rendered)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[track_caller]
pub(crate) fn assert_schema_snapshot(output_mode: OutputMode, stdout: &str, name: &str) {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("assert_schema_snapshot needs CARGO_MANIFEST_DIR, which `cargo test` sets");
    let value = match document_value(output_mode, stdout) {
        Ok(value) => value,
        Err(message) => panic!("{message}"),
    };
    let update = std::env::var_os(UPDATE_ENV).is_some_and(|v| !v.is_empty() && v != "0");
    if let Err(message) = check_snapshot(
        &snapshot_path(&manifest_dir, name),
        &schema_of(&value),
        update,
    ) {
        panic!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_schema_keeps_keys_and_types_and_drops_values() {
        let document = json!({
            "schema_version": 1,
            "items": [{"name": "a", "done": false}, {"name": "b", "done": true}],
            "intro": null,
            "tags": [],
            "mixed": [1, "x", 2]
        });
        assert_eq!(
            schema_of(&document),
            json!({
                "schema_version": "number",
                "items": [{"name": "string", "done": "boolean"}],
                "intro": "null",
                "tags": [],
                "mixed": ["number", "string"]
            })
        );
    }

    #[test]
    fn a_renamed_key_is_a_different_schema() {
        let before = schema_of(&json!({"items": [{"name": "a"}]}));
        let after = schema_of(&json!({"items": [{"title": "a"}]}));
        assert_ne!(before, after);
        assert_eq!(before, schema_of(&json!({"items": [{"name": "zzz"}]})));
    }

    #[test]
    fn a_missing_snapshot_is_recorded_and_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = snapshot_path(dir.path(), "list.json");
        let schema = schema_of(&json!({"items": []}));

        let error = check_snapshot(&path, &schema, false).unwrap_err();
        assert!(error.contains("recorded this run's schema"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"items\": []\n}\n"
        );
        check_snapshot(&path, &schema, false).unwrap();
    }

    #[test]
    fn a_changed_schema_fails_until_accepted() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = snapshot_path(dir.path(), "list.json");
        let stored = schema_of(&json!({"items": [{"name": "a"}]}));
        let _ = check_snapshot(&path, &stored, false);

        let renamed = schema_of(&json!({"items": [{"title": "a"}]}));
        let error = check_snapshot(&path, &renamed, false).unwrap_err();
        assert!(error.contains("schema mismatch"), "{error}");
        assert!(error.contains("\"title\""), "{error}");
        assert!(error.contains(UPDATE_ENV), "{error}");

        check_snapshot(&path, &renamed, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"items\": [\n    {\n      \"title\": \"string\"\n    }\n  ]\n}\n"
        );
        check_snapshot(&path, &renamed, false).unwrap();
        assert!(check_snapshot(&path, &stored, false).is_err());
    }

    #[test]
    fn the_document_is_read_in_the_runs_mode() {
        let json = document_value(OutputMode::Json, "{\"a\": 1}").unwrap();
        let yaml = document_value(OutputMode::Yaml, "a: 1\n").unwrap();
        assert_eq!(schema_of(&json), schema_of(&yaml));
        let error = document_value(OutputMode::Text, "hello").unwrap_err();
        assert!(error.contains("not a Text document"), "{error}");
    }
}
