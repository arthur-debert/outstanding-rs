use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use crate::collector::{InputSourceKind, ResolvedInput};

#[derive(Default)]
pub struct Inputs {
    entries: HashMap<Cow<'static, str>, Entry>,
}

struct Entry {
    type_id: TypeId,
    type_name: &'static str,
    source: InputSourceKind,
    value: Box<dyn Any + Send + Sync>,
}

impl Inputs {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn insert<T>(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        resolved: ResolvedInput<T>,
    ) -> Option<InputSourceKind>
    where
        T: Send + Sync + 'static,
    {
        let prev = self.entries.insert(
            name.into(),
            Entry {
                type_id: TypeId::of::<T>(),
                type_name: std::any::type_name::<T>(),
                source: resolved.source,
                value: Box::new(resolved.value),
            },
        );
        prev.map(|e| e.source)
    }

    pub fn get<T: 'static>(&self, name: &str) -> Option<&T> {
        let entry = self.entries.get(name)?;
        if entry.type_id != TypeId::of::<T>() {
            return None;
        }
        entry.value.downcast_ref::<T>()
    }

    pub fn get_required<T: 'static>(&self, name: &str) -> Result<&T, MissingInput> {
        let Some(entry) = self.entries.get(name) else {
            return Err(MissingInput::NotRegistered {
                name: name.to_string(),
            });
        };
        if entry.type_id != TypeId::of::<T>() {
            return Err(MissingInput::TypeMismatch {
                name: name.to_string(),
                expected: std::any::type_name::<T>(),
                actual: entry.type_name,
            });
        }
        entry
            .value
            .downcast_ref::<T>()
            .ok_or_else(|| MissingInput::TypeMismatch {
                name: name.to_string(),
                expected: std::any::type_name::<T>(),
                actual: entry.type_name,
            })
    }

    pub fn source_of(&self, name: &str) -> Option<InputSourceKind> {
        self.entries.get(name).map(|e| e.source)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter_sources(&self) -> impl Iterator<Item = (&str, InputSourceKind)> + '_ {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_ref(), entry.source))
    }
}

impl fmt::Debug for Inputs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Inputs");
        for (name, entry) in &self.entries {
            s.field(
                name.as_ref(),
                &format_args!("{} from {}", entry.type_name, entry.source),
            );
        }
        s.finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MissingInput {
    #[error("no input named `{name}` was registered for this command")]
    NotRegistered { name: String },
    #[error("input `{name}` is registered as `{actual}`, not `{expected}`")]
    TypeMismatch {
        name: String,
        expected: &'static str,
        actual: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg<T>(value: T) -> ResolvedInput<T> {
        ResolvedInput {
            value,
            source: InputSourceKind::Arg,
        }
    }

    #[test]
    fn insert_and_get() {
        let mut inputs = Inputs::new();
        inputs.insert("body", arg("hello".to_string()));

        let body: &String = inputs.get("body").unwrap();
        assert_eq!(body, "hello");
    }

    #[test]
    fn get_missing_returns_none() {
        let inputs = Inputs::new();
        assert!(inputs.get::<String>("missing").is_none());
    }

    #[test]
    fn get_wrong_type_returns_none() {
        let mut inputs = Inputs::new();
        inputs.insert("body", arg("hello".to_string()));
        assert!(inputs.get::<u32>("body").is_none());
    }

    #[test]
    fn get_required_reports_missing() {
        let inputs = Inputs::new();
        let err = inputs.get_required::<String>("body").unwrap_err();
        assert!(matches!(err, MissingInput::NotRegistered { .. }));
        assert!(err.to_string().contains("body"));
    }

    #[test]
    fn get_required_reports_type_mismatch() {
        let mut inputs = Inputs::new();
        inputs.insert("body", arg("hello".to_string()));
        let err = inputs.get_required::<u32>("body").unwrap_err();
        match err {
            MissingInput::TypeMismatch {
                ref name,
                expected,
                actual,
            } => {
                assert_eq!(name, "body");
                assert!(expected.contains("u32"));
                assert!(actual.contains("String"));
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    #[test]
    fn accepts_owned_string_name() {
        let mut inputs = Inputs::new();
        let runtime_name: String = format!("input_{}", 42);
        inputs.insert(runtime_name.clone(), arg("x".to_string()));

        assert_eq!(inputs.get::<String>(runtime_name.as_str()).unwrap(), "x");
    }

    #[test]
    fn two_inputs_of_same_type_do_not_collide() {
        let mut inputs = Inputs::new();
        inputs.insert("body", arg("the body".to_string()));
        inputs.insert("title", arg("the title".to_string()));

        assert_eq!(inputs.get::<String>("body").unwrap(), "the body");
        assert_eq!(inputs.get::<String>("title").unwrap(), "the title");
    }

    #[test]
    fn insert_returns_previous_source() {
        let mut inputs = Inputs::new();
        assert!(inputs.insert("body", arg("first".to_string())).is_none());
        let prev = inputs.insert(
            "body",
            ResolvedInput {
                value: "second".to_string(),
                source: InputSourceKind::Stdin,
            },
        );
        assert_eq!(prev, Some(InputSourceKind::Arg));
        assert_eq!(inputs.source_of("body"), Some(InputSourceKind::Stdin));
    }

    #[test]
    fn source_of_and_contains() {
        let mut inputs = Inputs::new();
        assert!(!inputs.contains("body"));
        inputs.insert("body", arg("x".to_string()));
        assert!(inputs.contains("body"));
        assert_eq!(inputs.source_of("body"), Some(InputSourceKind::Arg));
        assert_eq!(inputs.source_of("missing"), None);
    }

    #[test]
    fn iter_sources_yields_all_entries() {
        let mut inputs = Inputs::new();
        inputs.insert("body", arg("x".to_string()));
        inputs.insert(
            "yes",
            ResolvedInput {
                value: true,
                source: InputSourceKind::Flag,
            },
        );

        let mut pairs: Vec<_> = inputs.iter_sources().collect();
        pairs.sort_by_key(|(name, _)| *name);
        assert_eq!(
            pairs,
            vec![
                ("body", InputSourceKind::Arg),
                ("yes", InputSourceKind::Flag)
            ]
        );
    }

    #[test]
    fn len_and_is_empty() {
        let mut inputs = Inputs::new();
        assert!(inputs.is_empty());
        assert_eq!(inputs.len(), 0);
        inputs.insert("body", arg("x".to_string()));
        assert!(!inputs.is_empty());
        assert_eq!(inputs.len(), 1);
    }
}
