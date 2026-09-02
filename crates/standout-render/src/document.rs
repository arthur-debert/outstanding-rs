use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::RenderError;
use crate::output::OutputMode;

/// One value as the whole document of a structured mode: pretty JSON, YAML,
/// one compact JSON line for `ndjson`, or CSV under the flat-record rule of
/// [`crate::csv_records`] (one row for a record, one per element for an array
/// of records). Every line ends in a newline, so the CSV of an empty array is
/// the empty string.
pub fn serialize_document<T: Serialize>(
    data: &T,
    output_mode: OutputMode,
) -> Result<String, RenderError> {
    match output_mode {
        OutputMode::Json => {
            let mut json = serde_json::to_string_pretty(data)?;
            json.push('\n');
            Ok(json)
        }
        OutputMode::Yaml => {
            let mut yaml = serde_yaml::to_string(data)?;
            if !yaml.ends_with('\n') {
                yaml.push('\n');
            }
            Ok(yaml)
        }
        OutputMode::Csv => crate::util::write_csv(&serde_json::to_value(data)?),
        OutputMode::Ndjson => {
            let mut line = serde_json::to_string(data)?;
            line.push('\n');
            Ok(line)
        }
        mode => Err(RenderError::OperationError(format!(
            "{mode:?} is not a document mode"
        ))),
    }
}

/// The `ndjson` form of a handler's rendered value: the one line
/// `{"type":"result","data":<value>}`, without its newline.
pub fn result_entry<T: Serialize>(data: &T) -> Result<String, RenderError> {
    #[derive(Serialize)]
    struct ResultEntry<'a, T> {
        #[serde(rename = "type")]
        entry_type: &'static str,
        data: &'a T,
    }
    Ok(serde_json::to_string(&ResultEntry {
        entry_type: "result",
        data,
    })?)
}

/// The inverse of [`serialize_document`] for the same mode.
pub fn deserialize_document<T: DeserializeOwned>(
    output_mode: OutputMode,
    text: &str,
) -> Result<T, RenderError> {
    match output_mode {
        OutputMode::Json | OutputMode::Ndjson => Ok(serde_json::from_str(text)?),
        OutputMode::Yaml => Ok(serde_yaml::from_str(text)?),
        OutputMode::Csv => {
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
        for mode in [OutputMode::Json, OutputMode::Yaml, OutputMode::Csv] {
            let text = serialize_document(&record, mode).unwrap();
            assert!(text.ends_with('\n'), "{mode:?}: {text:?}");
            let back: Record = deserialize_document(mode, &text).unwrap();
            assert_eq!(back, record, "{mode:?}");
        }
        assert_eq!(
            serialize_document(&record, OutputMode::Csv).unwrap(),
            "name,count,note\n\"a, \"\"quoted\"\"\",2,\n"
        );
    }

    #[test]
    fn a_human_mode_is_not_a_document_mode() {
        let record = Record {
            name: "x".into(),
            count: 0,
            note: None,
        };
        assert!(serialize_document(&record, OutputMode::Text).is_err());
        assert!(deserialize_document::<Record>(OutputMode::Term, "").is_err());
        assert!(deserialize_document::<Record>(OutputMode::Csv, "name,count,note\n").is_err());
    }

    #[test]
    fn a_document_is_exactly_one_value() {
        let error =
            deserialize_document::<Record>(OutputMode::Csv, "name,count,note\na,1,\nb,2,\n")
                .unwrap_err();
        assert!(error.to_string().contains("more than one row"), "{error}");
        let json = "{\"name\":\"a\",\"count\":1,\"note\":null}\n";
        assert!(
            deserialize_document::<Record>(OutputMode::Json, &format!("{json}{json}")).is_err()
        );
        let yaml = "name: a\ncount: 1\nnote: null\n";
        assert!(
            deserialize_document::<Record>(OutputMode::Yaml, &format!("{yaml}---\n{yaml}"))
                .is_err()
        );
    }
}
