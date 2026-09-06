use serde::{ser, Serialize};

use super::RenderData;

type Error = serde_json::Error;
type Result<T> = std::result::Result<T, Error>;

pub(crate) const FORMATTED_TEXT: &str = "standout::FormattedText/v1";

pub(crate) fn is_presentation_serializer<S>() -> bool {
    std::any::type_name::<S>() == std::any::type_name::<Serializer>()
}

pub(super) fn serialize<T: Serialize>(value: T) -> Result<RenderData> {
    value.serialize(Serializer)
}

struct Serializer;

impl ser::Serializer for Serializer {
    type Ok = RenderData;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = TupleStruct;
    type SerializeTupleVariant = Variant<Sequence>;
    type SerializeMap = Map;
    type SerializeStruct = Struct;
    type SerializeStructVariant = Variant<Map>;

    fn serialize_bool(self, value: bool) -> Result<RenderData> {
        Ok(RenderData::Bool(value))
    }
    fn serialize_i8(self, value: i8) -> Result<RenderData> {
        self.serialize_i64(value.into())
    }
    fn serialize_i16(self, value: i16) -> Result<RenderData> {
        self.serialize_i64(value.into())
    }
    fn serialize_i32(self, value: i32) -> Result<RenderData> {
        self.serialize_i64(value.into())
    }
    fn serialize_i64(self, value: i64) -> Result<RenderData> {
        Ok(RenderData::Number(value.into()))
    }
    fn serialize_i128(self, value: i128) -> Result<RenderData> {
        serde_json::to_value(value).map(Into::into)
    }
    fn serialize_u8(self, value: u8) -> Result<RenderData> {
        self.serialize_u64(value.into())
    }
    fn serialize_u16(self, value: u16) -> Result<RenderData> {
        self.serialize_u64(value.into())
    }
    fn serialize_u32(self, value: u32) -> Result<RenderData> {
        self.serialize_u64(value.into())
    }
    fn serialize_u64(self, value: u64) -> Result<RenderData> {
        Ok(RenderData::Number(value.into()))
    }
    fn serialize_u128(self, value: u128) -> Result<RenderData> {
        serde_json::to_value(value).map(Into::into)
    }
    fn serialize_f32(self, value: f32) -> Result<RenderData> {
        serde_json::to_value(value).map(Into::into)
    }
    fn serialize_f64(self, value: f64) -> Result<RenderData> {
        serde_json::to_value(value).map(Into::into)
    }
    fn serialize_char(self, value: char) -> Result<RenderData> {
        self.serialize_str(&value.to_string())
    }
    fn serialize_str(self, value: &str) -> Result<RenderData> {
        Ok(RenderData::String(value.into()))
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<RenderData> {
        Ok(RenderData::Array(
            value
                .iter()
                .map(|v| RenderData::Number((*v).into()))
                .collect(),
        ))
    }
    fn serialize_none(self) -> Result<RenderData> {
        self.serialize_unit()
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<RenderData> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<RenderData> {
        Ok(RenderData::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<RenderData> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<RenderData> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<RenderData> {
        if name == FORMATTED_TEXT {
            let nodes = serde_json::from_value(serde_json::to_value(value)?)?;
            return crate::FormattedText::from_nodes(nodes)
                .map(RenderData::Formatted)
                .map_err(<Error as ser::Error>::custom);
        }
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<RenderData> {
        Ok(wrap_variant(variant, value.serialize(self)?))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Sequence> {
        Ok(Sequence(Vec::new()))
    }
    fn serialize_tuple(self, _: usize) -> Result<Sequence> {
        self.serialize_seq(None)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<TupleStruct> {
        Ok(TupleStruct { fields: Vec::new() })
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Variant<Sequence>> {
        Ok(Variant {
            name,
            value: Sequence(Vec::new()),
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Map> {
        Ok(Map {
            values: indexmap::IndexMap::new(),
            key: None,
        })
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Struct> {
        if matches!(
            name,
            "$serde_json::private::RawValue" | "$serde_json::private::Number"
        ) {
            return serde::Serializer::serialize_struct(serde_json::value::Serializer, name, len)
                .map(Struct::Json);
        }
        self.serialize_map(None).map(Struct::Data)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Variant<Map>> {
        Ok(Variant {
            name,
            value: self.serialize_map(None)?,
        })
    }
}

struct Sequence(Vec<RenderData>);
impl ser::SerializeSeq for Sequence {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.0.push(value.serialize(Serializer)?);
        Ok(())
    }
    fn end(self) -> Result<RenderData> {
        Ok(RenderData::Array(self.0))
    }
}
impl ser::SerializeTuple for Sequence {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<RenderData> {
        ser::SerializeSeq::end(self)
    }
}

struct TupleStruct {
    fields: Vec<RenderData>,
}
impl ser::SerializeTupleStruct for TupleStruct {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.fields.push(value.serialize(Serializer)?);
        Ok(())
    }
    fn end(self) -> Result<RenderData> {
        Ok(RenderData::Array(self.fields))
    }
}

struct Map {
    values: indexmap::IndexMap<String, RenderData>,
    key: Option<String>,
}
impl ser::SerializeMap for Map {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        struct Key<'a, T: ?Sized>(&'a T);
        impl<T: ?Sized + Serialize> Serialize for Key<'_, T> {
            fn serialize<S: ser::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(1))?;
                ser::SerializeMap::serialize_entry(&mut map, self.0, &())?;
                ser::SerializeMap::end(map)
            }
        }
        self.key = Some(
            serde_json::to_value(Key(key))?
                .as_object()
                .unwrap()
                .keys()
                .next()
                .unwrap()
                .clone(),
        );
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        let key = self
            .key
            .take()
            .ok_or_else(|| <Error as ser::Error>::custom("map value has no key"))?;
        self.values.insert(key, value.serialize(Serializer)?);
        Ok(())
    }
    fn end(self) -> Result<RenderData> {
        Ok(RenderData::Object(self.values))
    }
}
impl ser::SerializeStruct for Map {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.values.insert(key.into(), value.serialize(Serializer)?);
        Ok(())
    }
    fn end(self) -> Result<RenderData> {
        ser::SerializeMap::end(self)
    }
}

struct Variant<T> {
    name: &'static str,
    value: T,
}
fn wrap_variant(name: &str, value: RenderData) -> RenderData {
    RenderData::Object([(name.into(), value)].into_iter().collect())
}
impl ser::SerializeTupleVariant for Variant<Sequence> {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(&mut self.value, value)
    }
    fn end(self) -> Result<RenderData> {
        Ok(wrap_variant(self.name, ser::SerializeSeq::end(self.value)?))
    }
}
impl ser::SerializeStructVariant for Variant<Map> {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        ser::SerializeStruct::serialize_field(&mut self.value, key, value)
    }
    fn end(self) -> Result<RenderData> {
        Ok(wrap_variant(
            self.name,
            ser::SerializeStruct::end(self.value)?,
        ))
    }
}

enum Struct {
    Data(Map),
    Json(<serde_json::value::Serializer as ser::Serializer>::SerializeStruct),
}
impl ser::SerializeStruct for Struct {
    type Ok = RenderData;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        match self {
            Self::Data(map) => ser::SerializeStruct::serialize_field(map, key, value),
            Self::Json(map) => ser::SerializeStruct::serialize_field(map, key, value),
        }
    }
    fn end(self) -> Result<RenderData> {
        match self {
            Self::Data(map) => ser::SerializeStruct::end(map),
            Self::Json(map) => ser::SerializeStruct::end(map).map(Into::into),
        }
    }
}
