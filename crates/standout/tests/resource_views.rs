//! Integration tests for Resource view result types.

use serde::Serialize;
use standout::views::{
    create_view, delete_view, detail_view, update_view, ActionSuggestion, MessageLevel,
    ValidationError,
};

#[derive(Clone, Serialize)]
struct Task {
    id: String,
    title: String,
    status: String,
}

fn sample_task() -> Task {
    Task {
        id: "task-1".to_string(),
        title: "Implement feature".to_string(),
        status: "pending".to_string(),
    }
}

// ============================================================================
// DetailViewResult tests
// ============================================================================

#[test]
fn test_detail_view_result_basic() {
    let task = sample_task();
    let result = detail_view(task).build();

    assert!(result.title.is_none());
    assert!(result.subtitle.is_none());
    assert!(result.related.is_empty());
    assert!(result.actions.is_empty());
    assert!(result.messages.is_empty());
}

#[test]
fn test_detail_view_result_with_header() {
    let task = sample_task();
    let result = detail_view(task)
        .title("Task Details")
        .subtitle("task-1")
        .build();

    assert_eq!(result.title, Some("Task Details".to_string()));
    assert_eq!(result.subtitle, Some("task-1".to_string()));
}

#[test]
fn test_detail_view_result_with_related() {
    let task = sample_task();
    let result = detail_view(task)
        .related("author", serde_json::json!({"name": "Alice"}))
        .related("project", "Backend API")
        .build();

    assert!(result.has_related());
    assert_eq!(result.related.len(), 2);
    assert!(result.related.contains_key("author"));
    assert!(result.related.contains_key("project"));
}

#[test]
fn test_detail_view_result_with_actions() {
    let task = sample_task();
    let result = detail_view(task)
        .action("Edit", "task update task-1")
        .action("Delete", "task delete task-1")
        .action("Mark Done", "task done task-1")
        .build();

    assert!(result.has_actions());
    assert_eq!(result.actions.len(), 3);
    assert_eq!(result.actions[0].label, "Edit");
    assert_eq!(result.actions[0].command, "task update task-1");
}

#[test]
fn test_detail_view_result_with_messages() {
    let task = sample_task();
    let result = detail_view(task)
        .info("Task is on track")
        .warning("Due date approaching")
        .success("Last update was successful")
        .error("Sync failed")
        .build();

    assert_eq!(result.messages.len(), 4);
    assert_eq!(result.messages[0].level, MessageLevel::Info);
    assert_eq!(result.messages[1].level, MessageLevel::Warning);
    assert_eq!(result.messages[2].level, MessageLevel::Success);
    assert_eq!(result.messages[3].level, MessageLevel::Error);
}

#[test]
fn test_detail_view_result_serialization() {
    let task = sample_task();
    let result = detail_view(task)
        .title("Task Details")
        .action("Edit", "edit task-1")
        .build();

    let json = serde_json::to_value(&result).unwrap();

    // Item should be present
    assert!(json["item"].is_object());
    assert_eq!(json["item"]["id"], "task-1");

    // Title should be present
    assert_eq!(json["title"], "Task Details");

    // Actions should be present
    assert!(json["actions"].is_array());
    assert_eq!(json["actions"][0]["label"], "Edit");
}

#[test]
fn test_detail_view_result_serialization_skips_empty() {
    let task = sample_task();
    let result = detail_view(task).build();
    let json = serde_json::to_string(&result).unwrap();

    // Item is always present, but optional fields should be skipped when empty
    assert!(json.contains("\"item\""));
    // These should NOT be present when None/empty
    assert!(!json.contains("\"related\""));
    assert!(!json.contains("\"actions\""));
    assert!(!json.contains("\"messages\""));
}

// ============================================================================
// CreateViewResult tests
// ============================================================================

#[test]
fn test_create_view_result_basic() {
    let task = sample_task();
    let result = create_view(task).build();

    assert!(!result.dry_run);
    assert!(result.validation_errors.is_empty());
    assert!(result.messages.is_empty());
    assert!(result.is_valid());
}

#[test]
fn test_create_view_result_dry_run() {
    let task = sample_task();
    let result = create_view(task).dry_run().build();

    assert!(result.is_dry_run());
}

#[test]
fn test_create_view_result_with_validation_errors() {
    let task = sample_task();
    let result = create_view(task)
        .validation_error("title", "Title cannot be empty")
        .validation_error("status", "Invalid status value")
        .build();

    assert!(result.has_validation_errors());
    assert!(!result.is_valid());
    assert_eq!(result.validation_errors.len(), 2);
    assert_eq!(result.validation_errors[0].field, "title");
    assert_eq!(result.validation_errors[0].message, "Title cannot be empty");
}

#[test]
fn test_create_view_result_with_batch_validation_errors() {
    let errors = vec![
        ValidationError::new("email", "Invalid email format"),
        ValidationError::new("password", "Password too short"),
    ];

    let task = sample_task();
    let result = create_view(task).validation_errors(errors).build();

    assert_eq!(result.validation_errors.len(), 2);
}

#[test]
fn test_create_view_result_with_messages() {
    let task = sample_task();
    let result = create_view(task)
        .success("Task created successfully")
        .build();

    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].level, MessageLevel::Success);
}

#[test]
fn test_create_view_result_serialization() {
    let task = sample_task();
    let result = create_view(task)
        .dry_run()
        .success("Would create task")
        .build();

    let json = serde_json::to_value(&result).unwrap();

    assert!(json["item"].is_object());
    assert_eq!(json["dry_run"], true);
    assert!(json["messages"].is_array());
}

// ============================================================================
// UpdateViewResult tests
// ============================================================================

#[test]
fn test_update_view_result_basic() {
    let task = sample_task();
    let result = update_view(task).build();

    assert!(result.before.is_none());
    assert!(result.changed_fields.is_empty());
    assert!(!result.dry_run);
    assert!(result.validation_errors.is_empty());
    assert!(result.is_valid());
    assert!(!result.has_changes());
}

#[test]
fn test_update_view_result_with_before() {
    let before = Task {
        id: "task-1".to_string(),
        title: "Old title".to_string(),
        status: "pending".to_string(),
    };
    let after = Task {
        id: "task-1".to_string(),
        title: "New title".to_string(),
        status: "pending".to_string(),
    };

    let result = update_view(after)
        .before(before)
        .changed_field("title")
        .build();

    assert!(result.has_before());
    assert!(result.has_changes());
    assert_eq!(result.changed_fields, vec!["title"]);
}

#[test]
fn test_update_view_result_with_multiple_changes() {
    let task = sample_task();
    let result = update_view(task)
        .changed_fields(["title", "status", "priority"])
        .build();

    assert_eq!(result.changed_fields.len(), 3);
}

#[test]
fn test_update_view_result_dry_run() {
    let task = sample_task();
    let result = update_view(task).changed_field("title").dry_run().build();

    assert!(result.is_dry_run());
    assert!(result.has_changes());
}

#[test]
fn test_update_view_result_with_validation_errors() {
    let task = sample_task();
    let result = update_view(task)
        .validation_error("title", "Title required")
        .build();

    assert!(result.has_validation_errors());
    assert!(!result.is_valid());
}

#[test]
fn test_update_view_result_serialization() {
    let before = sample_task();
    let after = sample_task();
    let result = update_view(after)
        .before(before)
        .changed_field("title")
        .success("Updated")
        .build();

    let json = serde_json::to_value(&result).unwrap();

    assert!(json["before"].is_object());
    assert!(json["after"].is_object());
    assert_eq!(json["changed_fields"][0], "title");
}

// ============================================================================
// DeleteViewResult tests
// ============================================================================

#[test]
fn test_delete_view_result_basic() {
    let task = sample_task();
    let result = delete_view(task).build();

    assert!(!result.confirmed);
    assert!(!result.soft_deleted);
    assert!(result.undo_command.is_none());
    assert!(result.messages.is_empty());
}

#[test]
fn test_delete_view_result_confirmed() {
    let task = sample_task();
    let result = delete_view(task).confirmed().build();

    assert!(result.is_confirmed());
}

#[test]
fn test_delete_view_result_with_confirmed_bool() {
    let task = sample_task();
    let result = delete_view(task).with_confirmed(true).build();
    assert!(result.is_confirmed());

    let task = sample_task();
    let result = delete_view(task).with_confirmed(false).build();
    assert!(!result.is_confirmed());
}

#[test]
fn test_delete_view_result_soft_delete() {
    let task = sample_task();
    let result = delete_view(task)
        .confirmed()
        .soft_deleted()
        .undo_command("task restore task-1")
        .build();

    assert!(result.is_confirmed());
    assert!(result.is_soft_deleted());
    assert!(result.has_undo());
    assert_eq!(result.undo_command, Some("task restore task-1".to_string()));
}

#[test]
fn test_delete_view_result_pending_confirmation() {
    let task = sample_task();
    let result = delete_view(task)
        .warning("Are you sure? Use --confirm to proceed")
        .build();

    assert!(!result.is_confirmed());
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].level, MessageLevel::Warning);
}

#[test]
fn test_delete_view_result_serialization() {
    let task = sample_task();
    let result = delete_view(task)
        .confirmed()
        .soft_deleted()
        .undo_command("restore")
        .success("Deleted")
        .build();

    let json = serde_json::to_value(&result).unwrap();

    assert!(json["item"].is_object());
    assert_eq!(json["confirmed"], true);
    assert_eq!(json["soft_deleted"], true);
    assert_eq!(json["undo_command"], "restore");
}

#[test]
fn test_delete_view_result_serialization_skips_false() {
    let task = sample_task();
    let result = delete_view(task).build();
    let json = serde_json::to_string(&result).unwrap();

    // Should not contain false booleans or None values
    assert!(!json.contains("\"confirmed\""));
    assert!(!json.contains("\"soft_deleted\""));
    assert!(!json.contains("\"undo_command\""));
}

// ============================================================================
// ActionSuggestion tests
// ============================================================================

#[test]
fn test_action_suggestion() {
    let action = ActionSuggestion::new("Edit", "task edit 1");
    assert_eq!(action.label, "Edit");
    assert_eq!(action.command, "task edit 1");
}

// ============================================================================
// ValidationError tests
// ============================================================================

#[test]
fn test_validation_error() {
    let error = ValidationError::new("email", "Invalid email format");
    assert_eq!(error.field, "email");
    assert_eq!(error.message, "Invalid email format");
}
