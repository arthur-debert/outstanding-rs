use std::cmp::Ordering;
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    String(&'a str),
    Number(Number),
    Timestamp(Timestamp),
    Enum(u32),
    Bool(bool),
    None,
}
impl<'a> Value<'a> {
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Value::Timestamp(_))
    }
    pub fn is_enum(&self) -> bool {
        matches!(self, Value::Enum(_))
    }
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_number(&self) -> Option<Number> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_timestamp(&self) -> Option<Timestamp> {
        match self {
            Value::Timestamp(t) => Some(*t),
            _ => None,
        }
    }
    pub fn as_enum(&self) -> Option<u32> {
        match self {
            Value::Enum(d) => Some(*d),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    I64(i64),
    U64(u64),
    F64(f64),
}
impl Number {
    pub fn to_f64(self) -> f64 {
        match self {
            Number::I64(n) => n as f64,
            Number::U64(n) => n as f64,
            Number::F64(n) => n,
        }
    }
    pub fn compare(self, other: Number) -> Option<Ordering> {
        match (self, other) {
            (Number::I64(a), Number::I64(b)) => Some(a.cmp(&b)),
            (Number::U64(a), Number::U64(b)) => Some(a.cmp(&b)),
            (Number::F64(a), Number::F64(b)) => a.partial_cmp(&b),
            _ => self.to_f64().partial_cmp(&other.to_f64()),
        }
    }
}
impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.compare(*other)
    }
}
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);
impl Timestamp {
    pub fn from_millis(millis: i64) -> Self {
        Timestamp(millis)
    }
    pub fn from_secs(secs: i64) -> Self {
        Timestamp(secs * 1000)
    }
    pub fn as_millis(self) -> i64 {
        self.0
    }
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
