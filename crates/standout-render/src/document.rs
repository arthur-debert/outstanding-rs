use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::RenderError;
use crate::output::OutputMode;

/// One value as the whole document of a structured mode, newline-terminated:
/// pretty JSON, YAML, or — for a flat record — one CSV row under its header.
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
        OutputMode::Yaml => Ok(serde_yaml::to_string(data)?),
        OutputMode::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.serialize(data)?;
            Ok(String::from_utf8(writer.into_inner()?)?)
        }
        mode => Err(RenderError::OperationError(format!(
            "{mode:?} is not a document mode"
        ))),
    }
}

/// The inverse of [`serialize_document`] for the same mode.
pub fn deserialize_document<T: DeserializeOwned>(
    output_mode: OutputMode,
    text: &str,
) -> Result<T, RenderError> {
    match output_mode {
        OutputMode::Json => Ok(serde_json::from_str(text)?),
        OutputMode::Yaml => Ok(serde_yaml::from_str(text)?),
        OutputMode::Csv => csv::Reader::from_reader(text.as_bytes())
            .deserialize()
            .next()
            .ok_or_else(|| RenderError::OperationError("the CSV document has no row".into()))?
            .map_err(RenderError::from),
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
}
