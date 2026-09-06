pub(crate) mod serializer;

use std::ops::{Index, IndexMut};

use indexmap::IndexMap;
use minijinja::value::ValueKind;
use serde::Serialize;

use crate::FormattedText;

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum RenderData {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<RenderData>),
    Object(IndexMap<String, RenderData>),
    Formatted(FormattedText),
}

impl RenderData {
    pub fn from_serialize<T: Serialize>(value: T) -> Result<Self, serde_json::Error> {
        serializer::serialize(value)
    }

    pub fn from_template_value(value: minijinja::Value) -> Result<Self, serde_json::Error> {
        use serde::ser::Error;
        if let Err(error) = value.get_attr("") {
            if error.kind() == minijinja::ErrorKind::BadSerialization {
                return Err(serde_json::Error::custom(error));
            }
        }
        if let Some(text) = FormattedText::from_value(&value) {
            return Ok(Self::Formatted(text.clone()));
        }
        match value.kind() {
            ValueKind::Map => {
                let mut map = IndexMap::new();
                for key in value.try_iter().map_err(serde_json::Error::custom)? {
                    let child = value.get_item(&key).map_err(serde_json::Error::custom)?;
                    let key_map = std::collections::BTreeMap::from([(key, ())]);
                    let key_json = serde_json::to_value(key_map)?;
                    let key = key_json.as_object().unwrap().keys().next().unwrap().clone();
                    map.insert(key, Self::from_template_value(child)?);
                }
                Ok(Self::Object(map))
            }
            ValueKind::Seq | ValueKind::Iterable => value
                .try_iter()
                .map_err(serde_json::Error::custom)?
                .map(Self::from_template_value)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Array),
            _ => Ok(serde_json::to_value(value)?.into()),
        }
    }

    pub fn to_template_value(&self) -> minijinja::Value {
        match self {
            Self::Formatted(text) => minijinja::Value::from_object(text.clone()),
            Self::Array(values) => minijinja::Value::from(
                values
                    .iter()
                    .map(Self::to_template_value)
                    .collect::<Vec<_>>(),
            ),
            Self::Object(values) => minijinja::Value::from_iter(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_template_value())),
            ),
            Self::Null => minijinja::Value::from(()),
            Self::Bool(value) => minijinja::Value::from(*value),
            Self::String(value) => minijinja::Value::from(value.clone()),
            Self::Number(value) => {
                if let Some(number) = value.as_i64() {
                    minijinja::Value::from(number)
                } else if let Some(number) = value.as_u64() {
                    minijinja::Value::from(number)
                } else if let Some(number) = value.as_i128() {
                    minijinja::Value::from(number)
                } else if let Some(number) = value.as_u128() {
                    minijinja::Value::from(number)
                } else if value.is_f64() {
                    value
                        .as_f64()
                        .map(minijinja::Value::from)
                        .unwrap_or_else(|| minijinja::Value::from(value.to_string()))
                } else {
                    minijinja::Value::from(value.to_string())
                }
            }
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("render data contains only serializable values")
    }

    pub fn as_object(&self) -> Option<&IndexMap<String, Self>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Self>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(value) => value.as_u64(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => value.as_i64(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(key)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        self.as_object_mut()?.get_mut(key)
    }
}

impl From<serde_json::Value> for RenderData {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Number(value) => Self::Number(value),
            serde_json::Value::String(value) => Self::String(value),
            serde_json::Value::Array(value) => {
                Self::Array(value.into_iter().map(Self::from).collect())
            }
            serde_json::Value::Object(value) => {
                Self::Object(value.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
        }
    }
}

impl From<FormattedText> for RenderData {
    fn from(value: FormattedText) -> Self {
        Self::Formatted(value)
    }
}

impl Index<&str> for RenderData {
    type Output = Self;
    fn index(&self, key: &str) -> &Self {
        self.get(key).unwrap_or(&Self::Null)
    }
}

impl IndexMut<&str> for RenderData {
    fn index_mut(&mut self, key: &str) -> &mut Self {
        if self.is_null() {
            *self = Self::Object(IndexMap::new());
        }
        self.as_object_mut()
            .expect("cannot index non-object")
            .entry(key.into())
            .or_insert(Self::Null)
    }
}

impl Index<usize> for RenderData {
    type Output = Self;
    fn index(&self, key: usize) -> &Self {
        self.as_array()
            .and_then(|v| v.get(key))
            .unwrap_or(&Self::Null)
    }
}

impl std::fmt::Display for RenderData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_json().fmt(f)
    }
}

impl<'de> serde::Deserialize<'de> for RenderData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde_json::Value::deserialize(deserializer).map(Into::into)
    }
}

impl<T> PartialEq<T> for RenderData
where
    T: Serialize,
{
    fn eq(&self, other: &T) -> bool {
        serde_json::to_value(other).is_ok_and(|other| self.to_json() == other)
    }
}

impl From<&str> for RenderData {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}
impl From<String> for RenderData {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<bool> for RenderData {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<i32> for RenderData {
    fn from(value: i32) -> Self {
        Self::Number(value.into())
    }
}
impl From<u64> for RenderData {
    fn from(value: u64) -> Self {
        Self::Number(value.into())
    }
}

impl IndexMut<usize> for RenderData {
    fn index_mut(&mut self, key: usize) -> &mut Self {
        &mut self.as_array_mut().expect("cannot index non-array")[key]
    }
}

impl From<i64> for RenderData {
    fn from(value: i64) -> Self {
        Self::Number(value.into())
    }
}

impl From<usize> for RenderData {
    fn from(value: usize) -> Self {
        Self::Number(value.into())
    }
}

impl<K: Into<String>, V: Into<RenderData>> FromIterator<(K, V)> for RenderData {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(values: T) -> Self {
        Self::Object(
            values
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn typed_values_preserve_order_identity_and_plain_projection() {
        #[derive(Serialize)]
        struct Data {
            z: FormattedText,
            a: String,
        }
        let data = RenderData::from_serialize(Data {
            z: FormattedText::text("[draft]").styled("heading").unwrap(),
            a: "tail".into(),
        })
        .unwrap();
        assert_eq!(
            data.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["z", "a"]
        );
        assert!(matches!(data["z"], RenderData::Formatted(_)));
        assert_eq!(
            data.to_json(),
            serde_json::json!({"z":"[draft]","a":"tail"})
        );
        assert!(
            FormattedText::from_value(&data.to_template_value().get_attr("z").unwrap()).is_some()
        );
    }

    #[test]
    fn nested_serialization_errors_are_not_lost() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("broken child"))
            }
        }
        #[derive(Serialize)]
        struct Data {
            children: Vec<Broken>,
        }
        assert!(RenderData::from_serialize(Data {
            children: vec![Broken]
        })
        .unwrap_err()
        .to_string()
        .contains("broken child"));
    }

    #[test]
    fn intermediate_json_serialization_is_plain_text() {
        struct Projected(FormattedText);
        impl Serialize for Projected {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serde_json::to_value(&self.0).unwrap().serialize(serializer)
            }
        }
        let data = RenderData::from_serialize(Projected(
            FormattedText::text("[draft]").styled("heading").unwrap(),
        ))
        .unwrap();
        assert!(matches!(&data, RenderData::String(text) if text == "[draft]"));
    }

    #[test]
    fn metadata_shaped_json_cannot_become_formatted() {
        let value = serde_json::json!({"standout::FormattedText": [{"Styled":{"style":{"Semantic":"heading"},"children":[{"Text":"[draft]"}]}}]});
        assert_eq!(RenderData::from_serialize(&value).unwrap().to_json(), value);
    }

    #[test]
    fn serde_json_protocol_events_keep_their_json_meaning() {
        let raw =
            serde_json::value::RawValue::from_string(r#"{"value":"[draft]"}"#.into()).unwrap();
        assert_eq!(
            RenderData::from_serialize(&raw).unwrap().to_json(),
            serde_json::json!({"value":"[draft]"})
        );
        let number: serde_json::Number = "123456789012345678901234567890".parse().unwrap();
        assert_eq!(
            RenderData::from_serialize(&number).unwrap().to_json(),
            serde_json::Value::Number(number)
        );
        let ordinary = serde_json::json!({"$serde_json::private::RawValue":"[draft]", "$serde_json::private::Number":"not a number"});
        assert_eq!(
            RenderData::from_serialize(&ordinary).unwrap().to_json(),
            ordinary
        );
    }

    #[test]
    fn map_key_spelling_and_rejection_match_json() {
        let keys = std::collections::BTreeMap::from([(false, 1), (true, 2)]);
        assert_eq!(
            RenderData::from_serialize(&keys).unwrap().to_json(),
            serde_json::to_value(keys).unwrap()
        );
        assert!(
            RenderData::from_serialize(std::collections::BTreeMap::from([(vec![1, 2], 1)]))
                .is_err()
        );
    }
}
