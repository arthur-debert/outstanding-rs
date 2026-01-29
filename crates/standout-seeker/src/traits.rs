//! Traits for derive macro support.
//!
//! This module provides the [`Seekable`] trait which is implemented by
//! the `#[derive(Seekable)]` macro to enable type-safe field access.

use crate::value::Value;

/// Trait for types that can be queried using Seeker.
///
/// This trait is typically derived using `#[derive(Seekable)]` from the
/// `standout-macros` crate, but can also be implemented manually.
///
/// # Derive Usage
///
/// ```ignore
/// use standout_macros::Seekable;
/// use standout_seeker::{Seekable, Query, Value, Number};
///
/// #[derive(Seekable)]
/// struct Task {
/// struct Task {
///     #[seek(String)]
///     name: String,
///     #[seek(Number)]
///     priority: u8,
///     #[seek(Bool)]
///     done: bool,
/// }
///
/// let tasks = vec![
///     Task { name: "Write docs".into(), priority: 3, done: false },
///     Task { name: "Fix bug".into(), priority: 5, done: true },
/// ];
///
/// let query = Query::new()
///     .and_gte(Task::PRIORITY, 3u8)
///     .not_eq(Task::DONE, true)
///     .build();
///
/// // Use the generated accessor
/// let results = query.filter(&tasks, Task::accessor);
/// ```
///
/// # Manual Implementation
///
/// ```
/// use standout_seeker::{Seekable, Value, Number};
///
/// struct Task {
///     name: String,
///     priority: u8,
/// }
///
/// impl Seekable for Task {
///     fn seeker_field_value(&self, field: &str) -> Value<'_> {
///         match field {
///             "name" => Value::String(&self.name),
///             "priority" => Value::Number(Number::U64(self.priority as u64)),
///             _ => Value::None,
///         }
///     }
/// }
/// ```
pub trait Seekable {
    /// Returns the value of a field for query comparison.
    ///
    /// This method is called by the query engine to extract field values
    /// from items during filtering and sorting operations.
    ///
    /// # Parameters
    ///
    /// * `field` - The name of the field to access
    ///
    /// # Returns
    ///
    /// The field value wrapped in a [`Value`] enum variant, or [`Value::None`]
    /// if the field doesn't exist or is not queryable.
    fn seeker_field_value(&self, field: &str) -> Value<'_>;

    /// Returns a static accessor function suitable for use with [`Query::filter`].
    ///
    /// This is a convenience method that returns a function pointer compatible
    /// with the query execution methods.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let results = query.filter(&items, Task::accessor);
    /// ```
    fn accessor<'a>(item: &'a Self, field: &str) -> Value<'a>
    where
        Self: Sized,
    {
        item.seeker_field_value(field)
    }
}

/// Helper trait for converting enum types to their discriminant values.
///
/// This trait is used by the `#[derive(Seekable)]` macro when a field is
/// marked with `#[seek(Enum)]`. Implement this trait for your enum types
/// to enable enum querying.
///
/// # Example
///
/// ```
/// use standout_seeker::SeekerEnum;
///
/// #[derive(Clone, Copy)]
/// enum Status {
///     Pending,
///     Active,
///     Completed,
/// }
///
/// impl SeekerEnum for Status {
///     fn seeker_discriminant(&self) -> u32 {
///         match self {
///             Status::Pending => 0,
///             Status::Active => 1,
///             Status::Completed => 2,
///         }
///     }
/// }
/// ```
pub trait SeekerEnum {
    /// Returns the discriminant value for this enum variant.
    ///
    /// The discriminant should be a stable `u32` value that uniquely
    /// identifies the variant. Use explicit values rather than relying
    /// on derive ordering to ensure stable query behavior.
    fn seeker_discriminant(&self) -> u32;
}

/// Helper trait for converting types to timestamps.
///
/// This trait is used by the `#[derive(Seekable)]` macro when a field is
/// marked with `#[seek(Timestamp)]`. Implement this trait for your datetime
/// types to enable timestamp querying.
///
/// # Example
///
/// ```
/// use standout_seeker::{SeekerTimestamp, Timestamp};
///
/// struct MyDateTime(i64);
///
/// impl SeekerTimestamp for MyDateTime {
///     fn seeker_timestamp(&self) -> Timestamp {
///         Timestamp::from_millis(self.0)
///     }
/// }
/// ```
pub trait SeekerTimestamp {
    /// Converts this value to a [`Timestamp`] for comparison.
    fn seeker_timestamp(&self) -> crate::Timestamp;
}

// Implement SeekerTimestamp for common types
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
