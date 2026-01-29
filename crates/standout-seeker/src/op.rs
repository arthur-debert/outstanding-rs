//! Comparison operators for query clauses.
//!
//! The [`Op`] enum defines all supported comparison operators, organized by
//! the types they apply to. Not all operators are valid for all types.

use std::cmp::Ordering;

/// Comparison operator for a query clause.
///
/// Operators are grouped by the types they support:
/// - **Universal**: `Eq`, `Ne` - work on all types
/// - **String**: `StartsWith`, `EndsWith`, `Contains`, `Regex`
/// - **Numeric/Timestamp**: `Gt`, `Gte`, `Lt`, `Lte`
/// - **Timestamp aliases**: `Before` (alias for `Lt`), `After` (alias for `Gt`)
/// - **Enum**: `In` - check membership in a set
/// - **Bool alias**: `Is` (alias for `Eq`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    // Universal operators
    /// Equal (exact match). Valid for all types.
    Eq,
    /// Not equal. Valid for all types.
    Ne,

    // String operators
    /// String starts with prefix.
    StartsWith,
    /// String ends with suffix.
    EndsWith,
    /// String contains substring.
    Contains,
    /// String matches regular expression.
    Regex,

    // Numeric/Timestamp comparison operators
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,

    // Timestamp aliases (for readability)
    /// Earlier than (alias for `Lt` on timestamps).
    Before,
    /// Later than (alias for `Gt` on timestamps).
    After,

    // Enum operators
    /// Value is one of the given set.
    In,

    // Bool alias
    /// Alias for `Eq` (reads naturally: `archived.is(true)`).
    Is,
}

impl Op {
    /// Returns `true` if this operator is valid for string comparisons.
    pub fn is_string_op(self) -> bool {
        matches!(
            self,
            Op::Eq | Op::Ne | Op::StartsWith | Op::EndsWith | Op::Contains | Op::Regex
        )
    }

    /// Returns `true` if this operator is valid for numeric comparisons.
    pub fn is_number_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Gt | Op::Gte | Op::Lt | Op::Lte)
    }

    /// Returns `true` if this operator is valid for timestamp comparisons.
    pub fn is_timestamp_op(self) -> bool {
        matches!(
            self,
            Op::Eq | Op::Ne | Op::Gt | Op::Gte | Op::Lt | Op::Lte | Op::Before | Op::After
        )
    }

    /// Returns `true` if this operator is valid for enum comparisons.
    pub fn is_enum_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::In)
    }

    /// Returns `true` if this operator is valid for boolean comparisons.
    pub fn is_bool_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Is)
    }

    /// Normalizes timestamp aliases to their canonical form.
    ///
    /// - `Before` -> `Lt`
    /// - `After` -> `Gt`
    /// - `Is` -> `Eq`
    /// - Others unchanged
    pub fn normalize(self) -> Op {
        match self {
            Op::Before => Op::Lt,
            Op::After => Op::Gt,
            Op::Is => Op::Eq,
            other => other,
        }
    }

    /// Evaluates a comparison given an ordering result.
    ///
    /// This is used for numeric and timestamp comparisons where we have
    /// an `Ordering` from comparing two values.
    pub fn eval_ordering(self, ordering: Ordering) -> bool {
        match self.normalize() {
            Op::Eq => ordering == Ordering::Equal,
            Op::Ne => ordering != Ordering::Equal,
            Op::Gt => ordering == Ordering::Greater,
            Op::Gte => ordering != Ordering::Less,
            Op::Lt => ordering == Ordering::Less,
            Op::Lte => ordering != Ordering::Greater,
            _ => false, // Not an ordering-based operator
        }
    }

    /// Returns the display name of this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::StartsWith => "startswith",
            Op::EndsWith => "endswith",
            Op::Contains => "contains",
            Op::Regex => "regex",
            Op::Gt => "gt",
            Op::Gte => "gte",
            Op::Lt => "lt",
            Op::Lte => "lte",
            Op::Before => "before",
            Op::After => "after",
            Op::In => "in",
            Op::Is => "is",
        }
    }
}

impl std::fmt::Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_type_checks() {
        // String ops
        assert!(Op::Eq.is_string_op());
        assert!(Op::Contains.is_string_op());
        assert!(Op::Regex.is_string_op());
        assert!(!Op::Gt.is_string_op());

        // Number ops
        assert!(Op::Eq.is_number_op());
        assert!(Op::Gt.is_number_op());
        assert!(!Op::Contains.is_number_op());
        assert!(!Op::Before.is_number_op());

        // Timestamp ops
        assert!(Op::Eq.is_timestamp_op());
        assert!(Op::Before.is_timestamp_op());
        assert!(Op::After.is_timestamp_op());
        assert!(!Op::Contains.is_timestamp_op());

        // Enum ops
        assert!(Op::Eq.is_enum_op());
        assert!(Op::In.is_enum_op());
        assert!(!Op::Gt.is_enum_op());

        // Bool ops
        assert!(Op::Eq.is_bool_op());
        assert!(Op::Is.is_bool_op());
        assert!(!Op::Gt.is_bool_op());
    }

    #[test]
    fn op_normalization() {
        assert_eq!(Op::Before.normalize(), Op::Lt);
        assert_eq!(Op::After.normalize(), Op::Gt);
        assert_eq!(Op::Is.normalize(), Op::Eq);
        assert_eq!(Op::Eq.normalize(), Op::Eq);
        assert_eq!(Op::Contains.normalize(), Op::Contains);
    }

    #[test]
    fn op_eval_ordering() {
        // Equal
        assert!(Op::Eq.eval_ordering(Ordering::Equal));
        assert!(!Op::Eq.eval_ordering(Ordering::Less));
        assert!(!Op::Eq.eval_ordering(Ordering::Greater));

        // Not equal
        assert!(!Op::Ne.eval_ordering(Ordering::Equal));
        assert!(Op::Ne.eval_ordering(Ordering::Less));
        assert!(Op::Ne.eval_ordering(Ordering::Greater));

        // Greater than
        assert!(!Op::Gt.eval_ordering(Ordering::Equal));
        assert!(!Op::Gt.eval_ordering(Ordering::Less));
        assert!(Op::Gt.eval_ordering(Ordering::Greater));

        // Greater than or equal
        assert!(Op::Gte.eval_ordering(Ordering::Equal));
        assert!(!Op::Gte.eval_ordering(Ordering::Less));
        assert!(Op::Gte.eval_ordering(Ordering::Greater));

        // Less than
        assert!(!Op::Lt.eval_ordering(Ordering::Equal));
        assert!(Op::Lt.eval_ordering(Ordering::Less));
        assert!(!Op::Lt.eval_ordering(Ordering::Greater));

        // Less than or equal
        assert!(Op::Lte.eval_ordering(Ordering::Equal));
        assert!(Op::Lte.eval_ordering(Ordering::Less));
        assert!(!Op::Lte.eval_ordering(Ordering::Greater));

        // Aliases work the same
        assert!(Op::Before.eval_ordering(Ordering::Less));
        assert!(Op::After.eval_ordering(Ordering::Greater));
    }

    #[test]
    fn op_display() {
        assert_eq!(Op::Eq.to_string(), "eq");
        assert_eq!(Op::StartsWith.to_string(), "startswith");
        assert_eq!(Op::Before.to_string(), "before");
    }
}
