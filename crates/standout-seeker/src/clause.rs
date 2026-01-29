//! Clause types for query predicates.
//!
//! A [`Clause`] represents a single filter predicate: a field name,
//! an operator, and a comparison value.

use regex::Regex;

use crate::op::Op;
use crate::value::{Number, Timestamp, Value};

/// A single filter predicate.
///
/// A clause consists of:
/// - A field name (the field to compare)
/// - An operator (how to compare)
/// - A value (what to compare against)
///
/// # Example
///
/// ```
/// use standout_seeker::{Clause, Op, ClauseValue};
///
/// let clause = Clause {
///     field: "name".to_string(),
///     op: Op::Contains,
///     value: ClauseValue::String("test".to_string()),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Clause {
    /// The field name to compare.
    pub field: String,
    /// The comparison operator.
    pub op: Op,
    /// The value to compare against.
    pub value: ClauseValue,
}

impl Clause {
    /// Creates a new clause.
    pub fn new(field: impl Into<String>, op: Op, value: impl Into<ClauseValue>) -> Self {
        Clause {
            field: field.into(),
            op,
            value: value.into(),
        }
    }

    /// Evaluates this clause against a field value.
    ///
    /// Returns `true` if the value matches the clause's predicate.
    /// Returns `false` if the value doesn't match or if the types are incompatible.
    pub fn matches(&self, field_value: &Value<'_>) -> bool {
        match (&self.value, field_value) {
            // String comparisons
            (ClauseValue::String(pattern), Value::String(s)) => self.match_string(s, pattern),

            // Regex comparison
            (ClauseValue::Regex(regex), Value::String(s)) => regex.is_match(s),

            // Number comparisons
            (ClauseValue::Number(clause_num), Value::Number(field_num)) => {
                self.match_number(*field_num, *clause_num)
            }

            // Timestamp comparisons
            (ClauseValue::Timestamp(clause_ts), Value::Timestamp(field_ts)) => {
                self.match_timestamp(*field_ts, *clause_ts)
            }

            // Enum comparisons
            (ClauseValue::Enum(clause_disc), Value::Enum(field_disc)) => {
                self.match_enum(*field_disc, *clause_disc)
            }

            // Enum set membership
            (ClauseValue::EnumSet(set), Value::Enum(field_disc)) => {
                self.match_enum_set(*field_disc, set)
            }

            // Bool comparisons
            (ClauseValue::Bool(clause_bool), Value::Bool(field_bool)) => {
                self.match_bool(*field_bool, *clause_bool)
            }

            // None field value - never matches (except possibly Ne)
            (_, Value::None) => {
                // A missing field never matches any positive assertion
                // For Ne, we could argue it should match, but for safety we say no
                false
            }

            // Type mismatch - doesn't match
            _ => false,
        }
    }

    fn match_string(&self, field: &str, pattern: &str) -> bool {
        match self.op.normalize() {
            Op::Eq => field == pattern,
            Op::Ne => field != pattern,
            Op::StartsWith => field.starts_with(pattern),
            Op::EndsWith => field.ends_with(pattern),
            Op::Contains => field.contains(pattern),
            // Regex handled separately
            _ => false,
        }
    }

    fn match_number(&self, field: Number, clause: Number) -> bool {
        match field.compare(clause) {
            Some(ordering) => self.op.eval_ordering(ordering),
            None => false, // NaN comparison
        }
    }

    fn match_timestamp(&self, field: Timestamp, clause: Timestamp) -> bool {
        let ordering = field.cmp(&clause);
        self.op.eval_ordering(ordering)
    }

    fn match_enum(&self, field: u32, clause: u32) -> bool {
        match self.op.normalize() {
            Op::Eq => field == clause,
            Op::Ne => field != clause,
            _ => false,
        }
    }

    fn match_enum_set(&self, field: u32, set: &[u32]) -> bool {
        match self.op {
            Op::In => set.contains(&field),
            _ => false,
        }
    }

    fn match_bool(&self, field: bool, clause: bool) -> bool {
        match self.op.normalize() {
            Op::Eq => field == clause,
            Op::Ne => field != clause,
            _ => false,
        }
    }
}

/// Owned value for storage in a clause.
///
/// Unlike [`Value`], which borrows from the source struct, `ClauseValue`
/// owns its data so it can be stored in query definitions.
#[derive(Debug, Clone)]
pub enum ClauseValue {
    /// String value.
    String(String),
    /// Numeric value.
    Number(Number),
    /// Timestamp value.
    Timestamp(Timestamp),
    /// Single enum discriminant.
    Enum(u32),
    /// Set of enum discriminants (for `In` operator).
    EnumSet(Vec<u32>),
    /// Boolean value.
    Bool(bool),
    /// Compiled regular expression.
    Regex(Regex),
}

// Conversions from common types to ClauseValue

impl From<String> for ClauseValue {
    fn from(s: String) -> Self {
        ClauseValue::String(s)
    }
}

impl From<&str> for ClauseValue {
    fn from(s: &str) -> Self {
        ClauseValue::String(s.to_string())
    }
}

impl From<Number> for ClauseValue {
    fn from(n: Number) -> Self {
        ClauseValue::Number(n)
    }
}

impl From<Timestamp> for ClauseValue {
    fn from(t: Timestamp) -> Self {
        ClauseValue::Timestamp(t)
    }
}

impl From<bool> for ClauseValue {
    fn from(b: bool) -> Self {
        ClauseValue::Bool(b)
    }
}

impl From<Regex> for ClauseValue {
    fn from(r: Regex) -> Self {
        ClauseValue::Regex(r)
    }
}

// Numeric type conversions
impl From<i8> for ClauseValue {
    fn from(n: i8) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<i16> for ClauseValue {
    fn from(n: i16) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<i32> for ClauseValue {
    fn from(n: i32) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<i64> for ClauseValue {
    fn from(n: i64) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<u8> for ClauseValue {
    fn from(n: u8) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<u16> for ClauseValue {
    fn from(n: u16) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<u32> for ClauseValue {
    fn from(n: u32) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<u64> for ClauseValue {
    fn from(n: u64) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<f32> for ClauseValue {
    fn from(n: f32) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<f64> for ClauseValue {
    fn from(n: f64) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<usize> for ClauseValue {
    fn from(n: usize) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

impl From<isize> for ClauseValue {
    fn from(n: isize) -> Self {
        ClauseValue::Number(Number::from(n))
    }
}

// Enum discriminant conversion
impl From<Vec<u32>> for ClauseValue {
    fn from(v: Vec<u32>) -> Self {
        ClauseValue::EnumSet(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_eq() {
        let clause = Clause::new("name", Op::Eq, "hello");
        assert!(clause.matches(&Value::String("hello")));
        assert!(!clause.matches(&Value::String("world")));
        assert!(!clause.matches(&Value::String("Hello"))); // case sensitive
    }

    #[test]
    fn string_ne() {
        let clause = Clause::new("name", Op::Ne, "hello");
        assert!(!clause.matches(&Value::String("hello")));
        assert!(clause.matches(&Value::String("world")));
    }

    #[test]
    fn string_startswith() {
        let clause = Clause::new("name", Op::StartsWith, "hello");
        assert!(clause.matches(&Value::String("hello world")));
        assert!(clause.matches(&Value::String("hello")));
        assert!(!clause.matches(&Value::String("say hello")));
    }

    #[test]
    fn string_endswith() {
        let clause = Clause::new("name", Op::EndsWith, "world");
        assert!(clause.matches(&Value::String("hello world")));
        assert!(clause.matches(&Value::String("world")));
        assert!(!clause.matches(&Value::String("world!")));
    }

    #[test]
    fn string_contains() {
        let clause = Clause::new("name", Op::Contains, "llo");
        assert!(clause.matches(&Value::String("hello")));
        assert!(clause.matches(&Value::String("llo")));
        assert!(!clause.matches(&Value::String("helo")));
    }

    #[test]
    fn string_regex() {
        let regex = Regex::new(r"^hello\d+$").unwrap();
        let clause = Clause::new("name", Op::Regex, regex);
        assert!(clause.matches(&Value::String("hello123")));
        assert!(clause.matches(&Value::String("hello1")));
        assert!(!clause.matches(&Value::String("hello")));
        assert!(!clause.matches(&Value::String("hello123!")));
    }

    #[test]
    fn number_comparisons() {
        let clause_eq = Clause::new("count", Op::Eq, 10i64);
        assert!(clause_eq.matches(&Value::Number(Number::I64(10))));
        assert!(!clause_eq.matches(&Value::Number(Number::I64(11))));

        let clause_gt = Clause::new("count", Op::Gt, 10i64);
        assert!(clause_gt.matches(&Value::Number(Number::I64(11))));
        assert!(!clause_gt.matches(&Value::Number(Number::I64(10))));
        assert!(!clause_gt.matches(&Value::Number(Number::I64(9))));

        let clause_gte = Clause::new("count", Op::Gte, 10i64);
        assert!(clause_gte.matches(&Value::Number(Number::I64(11))));
        assert!(clause_gte.matches(&Value::Number(Number::I64(10))));
        assert!(!clause_gte.matches(&Value::Number(Number::I64(9))));

        let clause_lt = Clause::new("count", Op::Lt, 10i64);
        assert!(!clause_lt.matches(&Value::Number(Number::I64(11))));
        assert!(!clause_lt.matches(&Value::Number(Number::I64(10))));
        assert!(clause_lt.matches(&Value::Number(Number::I64(9))));

        let clause_lte = Clause::new("count", Op::Lte, 10i64);
        assert!(!clause_lte.matches(&Value::Number(Number::I64(11))));
        assert!(clause_lte.matches(&Value::Number(Number::I64(10))));
        assert!(clause_lte.matches(&Value::Number(Number::I64(9))));
    }

    #[test]
    fn number_mixed_types() {
        let clause = Clause::new("count", Op::Eq, 10i64);
        // i64 clause value compared with u64 field value
        assert!(clause.matches(&Value::Number(Number::U64(10))));
        // i64 clause value compared with f64 field value
        assert!(clause.matches(&Value::Number(Number::F64(10.0))));
    }

    #[test]
    fn timestamp_comparisons() {
        let clause_eq = Clause::new("created", Op::Eq, Timestamp(1000));
        assert!(clause_eq.matches(&Value::Timestamp(Timestamp(1000))));
        assert!(!clause_eq.matches(&Value::Timestamp(Timestamp(2000))));

        let clause_before = Clause::new("created", Op::Before, Timestamp(1000));
        assert!(clause_before.matches(&Value::Timestamp(Timestamp(500))));
        assert!(!clause_before.matches(&Value::Timestamp(Timestamp(1000))));
        assert!(!clause_before.matches(&Value::Timestamp(Timestamp(1500))));

        let clause_after = Clause::new("created", Op::After, Timestamp(1000));
        assert!(!clause_after.matches(&Value::Timestamp(Timestamp(500))));
        assert!(!clause_after.matches(&Value::Timestamp(Timestamp(1000))));
        assert!(clause_after.matches(&Value::Timestamp(Timestamp(1500))));
    }

    #[test]
    fn enum_comparisons() {
        let clause_eq = Clause::new("status", Op::Eq, ClauseValue::Enum(1));
        assert!(clause_eq.matches(&Value::Enum(1)));
        assert!(!clause_eq.matches(&Value::Enum(2)));

        let clause_ne = Clause::new("status", Op::Ne, ClauseValue::Enum(1));
        assert!(!clause_ne.matches(&Value::Enum(1)));
        assert!(clause_ne.matches(&Value::Enum(2)));
    }

    #[test]
    fn enum_in() {
        let clause = Clause::new("status", Op::In, vec![1u32, 2, 3]);
        assert!(clause.matches(&Value::Enum(1)));
        assert!(clause.matches(&Value::Enum(2)));
        assert!(clause.matches(&Value::Enum(3)));
        assert!(!clause.matches(&Value::Enum(4)));
    }

    #[test]
    fn bool_comparisons() {
        let clause_true = Clause::new("archived", Op::Eq, true);
        assert!(clause_true.matches(&Value::Bool(true)));
        assert!(!clause_true.matches(&Value::Bool(false)));

        let clause_is = Clause::new("archived", Op::Is, true);
        assert!(clause_is.matches(&Value::Bool(true)));
        assert!(!clause_is.matches(&Value::Bool(false)));
    }

    #[test]
    fn none_value_never_matches() {
        let clause = Clause::new("name", Op::Eq, "test");
        assert!(!clause.matches(&Value::None));

        let clause_ne = Clause::new("name", Op::Ne, "test");
        assert!(!clause_ne.matches(&Value::None));
    }

    #[test]
    fn type_mismatch_doesnt_match() {
        let clause = Clause::new("name", Op::Eq, "test");
        // String clause vs number value
        assert!(!clause.matches(&Value::Number(Number::I64(42))));
        assert!(!clause.matches(&Value::Bool(true)));
    }

    #[test]
    fn clause_value_conversions() {
        // Test that various types convert properly
        let _: ClauseValue = "test".into();
        let _: ClauseValue = String::from("test").into();
        let _: ClauseValue = 42i64.into();
        let _: ClauseValue = 42u32.into();
        let _: ClauseValue = 3.14f64.into();
        let _: ClauseValue = true.into();
        let _: ClauseValue = Timestamp(1000).into();
        let _: ClauseValue = vec![1u32, 2, 3].into();
    }
}
