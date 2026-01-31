//! Integration tests for ResourceStore trait.

use serde::{Deserialize, Serialize};
use standout::cli::{ResourceQuery, ResourceStore};
use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// Test fixtures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Task {
    id: String,
    title: String,
    status: String,
}

#[derive(Debug)]
struct TestError(String);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TestError {}

/// In-memory store for testing
struct InMemoryTaskStore {
    tasks: RwLock<HashMap<String, Task>>,
}

impl InMemoryTaskStore {
    fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    fn with_tasks(tasks: Vec<Task>) -> Self {
        let store = Self::new();
        for task in tasks {
            store.tasks.write().unwrap().insert(task.id.clone(), task);
        }
        store
    }
}

impl ResourceStore for InMemoryTaskStore {
    type Item = Task;
    type Id = String;
    type Error = TestError;

    fn parse_id(&self, id_str: &str) -> Result<Self::Id, Self::Error> {
        if id_str.is_empty() {
            Err(TestError("ID cannot be empty".to_string()))
        } else {
            Ok(id_str.to_string())
        }
    }

    fn get(&self, id: &Self::Id) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.tasks.read().unwrap().get(id).cloned())
    }

    fn not_found_error(id: &Self::Id) -> Self::Error {
        TestError(format!("Task '{}' not found", id))
    }

    fn list(&self, query: Option<&ResourceQuery>) -> Result<Vec<Self::Item>, Self::Error> {
        let tasks = self.tasks.read().unwrap();
        let mut result: Vec<_> = tasks.values().cloned().collect();

        // Sort by id for consistent ordering
        result.sort_by(|a, b| a.id.cmp(&b.id));

        if let Some(q) = query {
            if let Some(limit) = q.limit {
                result.truncate(limit);
            }
        }

        Ok(result)
    }

    fn create(&self, data: serde_json::Value) -> Result<Self::Item, Self::Error> {
        let task: Task = serde_json::from_value(data).map_err(|e| TestError(e.to_string()))?;
        self.tasks
            .write()
            .unwrap()
            .insert(task.id.clone(), task.clone());
        Ok(task)
    }

    fn update(&self, id: &Self::Id, data: serde_json::Value) -> Result<Self::Item, Self::Error> {
        let mut tasks = self.tasks.write().unwrap();
        let task = tasks.get_mut(id).ok_or_else(|| Self::not_found_error(id))?;

        if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
            task.title = title.to_string();
        }
        if let Some(status) = data.get("status").and_then(|v| v.as_str()) {
            task.status = status.to_string();
        }

        Ok(task.clone())
    }

    fn delete(&self, id: &Self::Id) -> Result<(), Self::Error> {
        let mut tasks = self.tasks.write().unwrap();
        tasks.remove(id).ok_or_else(|| Self::not_found_error(id))?;
        Ok(())
    }
}

// ============================================================================
// ResourceQuery tests
// ============================================================================

#[test]
fn test_resource_query_builder() {
    let query = ResourceQuery::new()
        .filter("status=pending")
        .sort("created_at")
        .descending()
        .limit(10)
        .offset(5);

    assert_eq!(query.filter, Some("status=pending".to_string()));
    assert_eq!(query.sort, Some("created_at".to_string()));
    assert!(query.sort_desc);
    assert_eq!(query.limit, Some(10));
    assert_eq!(query.offset, Some(5));
    assert!(query.has_constraints());
}

#[test]
fn test_resource_query_empty() {
    let query = ResourceQuery::new();
    assert!(query.filter.is_none());
    assert!(query.sort.is_none());
    assert!(!query.sort_desc);
    assert!(query.limit.is_none());
    assert!(query.offset.is_none());
    assert!(!query.has_constraints());
}

#[test]
fn test_resource_query_ascending() {
    let query = ResourceQuery::new().sort("name").ascending();
    assert!(!query.sort_desc);
}

// ============================================================================
// ResourceStore implementation tests
// ============================================================================

#[test]
fn test_parse_id_valid() {
    let store = InMemoryTaskStore::new();
    assert_eq!(store.parse_id("task-1").unwrap(), "task-1");
}

#[test]
fn test_parse_id_invalid() {
    let store = InMemoryTaskStore::new();
    assert!(store.parse_id("").is_err());
}

#[test]
fn test_get_existing() {
    let store = InMemoryTaskStore::with_tasks(vec![Task {
        id: "t1".to_string(),
        title: "Test Task".to_string(),
        status: "pending".to_string(),
    }]);

    let task = store.get(&"t1".to_string()).unwrap();
    assert!(task.is_some());
    assert_eq!(task.unwrap().title, "Test Task");
}

#[test]
fn test_get_missing() {
    let store = InMemoryTaskStore::new();
    let task = store.get(&"nonexistent".to_string()).unwrap();
    assert!(task.is_none());
}

#[test]
fn test_resolve_existing() {
    let store = InMemoryTaskStore::with_tasks(vec![Task {
        id: "t1".to_string(),
        title: "Test Task".to_string(),
        status: "pending".to_string(),
    }]);

    let task = store.resolve(&"t1".to_string()).unwrap();
    assert_eq!(task.title, "Test Task");
}

#[test]
fn test_resolve_missing() {
    let store = InMemoryTaskStore::new();
    let result = store.resolve(&"nonexistent".to_string());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_list_all() {
    let store = InMemoryTaskStore::with_tasks(vec![
        Task {
            id: "t1".to_string(),
            title: "First".to_string(),
            status: "pending".to_string(),
        },
        Task {
            id: "t2".to_string(),
            title: "Second".to_string(),
            status: "done".to_string(),
        },
    ]);

    let tasks = store.list(None).unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn test_list_with_limit() {
    let store = InMemoryTaskStore::with_tasks(vec![
        Task {
            id: "t1".to_string(),
            title: "First".to_string(),
            status: "pending".to_string(),
        },
        Task {
            id: "t2".to_string(),
            title: "Second".to_string(),
            status: "done".to_string(),
        },
        Task {
            id: "t3".to_string(),
            title: "Third".to_string(),
            status: "pending".to_string(),
        },
    ]);

    let query = ResourceQuery::new().limit(2);
    let tasks = store.list(Some(&query)).unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn test_create() {
    let store = InMemoryTaskStore::new();

    let data = serde_json::json!({
        "id": "t1",
        "title": "New Task",
        "status": "pending"
    });

    let task = store.create(data).unwrap();
    assert_eq!(task.id, "t1");
    assert_eq!(task.title, "New Task");
    assert_eq!(task.status, "pending");

    // Verify it was stored
    assert!(store.get(&"t1".to_string()).unwrap().is_some());
}

#[test]
fn test_create_invalid_data() {
    let store = InMemoryTaskStore::new();

    // Missing required fields
    let data = serde_json::json!({
        "title": "No ID"
    });

    let result = store.create(data);
    assert!(result.is_err());
}

#[test]
fn test_update() {
    let store = InMemoryTaskStore::with_tasks(vec![Task {
        id: "t1".to_string(),
        title: "Original".to_string(),
        status: "pending".to_string(),
    }]);

    let data = serde_json::json!({
        "title": "Updated Title"
    });

    let task = store.update(&"t1".to_string(), data).unwrap();
    assert_eq!(task.title, "Updated Title");
    assert_eq!(task.status, "pending"); // Unchanged
}

#[test]
fn test_update_multiple_fields() {
    let store = InMemoryTaskStore::with_tasks(vec![Task {
        id: "t1".to_string(),
        title: "Original".to_string(),
        status: "pending".to_string(),
    }]);

    let data = serde_json::json!({
        "title": "New Title",
        "status": "done"
    });

    let task = store.update(&"t1".to_string(), data).unwrap();
    assert_eq!(task.title, "New Title");
    assert_eq!(task.status, "done");
}

#[test]
fn test_update_missing() {
    let store = InMemoryTaskStore::new();

    let data = serde_json::json!({
        "title": "Updated"
    });

    let result = store.update(&"nonexistent".to_string(), data);
    assert!(result.is_err());
}

#[test]
fn test_delete() {
    let store = InMemoryTaskStore::with_tasks(vec![Task {
        id: "t1".to_string(),
        title: "Test".to_string(),
        status: "pending".to_string(),
    }]);

    store.delete(&"t1".to_string()).unwrap();
    assert!(store.get(&"t1".to_string()).unwrap().is_none());
}

#[test]
fn test_delete_missing() {
    let store = InMemoryTaskStore::new();
    let result = store.delete(&"nonexistent".to_string());
    assert!(result.is_err());
}

// ============================================================================
// Full Resource workflow test
// ============================================================================

#[test]
fn test_resource_workflow() {
    let store = InMemoryTaskStore::new();

    // Create
    let task = store
        .create(serde_json::json!({
            "id": "workflow-1",
            "title": "Workflow Task",
            "status": "pending"
        }))
        .unwrap();
    assert_eq!(task.title, "Workflow Task");

    // Read (list)
    let all = store.list(None).unwrap();
    assert_eq!(all.len(), 1);

    // Read (single)
    let retrieved = store.resolve(&"workflow-1".to_string()).unwrap();
    assert_eq!(retrieved.title, "Workflow Task");

    // Update
    let updated = store
        .update(
            &"workflow-1".to_string(),
            serde_json::json!({"status": "done"}),
        )
        .unwrap();
    assert_eq!(updated.status, "done");

    // Delete
    store.delete(&"workflow-1".to_string()).unwrap();

    // Verify deleted
    let all = store.list(None).unwrap();
    assert!(all.is_empty());
}
