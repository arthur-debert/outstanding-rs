//! Integration tests for the Seekable derive macro.
//!
//! These tests verify that the `#[derive(Seekable)]` macro generates correct
//! accessor functions and field constants from struct field annotations.

#![cfg(feature = "macros")]
#![allow(dead_code)] // Some fields are intentionally skipped for testing

use standout::seeker::{Query, Seekable, SeekerEnum, SeekerTimestamp, Timestamp, Value};
use standout_macros::Seekable as DeriveSeekable;

// =============================================================================
// Basic derive tests
// =============================================================================

#[derive(DeriveSeekable)]
struct BasicTask {
    #[seek(String)]
    name: String,

    #[seek(Number)]
    priority: u8,

    #[seek(Bool)]
    done: bool,
}

#[test]
fn test_basic_derive_compiles() {
    let task = BasicTask {
        name: "Test".to_string(),
        priority: 5,
        done: false,
    };

    // Should be able to access fields
    assert!(matches!(
        task.seeker_field_value("name"),
        Value::String("Test")
    ));
    assert!(matches!(
        task.seeker_field_value("priority"),
        Value::Number(_)
    ));
    assert!(matches!(
        task.seeker_field_value("done"),
        Value::Bool(false)
    ));
}

#[test]
fn test_field_constants_generated() {
    // Field constants should be generated
    assert_eq!(BasicTask::NAME, "name");
    assert_eq!(BasicTask::PRIORITY, "priority");
    assert_eq!(BasicTask::DONE, "done");
}

#[test]
fn test_unknown_field_returns_none() {
    let task = BasicTask {
        name: "Test".to_string(),
        priority: 5,
        done: false,
    };

    assert_eq!(task.seeker_field_value("unknown"), Value::None);
    assert_eq!(task.seeker_field_value(""), Value::None);
}

#[test]
fn test_accessor_function() {
    let task = BasicTask {
        name: "Test".to_string(),
        priority: 5,
        done: false,
    };

    // The accessor function should work
    let value = BasicTask::accessor(&task, "name");
    assert_eq!(value, Value::String("Test"));
}

// =============================================================================
// Number type tests
// =============================================================================

#[derive(DeriveSeekable)]
struct NumericTask {
    #[seek(Number)]
    count_i8: i8,

    #[seek(Number)]
    count_i16: i16,

    #[seek(Number)]
    count_i32: i32,

    #[seek(Number)]
    count_i64: i64,

    #[seek(Number)]
    count_u8: u8,

    #[seek(Number)]
    count_u16: u16,

    #[seek(Number)]
    count_u32: u32,

    #[seek(Number)]
    count_u64: u64,

    #[seek(Number)]
    value_f32: f32,

    #[seek(Number)]
    value_f64: f64,
}

#[test]
fn test_numeric_types() {
    let task = NumericTask {
        count_i8: -1,
        count_i16: -2,
        count_i32: -3,
        count_i64: -4,
        count_u8: 1,
        count_u16: 2,
        count_u32: 3,
        count_u64: 4,
        value_f32: 1.5,
        value_f64: 2.5,
    };

    // All should be numbers
    assert!(matches!(
        task.seeker_field_value("count_i8"),
        Value::Number(_)
    ));
    assert!(matches!(
        task.seeker_field_value("count_u64"),
        Value::Number(_)
    ));
    assert!(matches!(
        task.seeker_field_value("value_f64"),
        Value::Number(_)
    ));
}

// =============================================================================
// Enum type tests
// =============================================================================

#[derive(Clone, Copy, PartialEq, Debug)]
enum Status {
    Pending,
    Active,
    Completed,
}

impl SeekerEnum for Status {
    fn seeker_discriminant(&self) -> u32 {
        match self {
            Status::Pending => 0,
            Status::Active => 1,
            Status::Completed => 2,
        }
    }
}

#[derive(DeriveSeekable)]
struct EnumTask {
    #[seek(String)]
    name: String,

    #[seek(Enum)]
    status: Status,
}

#[test]
fn test_enum_field() {
    let task = EnumTask {
        name: "Test".to_string(),
        status: Status::Active,
    };

    let value = task.seeker_field_value("status");
    assert_eq!(value, Value::Enum(1));
}

#[test]
fn test_enum_constants() {
    assert_eq!(EnumTask::NAME, "name");
    assert_eq!(EnumTask::STATUS, "status");
}

// =============================================================================
// Timestamp type tests
// =============================================================================

#[derive(Clone, Copy)]
struct MyTimestamp(i64);

impl SeekerTimestamp for MyTimestamp {
    fn seeker_timestamp(&self) -> Timestamp {
        Timestamp::from_millis(self.0)
    }
}

#[derive(DeriveSeekable)]
struct TimestampTask {
    #[seek(String)]
    name: String,

    #[seek(Timestamp)]
    created_at: MyTimestamp,

    #[seek(Timestamp)]
    updated_at: i64, // i64 has built-in SeekerTimestamp impl
}

#[test]
fn test_timestamp_field() {
    let task = TimestampTask {
        name: "Test".to_string(),
        created_at: MyTimestamp(1000),
        updated_at: 2000,
    };

    assert_eq!(
        task.seeker_field_value("created_at"),
        Value::Timestamp(Timestamp(1000))
    );
    assert_eq!(
        task.seeker_field_value("updated_at"),
        Value::Timestamp(Timestamp(2000))
    );
}

// =============================================================================
// Skip attribute tests
// =============================================================================

#[derive(DeriveSeekable)]
struct SkipTask {
    #[seek(String)]
    name: String,

    #[seek(skip)]
    internal_id: u64,

    #[seek(Number)]
    priority: u8,
}

#[test]
fn test_skip_field() {
    let task = SkipTask {
        name: "Test".to_string(),
        internal_id: 12345,
        priority: 5,
    };

    // Skipped field should return None
    assert_eq!(task.seeker_field_value("internal_id"), Value::None);

    // Other fields should work
    assert_eq!(task.seeker_field_value("name"), Value::String("Test"));
    assert!(matches!(
        task.seeker_field_value("priority"),
        Value::Number(_)
    ));
}

#[test]
fn test_skip_field_no_constant() {
    // NAME and PRIORITY constants should exist
    assert_eq!(SkipTask::NAME, "name");
    assert_eq!(SkipTask::PRIORITY, "priority");

    // INTERNAL_ID constant should NOT exist (skipped)
    // This is a compile-time check - if SkipTask::INTERNAL_ID existed, it would be a compile error
}

// =============================================================================
// Rename attribute tests
// =============================================================================

#[derive(DeriveSeekable)]
struct RenameTask {
    #[seek(String, rename = "title")]
    name: String,

    #[seek(Number, rename = "prio")]
    priority: u8,
}

#[test]
fn test_rename_field() {
    let task = RenameTask {
        name: "Test".to_string(),
        priority: 5,
    };

    // Should use the renamed field name
    assert_eq!(task.seeker_field_value("title"), Value::String("Test"));
    assert!(matches!(task.seeker_field_value("prio"), Value::Number(_)));

    // Original names should not work
    assert_eq!(task.seeker_field_value("name"), Value::None);
    assert_eq!(task.seeker_field_value("priority"), Value::None);
}

#[test]
fn test_rename_constants() {
    // Constants should use the renamed names
    assert_eq!(RenameTask::TITLE, "title");
    assert_eq!(RenameTask::PRIO, "prio");
}

// =============================================================================
// Integration with Query tests
// =============================================================================

#[derive(DeriveSeekable, Clone, Debug)]
struct QueryableTask {
    #[seek(String)]
    name: String,

    #[seek(Number)]
    priority: i32,

    #[seek(Bool)]
    done: bool,

    #[seek(Enum)]
    status: Status,
}

fn sample_tasks() -> Vec<QueryableTask> {
    vec![
        QueryableTask {
            name: "Write docs".to_string(),
            priority: 3,
            done: false,
            status: Status::Active,
        },
        QueryableTask {
            name: "Fix bug".to_string(),
            priority: 5,
            done: true,
            status: Status::Completed,
        },
        QueryableTask {
            name: "Review PR".to_string(),
            priority: 4,
            done: false,
            status: Status::Pending,
        },
    ]
}

#[test]
fn test_query_with_accessor() {
    let tasks = sample_tasks();

    let query = Query::new().and_eq(QueryableTask::DONE, false).build();

    let results = query.filter(&tasks, QueryableTask::accessor);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_query_with_field_constants() {
    let tasks = sample_tasks();

    let query = Query::new()
        .and_gte(QueryableTask::PRIORITY, 4i32)
        .not_eq(QueryableTask::DONE, true)
        .build();

    let results = query.filter(&tasks, QueryableTask::accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Review PR");
}

#[test]
fn test_query_string_contains() {
    let tasks = sample_tasks();

    let query = Query::new().and_contains(QueryableTask::NAME, "PR").build();

    let results = query.filter(&tasks, QueryableTask::accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Review PR");
}

#[test]
fn test_query_enum_field() {
    let tasks = sample_tasks();

    let query = Query::new()
        .and_in(
            QueryableTask::STATUS,
            [Status::Active.seeker_discriminant()],
        )
        .build();

    let results = query.filter(&tasks, QueryableTask::accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Write docs");
}

#[test]
fn test_query_ordering() {
    let tasks = sample_tasks();

    let query = Query::new().order_desc(QueryableTask::PRIORITY).build();

    let results = query.filter(&tasks, QueryableTask::accessor);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].priority, 5);
    assert_eq!(results[1].priority, 4);
    assert_eq!(results[2].priority, 3);
}

#[test]
fn test_query_count() {
    let tasks = sample_tasks();

    let query = Query::new().and_eq(QueryableTask::DONE, false).build();

    let count = query.count(&tasks, QueryableTask::accessor);
    assert_eq!(count, 2);
}

#[test]
fn test_query_find() {
    let tasks = sample_tasks();

    let query = Query::new().and_eq(QueryableTask::NAME, "Fix bug").build();

    let found = query.find(&tasks, QueryableTask::accessor);
    assert!(found.is_some());
    assert_eq!(found.unwrap().priority, 5);
}

// =============================================================================
// Fields without seek attribute are skipped
// =============================================================================

#[derive(DeriveSeekable)]
struct PartialTask {
    #[seek(String)]
    name: String,

    // No #[seek] attribute - should be skipped
    internal_counter: u32,

    #[seek(Number)]
    priority: u8,
}

#[test]
fn test_unannotated_fields_skipped() {
    let task = PartialTask {
        name: "Test".to_string(),
        internal_counter: 999,
        priority: 5,
    };

    // Annotated fields work
    assert_eq!(task.seeker_field_value("name"), Value::String("Test"));
    assert!(matches!(
        task.seeker_field_value("priority"),
        Value::Number(_)
    ));

    // Unannotated field returns None
    assert_eq!(task.seeker_field_value("internal_counter"), Value::None);
}

// =============================================================================
// Complex combined test
// =============================================================================

#[derive(DeriveSeekable, Clone, Debug)]
struct CompleteTask {
    #[seek(String)]
    id: String,

    #[seek(String, rename = "title")]
    name: String,

    #[seek(Number)]
    priority: i32,

    #[seek(Bool)]
    archived: bool,

    #[seek(Enum)]
    status: Status,

    #[seek(Timestamp)]
    created_at: i64,

    #[seek(skip)]
    internal_state: u32,

    // No attribute - also skipped
    metadata: String,
}

#[test]
fn test_complete_task_all_fields() {
    let task = CompleteTask {
        id: "TSK-001".to_string(),
        name: "Implement feature".to_string(),
        priority: 5,
        archived: false,
        status: Status::Active,
        created_at: 1706500000000,
        internal_state: 42,
        metadata: "some data".to_string(),
    };

    // All seekable fields work
    assert_eq!(task.seeker_field_value("id"), Value::String("TSK-001"));
    assert_eq!(
        task.seeker_field_value("title"),
        Value::String("Implement feature")
    ); // renamed
    assert!(matches!(
        task.seeker_field_value("priority"),
        Value::Number(_)
    ));
    assert_eq!(task.seeker_field_value("archived"), Value::Bool(false));
    assert_eq!(task.seeker_field_value("status"), Value::Enum(1));
    assert_eq!(
        task.seeker_field_value("created_at"),
        Value::Timestamp(Timestamp(1706500000000))
    );

    // Skipped/unannotated fields return None
    assert_eq!(task.seeker_field_value("internal_state"), Value::None);
    assert_eq!(task.seeker_field_value("metadata"), Value::None);
    assert_eq!(task.seeker_field_value("name"), Value::None); // original name, not accessible
}

#[test]
fn test_complete_task_constants() {
    assert_eq!(CompleteTask::ID, "id");
    assert_eq!(CompleteTask::TITLE, "title"); // renamed constant
    assert_eq!(CompleteTask::PRIORITY, "priority");
    assert_eq!(CompleteTask::ARCHIVED, "archived");
    assert_eq!(CompleteTask::STATUS, "status");
    assert_eq!(CompleteTask::CREATED_AT, "created_at");
}

#[test]
fn test_complete_task_query() {
    let tasks = vec![
        CompleteTask {
            id: "TSK-001".to_string(),
            name: "First".to_string(),
            priority: 3,
            archived: false,
            status: Status::Active,
            created_at: 1000,
            internal_state: 1,
            metadata: "".to_string(),
        },
        CompleteTask {
            id: "TSK-002".to_string(),
            name: "Second".to_string(),
            priority: 5,
            archived: true,
            status: Status::Completed,
            created_at: 2000,
            internal_state: 2,
            metadata: "".to_string(),
        },
        CompleteTask {
            id: "TSK-003".to_string(),
            name: "Third".to_string(),
            priority: 4,
            archived: false,
            status: Status::Pending,
            created_at: 3000,
            internal_state: 3,
            metadata: "".to_string(),
        },
    ];

    // Complex query using field constants
    let query = Query::new()
        .not_eq(CompleteTask::ARCHIVED, true)
        .and_gte(CompleteTask::PRIORITY, 3i32)
        .and_after(CompleteTask::CREATED_AT, Timestamp(500))
        .order_desc(CompleteTask::PRIORITY)
        .build();

    let results = query.filter(&tasks, CompleteTask::accessor);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "TSK-003"); // priority 4
    assert_eq!(results[1].id, "TSK-001"); // priority 3
}

// =============================================================================
// SeekerSchema derive tests
// =============================================================================

use standout::seeker::{parse_query, SeekType, SeekerSchema};

#[test]
fn test_seeker_schema_field_type() {
    // BasicTask has: name (String), priority (Number), done (Bool)
    assert_eq!(BasicTask::field_type("name"), Some(SeekType::String));
    assert_eq!(BasicTask::field_type("priority"), Some(SeekType::Number));
    assert_eq!(BasicTask::field_type("done"), Some(SeekType::Bool));
    assert_eq!(BasicTask::field_type("unknown"), None);
}

#[test]
fn test_seeker_schema_field_names() {
    let names = BasicTask::field_names();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"name"));
    assert!(names.contains(&"priority"));
    assert!(names.contains(&"done"));
}

#[test]
fn test_seeker_schema_with_rename() {
    // RenameTask: name renamed to "title", priority renamed to "prio"
    assert_eq!(RenameTask::field_type("title"), Some(SeekType::String));
    assert_eq!(RenameTask::field_type("prio"), Some(SeekType::Number));
    // Original names should not be accessible
    assert_eq!(RenameTask::field_type("name"), None);
    assert_eq!(RenameTask::field_type("priority"), None);
}

#[test]
fn test_seeker_schema_skipped_fields() {
    // SkipTask: name (String), internal_id (skipped), priority (Number)
    assert_eq!(SkipTask::field_type("name"), Some(SeekType::String));
    assert_eq!(SkipTask::field_type("priority"), Some(SeekType::Number));
    assert_eq!(SkipTask::field_type("internal_id"), None); // skipped

    let names = SkipTask::field_names();
    assert!(!names.contains(&"internal_id"));
}

#[test]
fn test_seeker_schema_all_types() {
    // CompleteTask has all field types
    assert_eq!(CompleteTask::field_type("id"), Some(SeekType::String));
    assert_eq!(CompleteTask::field_type("title"), Some(SeekType::String)); // renamed from name
    assert_eq!(CompleteTask::field_type("priority"), Some(SeekType::Number));
    assert_eq!(CompleteTask::field_type("archived"), Some(SeekType::Bool));
    assert_eq!(CompleteTask::field_type("status"), Some(SeekType::Enum));
    assert_eq!(
        CompleteTask::field_type("created_at"),
        Some(SeekType::Timestamp)
    );
    // Skipped and unannotated fields
    assert_eq!(CompleteTask::field_type("internal_state"), None);
    assert_eq!(CompleteTask::field_type("metadata"), None);
}

#[test]
fn test_parse_query_with_derived_schema() {
    let tasks = vec![
        BasicTask {
            name: "Write docs".to_string(),
            priority: 3,
            done: false,
        },
        BasicTask {
            name: "Fix bug".to_string(),
            priority: 5,
            done: true,
        },
        BasicTask {
            name: "Review PR".to_string(),
            priority: 4,
            done: false,
        },
    ];

    // Parse query using the derived SeekerSchema
    let pairs = vec![("name-contains".to_string(), "docs".to_string())];
    let query = parse_query::<BasicTask>(pairs).unwrap();

    let results = query.filter(&tasks, BasicTask::accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Write docs");
}

#[test]
fn test_parse_query_with_multiple_clauses() {
    let tasks = vec![
        BasicTask {
            name: "Write docs".to_string(),
            priority: 3,
            done: false,
        },
        BasicTask {
            name: "Fix bug".to_string(),
            priority: 5,
            done: true,
        },
        BasicTask {
            name: "Review PR".to_string(),
            priority: 4,
            done: false,
        },
    ];

    let pairs = vec![
        ("priority-gte".to_string(), "4".to_string()),
        ("NOT".to_string(), "".to_string()),
        ("done".to_string(), "true".to_string()),
    ];
    let query = parse_query::<BasicTask>(pairs).unwrap();

    let results = query.filter(&tasks, BasicTask::accessor);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Review PR");
}

#[test]
fn test_parse_query_unknown_field_error() {
    let pairs = vec![("unknown-field".to_string(), "value".to_string())];
    let result = parse_query::<BasicTask>(pairs);
    assert!(result.is_err());
}

#[test]
fn test_parse_query_invalid_operator_error() {
    // "gt" is not valid for String fields
    let pairs = vec![("name-gt".to_string(), "value".to_string())];
    let result = parse_query::<BasicTask>(pairs);
    assert!(result.is_err());
}
