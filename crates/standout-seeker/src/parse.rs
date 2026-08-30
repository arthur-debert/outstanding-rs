use crate::clause::ClauseValue;
use crate::schema::{SeekType, SeekerSchema};
use crate::{Dir, Number, Op, OrderBy, Query, Timestamp};
use std::collections::HashSet;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnknownField {
        field: String,
        available: Vec<String>,
    },
    InvalidOperator {
        field: String,
        operator: String,
        field_type: SeekType,
    },
    InvalidValue {
        field: String,
        value: String,
        expected: SeekType,
        reason: String,
    },
    InvalidRegex {
        field: String,
        pattern: String,
        error: String,
    },
    InvalidOrdering {
        value: String,
        reason: String,
    },
    InvalidLimit {
        key: String,
        value: String,
    },
    UnknownOperator {
        operator: String,
    },
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownField { field, available } => {
                write!(
                    f,
                    "unknown field '{}'. Available: {}",
                    field,
                    available.join(", ")
                )
            }
            ParseError::InvalidOperator {
                field,
                operator,
                field_type,
            } => {
                write!(
                    f,
                    "operator '{}' is not valid for {} field '{}'",
                    operator, field_type, field
                )
            }
            ParseError::InvalidValue {
                field,
                value,
                expected,
                reason,
            } => {
                write!(
                    f,
                    "invalid value '{}' for {} field '{}': {}",
                    value, expected, field, reason
                )
            }
            ParseError::InvalidRegex {
                field,
                pattern,
                error,
            } => {
                write!(
                    f,
                    "invalid regex '{}' for field '{}': {}",
                    pattern, field, error
                )
            }
            ParseError::InvalidOrdering { value, reason } => {
                write!(f, "invalid ordering '{}': {}", value, reason)
            }
            ParseError::InvalidLimit { key, value } => {
                write!(
                    f,
                    "invalid {} value '{}': expected positive integer",
                    key, value
                )
            }
            ParseError::UnknownOperator { operator } => {
                write!(f, "unknown operator '{}'", operator)
            }
        }
    }
}
impl std::error::Error for ParseError {}
pub type ParseResult<T> = Result<T, ParseError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClauseGroup {
    #[default]
    And,
    Or,
    Not,
}
pub fn parse_operator(s: &str) -> Option<Op> {
    let s = s.to_lowercase();
    match s.as_str() {
        "eq" => Some(Op::Eq),
        "ne" | "neq" => Some(Op::Ne),
        "gt" => Some(Op::Gt),
        "gte" => Some(Op::Gte),
        "lt" => Some(Op::Lt),
        "lte" => Some(Op::Lte),
        "startswith" | "prefix" => Some(Op::StartsWith),
        "endswith" | "suffix" => Some(Op::EndsWith),
        "contains" => Some(Op::Contains),
        "regex" | "re" | "match" => Some(Op::Regex),
        "before" => Some(Op::Before),
        "after" => Some(Op::After),
        "in" => Some(Op::In),
        "is" => Some(Op::Is),
        _ => None,
    }
}
fn operator_names() -> HashSet<&'static str> {
    [
        "eq",
        "ne",
        "neq",
        "gt",
        "gte",
        "lt",
        "lte",
        "startswith",
        "prefix",
        "endswith",
        "suffix",
        "contains",
        "regex",
        "re",
        "match",
        "before",
        "after",
        "in",
        "is",
    ]
    .into_iter()
    .collect()
}
pub fn parse_key(key: &str) -> (String, Option<Op>) {
    let parts: Vec<&str> = key.split('-').collect();
    if parts.len() > 1 {
        let last = parts.last().unwrap().to_lowercase();
        if operator_names().contains(last.as_str()) {
            let field = parts[..parts.len() - 1].join("-");
            let op = parse_operator(&last);
            return (field, op);
        }
    }
    (key.to_string(), None)
}
pub fn parse_value<S: SeekerSchema>(
    value: &str,
    field: &str,
    field_type: SeekType,
    op: Op,
) -> ParseResult<ClauseValue> {
    match field_type {
        SeekType::String => {
            if op == Op::Regex {
                match regex::Regex::new(value) {
                    Ok(re) => Ok(ClauseValue::Regex(re)),
                    Err(e) => Err(ParseError::InvalidRegex {
                        field: field.to_string(),
                        pattern: value.to_string(),
                        error: e.to_string(),
                    }),
                }
            } else {
                Ok(ClauseValue::String(value.to_string()))
            }
        }
        SeekType::Number => parse_number(value, field),
        SeekType::Timestamp => parse_timestamp(value, field),
        SeekType::Enum => parse_enum::<S>(value, field, op),
        SeekType::Bool => parse_bool(value, field),
    }
}
fn parse_number(value: &str, field: &str) -> ParseResult<ClauseValue> {
    if let Ok(n) = value.parse::<i64>() {
        return Ok(ClauseValue::Number(Number::I64(n)));
    }
    if let Ok(n) = value.parse::<u64>() {
        return Ok(ClauseValue::Number(Number::U64(n)));
    }
    if let Ok(n) = value.parse::<f64>() {
        return Ok(ClauseValue::Number(Number::F64(n)));
    }
    Err(ParseError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
        expected: SeekType::Number,
        reason: "expected integer or decimal number".to_string(),
    })
}
fn parse_timestamp(value: &str, field: &str) -> ParseResult<ClauseValue> {
    if let Ok(ms) = value.parse::<i64>() {
        return Ok(ClauseValue::Timestamp(Timestamp(ms)));
    }
    if let Some(ts) = parse_date_only(value) {
        return Ok(ClauseValue::Timestamp(ts));
    }
    if let Some(ts) = parse_datetime(value) {
        return Ok(ClauseValue::Timestamp(ts));
    }
    if value.len() == 4 {
        if let Ok(year) = value.parse::<i32>() {
            let days_since_epoch = days_from_year(year);
            let ms = days_since_epoch * 24 * 60 * 60 * 1000;
            return Ok(ClauseValue::Timestamp(Timestamp(ms)));
        }
    }
    Err(ParseError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
        expected: SeekType::Timestamp,
        reason: "expected Unix timestamp (ms), ISO date (YYYY-MM-DD), or datetime".to_string(),
    })
}
fn parse_date_only(value: &str) -> Option<Timestamp> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_ymd(year, month, day)?;
    let ms = days * 24 * 60 * 60 * 1000;
    Some(Timestamp(ms))
}
fn parse_datetime(value: &str) -> Option<Timestamp> {
    let value = value.trim_end_matches('Z');
    let parts: Vec<&str> = value.split('T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }
    let year: i32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second_str = time_parts[2].split('.').next()?;
    let second: u32 = second_str.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let days = days_from_ymd(year, month, day)?;
    let seconds = hour * 3600 + minute * 60 + second;
    let ms = days * 24 * 60 * 60 * 1000 + seconds as i64 * 1000;
    Some(Timestamp(ms))
}
fn days_from_year(year: i32) -> i64 {
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    days
}
fn days_from_ymd(year: i32, month: u32, day: u32) -> Option<i64> {
    let days_in_months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = is_leap_year(year);
    let max_day = if month == 2 && leap {
        29
    } else {
        *days_in_months.get(month as usize - 1)?
    };
    if day > max_day {
        return None;
    }
    let mut days = days_from_year(year);
    for m in 1..month {
        days += days_in_months[m as usize - 1] as i64;
        if m == 2 && leap {
            days += 1;
        }
    }
    days += day as i64 - 1;
    Some(days)
}
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
fn parse_enum<S: SeekerSchema>(value: &str, field: &str, op: Op) -> ParseResult<ClauseValue> {
    if op == Op::In {
        let mut discriminants = Vec::new();
        for part in value.split(',') {
            let part = part.trim();
            let disc = parse_single_enum::<S>(part, field)?;
            discriminants.push(disc);
        }
        Ok(ClauseValue::EnumSet(discriminants))
    } else {
        let disc = parse_single_enum::<S>(value, field)?;
        Ok(ClauseValue::Enum(disc))
    }
}
fn parse_single_enum<S: SeekerSchema>(value: &str, field: &str) -> ParseResult<u32> {
    if let Ok(n) = value.parse::<u32>() {
        return Ok(n);
    }
    if let Some(disc) = S::resolve_enum_variant(field, value) {
        return Ok(disc);
    }
    Err(ParseError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
        expected: SeekType::Enum,
        reason: "expected numeric discriminant or variant name".to_string(),
    })
}
fn parse_bool(value: &str, field: &str) -> ParseResult<ClauseValue> {
    let lower = value.to_lowercase();
    match lower.as_str() {
        "true" | "1" | "yes" | "on" => Ok(ClauseValue::Bool(true)),
        "false" | "0" | "no" | "off" => Ok(ClauseValue::Bool(false)),
        _ => Err(ParseError::InvalidValue {
            field: field.to_string(),
            value: value.to_string(),
            expected: SeekType::Bool,
            reason: "expected true/false, 1/0, yes/no, or on/off".to_string(),
        }),
    }
}
pub fn parse_ordering(value: &str) -> ParseResult<OrderBy> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.is_empty() {
        return Err(ParseError::InvalidOrdering {
            value: value.to_string(),
            reason: "empty ordering specification".to_string(),
        });
    }
    let last = parts.last().unwrap().to_lowercase();
    let (field, dir) = if last == "asc" {
        (parts[..parts.len() - 1].join("-"), Dir::Asc)
    } else if last == "desc" {
        (parts[..parts.len() - 1].join("-"), Dir::Desc)
    } else {
        (value.to_string(), Dir::Asc)
    };
    if field.is_empty() {
        return Err(ParseError::InvalidOrdering {
            value: value.to_string(),
            reason: "missing field name".to_string(),
        });
    }
    Ok(OrderBy { field, dir })
}
pub fn parse_query<S: SeekerSchema>(
    pairs: impl IntoIterator<Item = (String, String)>,
) -> ParseResult<Query> {
    let mut query = Query::new();
    let mut current_group = ClauseGroup::And;
    for (key, value) in pairs {
        let key_upper = key.to_uppercase();
        match key_upper.as_str() {
            "AND" => {
                current_group = ClauseGroup::And;
                continue;
            }
            "OR" => {
                current_group = ClauseGroup::Or;
                continue;
            }
            "NOT" => {
                current_group = ClauseGroup::Not;
                continue;
            }
            _ => {}
        }
        let key_lower = key.to_lowercase();
        match key_lower.as_str() {
            "order" | "orderby" | "order-by" | "sort" => {
                let order = parse_ordering(&value)?;
                query = query.order_by(&order.field, order.dir);
                continue;
            }
            "limit" => {
                let n: usize = value.parse().map_err(|_| ParseError::InvalidLimit {
                    key: "limit".to_string(),
                    value: value.clone(),
                })?;
                query = query.limit(n);
                continue;
            }
            "offset" | "skip" => {
                let n: usize = value.parse().map_err(|_| ParseError::InvalidLimit {
                    key: "offset".to_string(),
                    value: value.clone(),
                })?;
                query = query.offset(n);
                continue;
            }
            _ => {}
        }
        let (field, parsed_op) = parse_key(&key);
        let field_type = S::field_type(&field).ok_or_else(|| ParseError::UnknownField {
            field: field.clone(),
            available: S::field_names().iter().map(|s| s.to_string()).collect(),
        })?;
        let op = parsed_op.unwrap_or_else(|| field_type.default_operator());
        if !field_type.is_valid_operator(op) {
            return Err(ParseError::InvalidOperator {
                field: field.clone(),
                operator: op.to_string(),
                field_type,
            });
        }
        let value = if value.is_empty() && field_type == SeekType::Bool {
            "true".to_string()
        } else {
            value
        };
        let clause_value = parse_value::<S>(&value, &field, field_type, op)?;
        query = match current_group {
            ClauseGroup::And => query.and(&field, op, clause_value),
            ClauseGroup::Or => query.or(&field, op, clause_value),
            ClauseGroup::Not => query.not(&field, op, clause_value),
        };
    }
    Ok(query.build())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct TestTask;
    impl SeekerSchema for TestTask {
        fn field_type(field: &str) -> Option<SeekType> {
            match field {
                "name" => Some(SeekType::String),
                "priority" => Some(SeekType::Number),
                "created-at" => Some(SeekType::Timestamp),
                "status" => Some(SeekType::Enum),
                "done" => Some(SeekType::Bool),
                _ => None,
            }
        }
        fn field_names() -> &'static [&'static str] {
            &["name", "priority", "created-at", "status", "done"]
        }
        fn resolve_enum_variant(field: &str, variant: &str) -> Option<u32> {
            if field == "status" {
                match variant.to_lowercase().as_str() {
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
    fn test_parse_operator_basic() {
        assert_eq!(parse_operator("eq"), Some(Op::Eq));
        assert_eq!(parse_operator("ne"), Some(Op::Ne));
        assert_eq!(parse_operator("gt"), Some(Op::Gt));
        assert_eq!(parse_operator("gte"), Some(Op::Gte));
        assert_eq!(parse_operator("lt"), Some(Op::Lt));
        assert_eq!(parse_operator("lte"), Some(Op::Lte));
    }
    #[test]
    fn test_parse_operator_string_ops() {
        assert_eq!(parse_operator("startswith"), Some(Op::StartsWith));
        assert_eq!(parse_operator("endswith"), Some(Op::EndsWith));
        assert_eq!(parse_operator("contains"), Some(Op::Contains));
        assert_eq!(parse_operator("regex"), Some(Op::Regex));
    }
    #[test]
    fn test_parse_operator_aliases() {
        assert_eq!(parse_operator("neq"), Some(Op::Ne));
        assert_eq!(parse_operator("prefix"), Some(Op::StartsWith));
        assert_eq!(parse_operator("suffix"), Some(Op::EndsWith));
        assert_eq!(parse_operator("re"), Some(Op::Regex));
        assert_eq!(parse_operator("match"), Some(Op::Regex));
    }
    #[test]
    fn test_parse_operator_case_insensitive() {
        assert_eq!(parse_operator("EQ"), Some(Op::Eq));
        assert_eq!(parse_operator("Contains"), Some(Op::Contains));
        assert_eq!(parse_operator("BEFORE"), Some(Op::Before));
    }
    #[test]
    fn test_parse_operator_unknown() {
        assert_eq!(parse_operator("unknown"), None);
        assert_eq!(parse_operator("equals"), None);
        assert_eq!(parse_operator(""), None);
    }
    #[test]
    fn test_parse_key_with_operator() {
        let (field, op) = parse_key("name-contains");
        assert_eq!(field, "name");
        assert_eq!(op, Some(Op::Contains));
    }
    #[test]
    fn test_parse_key_compound_field() {
        let (field, op) = parse_key("created-at-before");
        assert_eq!(field, "created-at");
        assert_eq!(op, Some(Op::Before));
    }
    #[test]
    fn test_parse_key_no_operator() {
        let (field, op) = parse_key("name");
        assert_eq!(field, "name");
        assert_eq!(op, None);
    }
    #[test]
    fn test_parse_key_field_looks_like_op_but_isnt() {
        let (field, op) = parse_key("name-equal");
        assert_eq!(field, "name-equal");
        assert_eq!(op, None);
    }
    #[test]
    fn test_parse_number_integer() {
        let val = parse_value::<TestTask>("42", "priority", SeekType::Number, Op::Eq).unwrap();
        assert!(matches!(val, ClauseValue::Number(Number::I64(42))));
    }
    #[test]
    fn test_parse_number_negative() {
        let val = parse_value::<TestTask>("-17", "priority", SeekType::Number, Op::Eq).unwrap();
        assert!(matches!(val, ClauseValue::Number(Number::I64(-17))));
    }
    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_number_float() {
        let val = parse_value::<TestTask>("3.14", "priority", SeekType::Number, Op::Eq).unwrap();
        if let ClauseValue::Number(Number::F64(n)) = val {
            assert!((n - 3.14).abs() < 0.001);
        } else {
            panic!("Expected Number::F64");
        }
    }
    #[test]
    fn test_parse_number_invalid() {
        let result = parse_value::<TestTask>("abc", "priority", SeekType::Number, Op::Eq);
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }
    #[test]
    fn test_parse_timestamp_unix_ms() {
        let val =
            parse_value::<TestTask>("1705312200000", "created-at", SeekType::Timestamp, Op::Eq)
                .unwrap();
        assert!(matches!(
            val,
            ClauseValue::Timestamp(Timestamp(1705312200000))
        ));
    }
    #[test]
    fn test_parse_timestamp_date_only() {
        let val = parse_value::<TestTask>("2024-01-15", "created-at", SeekType::Timestamp, Op::Eq)
            .unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }
    #[test]
    fn test_parse_timestamp_datetime() {
        let val = parse_value::<TestTask>(
            "2024-01-15T10:30:00Z",
            "created-at",
            SeekType::Timestamp,
            Op::Eq,
        )
        .unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }
    #[test]
    fn test_parse_timestamp_year_only() {
        let val =
            parse_value::<TestTask>("2024", "created-at", SeekType::Timestamp, Op::Eq).unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }
    #[test]
    fn test_parse_timestamp_invalid() {
        let result =
            parse_value::<TestTask>("not-a-date", "created-at", SeekType::Timestamp, Op::Eq);
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }
    #[test]
    fn test_parse_enum_numeric() {
        let val = parse_value::<TestTask>("1", "status", SeekType::Enum, Op::Eq).unwrap();
        assert!(matches!(val, ClauseValue::Enum(1)));
    }
    #[test]
    fn test_parse_enum_variant_name() {
        let val = parse_value::<TestTask>("active", "status", SeekType::Enum, Op::Eq).unwrap();
        assert!(matches!(val, ClauseValue::Enum(1)));
    }
    #[test]
    fn test_parse_enum_in_operator() {
        let val =
            parse_value::<TestTask>("pending,active", "status", SeekType::Enum, Op::In).unwrap();
        if let ClauseValue::EnumSet(set) = val {
            assert_eq!(set, vec![0, 1]);
        } else {
            panic!("Expected EnumSet");
        }
    }
    #[test]
    fn test_parse_enum_in_with_spaces() {
        let val =
            parse_value::<TestTask>("pending, active, done", "status", SeekType::Enum, Op::In)
                .unwrap();
        if let ClauseValue::EnumSet(set) = val {
            assert_eq!(set, vec![0, 1, 2]);
        } else {
            panic!("Expected EnumSet");
        }
    }
    #[test]
    fn test_parse_enum_unknown_variant() {
        let result = parse_value::<TestTask>("unknown", "status", SeekType::Enum, Op::Eq);
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }
    #[test]
    fn test_parse_bool_true_variants() {
        for s in &["true", "TRUE", "True", "1", "yes", "YES", "on", "ON"] {
            let val = parse_value::<TestTask>(s, "done", SeekType::Bool, Op::Eq).unwrap();
            assert!(matches!(val, ClauseValue::Bool(true)), "Failed for: {}", s);
        }
    }
    #[test]
    fn test_parse_bool_false_variants() {
        for s in &["false", "FALSE", "False", "0", "no", "NO", "off", "OFF"] {
            let val = parse_value::<TestTask>(s, "done", SeekType::Bool, Op::Eq).unwrap();
            assert!(matches!(val, ClauseValue::Bool(false)), "Failed for: {}", s);
        }
    }
    #[test]
    fn test_parse_bool_invalid() {
        let result = parse_value::<TestTask>("maybe", "done", SeekType::Bool, Op::Eq);
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }
    #[test]
    fn test_parse_string_basic() {
        let val = parse_value::<TestTask>("hello world", "name", SeekType::String, Op::Eq).unwrap();
        if let ClauseValue::String(s) = val {
            assert_eq!(s, "hello world");
        } else {
            panic!("Expected String");
        }
    }
    #[test]
    fn test_parse_string_regex() {
        let val = parse_value::<TestTask>("^test.*$", "name", SeekType::String, Op::Regex).unwrap();
        assert!(matches!(val, ClauseValue::Regex(_)));
    }
    #[test]
    fn test_parse_string_invalid_regex() {
        let result = parse_value::<TestTask>("(unclosed", "name", SeekType::String, Op::Regex);
        assert!(matches!(result, Err(ParseError::InvalidRegex { .. })));
    }
    #[test]
    fn test_parse_ordering_default_asc() {
        let order = parse_ordering("name").unwrap();
        assert_eq!(order.field, "name");
        assert_eq!(order.dir, Dir::Asc);
    }
    #[test]
    fn test_parse_ordering_explicit_asc() {
        let order = parse_ordering("name-asc").unwrap();
        assert_eq!(order.field, "name");
        assert_eq!(order.dir, Dir::Asc);
    }
    #[test]
    fn test_parse_ordering_desc() {
        let order = parse_ordering("priority-desc").unwrap();
        assert_eq!(order.field, "priority");
        assert_eq!(order.dir, Dir::Desc);
    }
    #[test]
    fn test_parse_ordering_compound_field() {
        let order = parse_ordering("created-at-desc").unwrap();
        assert_eq!(order.field, "created-at");
        assert_eq!(order.dir, Dir::Desc);
    }
    #[test]
    fn test_parse_ordering_empty() {
        let result = parse_ordering("");
        assert!(matches!(result, Err(ParseError::InvalidOrdering { .. })));
    }
    #[test]
    fn test_parse_ordering_just_direction() {
        let result = parse_ordering("desc");
        assert!(matches!(result, Err(ParseError::InvalidOrdering { .. })));
    }
    #[test]
    fn test_parse_query_simple() {
        let pairs = vec![("name-eq".to_string(), "test".to_string())];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_default_operator() {
        let pairs = vec![("name".to_string(), "test".to_string())];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_multiple_clauses() {
        let pairs = vec![
            ("name-contains".to_string(), "test".to_string()),
            ("priority-gte".to_string(), "5".to_string()),
        ];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_group_markers() {
        let pairs = vec![
            ("name-contains".to_string(), "a".to_string()),
            ("OR".to_string(), "".to_string()),
            ("name-contains".to_string(), "b".to_string()),
            ("NOT".to_string(), "".to_string()),
            ("done".to_string(), "true".to_string()),
        ];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_ordering() {
        let pairs = vec![
            ("name-contains".to_string(), "test".to_string()),
            ("order".to_string(), "priority-desc".to_string()),
        ];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_limit_offset() {
        let pairs = vec![
            ("name-contains".to_string(), "test".to_string()),
            ("limit".to_string(), "10".to_string()),
            ("offset".to_string(), "5".to_string()),
        ];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_bool_bare_flag() {
        let pairs = vec![("done".to_string(), "".to_string())];
        let query = parse_query::<TestTask>(pairs).unwrap();
        assert!(query.count(&Vec::<()>::new(), |_, _| crate::Value::None) == 0);
    }
    #[test]
    fn test_parse_query_unknown_field() {
        let pairs = vec![("unknown-field".to_string(), "test".to_string())];
        let result = parse_query::<TestTask>(pairs);
        assert!(matches!(result, Err(ParseError::UnknownField { .. })));
    }
    #[test]
    fn test_parse_query_invalid_operator() {
        let pairs = vec![("name-gt".to_string(), "test".to_string())];
        let result = parse_query::<TestTask>(pairs);
        assert!(matches!(result, Err(ParseError::InvalidOperator { .. })));
    }
    #[test]
    fn test_parse_query_invalid_limit() {
        let pairs = vec![("limit".to_string(), "abc".to_string())];
        let result = parse_query::<TestTask>(pairs);
        assert!(matches!(result, Err(ParseError::InvalidLimit { .. })));
    }
    #[test]
    fn test_is_leap_year() {
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
    }
    #[test]
    fn test_days_from_year() {
        assert_eq!(days_from_year(1970), 0);
        assert_eq!(days_from_year(1971), 365);
        assert_eq!(days_from_year(1972), 365 * 2);
    }
    #[test]
    fn test_days_from_ymd_epoch() {
        assert_eq!(days_from_ymd(1970, 1, 1), Some(0));
    }
    #[test]
    fn test_days_from_ymd_next_day() {
        assert_eq!(days_from_ymd(1970, 1, 2), Some(1));
    }
    #[test]
    fn test_days_from_ymd_invalid() {
        assert_eq!(days_from_ymd(2024, 2, 30), None);
        assert_eq!(days_from_ymd(2024, 13, 1), None);
    }
}
