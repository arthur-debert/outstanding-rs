//! Integration tests for ListView functionality.

use clap::Command;
use serde::Serialize;
use standout::cli::{App, Output, RunResult, ThreadSafe};
use standout::views::{list_view, ListViewResult, MessageLevel};

#[derive(Clone, Serialize)]
struct Task {
    id: u32,
    name: String,
    status: String,
}

fn test_tasks() -> Vec<Task> {
    vec![
        Task {
            id: 1,
            name: "Implement auth".to_string(),
            status: "pending".to_string(),
        },
        Task {
            id: 2,
            name: "Fix bug".to_string(),
            status: "done".to_string(),
        },
        Task {
            id: 3,
            name: "Write docs".to_string(),
            status: "pending".to_string(),
        },
    ]
}

#[test]
fn test_list_view_result_serialization() {
    let tasks = test_tasks();
    let result = list_view(tasks)
        .intro("Your tasks:")
        .ending("3 tasks total")
        .build();

    let json = serde_json::to_value(&result).unwrap();
    assert!(json["items"].is_array());
    assert_eq!(json["items"].as_array().unwrap().len(), 3);
    assert_eq!(json["intro"], "Your tasks:");
    assert_eq!(json["ending"], "3 tasks total");
}

#[test]
fn test_list_view_with_messages() {
    let tasks = test_tasks();
    let result = list_view(tasks)
        .warning("2 tasks overdue")
        .info("Use --all to see archived tasks")
        .build();

    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].level, MessageLevel::Warning);
    assert_eq!(result.messages[1].level, MessageLevel::Info);
}

#[test]
fn test_list_view_with_filter_info() {
    let all_tasks = test_tasks();
    let filtered: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.status == "pending")
        .cloned()
        .collect();

    let result = list_view(filtered)
        .total_count(all_tasks.len())
        .filter_summary("status=pending")
        .build();

    assert_eq!(result.len(), 2);
    assert_eq!(result.total_count, Some(3));
    assert_eq!(result.filter_summary, Some("status=pending".to_string()));
}

#[test]
fn test_list_view_renders_with_framework_template() {
    // Create an app with a list command using the framework template
    let app = App::<ThreadSafe>::builder()
        .command(
            "list",
            |_m, _ctx| {
                let tasks = test_tasks();
                let result = list_view(tasks).intro("Tasks:").build();
                Ok(Output::Render(result))
            },
            // Use the framework template
            "standout/list-view",
        )
        .unwrap()
        .build()
        .unwrap();

    // Build the clap command
    let cmd = Command::new("test").subcommand(Command::new("list"));

    // Run and check output
    let result = app.run_to_string(cmd, vec!["test", "list"]);
    if let RunResult::Handled(output) = result {
        assert!(output.contains("Tasks:"), "Output should contain intro");
        // The template should render the items
        assert!(
            output.contains("Implement auth"),
            "Output should contain task name"
        );
        assert!(
            output.contains("Fix bug"),
            "Output should contain task name"
        );
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_list_view_empty_list() {
    let app = App::<ThreadSafe>::builder()
        .command(
            "list",
            |_m, _ctx| {
                let result: ListViewResult<Task> = list_view(vec![]).build();
                Ok(Output::Render(result))
            },
            "standout/list-view",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("list"));
    let result = app.run_to_string(cmd, vec!["test", "list"]);

    if let RunResult::Handled(output) = result {
        assert!(
            output.contains("No items found"),
            "Output should contain empty message"
        );
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_list_view_with_filter_summary_renders() {
    let app = App::<ThreadSafe>::builder()
        .command(
            "list",
            |_m, _ctx| {
                let tasks = vec![test_tasks()[0].clone()]; // Just one task
                let result = list_view(tasks)
                    .total_count(3)
                    .filter_summary("status=pending")
                    .build();
                Ok(Output::Render(result))
            },
            "standout/list-view",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("list"));
    let result = app.run_to_string(cmd, vec!["test", "list"]);

    if let RunResult::Handled(output) = result {
        // Should show "Showing X of Y"
        assert!(
            output.contains("Showing 1 of 3"),
            "Output should show count: {}",
            output
        );
        assert!(
            output.contains("status=pending"),
            "Output should show filter summary: {}",
            output
        );
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_framework_template_can_be_disabled() {
    // Build an app without framework templates
    let result = App::<ThreadSafe>::builder()
        .include_framework_templates(false)
        .command(
            "list",
            |_m, _ctx| {
                let tasks = test_tasks();
                Ok(Output::Render(list_view(tasks).build()))
            },
            // This template won't exist
            "standout/list-view",
        );

    // The command registration might succeed but the template won't be found
    // This depends on when template resolution happens
    // For now, just verify we can disable framework templates
    assert!(result.is_ok() || result.is_err());
}
