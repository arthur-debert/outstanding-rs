use proptest::prelude::*;
use standout_seeker::{Dir, Number, Op, Query, Timestamp, Value};
fn number_accessor<'a>(n: &'a i64, _field: &str) -> Value<'a> {
    Value::Number(Number::I64(*n))
}
#[allow(clippy::ptr_arg)]
fn string_accessor<'a>(s: &'a String, _field: &str) -> Value<'a> {
    Value::String(s)
}
#[derive(Debug, Clone)]
struct TestItem {
    value: i64,
    name: String,
    active: bool,
}
fn item_accessor<'a>(item: &'a TestItem, field: &str) -> Value<'a> {
    match field {
        "value" => Value::Number(Number::I64(item.value)),
        "name" => Value::String(&item.name),
        "active" => Value::Bool(item.active),
        _ => Value::None,
    }
}
fn test_item_strategy() -> impl Strategy<Value = TestItem> {
    (any::<i64>(), "[a-z]{1,10}", any::<bool>()).prop_map(|(value, name, active)| TestItem {
        value,
        name,
        active,
    })
}
proptest! {
    #[test]
    fn filter_never_grows_collection(
        items in prop::collection::vec(any::<i64>(), 0..100),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gt, threshold)
            .build();
        let results = query.filter(&items, number_accessor);
        prop_assert!(results.len() <= items.len());
    }
    #[test]
    fn count_equals_filter_len(
        items in prop::collection::vec(any::<i64>(), 0..100),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gte, threshold)
            .build();
        let filtered = query.filter(&items, number_accessor);
        let counted = query.count(&items, number_accessor);
        prop_assert_eq!(filtered.len(), counted);
    }
    #[test]
    fn empty_query_matches_all(
        items in prop::collection::vec("[a-z]{1,20}".prop_map(String::from), 0..50),
    ) {
        let query = Query::new().build();
        let results = query.filter(&items, string_accessor);
        prop_assert_eq!(results.len(), items.len());
    }
    #[test]
    fn limit_respects_bound(
        items in prop::collection::vec(any::<i64>(), 0..100),
        limit in 1usize..50,
    ) {
        let query = Query::new()
            .limit(limit)
            .build();
        let results = query.filter(&items, number_accessor);
        prop_assert!(results.len() <= limit);
        prop_assert!(results.len() <= items.len());
    }
    #[test]
    fn offset_and_limit_work_together(
        items in prop::collection::vec(any::<i64>(), 0..100),
        offset in 0usize..50,
        limit in 1usize..50,
    ) {
        let query = Query::new()
            .offset(offset)
            .limit(limit)
            .build();
        let results = query.filter(&items, number_accessor);
        prop_assert!(results.len() <= limit);
        let available = items.len().saturating_sub(offset);
        prop_assert!(results.len() <= available);
    }
    #[test]
    fn any_consistent_with_filter(
        items in prop::collection::vec(any::<i64>(), 0..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Lt, threshold)
            .build();
        let has_any = query.any(&items, number_accessor);
        let filtered = query.filter(&items, number_accessor);
        prop_assert_eq!(has_any, !filtered.is_empty());
    }
    #[test]
    fn all_consistent_with_filter(
        items in prop::collection::vec(any::<i64>(), 0..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Lte, threshold)
            .build();
        let all_match = query.all(&items, number_accessor);
        let filtered = query.filter(&items, number_accessor);
        prop_assert_eq!(all_match, filtered.len() == items.len());
    }
    #[test]
    fn find_consistent_with_filter(
        items in prop::collection::vec(any::<i64>(), 0..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Eq, threshold)
            .build();
        let found = query.find(&items, number_accessor);
        let filtered = query.filter(&items, number_accessor);
        match (found, filtered.first()) {
            (Some(f), Some(&fi)) => prop_assert_eq!(f, fi),
            (None, None) => {}
            _ => prop_assert!(false, "find and filter().first() disagree"),
        }
    }
    #[test]
    fn not_excludes_matching(
        items in prop::collection::vec(test_item_strategy(), 1..50),
    ) {
        let query = Query::new()
            .not_eq("active", true)
            .build();
        let results = query.filter(&items, item_accessor);
        for item in results {
            prop_assert!(!item.active);
        }
    }
    #[test]
    fn and_all_satisfied(
        items in prop::collection::vec(test_item_strategy(), 1..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gte, threshold)
            .and("active", Op::Eq, true)
            .build();
        let results = query.filter(&items, item_accessor);
        for item in results {
            prop_assert!(item.value >= threshold);
            prop_assert!(item.active);
        }
    }
    #[test]
    fn or_at_least_one_satisfied(
        items in prop::collection::vec(test_item_strategy(), 1..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .or("value", Op::Lt, threshold)
            .or("active", Op::Eq, true)
            .build();
        let results = query.filter(&items, item_accessor);
        for item in results {
            let value_matches = item.value < threshold;
            let active_matches = item.active;
            prop_assert!(value_matches || active_matches);
        }
    }
    #[test]
    fn ordering_is_stable(
        base_items in prop::collection::vec((0i64..10, "[a-z]{3}".prop_map(String::from)), 5..20),
    ) {
        let items: Vec<TestItem> = base_items
            .into_iter()
            .map(|(value, name)| TestItem {
                value,
                name,
                active: true,
            })
            .collect();
        let query = Query::new()
            .order_asc("value")
            .build();
        let results = query.filter(&items, item_accessor);
        for i in 1..results.len() {
            let prev = results[i - 1];
            let curr = results[i];
            if prev.value == curr.value {
                let prev_pos = items.iter().position(|x| std::ptr::eq(x, prev));
                let curr_pos = items.iter().position(|x| std::ptr::eq(x, curr));
                if let (Some(pp), Some(cp)) = (prev_pos, curr_pos) {
                    prop_assert!(pp < cp, "Stable sort violated: equal items reordered");
                }
            } else {
                prop_assert!(prev.value <= curr.value, "Sort order violated");
            }
        }
    }
    #[test]
    fn filter_cloned_matches_filter(
        items in prop::collection::vec(test_item_strategy(), 0..50),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gt, threshold)
            .build();
        let refs = query.filter(&items, item_accessor);
        let cloned = query.filter_cloned(&items, item_accessor);
        prop_assert_eq!(refs.len(), cloned.len());
        for (r, c) in refs.iter().zip(cloned.iter()) {
            prop_assert_eq!(r.value, c.value);
            prop_assert_eq!(&r.name, &c.name);
            prop_assert_eq!(r.active, c.active);
        }
    }
    #[test]
    fn timestamp_ordering_correct(
        timestamps in prop::collection::vec(any::<i64>(), 1..50),
        threshold in any::<i64>(),
    ) {
        fn ts_accessor<'a>(ts: &'a i64, _field: &str) -> Value<'a> {
            Value::Timestamp(Timestamp(*ts))
        }
        let query_before = Query::new()
            .and("ts", Op::Before, Timestamp(threshold))
            .build();
        let query_after = Query::new()
            .and("ts", Op::After, Timestamp(threshold))
            .build();
        let before_results = query_before.filter(&timestamps, ts_accessor);
        let after_results = query_after.filter(&timestamps, ts_accessor);
        for ts in before_results {
            prop_assert!(*ts < threshold);
        }
        for ts in after_results {
            prop_assert!(*ts > threshold);
        }
    }
}
#[test]
fn empty_collection_returns_empty() {
    let items: Vec<i64> = vec![];
    let query = Query::new().and("value", Op::Eq, 42i64).build();
    assert!(query.filter(&items, number_accessor).is_empty());
    assert_eq!(query.count(&items, number_accessor), 0);
    assert!(!query.any(&items, number_accessor));
    assert!(query.all(&items, number_accessor));
    assert!(query.find(&items, number_accessor).is_none());
}
#[test]
fn offset_equal_to_length_returns_empty() {
    let items = vec![1i64, 2, 3, 4, 5];
    let query = Query::new().offset(5).build();
    assert!(query.filter(&items, number_accessor).is_empty());
}
#[test]
fn offset_greater_than_length_returns_empty() {
    let items = vec![1i64, 2, 3, 4, 5];
    let query = Query::new().offset(100).build();
    assert!(query.filter(&items, number_accessor).is_empty());
}
#[test]
fn limit_zero_returns_empty() {
    let items = vec![1i64, 2, 3, 4, 5];
    let query = Query::new().limit(0).build();
    assert!(query.filter(&items, number_accessor).is_empty());
}
use standout_seeker::{
    parse_key, parse_operator, parse_ordering, parse_query, SeekType, SeekerSchema,
};
struct ParseTestSchema;
impl SeekerSchema for ParseTestSchema {
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
}
proptest! {
    #[test]
    fn parse_key_always_returns_field(key in "[a-z][a-z0-9-]{0,20}") {
        let (field, _op) = parse_key(&key);
        prop_assert!(!field.is_empty());
    }
    #[test]
    fn parse_key_with_operator_suffix(
        field in "[a-z][a-z0-9]{0,10}",
        op in prop::sample::select(vec!["eq", "ne", "gt", "gte", "lt", "lte", "contains", "startswith", "endswith"])
    ) {
        let key = format!("{}-{}", field, op);
        let (parsed_field, parsed_op) = parse_key(&key);
        prop_assert_eq!(parsed_field, field);
        prop_assert!(parsed_op.is_some());
    }
    #[test]
    fn parse_operator_case_insensitive(
        base_op in prop::sample::select(vec!["eq", "ne", "gt", "gte", "lt", "lte", "contains"])
    ) {
        let lower = parse_operator(base_op);
        let upper = parse_operator(&base_op.to_uppercase());
        prop_assert_eq!(lower, upper);
    }
    #[test]
    fn parse_number_value_succeeds(n in any::<i64>()) {
        let pairs = vec![
            ("priority-eq".to_string(), n.to_string()),
        ];
        let result = parse_query::<ParseTestSchema>(pairs);
        prop_assert!(result.is_ok());
    }
    #[test]
    fn parse_bool_value_formats(
        val in prop::sample::select(vec!["true", "false", "1", "0", "yes", "no", "on", "off"])
    ) {
        let pairs = vec![
            ("done-eq".to_string(), val.to_string()),
        ];
        let result = parse_query::<ParseTestSchema>(pairs);
        prop_assert!(result.is_ok());
    }
    #[test]
    fn parse_ordering_handles_direction(
        field in "[a-z][a-z0-9]{0,10}",
        dir in prop::sample::select(vec!["asc", "desc"])
    ) {
        let value = format!("{}-{}", field, dir);
        let result = parse_ordering(&value);
        prop_assert!(result.is_ok());
        let order = result.unwrap();
        prop_assert_eq!(order.field, field);
        if dir == "asc" {
            prop_assert_eq!(order.dir, Dir::Asc);
        } else {
            prop_assert_eq!(order.dir, Dir::Desc);
        }
    }
    #[test]
    fn group_markers_dont_break_parsing(
        groups in prop::collection::vec(
            prop::sample::select(vec!["AND", "OR", "NOT"]),
            0..5
        )
    ) {
        let mut pairs: Vec<(String, String)> = Vec::new();
        pairs.push(("name-eq".to_string(), "test".to_string()));
        for group in groups {
            pairs.push((group.to_string(), "".to_string()));
            pairs.push(("priority-eq".to_string(), "5".to_string()));
        }
        let result = parse_query::<ParseTestSchema>(pairs);
        prop_assert!(result.is_ok());
    }
    #[test]
    fn limit_offset_parsing(
        limit in 0usize..1000,
        offset in 0usize..1000
    ) {
        let pairs = vec![
            ("name-eq".to_string(), "test".to_string()),
            ("limit".to_string(), limit.to_string()),
            ("offset".to_string(), offset.to_string()),
        ];
        let result = parse_query::<ParseTestSchema>(pairs);
        prop_assert!(result.is_ok());
    }
}
#[test]
fn parse_unknown_field_fails() {
    let pairs = vec![("unknown-field".to_string(), "value".to_string())];
    let result = parse_query::<ParseTestSchema>(pairs);
    assert!(result.is_err());
}
#[test]
fn parse_invalid_operator_for_type_fails() {
    let pairs = vec![("name-gt".to_string(), "value".to_string())];
    let result = parse_query::<ParseTestSchema>(pairs);
    assert!(result.is_err());
}
#[test]
fn parse_empty_pairs_succeeds() {
    let pairs: Vec<(String, String)> = vec![];
    let result = parse_query::<ParseTestSchema>(pairs);
    assert!(result.is_ok());
}
#[test]
fn parse_only_group_markers_succeeds() {
    let pairs = vec![
        ("AND".to_string(), "".to_string()),
        ("OR".to_string(), "".to_string()),
        ("NOT".to_string(), "".to_string()),
    ];
    let result = parse_query::<ParseTestSchema>(pairs);
    assert!(result.is_ok());
}
#[test]
fn parse_compound_field_name() {
    let pairs = vec![("created-at-before".to_string(), "1000".to_string())];
    let result = parse_query::<ParseTestSchema>(pairs);
    assert!(result.is_ok());
}
