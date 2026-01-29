//! Schema types for string-based query parsing.
//!
//! This module provides the [`SeekerSchema`] trait and [`SeekType`] enum
//! for field metadata. These types enable the string parsing layer to
//! validate field names, determine field types, and parse values correctly.
//!
//! # Optional Usage
//!
//! The schema types are only needed for string-based query parsing (Phase 3+).
//! Code using only the imperative API (Phase 1) or derive macros for
//! programmatic queries (Phase 2) doesn't need to implement these traits.

use crate::Op;

/// The type of a seekable field.
///
/// This enum mirrors the field type annotations in the derive macro
/// (`#[seek(String)]`, `#[seek(Number)]`, etc.) and is used by the
/// string parser to determine how to parse values and validate operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekType {
    /// String field - supports text comparison operators.
    String,
    /// Numeric field - supports ordering comparisons.
    Number,
    /// Timestamp field - supports temporal comparisons.
    Timestamp,
    /// Enum field - supports equality and set membership.
    Enum,
    /// Boolean field - supports equality checks.
    Bool,
}

impl SeekType {
    /// Returns the default operator for this field type.
    ///
    /// When a query key doesn't include an explicit operator (e.g., `--name`
    /// instead of `--name-eq`), this operator is used.
    ///
    /// | Type | Default Operator |
    /// |------|------------------|
    /// | String | `Eq` |
    /// | Number | `Eq` |
    /// | Timestamp | `Eq` |
    /// | Enum | `Eq` |
    /// | Bool | `Is` (with implicit `true` value) |
    pub fn default_operator(self) -> Op {
        match self {
            SeekType::String => Op::Eq,
            SeekType::Number => Op::Eq,
            SeekType::Timestamp => Op::Eq,
            SeekType::Enum => Op::Eq,
            SeekType::Bool => Op::Is,
        }
    }

    /// Returns `true` if the given operator is valid for this field type.
    pub fn is_valid_operator(self, op: Op) -> bool {
        match self {
            SeekType::String => op.is_string_op(),
            SeekType::Number => op.is_number_op(),
            SeekType::Timestamp => op.is_timestamp_op(),
            SeekType::Enum => op.is_enum_op(),
            SeekType::Bool => op.is_bool_op(),
        }
    }

    /// Returns a human-readable name for this type.
    pub fn as_str(self) -> &'static str {
        match self {
            SeekType::String => "string",
            SeekType::Number => "number",
            SeekType::Timestamp => "timestamp",
            SeekType::Enum => "enum",
            SeekType::Bool => "boolean",
        }
    }
}

impl std::fmt::Display for SeekType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Trait providing field metadata for string-based query parsing.
///
/// This trait is optional — only needed when parsing queries from strings
/// (CLI arguments, URL query parameters, etc.). Types using only the
/// imperative API don't need to implement it.
///
/// # Derive Macro Support
///
/// The `#[derive(Seekable)]` macro can generate a `SeekerSchema` implementation
/// automatically. To expose only specific fields in string-based interfaces,
/// implement this trait manually.
///
/// # Example
///
/// ```
/// use standout_seeker::{SeekerSchema, SeekType};
///
/// struct Task {
///     name: String,
///     priority: u8,
///     internal_id: u64, // Not exposed to queries
/// }
///
/// impl SeekerSchema for Task {
///     fn field_type(field: &str) -> Option<SeekType> {
///         match field {
///             "name" => Some(SeekType::String),
///             "priority" => Some(SeekType::Number),
///             _ => None, // internal_id not exposed
///         }
///     }
///
///     fn field_names() -> &'static [&'static str] {
///         &["name", "priority"]
///     }
/// }
/// ```
pub trait SeekerSchema {
    /// Returns the type of a field, or `None` if the field doesn't exist.
    ///
    /// This is used to:
    /// - Validate that a field name is queryable
    /// - Determine how to parse the value string
    /// - Validate that the operator is appropriate for the field type
    fn field_type(field: &str) -> Option<SeekType>;

    /// Returns all queryable field names.
    ///
    /// This is used for help text generation and validation error messages.
    fn field_names() -> &'static [&'static str];

    /// Resolves an enum variant name to its discriminant value.
    ///
    /// Override this to support string variant names in queries (e.g.,
    /// `--status=active` instead of `--status=1`).
    ///
    /// # Parameters
    ///
    /// * `field` - The field name (to disambiguate if multiple enum fields exist)
    /// * `variant` - The variant name to resolve
    ///
    /// # Returns
    ///
    /// The discriminant value, or `None` if:
    /// - The field is not an enum
    /// - The variant name is not recognized
    ///
    /// # Default Implementation
    ///
    /// Returns `None`, meaning only numeric discriminants are supported.
    fn resolve_enum_variant(_field: &str, _variant: &str) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_type_default_operators() {
        assert_eq!(SeekType::String.default_operator(), Op::Eq);
        assert_eq!(SeekType::Number.default_operator(), Op::Eq);
        assert_eq!(SeekType::Timestamp.default_operator(), Op::Eq);
        assert_eq!(SeekType::Enum.default_operator(), Op::Eq);
        assert_eq!(SeekType::Bool.default_operator(), Op::Is);
    }

    #[test]
    fn seek_type_valid_operators() {
        // String
        assert!(SeekType::String.is_valid_operator(Op::Eq));
        assert!(SeekType::String.is_valid_operator(Op::Contains));
        assert!(SeekType::String.is_valid_operator(Op::Regex));
        assert!(!SeekType::String.is_valid_operator(Op::Gt));
        assert!(!SeekType::String.is_valid_operator(Op::Before));

        // Number
        assert!(SeekType::Number.is_valid_operator(Op::Eq));
        assert!(SeekType::Number.is_valid_operator(Op::Gt));
        assert!(SeekType::Number.is_valid_operator(Op::Lte));
        assert!(!SeekType::Number.is_valid_operator(Op::Contains));
        assert!(!SeekType::Number.is_valid_operator(Op::Before));

        // Timestamp
        assert!(SeekType::Timestamp.is_valid_operator(Op::Eq));
        assert!(SeekType::Timestamp.is_valid_operator(Op::Before));
        assert!(SeekType::Timestamp.is_valid_operator(Op::After));
        assert!(SeekType::Timestamp.is_valid_operator(Op::Lt));
        assert!(!SeekType::Timestamp.is_valid_operator(Op::Contains));

        // Enum
        assert!(SeekType::Enum.is_valid_operator(Op::Eq));
        assert!(SeekType::Enum.is_valid_operator(Op::In));
        assert!(!SeekType::Enum.is_valid_operator(Op::Gt));
        assert!(!SeekType::Enum.is_valid_operator(Op::Contains));

        // Bool
        assert!(SeekType::Bool.is_valid_operator(Op::Eq));
        assert!(SeekType::Bool.is_valid_operator(Op::Is));
        assert!(!SeekType::Bool.is_valid_operator(Op::Gt));
        assert!(!SeekType::Bool.is_valid_operator(Op::Contains));
    }

    #[test]
    fn seek_type_display() {
        assert_eq!(SeekType::String.to_string(), "string");
        assert_eq!(SeekType::Number.to_string(), "number");
        assert_eq!(SeekType::Timestamp.to_string(), "timestamp");
        assert_eq!(SeekType::Enum.to_string(), "enum");
        assert_eq!(SeekType::Bool.to_string(), "boolean");
    }

    // Test manual SeekerSchema implementation
    struct TestSchema;

    impl SeekerSchema for TestSchema {
        fn field_type(field: &str) -> Option<SeekType> {
            match field {
                "name" => Some(SeekType::String),
                "count" => Some(SeekType::Number),
                "status" => Some(SeekType::Enum),
                _ => None,
            }
        }

        fn field_names() -> &'static [&'static str] {
            &["name", "count", "status"]
        }

        fn resolve_enum_variant(field: &str, variant: &str) -> Option<u32> {
            if field == "status" {
                match variant {
                    "pending" => Some(0),
                    "active" => Some(1),
                    "done" => Some(2),
                    _ => None,
                }
            } else {
                None
            }
        }
    }

    #[test]
    fn seeker_schema_field_type() {
        assert_eq!(TestSchema::field_type("name"), Some(SeekType::String));
        assert_eq!(TestSchema::field_type("count"), Some(SeekType::Number));
        assert_eq!(TestSchema::field_type("status"), Some(SeekType::Enum));
        assert_eq!(TestSchema::field_type("unknown"), None);
    }

    #[test]
    fn seeker_schema_field_names() {
        assert_eq!(TestSchema::field_names(), &["name", "count", "status"]);
    }

    #[test]
    fn seeker_schema_enum_variant_resolution() {
        assert_eq!(
            TestSchema::resolve_enum_variant("status", "pending"),
            Some(0)
        );
        assert_eq!(
            TestSchema::resolve_enum_variant("status", "active"),
            Some(1)
        );
        assert_eq!(TestSchema::resolve_enum_variant("status", "done"), Some(2));
        assert_eq!(TestSchema::resolve_enum_variant("status", "unknown"), None);
        assert_eq!(TestSchema::resolve_enum_variant("name", "pending"), None);
    }
}
