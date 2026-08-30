use crate::value::Value;
pub trait Seekable {
    fn seeker_field_value(&self, field: &str) -> Value<'_>;
    fn accessor<'a>(item: &'a Self, field: &str) -> Value<'a>
    where
        Self: Sized,
    {
        item.seeker_field_value(field)
    }
}
pub trait SeekerEnum {
    fn seeker_discriminant(&self) -> u32;
}
pub trait SeekerTimestamp {
    fn seeker_timestamp(&self) -> crate::Timestamp;
}
impl SeekerTimestamp for i64 {
    fn seeker_timestamp(&self) -> crate::Timestamp {
        crate::Timestamp::from_millis(*self)
    }
}
impl SeekerTimestamp for u64 {
    fn seeker_timestamp(&self) -> crate::Timestamp {
        crate::Timestamp::from_millis(*self as i64)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Number;
    struct TestItem {
        name: String,
        count: i32,
    }
    impl Seekable for TestItem {
        fn seeker_field_value(&self, field: &str) -> Value<'_> {
            match field {
                "name" => Value::String(&self.name),
                "count" => Value::Number(Number::I64(self.count as i64)),
                _ => Value::None,
            }
        }
    }
    #[test]
    fn seekable_manual_impl() {
        let item = TestItem {
            name: "test".to_string(),
            count: 42,
        };
        assert_eq!(item.seeker_field_value("name"), Value::String("test"));
        assert_eq!(
            item.seeker_field_value("count"),
            Value::Number(Number::I64(42))
        );
        assert_eq!(item.seeker_field_value("unknown"), Value::None);
    }
    #[test]
    fn seekable_accessor() {
        let item = TestItem {
            name: "test".to_string(),
            count: 42,
        };
        assert_eq!(TestItem::accessor(&item, "name"), Value::String("test"));
    }
    #[derive(Clone, Copy)]
    enum Status {
        Pending,
        Active,
    }
    impl SeekerEnum for Status {
        fn seeker_discriminant(&self) -> u32 {
            match self {
                Status::Pending => 0,
                Status::Active => 1,
            }
        }
    }
    #[test]
    fn seeker_enum_discriminant() {
        assert_eq!(Status::Pending.seeker_discriminant(), 0);
        assert_eq!(Status::Active.seeker_discriminant(), 1);
    }
    #[test]
    fn seeker_timestamp_i64() {
        let ts: i64 = 1000;
        assert_eq!(ts.seeker_timestamp(), crate::Timestamp(1000));
    }
}
