use crate::value::Value;
use std::cmp::Ordering;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dir {
    #[default]
    Asc,
    Desc,
}
impl Dir {
    pub fn is_asc(self) -> bool {
        matches!(self, Dir::Asc)
    }
    pub fn is_desc(self) -> bool {
        matches!(self, Dir::Desc)
    }
    pub fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Dir::Asc => ordering,
            Dir::Desc => ordering.reverse(),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Asc => "asc",
            Dir::Desc => "desc",
        }
    }
}
impl std::fmt::Display for Dir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    pub field: String,
    pub dir: Dir,
}
impl OrderBy {
    pub fn asc(field: impl Into<String>) -> Self {
        OrderBy {
            field: field.into(),
            dir: Dir::Asc,
        }
    }
    pub fn desc(field: impl Into<String>) -> Self {
        OrderBy {
            field: field.into(),
            dir: Dir::Desc,
        }
    }
    pub fn new(field: impl Into<String>, dir: Dir) -> Self {
        OrderBy {
            field: field.into(),
            dir,
        }
    }
    pub fn compare<'a>(&self, a: &Value<'a>, b: &Value<'a>) -> Option<Ordering> {
        let base_ordering = compare_values(a, b)?;
        Some(self.dir.apply(base_ordering))
    }
}
pub fn compare_values<'a>(a: &Value<'a>, b: &Value<'a>) -> Option<Ordering> {
    match (a, b) {
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Number(a), Value::Number(b)) => a.compare(*b),
        (Value::Timestamp(a), Value::Timestamp(b)) => Some(a.cmp(b)),
        (Value::Enum(a), Value::Enum(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::None, Value::None) => Some(Ordering::Equal),
        (Value::None, _) => Some(Ordering::Greater),
        (_, Value::None) => Some(Ordering::Less),
        _ => None,
    }
}
pub fn compare_by_orderings<T, F>(a: &T, b: &T, orderings: &[OrderBy], accessor: &F) -> Ordering
where
    for<'a> F: Fn(&'a T, &str) -> Value<'a>,
{
    for order_by in orderings {
        let val_a = accessor(a, &order_by.field);
        let val_b = accessor(b, &order_by.field);
        if let Some(ordering) = order_by.compare(&val_a, &val_b) {
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
    }
    Ordering::Equal
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Number, Timestamp};
    #[test]
    fn dir_apply() {
        assert_eq!(Dir::Asc.apply(Ordering::Less), Ordering::Less);
        assert_eq!(Dir::Asc.apply(Ordering::Greater), Ordering::Greater);
        assert_eq!(Dir::Asc.apply(Ordering::Equal), Ordering::Equal);
        assert_eq!(Dir::Desc.apply(Ordering::Less), Ordering::Greater);
        assert_eq!(Dir::Desc.apply(Ordering::Greater), Ordering::Less);
        assert_eq!(Dir::Desc.apply(Ordering::Equal), Ordering::Equal);
    }
    #[test]
    fn dir_display() {
        assert_eq!(Dir::Asc.to_string(), "asc");
        assert_eq!(Dir::Desc.to_string(), "desc");
    }
    #[test]
    fn order_by_constructors() {
        let asc = OrderBy::asc("name");
        assert_eq!(asc.field, "name");
        assert_eq!(asc.dir, Dir::Asc);
        let desc = OrderBy::desc("priority");
        assert_eq!(desc.field, "priority");
        assert_eq!(desc.dir, Dir::Desc);
    }
    #[test]
    fn compare_strings() {
        let a = Value::String("apple");
        let b = Value::String("banana");
        let c = Value::String("apple");
        assert_eq!(compare_values(&a, &b), Some(Ordering::Less));
        assert_eq!(compare_values(&b, &a), Some(Ordering::Greater));
        assert_eq!(compare_values(&a, &c), Some(Ordering::Equal));
    }
    #[test]
    fn compare_numbers() {
        let a = Value::Number(Number::I64(10));
        let b = Value::Number(Number::I64(20));
        assert_eq!(compare_values(&a, &b), Some(Ordering::Less));
        assert_eq!(compare_values(&b, &a), Some(Ordering::Greater));
    }
    #[test]
    fn compare_numbers_nan() {
        let nan = Value::Number(Number::F64(f64::NAN));
        let num = Value::Number(Number::F64(1.0));
        assert_eq!(compare_values(&nan, &num), None);
    }
    #[test]
    fn compare_timestamps() {
        let a = Value::Timestamp(Timestamp(1000));
        let b = Value::Timestamp(Timestamp(2000));
        assert_eq!(compare_values(&a, &b), Some(Ordering::Less));
    }
    #[test]
    fn compare_bools() {
        let t = Value::Bool(true);
        let f = Value::Bool(false);
        assert_eq!(compare_values(&f, &t), Some(Ordering::Less));
    }
    #[test]
    fn compare_none_values() {
        let none = Value::None;
        let some = Value::String("test");
        assert_eq!(compare_values(&none, &some), Some(Ordering::Greater));
        assert_eq!(compare_values(&some, &none), Some(Ordering::Less));
        assert_eq!(compare_values(&none, &none), Some(Ordering::Equal));
    }
    #[test]
    fn compare_type_mismatch() {
        let s = Value::String("test");
        let n = Value::Number(Number::I64(42));
        assert_eq!(compare_values(&s, &n), None);
    }
    #[test]
    fn order_by_compare() {
        let asc = OrderBy::asc("field");
        let desc = OrderBy::desc("field");
        let a = Value::Number(Number::I64(10));
        let b = Value::Number(Number::I64(20));
        assert_eq!(asc.compare(&a, &b), Some(Ordering::Less));
        assert_eq!(desc.compare(&a, &b), Some(Ordering::Greater));
    }
    #[test]
    fn compare_by_multiple_orderings() {
        #[derive(Debug)]
        struct Item {
            name: String,
            priority: i64,
        }
        fn item_accessor<'a>(item: &'a Item, field: &str) -> Value<'a> {
            match field {
                "name" => Value::String(&item.name),
                "priority" => Value::Number(Number::I64(item.priority)),
                _ => Value::None,
            }
        }
        let items = [
            Item {
                name: "a".to_string(),
                priority: 1,
            },
            Item {
                name: "b".to_string(),
                priority: 1,
            },
            Item {
                name: "a".to_string(),
                priority: 2,
            },
        ];
        let orderings = [OrderBy::asc("priority"), OrderBy::asc("name")];
        assert_eq!(
            compare_by_orderings(&items[0], &items[1], &orderings, &item_accessor),
            Ordering::Less
        );
        assert_eq!(
            compare_by_orderings(&items[0], &items[2], &orderings, &item_accessor),
            Ordering::Less
        );
    }
}
