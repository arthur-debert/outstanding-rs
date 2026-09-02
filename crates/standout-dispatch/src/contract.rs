//! The versioned contract surface (`docs/topics/stability.md`).
//!
//! A type that implements [`ContractSurface`] names the version of the shape
//! it serializes to. `T::envelope(self)` wraps a value as an [`Envelope`],
//! which serializes as `{"schema_version": N, "data": <T>}`, so a consumer of
//! `--output json` can tell a shape change from a data change without parsing
//! anything else. A framework-owned document (the diagnostic, the list view,
//! the help document) implements the trait too but carries `schema_version`
//! as a top-level key beside its own fields instead of nesting under `data`.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub trait ContractSurface {
    const SCHEMA_VERSION: u32;

    fn envelope(self) -> Envelope<Self>
    where
        Self: Sized,
    {
        Envelope { data: self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Envelope<T> {
    data: T,
}

impl<T: ContractSurface> Envelope<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub const fn schema_version(&self) -> u32 {
        T::SCHEMA_VERSION
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_data(self) -> T {
        self.data
    }
}

impl<T: ContractSurface + Serialize> Serialize for Envelope<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut envelope = serializer.serialize_struct("Envelope", 2)?;
        envelope.serialize_field("schema_version", &T::SCHEMA_VERSION)?;
        envelope.serialize_field("data", &self.data)?;
        envelope.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Listing {
        items: Vec<&'static str>,
    }

    impl ContractSurface for Listing {
        const SCHEMA_VERSION: u32 = 3;
    }

    #[test]
    fn the_envelope_stamps_the_version_before_the_data() {
        let json = serde_json::to_string(&Listing { items: vec!["a"] }.envelope()).unwrap();
        assert_eq!(json, r#"{"schema_version":3,"data":{"items":["a"]}}"#);
    }

    #[test]
    fn the_envelope_hands_the_data_back() {
        let envelope = Envelope::new(Listing {
            items: vec!["a", "b"],
        });
        assert_eq!(envelope.schema_version(), 3);
        assert_eq!(envelope.data().items.len(), 2);
        assert_eq!(envelope.into_data().items, ["a", "b"]);
    }
}
