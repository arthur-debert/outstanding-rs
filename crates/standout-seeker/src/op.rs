use std::cmp::Ordering;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Eq,
    Ne,
    StartsWith,
    EndsWith,
    Contains,
    Regex,
    Gt,
    Gte,
    Lt,
    Lte,
    Before,
    After,
    In,
    Is,
}
impl Op {
    pub fn is_string_op(self) -> bool {
        matches!(
            self,
            Op::Eq | Op::Ne | Op::StartsWith | Op::EndsWith | Op::Contains | Op::Regex
        )
    }
    pub fn is_number_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Gt | Op::Gte | Op::Lt | Op::Lte)
    }
    pub fn is_timestamp_op(self) -> bool {
        matches!(
            self,
            Op::Eq | Op::Ne | Op::Gt | Op::Gte | Op::Lt | Op::Lte | Op::Before | Op::After
        )
    }
    pub fn is_enum_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::In)
    }
    pub fn is_bool_op(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Is)
    }
    pub fn normalize(self) -> Op {
        match self {
            Op::Before => Op::Lt,
            Op::After => Op::Gt,
            Op::Is => Op::Eq,
            other => other,
        }
    }
    pub fn eval_ordering(self, ordering: Ordering) -> bool {
        match self.normalize() {
            Op::Eq => ordering == Ordering::Equal,
            Op::Ne => ordering != Ordering::Equal,
            Op::Gt => ordering == Ordering::Greater,
            Op::Gte => ordering != Ordering::Less,
            Op::Lt => ordering == Ordering::Less,
            Op::Lte => ordering != Ordering::Greater,
            _ => false,
        }
    }
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
        assert!(Op::Eq.is_string_op());
        assert!(Op::Contains.is_string_op());
        assert!(Op::Regex.is_string_op());
        assert!(!Op::Gt.is_string_op());
        assert!(Op::Eq.is_number_op());
        assert!(Op::Gt.is_number_op());
        assert!(!Op::Contains.is_number_op());
        assert!(!Op::Before.is_number_op());
        assert!(Op::Eq.is_timestamp_op());
        assert!(Op::Before.is_timestamp_op());
        assert!(Op::After.is_timestamp_op());
        assert!(!Op::Contains.is_timestamp_op());
        assert!(Op::Eq.is_enum_op());
        assert!(Op::In.is_enum_op());
        assert!(!Op::Gt.is_enum_op());
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
        assert!(Op::Eq.eval_ordering(Ordering::Equal));
        assert!(!Op::Eq.eval_ordering(Ordering::Less));
        assert!(!Op::Eq.eval_ordering(Ordering::Greater));
        assert!(!Op::Ne.eval_ordering(Ordering::Equal));
        assert!(Op::Ne.eval_ordering(Ordering::Less));
        assert!(Op::Ne.eval_ordering(Ordering::Greater));
        assert!(!Op::Gt.eval_ordering(Ordering::Equal));
        assert!(!Op::Gt.eval_ordering(Ordering::Less));
        assert!(Op::Gt.eval_ordering(Ordering::Greater));
        assert!(Op::Gte.eval_ordering(Ordering::Equal));
        assert!(!Op::Gte.eval_ordering(Ordering::Less));
        assert!(Op::Gte.eval_ordering(Ordering::Greater));
        assert!(!Op::Lt.eval_ordering(Ordering::Equal));
        assert!(Op::Lt.eval_ordering(Ordering::Less));
        assert!(!Op::Lt.eval_ordering(Ordering::Greater));
        assert!(Op::Lte.eval_ordering(Ordering::Equal));
        assert!(Op::Lte.eval_ordering(Ordering::Less));
        assert!(!Op::Lte.eval_ordering(Ordering::Greater));
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
