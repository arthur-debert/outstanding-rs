//! Additional tests to improve code coverage.

use standout_seeker::{Clause, ClauseValue, Dir, Number, Op, OrderBy, Query, Timestamp, Value};

// ============================================================================
// Value type coverage
// ============================================================================

#[test]
fn value_is_checks() {
    assert!(Value::String("test").is_string());
    assert!(!Value::String("test").is_number());
    assert!(!Value::String("test").is_timestamp());
    assert!(!Value::String("test").is_enum());
    assert!(!Value::String("test").is_bool());
    assert!(!Value::String("test").is_none());

    assert!(Value::Number(Number::I64(42)).is_number());
    assert!(Value::Timestamp(Timestamp(1000)).is_timestamp());
    assert!(Value::Enum(1).is_enum());
    assert!(Value::Bool(true).is_bool());
    assert!(Value::None.is_none());
}

#[test]
fn value_as_extractors() {
    assert_eq!(Value::String("hello").as_str(), Some("hello"));
    assert_eq!(Value::String("hello").as_number(), None);

    assert_eq!(
        Value::Number(Number::I64(42)).as_number(),
        Some(Number::I64(42))
    );
    assert_eq!(Value::Number(Number::I64(42)).as_str(), None);

    assert_eq!(
        Value::Timestamp(Timestamp(1000)).as_timestamp(),
        Some(Timestamp(1000))
    );
    assert_eq!(Value::Timestamp(Timestamp(1000)).as_enum(), None);

    assert_eq!(Value::Enum(5).as_enum(), Some(5));
    assert_eq!(Value::Enum(5).as_bool(), None);

    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert_eq!(Value::Bool(true).as_timestamp(), None);

    assert_eq!(Value::None.as_str(), None);
    assert_eq!(Value::None.as_number(), None);
    assert_eq!(Value::None.as_timestamp(), None);
    assert_eq!(Value::None.as_enum(), None);
    assert_eq!(Value::None.as_bool(), None);
}

#[test]
fn number_to_f64() {
    assert_eq!(Number::I64(42).to_f64(), 42.0);
    assert_eq!(Number::U64(42).to_f64(), 42.0);
    assert_eq!(Number::F64(42.5).to_f64(), 42.5);
}

#[test]
fn number_from_conversions() {
    let _: Number = 42i8.into();
    let _: Number = 42i16.into();
    let _: Number = 42i32.into();
    let _: Number = 42i64.into();
    let _: Number = 42u8.into();
    let _: Number = 42u16.into();
    let _: Number = 42u32.into();
    let _: Number = 42u64.into();
    let _: Number = 42f32.into();
    let _: Number = 42f64.into();
    let _: Number = 42usize.into();
    let _: Number = 42isize.into();
}

#[test]
fn timestamp_methods() {
    let ts = Timestamp::from_secs(100);
    assert_eq!(ts.as_millis(), 100_000);
    assert_eq!(ts.as_secs(), 100);

    let ts2 = Timestamp::from_millis(5000);
    assert_eq!(ts2.as_millis(), 5000);
    assert_eq!(ts2.as_secs(), 5);
}

// ============================================================================
// Operator coverage
// ============================================================================

#[test]
fn op_type_checks_comprehensive() {
    // String ops
    assert!(Op::Eq.is_string_op());
    assert!(Op::Ne.is_string_op());
    assert!(Op::StartsWith.is_string_op());
    assert!(Op::EndsWith.is_string_op());
    assert!(Op::Contains.is_string_op());
    assert!(Op::Regex.is_string_op());
    assert!(!Op::Gt.is_string_op());
    assert!(!Op::In.is_string_op());

    // Number ops
    assert!(Op::Eq.is_number_op());
    assert!(Op::Ne.is_number_op());
    assert!(Op::Gt.is_number_op());
    assert!(Op::Gte.is_number_op());
    assert!(Op::Lt.is_number_op());
    assert!(Op::Lte.is_number_op());
    assert!(!Op::Contains.is_number_op());
    assert!(!Op::In.is_number_op());

    // Timestamp ops
    assert!(Op::Eq.is_timestamp_op());
    assert!(Op::Ne.is_timestamp_op());
    assert!(Op::Gt.is_timestamp_op());
    assert!(Op::Gte.is_timestamp_op());
    assert!(Op::Lt.is_timestamp_op());
    assert!(Op::Lte.is_timestamp_op());
    assert!(Op::Before.is_timestamp_op());
    assert!(Op::After.is_timestamp_op());
    assert!(!Op::Contains.is_timestamp_op());

    // Enum ops
    assert!(Op::Eq.is_enum_op());
    assert!(Op::Ne.is_enum_op());
    assert!(Op::In.is_enum_op());
    assert!(!Op::Gt.is_enum_op());
    assert!(!Op::Contains.is_enum_op());

    // Bool ops
    assert!(Op::Eq.is_bool_op());
    assert!(Op::Ne.is_bool_op());
    assert!(Op::Is.is_bool_op());
    assert!(!Op::Gt.is_bool_op());
    assert!(!Op::Contains.is_bool_op());
}

#[test]
fn op_as_str() {
    assert_eq!(Op::Eq.as_str(), "eq");
    assert_eq!(Op::Ne.as_str(), "ne");
    assert_eq!(Op::StartsWith.as_str(), "startswith");
    assert_eq!(Op::EndsWith.as_str(), "endswith");
    assert_eq!(Op::Contains.as_str(), "contains");
    assert_eq!(Op::Regex.as_str(), "regex");
    assert_eq!(Op::Gt.as_str(), "gt");
    assert_eq!(Op::Gte.as_str(), "gte");
    assert_eq!(Op::Lt.as_str(), "lt");
    assert_eq!(Op::Lte.as_str(), "lte");
    assert_eq!(Op::Before.as_str(), "before");
    assert_eq!(Op::After.as_str(), "after");
    assert_eq!(Op::In.as_str(), "in");
    assert_eq!(Op::Is.as_str(), "is");
}

// ============================================================================
// Ordering coverage
// ============================================================================

#[test]
fn dir_is_checks() {
    assert!(Dir::Asc.is_asc());
    assert!(!Dir::Asc.is_desc());
    assert!(!Dir::Desc.is_asc());
    assert!(Dir::Desc.is_desc());
}

#[test]
fn dir_default() {
    assert_eq!(Dir::default(), Dir::Asc);
}

#[test]
fn order_by_compare_with_none() {
    let order = OrderBy::asc("field");
    let a = Value::String("test");
    let b = Value::None;

    // None values sort last
    let result = order.compare(&a, &b);
    assert_eq!(result, Some(std::cmp::Ordering::Less));

    let result2 = order.compare(&b, &a);
    assert_eq!(result2, Some(std::cmp::Ordering::Greater));
}

// ============================================================================
// ClauseValue conversions
// ============================================================================

#[test]
fn clause_value_from_conversions() {
    let _: ClauseValue = "test".into();
    let _: ClauseValue = String::from("test").into();
    let _: ClauseValue = 42i8.into();
    let _: ClauseValue = 42i16.into();
    let _: ClauseValue = 42i32.into();
    let _: ClauseValue = 42i64.into();
    let _: ClauseValue = 42u8.into();
    let _: ClauseValue = 42u16.into();
    let _: ClauseValue = 42u32.into();
    let _: ClauseValue = 42u64.into();
    let _: ClauseValue = 42f32.into();
    let _: ClauseValue = 42f64.into();
    let _: ClauseValue = 42usize.into();
    let _: ClauseValue = 42isize.into();
    let _: ClauseValue = true.into();
    let _: ClauseValue = Timestamp(1000).into();
    let _: ClauseValue = vec![1u32, 2, 3].into();
    let _: ClauseValue = Number::I64(42).into();
    let _: ClauseValue = regex::Regex::new("test").unwrap().into();
}

// ============================================================================
// Query shorthand methods coverage
// ============================================================================

fn accessor<'a>(item: &'a TestItem, field: &str) -> Value<'a> {
    match field {
        "name" => Value::String(&item.name),
        "value" => Value::Number(Number::I64(item.value)),
        "active" => Value::Bool(item.active),
        "status" => Value::Enum(item.status),
        "created" => Value::Timestamp(Timestamp(item.created)),
        _ => Value::None,
    }
}

#[derive(Debug, Clone)]
struct TestItem {
    name: String,
    value: i64,
    active: bool,
    status: u32,
    created: i64,
}

fn sample_items() -> Vec<TestItem> {
    vec![
        TestItem {
            name: "alpha".to_string(),
            value: 10,
            active: true,
            status: 1,
            created: 1000,
        },
        TestItem {
            name: "beta".to_string(),
            value: 20,
            active: false,
            status: 2,
            created: 2000,
        },
        TestItem {
            name: "gamma".to_string(),
            value: 30,
            active: true,
            status: 1,
            created: 3000,
        },
    ]
}

#[test]
fn query_or_shorthand_methods() {
    let items = sample_items();

    // or_ne
    let q = Query::new().or_ne("value", 10i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // or_gt
    let q = Query::new().or_gt("value", 25i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "gamma");

    // or_gte
    let q = Query::new().or_gte("value", 30i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // or_lt
    let q = Query::new().or_lt("value", 15i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "alpha");

    // or_lte
    let q = Query::new().or_lte("value", 10i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // or_startswith
    let q = Query::new().or_startswith("name", "al").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // or_endswith
    let q = Query::new().or_endswith("name", "ta").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "beta");

    // or_regex
    let q = Query::new().or_regex("name", "^[ab]").unwrap().build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // or_in
    let q = Query::new().or_in("status", [2u32]).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // or_before
    let q = Query::new().or_before("created", Timestamp(1500)).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // or_after
    let q = Query::new().or_after("created", Timestamp(2500)).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
}

#[test]
fn query_not_shorthand_methods() {
    let items = sample_items();

    // not_ne
    let q = Query::new().not_ne("value", 10i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "alpha");

    // not_gt
    let q = Query::new().not_gt("value", 15i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "alpha");

    // not_gte
    let q = Query::new().not_gte("value", 20i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);

    // not_lt
    let q = Query::new().not_lt("value", 15i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // not_lte
    let q = Query::new().not_lte("value", 10i64).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // not_contains
    let q = Query::new().not_contains("name", "a").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 0); // all have 'a'

    // not_startswith
    let q = Query::new().not_startswith("name", "a").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // not_endswith - all items end with "a", so filter for not ending with "ha"
    let q = Query::new().not_endswith("name", "ha").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2); // beta and gamma don't end with "ha"

    // not_regex
    let q = Query::new().not_regex("name", "^a").unwrap().build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 2);

    // not_in
    let q = Query::new().not_in("status", [1u32]).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "beta");

    // not_before
    let q = Query::new().not_before("created", Timestamp(2500)).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "gamma");

    // not_after
    let q = Query::new().not_after("created", Timestamp(1500)).build();
    let results = q.filter(&items, accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "alpha");
}

#[test]
fn query_ordering_shortcuts() {
    let items = sample_items();

    let q = Query::new().order_asc("value").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results[0].value, 10);
    assert_eq!(results[2].value, 30);

    let q = Query::new().order_desc("value").build();
    let results = q.filter(&items, accessor);
    assert_eq!(results[0].value, 30);
    assert_eq!(results[2].value, 10);
}

#[test]
fn query_position() {
    let items = sample_items();

    let q = Query::new().and_eq("name", "beta").build();
    let pos = q.position(&items, accessor);
    assert_eq!(pos, Some(1));

    let q = Query::new().and_eq("name", "nonexistent").build();
    let pos = q.position(&items, accessor);
    assert_eq!(pos, None);
}

// ============================================================================
// Clause edge cases
// ============================================================================

#[test]
fn clause_invalid_operator_for_type() {
    // String clause with a number operator - should not match
    let clause = Clause::new("field", Op::Gt, "test");
    let result = clause.matches(&Value::String("zzz"));
    // Gt is not a valid string op, so it returns false
    assert!(!result);
}

#[test]
fn clause_enum_with_non_in_operator() {
    // Enum clause with contains operator - invalid, should not match
    let clause = Clause::new("field", Op::Contains, ClauseValue::Enum(1));
    let result = clause.matches(&Value::Enum(1));
    assert!(!result);
}

#[test]
fn clause_enum_set_with_non_in_operator() {
    // EnumSet with Eq operator - invalid, should not match
    let clause = Clause::new("field", Op::Eq, vec![1u32, 2]);
    let result = clause.matches(&Value::Enum(1));
    assert!(!result);
}
