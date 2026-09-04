use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::RenderError;
use crate::output::Representation;

/// Serializes `value` as the whole document of a structured mode; every line
/// ends in a newline, so the CSV of an empty array is the empty string.
pub fn serialize_document<T: Serialize>(
    data: &T,
    representation: Representation,
) -> Result<String, RenderError> {
    match representation {
        Representation::Json => {
            let mut json = serde_json::to_string_pretty(data)?;
            json.push('\n');
            Ok(json)
        }
        Representation::Yaml => {
            let mut yaml = serde_yaml::to_string(data)?;
            if !yaml.ends_with('\n') {
                yaml.push('\n');
            }
            Ok(yaml)
        }
        Representation::Csv => crate::util::write_csv(&serde_json::to_value(data)?),
        Representation::Ndjson => {
            let mut line = serde_json::to_string(data)?;
            line.push('\n');
            Ok(line)
        }
        mode => Err(RenderError::OperationError(format!(
            "{mode:?} is not a document mode"
        ))),
    }
}

/// The record a run's document gives a handler's rendered value:
/// `{"type":"result","data":<value>}`.
pub fn result_record(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "type": "result", "data": data })
}

/// The `ndjson` form of a handler's rendered value: [`result_record`] as one
/// line, without its newline.
pub fn result_entry<T: Serialize>(data: &T) -> Result<String, RenderError> {
    Ok(serde_json::to_string(&result_record(
        serde_json::to_value(data)?,
    ))?)
}

/// The document a run's records become under an encoding with no line
/// framing: the array, in the form the framework writes a rendered value.
pub fn serialize_record_array(
    records: Vec<serde_json::Value>,
    representation: Representation,
) -> Result<String, RenderError> {
    serialize_structured(&serde_json::Value::Array(records), representation)
}

/// The document text of `data` under a structured representation, without the
/// trailing newline [`serialize_document`] adds: the form a rendered value
/// takes before the framework writes it. `ndjson`'s is the `result` record.
pub(crate) fn serialize_structured(
    data: &serde_json::Value,
    representation: Representation,
) -> Result<String, RenderError> {
    match representation {
        Representation::Json => Ok(serde_json::to_string_pretty(data)?),
        Representation::Yaml => Ok(serde_yaml::to_string(data)?),
        Representation::Csv => crate::util::write_csv(data),
        Representation::Ndjson => result_entry(data),
        mode => Err(RenderError::OperationError(format!(
            "{mode:?} is not a structured representation"
        ))),
    }
}

/// The inverse of [`serialize_document`] for the same mode.
pub fn deserialize_document<T: DeserializeOwned>(
    representation: Representation,
    text: &str,
) -> Result<T, RenderError> {
    match representation {
        Representation::Json | Representation::Ndjson => Ok(serde_json::from_str(text)?),
        Representation::Yaml => Ok(serde_yaml::from_str(text)?),
        Representation::Csv => {
            let mut reader = csv::Reader::from_reader(text.as_bytes());
            let mut rows = reader.deserialize::<T>();
            let row = rows.next().ok_or_else(|| {
                RenderError::OperationError("the CSV document has no row".into())
            })??;
            if rows.next().is_some() {
                return Err(RenderError::OperationError(
                    "the CSV document has more than one row".into(),
                ));
            }
            Ok(row)
        }
        mode => Err(RenderError::OperationError(format!(
            "{mode:?} is not a document mode"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Record {
        name: String,
        count: u32,
        note: Option<String>,
    }

    #[test]
    fn each_document_mode_round_trips_a_flat_record() {
        let record = Record {
            name: "a, \"quoted\"".into(),
            count: 2,
            note: None,
        };
        for mode in [
            Representation::Json,
            Representation::Yaml,
            Representation::Csv,
        ] {
            let text = serialize_document(&record, mode).unwrap();
            assert!(text.ends_with('\n'), "{mode:?}: {text:?}");
            let back: Record = deserialize_document(mode, &text).unwrap();
            assert_eq!(back, record, "{mode:?}");
        }
        assert_eq!(
            serialize_document(&record, Representation::Csv).unwrap(),
            "name,count,note\n\"a, \"\"quoted\"\"\",2,\n"
        );
    }

    #[test]
    fn a_record_array_is_the_line_framed_records_as_one_document() {
        let records = vec![
            serde_json::json!({"type": "apply_start"}),
            result_record(serde_json::json!({"add": 1})),
        ];
        for mode in [Representation::Json, Representation::Yaml] {
            let text = serialize_record_array(records.clone(), mode).unwrap();
            let back: Vec<serde_json::Value> = deserialize_document(mode, &text).unwrap();
            assert_eq!(back, records, "{mode:?}");
        }
    }

    #[test]
    fn a_human_mode_is_not_a_document_mode() {
        let record = Record {
            name: "x".into(),
            count: 0,
            note: None,
        };
        assert!(serialize_document(&record, Representation::Human).is_err());
        assert!(deserialize_document::<Record>(Representation::Human, "").is_err());
        assert!(deserialize_document::<Record>(Representation::Csv, "name,count,note\n").is_err());
    }

    #[test]
    fn a_document_is_exactly_one_value() {
        let error =
            deserialize_document::<Record>(Representation::Csv, "name,count,note\na,1,\nb,2,\n")
                .unwrap_err();
        assert!(error.to_string().contains("more than one row"), "{error}");
        let json = "{\"name\":\"a\",\"count\":1,\"note\":null}\n";
        assert!(
            deserialize_document::<Record>(Representation::Json, &format!("{json}{json}")).is_err()
        );
        let yaml = "name: a\ncount: 1\nnote: null\n";
        assert!(deserialize_document::<Record>(
            Representation::Yaml,
            &format!("{yaml}---\n{yaml}")
        )
        .is_err());
    }
}
