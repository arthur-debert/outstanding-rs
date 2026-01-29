//! Property-based tests for seeker using proptest.

use proptest::prelude::*;
use standout_seeker::{Number, Op, Query, Timestamp, Value};

// ============================================================================
// Test helpers
// ============================================================================

fn number_accessor<'a>(n: &'a i64, _field: &str) -> Value<'a> {
    Value::Number(Number::I64(*n))
}

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

// Strategy to generate test items
fn test_item_strategy() -> impl Strategy<Value = TestItem> {
    (any::<i64>(), "[a-z]{1,10}", any::<bool>()).prop_map(|(value, name, active)| TestItem {
        value,
        name,
        active,
    })
}

// ============================================================================
// Property tests
// ============================================================================

proptest! {
    /// Filter should never return more items than the input.
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

    /// Count should equal the length of filtered results.
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

    /// Empty query should match all items.
    #[test]
    fn empty_query_matches_all(
        items in prop::collection::vec("[a-z]{1,20}".prop_map(String::from), 0..50),
    ) {
        let query = Query::new().build();
        let results = query.filter(&items, string_accessor);
        prop_assert_eq!(results.len(), items.len());
    }

    /// Limit should be respected.
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

    /// Offset + limit should work correctly.
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

        // Results should not exceed limit
        prop_assert!(results.len() <= limit);

        // Results should not exceed items available after offset
        let available = items.len().saturating_sub(offset);
        prop_assert!(results.len() <= available);
    }

    /// any() should return true iff filter returns non-empty.
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

    /// all() should return true iff filter returns all items.
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

    /// find() should return the same as filter().first().
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

    /// NOT clause should exclude items that match the clause.
    #[test]
    fn not_excludes_matching(
        items in prop::collection::vec(test_item_strategy(), 1..50),
    ) {
        // Create a NOT query that excludes active items
        let query = Query::new()
            .not_eq("active", true)
            .build();

        let results = query.filter(&items, item_accessor);

        // All results should have active == false
        for item in results {
            prop_assert!(!item.active);
        }
    }

    /// AND clauses should all be satisfied.
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

    /// OR clauses: at least one should be satisfied.
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

    /// Ordering should be stable (equal items maintain original order).
    #[test]
    fn ordering_is_stable(
        base_items in prop::collection::vec((0i64..10, "[a-z]{3}".prop_map(String::from)), 5..20),
    ) {
        // Create items with some duplicate values
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

        // Check that items with equal values maintain their relative order
        for i in 1..results.len() {
            let prev = results[i - 1];
            let curr = results[i];

            if prev.value == curr.value {
                // Find original positions
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

    /// filter_cloned should return owned copies that equal the filtered refs.
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

    /// Timestamp before/after operators.
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

// ============================================================================
// Additional edge case tests
// ============================================================================

#[test]
fn empty_collection_returns_empty() {
    let items: Vec<i64> = vec![];
    let query = Query::new().and("value", Op::Eq, 42i64).build();

    assert!(query.filter(&items, number_accessor).is_empty());
    assert_eq!(query.count(&items, number_accessor), 0);
    assert!(!query.any(&items, number_accessor));
    assert!(query.all(&items, number_accessor)); // vacuously true
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
