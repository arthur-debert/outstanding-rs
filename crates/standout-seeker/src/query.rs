use crate::clause::{Clause, ClauseValue};
use crate::error::Result;
use crate::op::Op;
use crate::ordering::{compare_by_orderings, Dir, OrderBy};
use crate::value::{Timestamp, Value};
use regex::Regex;
#[derive(Debug, Clone, Default)]
pub struct Query {
    and_clauses: Vec<Clause>,
    or_clauses: Vec<Clause>,
    not_clauses: Vec<Clause>,
    orderings: Vec<OrderBy>,
    limit: Option<usize>,
    offset: Option<usize>,
}
impl Query {
    pub fn new() -> Self {
        Query::default()
    }
    pub fn and(mut self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self {
        self.and_clauses.push(Clause::new(field, op, value));
        self
    }
    pub fn or(mut self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self {
        self.or_clauses.push(Clause::new(field, op, value));
        self
    }
    pub fn not(mut self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self {
        self.not_clauses.push(Clause::new(field, op, value));
        self
    }
    pub fn and_eq(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Eq, value)
    }
    pub fn and_ne(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Ne, value)
    }
    pub fn and_gt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Gt, value)
    }
    pub fn and_gte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Gte, value)
    }
    pub fn and_lt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Lt, value)
    }
    pub fn and_lte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.and(field, Op::Lte, value)
    }
    pub fn and_contains(self, field: &str, value: &str) -> Self {
        self.and(field, Op::Contains, value)
    }
    pub fn and_startswith(self, field: &str, value: &str) -> Self {
        self.and(field, Op::StartsWith, value)
    }
    pub fn and_endswith(self, field: &str, value: &str) -> Self {
        self.and(field, Op::EndsWith, value)
    }
    pub fn and_regex(self, field: &str, pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern)?;
        Ok(self.and(field, Op::Regex, ClauseValue::Regex(regex)))
    }
    pub fn and_in<I>(self, field: &str, values: I) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        let set: Vec<u32> = values.into_iter().collect();
        self.and(field, Op::In, ClauseValue::EnumSet(set))
    }
    pub fn and_before(self, field: &str, ts: Timestamp) -> Self {
        self.and(field, Op::Before, ts)
    }
    pub fn and_after(self, field: &str, ts: Timestamp) -> Self {
        self.and(field, Op::After, ts)
    }
    pub fn or_eq(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Eq, value)
    }
    pub fn or_ne(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Ne, value)
    }
    pub fn or_gt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Gt, value)
    }
    pub fn or_gte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Gte, value)
    }
    pub fn or_lt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Lt, value)
    }
    pub fn or_lte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.or(field, Op::Lte, value)
    }
    pub fn or_contains(self, field: &str, value: &str) -> Self {
        self.or(field, Op::Contains, value)
    }
    pub fn or_startswith(self, field: &str, value: &str) -> Self {
        self.or(field, Op::StartsWith, value)
    }
    pub fn or_endswith(self, field: &str, value: &str) -> Self {
        self.or(field, Op::EndsWith, value)
    }
    pub fn or_regex(self, field: &str, pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern)?;
        Ok(self.or(field, Op::Regex, ClauseValue::Regex(regex)))
    }
    pub fn or_in<I>(self, field: &str, values: I) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        let set: Vec<u32> = values.into_iter().collect();
        self.or(field, Op::In, ClauseValue::EnumSet(set))
    }
    pub fn or_before(self, field: &str, ts: Timestamp) -> Self {
        self.or(field, Op::Before, ts)
    }
    pub fn or_after(self, field: &str, ts: Timestamp) -> Self {
        self.or(field, Op::After, ts)
    }
    pub fn not_eq(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Eq, value)
    }
    pub fn not_ne(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Ne, value)
    }
    pub fn not_gt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Gt, value)
    }
    pub fn not_gte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Gte, value)
    }
    pub fn not_lt(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Lt, value)
    }
    pub fn not_lte(self, field: &str, value: impl Into<ClauseValue>) -> Self {
        self.not(field, Op::Lte, value)
    }
    pub fn not_contains(self, field: &str, value: &str) -> Self {
        self.not(field, Op::Contains, value)
    }
    pub fn not_startswith(self, field: &str, value: &str) -> Self {
        self.not(field, Op::StartsWith, value)
    }
    pub fn not_endswith(self, field: &str, value: &str) -> Self {
        self.not(field, Op::EndsWith, value)
    }
    pub fn not_regex(self, field: &str, pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern)?;
        Ok(self.not(field, Op::Regex, ClauseValue::Regex(regex)))
    }
    pub fn not_in<I>(self, field: &str, values: I) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        let set: Vec<u32> = values.into_iter().collect();
        self.not(field, Op::In, ClauseValue::EnumSet(set))
    }
    pub fn not_before(self, field: &str, ts: Timestamp) -> Self {
        self.not(field, Op::Before, ts)
    }
    pub fn not_after(self, field: &str, ts: Timestamp) -> Self {
        self.not(field, Op::After, ts)
    }
    pub fn order_by(mut self, field: &str, dir: Dir) -> Self {
        self.orderings.push(OrderBy::new(field, dir));
        self
    }
    pub fn order_asc(self, field: &str) -> Self {
        self.order_by(field, Dir::Asc)
    }
    pub fn order_desc(self, field: &str) -> Self {
        self.order_by(field, Dir::Desc)
    }
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }
    pub fn build(self) -> Self {
        self
    }
    pub fn and_clauses(&self) -> &[Clause] {
        &self.and_clauses
    }
    pub fn or_clauses(&self) -> &[Clause] {
        &self.or_clauses
    }
    pub fn not_clauses(&self) -> &[Clause] {
        &self.not_clauses
    }
    pub fn orderings(&self) -> &[OrderBy] {
        &self.orderings
    }
    pub fn get_limit(&self) -> Option<usize> {
        self.limit
    }
    pub fn get_offset(&self) -> Option<usize> {
        self.offset
    }
    pub fn is_empty(&self) -> bool {
        self.and_clauses.is_empty() && self.or_clauses.is_empty() && self.not_clauses.is_empty()
    }
    pub fn matches<T, F>(&self, item: &T, accessor: F) -> bool
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        let and_pass = self
            .and_clauses
            .iter()
            .all(|clause| clause.matches(&accessor(item, &clause.field)));
        if !and_pass {
            return false;
        }
        let or_pass = self.or_clauses.is_empty()
            || self
                .or_clauses
                .iter()
                .any(|clause| clause.matches(&accessor(item, &clause.field)));
        if !or_pass {
            return false;
        }
        let not_pass = self
            .not_clauses
            .iter()
            .all(|clause| !clause.matches(&accessor(item, &clause.field)));
        not_pass
    }
    pub fn filter<'a, T, F>(&self, items: &'a [T], accessor: F) -> Vec<&'a T>
    where
        for<'b> F: Fn(&'b T, &str) -> Value<'b>,
    {
        let mut results: Vec<&'a T> = items
            .iter()
            .filter(|item| self.matches(*item, &accessor))
            .collect();
        if !self.orderings.is_empty() {
            results.sort_by(|a, b| compare_by_orderings(*a, *b, &self.orderings, &accessor));
        }
        let offset = self.offset.unwrap_or(0);
        if offset > 0 {
            if offset >= results.len() {
                return Vec::new();
            }
            results = results.into_iter().skip(offset).collect();
        }
        if let Some(limit) = self.limit {
            results.truncate(limit);
        }
        results
    }
    pub fn filter_cloned<T, F>(&self, items: &[T], accessor: F) -> Vec<T>
    where
        T: Clone,
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        self.filter(items, accessor).into_iter().cloned().collect()
    }
    pub fn filter_mut<T, F>(&self, items: &mut Vec<T>, accessor: F)
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        items.retain(|item| self.matches(item, &accessor));
    }
    pub fn count<T, F>(&self, items: &[T], accessor: F) -> usize
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        items
            .iter()
            .filter(|item| self.matches(*item, &accessor))
            .count()
    }
    pub fn any<T, F>(&self, items: &[T], accessor: F) -> bool
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        items.iter().any(|item| self.matches(item, &accessor))
    }
    pub fn all<T, F>(&self, items: &[T], accessor: F) -> bool
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        items.iter().all(|item| self.matches(item, &accessor))
    }
    pub fn find<'a, T, F>(&self, items: &'a [T], accessor: F) -> Option<&'a T>
    where
        for<'b> F: Fn(&'b T, &str) -> Value<'b>,
    {
        items.iter().find(|item| self.matches(*item, &accessor))
    }
    pub fn position<T, F>(&self, items: &[T], accessor: F) -> Option<usize>
    where
        for<'a> F: Fn(&'a T, &str) -> Value<'a>,
    {
        items.iter().position(|item| self.matches(item, &accessor))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Number;
    #[derive(Debug, Clone, PartialEq)]
    struct Task {
        name: String,
        priority: i64,
        status: u32,
        archived: bool,
    }
    fn accessor<'a>(task: &'a Task, field: &str) -> Value<'a> {
        match field {
            "name" => Value::String(&task.name),
            "priority" => Value::Number(Number::I64(task.priority)),
            "status" => Value::Enum(task.status),
            "archived" => Value::Bool(task.archived),
            _ => Value::None,
        }
    }
    fn sample_tasks() -> Vec<Task> {
        vec![
            Task {
                name: "Task A".to_string(),
                priority: 1,
                status: 0,
                archived: false,
            },
            Task {
                name: "Task B".to_string(),
                priority: 2,
                status: 1,
                archived: false,
            },
            Task {
                name: "Urgent Task".to_string(),
                priority: 5,
                status: 1,
                archived: false,
            },
            Task {
                name: "Critical Task".to_string(),
                priority: 5,
                status: 2,
                archived: true,
            },
            Task {
                name: "Done Task".to_string(),
                priority: 3,
                status: 2,
                archived: true,
            },
        ]
    }
    #[test]
    fn empty_query_matches_all() {
        let tasks = sample_tasks();
        let query = Query::new().build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 5);
    }
    #[test]
    fn and_single_clause() {
        let tasks = sample_tasks();
        let query = Query::new().and_eq("priority", 5i64).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.priority == 5));
    }
    #[test]
    fn and_multiple_clauses() {
        let tasks = sample_tasks();
        let query = Query::new()
            .and_eq("priority", 5i64)
            .and_eq("archived", false)
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Urgent Task");
    }
    #[test]
    fn or_clauses() {
        let tasks = sample_tasks();
        let query = Query::new()
            .or_contains("name", "Urgent")
            .or_contains("name", "Critical")
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
    }
    #[test]
    fn and_with_or() {
        let tasks = sample_tasks();
        let query = Query::new()
            .and_eq("priority", 5i64)
            .or_contains("name", "Urgent")
            .or_contains("name", "Critical")
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
    }
    #[test]
    fn not_clauses() {
        let tasks = sample_tasks();
        let query = Query::new().not_eq("archived", true).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|t| !t.archived));
    }
    #[test]
    fn combined_and_or_not() {
        let tasks = sample_tasks();
        let query = Query::new()
            .and_gte("priority", 3i64)
            .or_contains("name", "Urgent")
            .or_contains("name", "Done")
            .not_eq("archived", true)
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Urgent Task");
    }
    #[test]
    fn ordering_single_field() {
        let tasks = sample_tasks();
        let query = Query::new().order_desc("priority").build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results[0].priority, 5);
        assert_eq!(results[1].priority, 5);
        assert_eq!(results[4].priority, 1);
    }
    #[test]
    fn ordering_multiple_fields() {
        let tasks = sample_tasks();
        let query = Query::new()
            .order_desc("priority")
            .order_asc("name")
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results[0].name, "Critical Task");
        assert_eq!(results[1].name, "Urgent Task");
    }
    #[test]
    fn limit() {
        let tasks = sample_tasks();
        let query = Query::new().limit(2).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
    }
    #[test]
    fn offset() {
        let tasks = sample_tasks();
        let query = Query::new().offset(2).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 3);
    }
    #[test]
    fn offset_and_limit() {
        let tasks = sample_tasks();
        let query = Query::new().offset(1).limit(2).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Task B");
        assert_eq!(results[1].name, "Urgent Task");
    }
    #[test]
    fn offset_beyond_results() {
        let tasks = sample_tasks();
        let query = Query::new().offset(100).build();
        let results = query.filter(&tasks, accessor);
        assert!(results.is_empty());
    }
    #[test]
    fn count() {
        let tasks = sample_tasks();
        let query = Query::new().and_eq("archived", true).build();
        assert_eq!(query.count(&tasks, accessor), 2);
    }
    #[test]
    fn any_and_all() {
        let tasks = sample_tasks();
        let query_some_archived = Query::new().and_eq("archived", true).build();
        assert!(query_some_archived.any(&tasks, accessor));
        assert!(!query_some_archived.all(&tasks, accessor));
        let query_all_have_name = Query::new().and_contains("name", "Task").build();
        assert!(query_all_have_name.all(&tasks, accessor));
    }
    #[test]
    fn find() {
        let tasks = sample_tasks();
        let query = Query::new().and_contains("name", "Urgent").build();
        let found = query.find(&tasks, accessor);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Urgent Task");
    }
    #[test]
    fn find_none() {
        let tasks = sample_tasks();
        let query = Query::new().and_contains("name", "Nonexistent").build();
        assert!(query.find(&tasks, accessor).is_none());
    }
    #[test]
    fn filter_cloned() {
        let tasks = sample_tasks();
        let query = Query::new().and_eq("priority", 5i64).build();
        let results: Vec<Task> = query.filter_cloned(&tasks, accessor);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.priority == 5));
    }
    #[test]
    fn filter_mut() {
        let mut tasks = sample_tasks();
        let query = Query::new().and_eq("archived", false).build();
        query.filter_mut(&mut tasks, accessor);
        assert_eq!(tasks.len(), 3);
        assert!(tasks.iter().all(|t| !t.archived));
    }
    #[test]
    fn regex_query() {
        let tasks = sample_tasks();
        let query = Query::new()
            .and_regex("name", r"^Task [A-Z]$")
            .unwrap()
            .build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 2);
    }
    #[test]
    fn enum_in() {
        let tasks = sample_tasks();
        let query = Query::new().and_in("status", [1u32, 2]).build();
        let results = query.filter(&tasks, accessor);
        assert_eq!(results.len(), 4);
    }
    #[test]
    fn introspection() {
        let query = Query::new()
            .and_eq("a", "1")
            .or_eq("b", "2")
            .not_eq("c", "3")
            .order_asc("d")
            .limit(10)
            .offset(5)
            .build();
        assert_eq!(query.and_clauses().len(), 1);
        assert_eq!(query.or_clauses().len(), 1);
        assert_eq!(query.not_clauses().len(), 1);
        assert_eq!(query.orderings().len(), 1);
        assert_eq!(query.get_limit(), Some(10));
        assert_eq!(query.get_offset(), Some(5));
        assert!(!query.is_empty());
    }
    #[test]
    fn is_empty() {
        assert!(Query::new().is_empty());
        assert!(!Query::new().and_eq("a", "1").is_empty());
    }
}
