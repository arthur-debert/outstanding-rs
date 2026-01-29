//! Runtime value types for field comparison.
//!
//! The [`Value`] enum represents the runtime value of a field extracted from a struct.
//! It supports all the core types: strings, numbers, timestamps, enums, and booleans.

use std::cmp::Ordering;

/// Runtime value for comparison, borrowed from the source struct.
///
/// This enum represents the value of a field at query execution time.
/// The accessor function provided to query methods returns this type.
///
/// # Example
///
/// ```
/// use standout_seeker::{Value, Number};
///
/// struct Task {
///     name: String,
///     priority: u8,
/// }
///
/// fn accessor<'a>(task: &'a Task, field: &str) -> Value<'a> {
///     match field {
///         "name" => Value::String(&task.name),
///         "priority" => Value::Number(Number::U64(task.priority as u64)),
///         _ => Value::None,
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// String value (borrowed).
    String(&'a str),
    /// Numeric value.
    Number(Number),
    /// Timestamp value (milliseconds since Unix epoch).
    Timestamp(Timestamp),
    /// Enum discriminant value.
    Enum(u32),
    /// Boolean value.
    Bool(bool),
    /// Field not present, null, or unsupported.
    None,
}

impl<'a> Value<'a> {
    /// Returns `true` if this is a `None` value.
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// Returns `true` if this is a `String` value.
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Returns `true` if this is a `Number` value.
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }

    /// Returns `true` if this is a `Timestamp` value.
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Value::Timestamp(_))
    }

    /// Returns `true` if this is an `Enum` value.
    pub fn is_enum(&self) -> bool {
        matches!(self, Value::Enum(_))
    }

    /// Returns `true` if this is a `Bool` value.
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    /// Extracts the string value, if present.
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Extracts the number value, if present.
    pub fn as_number(&self) -> Option<Number> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Extracts the timestamp value, if present.
    pub fn as_timestamp(&self) -> Option<Timestamp> {
        match self {
            Value::Timestamp(t) => Some(*t),
            _ => None,
        }
    }

    /// Extracts the enum discriminant, if present.
    pub fn as_enum(&self) -> Option<u32> {
        match self {
            Value::Enum(d) => Some(*d),
            _ => None,
        }
    }

    /// Extracts the boolean value, if present.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Numeric value supporting all common numeric types.
///
/// Numbers are stored in one of three variants to preserve precision:
/// - `I64` for signed integers
/// - `U64` for unsigned integers
/// - `F64` for floating point
///
/// Comparisons between different numeric types are handled by converting
/// to the appropriate common type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// Signed 64-bit integer.
    I64(i64),
    /// Unsigned 64-bit integer.
    U64(u64),
    /// 64-bit floating point.
    F64(f64),
}

impl Number {
    /// Converts the number to f64 for comparison.
    pub fn to_f64(self) -> f64 {
        match self {
            Number::I64(n) => n as f64,
            Number::U64(n) => n as f64,
            Number::F64(n) => n,
        }
    }

    /// Compares two numbers, handling mixed types.
    pub fn compare(self, other: Number) -> Option<Ordering> {
        match (self, other) {
            // Same type comparisons
            (Number::I64(a), Number::I64(b)) => Some(a.cmp(&b)),
            (Number::U64(a), Number::U64(b)) => Some(a.cmp(&b)),
            (Number::F64(a), Number::F64(b)) => a.partial_cmp(&b),

            // Mixed type comparisons - convert to f64
            _ => self.to_f64().partial_cmp(&other.to_f64()),
        }
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.compare(*other)
    }
}

// Conversions from primitive types
impl From<i8> for Number {
    fn from(n: i8) -> Self {
        Number::I64(n as i64)
    }
}

impl From<i16> for Number {
    fn from(n: i16) -> Self {
        Number::I64(n as i64)
    }
}

impl From<i32> for Number {
    fn from(n: i32) -> Self {
        Number::I64(n as i64)
    }
}

impl From<i64> for Number {
    fn from(n: i64) -> Self {
        Number::I64(n)
    }
}

impl From<u8> for Number {
    fn from(n: u8) -> Self {
        Number::U64(n as u64)
    }
}

impl From<u16> for Number {
    fn from(n: u16) -> Self {
        Number::U64(n as u64)
    }
}

impl From<u32> for Number {
    fn from(n: u32) -> Self {
        Number::U64(n as u64)
    }
}

impl From<u64> for Number {
    fn from(n: u64) -> Self {
        Number::U64(n)
    }
}

impl From<f32> for Number {
    fn from(n: f32) -> Self {
        Number::F64(n as f64)
    }
}

impl From<f64> for Number {
    fn from(n: f64) -> Self {
        Number::F64(n)
    }
}

impl From<usize> for Number {
    fn from(n: usize) -> Self {
        Number::U64(n as u64)
    }
}

impl From<isize> for Number {
    fn from(n: isize) -> Self {
        Number::I64(n as i64)
    }
}

/// Timestamp value represented as milliseconds since Unix epoch.
///
/// This provides a simple, timezone-agnostic representation suitable
/// for comparison operations. Users can convert from their preferred
/// datetime type (e.g., `chrono::DateTime`, `std::time::SystemTime`).
///
/// # Example
///
/// ```
/// use standout_seeker::Timestamp;
///
/// // Create from milliseconds
/// let ts = Timestamp(1706500000000); // 2024-01-29 approx
///
/// // Timestamps are ordered
/// assert!(Timestamp(1000) < Timestamp(2000));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Creates a new timestamp from milliseconds since Unix epoch.
    pub fn from_millis(millis: i64) -> Self {
        Timestamp(millis)
    }

    /// Creates a new timestamp from seconds since Unix epoch.
    pub fn from_secs(secs: i64) -> Self {
        Timestamp(secs * 1000)
    }

    /// Returns the timestamp as milliseconds since Unix epoch.
    pub fn as_millis(self) -> i64 {
        self.0
    }

    /// Returns the timestamp as seconds since Unix epoch.
    pub fn as_secs(self) -> i64 {
        self.0 / 1000
    }
}

impl From<i64> for Timestamp {
    fn from(millis: i64) -> Self {
        Timestamp(millis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_type_checks() {
        assert!(Value::String("test").is_string());
        assert!(Value::Number(Number::I64(42)).is_number());
        assert!(Value::Timestamp(Timestamp(0)).is_timestamp());
        assert!(Value::Enum(1).is_enum());
        assert!(Value::Bool(true).is_bool());
        assert!(Value::None.is_none());
    }

    #[test]
    fn value_extractors() {
        assert_eq!(Value::String("hello").as_str(), Some("hello"));
        assert_eq!(
            Value::Number(Number::I64(42)).as_number(),
            Some(Number::I64(42))
        );
        assert_eq!(
            Value::Timestamp(Timestamp(1000)).as_timestamp(),
            Some(Timestamp(1000))
        );
        assert_eq!(Value::Enum(5).as_enum(), Some(5));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));

        // Wrong type returns None
        assert_eq!(Value::String("test").as_number(), None);
        assert_eq!(Value::Number(Number::I64(1)).as_str(), None);
    }

    #[test]
    fn number_comparisons_same_type() {
        assert_eq!(
            Number::I64(5).compare(Number::I64(10)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Number::I64(10).compare(Number::I64(5)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Number::I64(5).compare(Number::I64(5)),
            Some(Ordering::Equal)
        );

        assert_eq!(
            Number::U64(5).compare(Number::U64(10)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Number::F64(5.0).compare(Number::F64(10.0)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn number_comparisons_mixed_types() {
        assert_eq!(
            Number::I64(5).compare(Number::U64(10)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Number::I64(5).compare(Number::F64(5.0)),
            Some(Ordering::Equal)
        );
        assert_eq!(
            Number::U64(10).compare(Number::F64(5.5)),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn number_nan_comparison() {
        assert_eq!(Number::F64(f64::NAN).compare(Number::F64(1.0)), None);
        assert_eq!(Number::F64(1.0).compare(Number::F64(f64::NAN)), None);
    }

    #[test]
    fn number_conversions() {
        assert_eq!(Number::from(42i32), Number::I64(42));
        assert_eq!(Number::from(42u32), Number::U64(42));
        assert_eq!(Number::from(42.5f64), Number::F64(42.5));
    }

    #[test]
    fn timestamp_ordering() {
        assert!(Timestamp(1000) < Timestamp(2000));
        assert!(Timestamp(2000) > Timestamp(1000));
        assert_eq!(Timestamp(1000), Timestamp(1000));
    }

    #[test]
    fn timestamp_conversions() {
        assert_eq!(Timestamp::from_secs(1).as_millis(), 1000);
        assert_eq!(Timestamp::from_millis(5000).as_secs(), 5);
    }
}
