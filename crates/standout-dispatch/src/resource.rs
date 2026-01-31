//! Resource store trait for object-centric CLI operations.
//!
//! This module provides the [`ResourceStore`] trait that users implement to connect
//! their data stores to the Resource framework. The trait is sync-only; for async
//! stores, users should use `block_on()` internally.
//!
//! # Example
//!
//! ```rust,ignore
//! use standout_dispatch::ResourceStore;
//!
//! struct TaskStore {
//!     db: Database,
//! }
//!
//! impl ResourceStore for TaskStore {
//!     type Item = Task;
//!     type Id = String;
//!     type Error = DatabaseError;
//!
//!     fn parse_id(&self, id_str: &str) -> Result<Self::Id, Self::Error> {
//!         Ok(id_str.to_string())
//!     }
//!
//!     fn get(&self, id: &Self::Id) -> Result<Option<Self::Item>, Self::Error> {
//!         self.db.find_task(id)
//!     }
//!
//!     fn not_found_error(id: &Self::Id) -> Self::Error {
//!         DatabaseError::NotFound(format!("Task '{}' not found", id))
//!     }
//!
//!     fn list(&self, query: Option<&ResourceQuery>) -> Result<Vec<Self::Item>, Self::Error> {
//!         self.db.list_tasks(query)
//!     }
//!
//!     fn create(&self, data: serde_json::Value) -> Result<Self::Item, Self::Error> {
//!         let task: Task = serde_json::from_value(data)?;
//!         self.db.insert_task(task)
//!     }
//!
//!     fn update(&self, id: &Self::Id, data: serde_json::Value) -> Result<Self::Item, Self::Error> {
//!         self.db.update_task(id, data)
//!     }
//!
//!     fn delete(&self, id: &Self::Id) -> Result<(), Self::Error> {
//!         self.db.delete_task(id)
//!     }
//! }
//! ```

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Display;
use std::str::FromStr;

/// Query parameters for list operations.
///
/// This struct captures common filtering, sorting, and pagination options
/// that can be passed to [`ResourceStore::list`].
#[derive(Debug, Clone, Default)]
pub struct ResourceQuery {
    /// Filter expression (e.g., "status=pending")
    pub filter: Option<String>,
    /// Sort field (e.g., "created_at")
    pub sort: Option<String>,
    /// Sort direction
    pub sort_desc: bool,
    /// Maximum number of items to return
    pub limit: Option<usize>,
    /// Number of items to skip
    pub offset: Option<usize>,
}

impl ResourceQuery {
    /// Creates a new empty query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the filter expression.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Sets the sort field.
    pub fn sort(mut self, field: impl Into<String>) -> Self {
        self.sort = Some(field.into());
        self
    }

    /// Sets sort direction to descending.
    pub fn descending(mut self) -> Self {
        self.sort_desc = true;
        self
    }

    /// Sets sort direction to ascending.
    pub fn ascending(mut self) -> Self {
        self.sort_desc = false;
        self
    }

    /// Sets the limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the offset.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Returns true if this query has any filters or constraints.
    pub fn has_constraints(&self) -> bool {
        self.filter.is_some()
            || self.sort.is_some()
            || self.limit.is_some()
            || self.offset.is_some()
    }
}

/// Trait for Resource storage backends.
///
/// Implement this trait to connect your data store (database, file, API, etc.)
/// to the Resource framework. The framework handles CLI argument parsing, validation,
/// and view rendering while delegating all data operations to this trait.
///
/// # Design Notes
///
/// - **Sync-only**: This trait is synchronous. For async stores, use `block_on()`
///   internally or implement a sync wrapper.
///
/// - **Two-stage ID resolution**: `parse_id` validates ID format before `get`
///   fetches the item. This allows early validation errors.
///
/// - **JSON data**: Create and update operations receive data as `serde_json::Value`.
///   This allows the framework to handle field extraction from CLI args uniformly.
///
/// # Type Parameters
///
/// - `Item`: The domain object type (must be serializable/deserializable)
/// - `Id`: The identifier type (must be displayable and parseable from strings)
/// - `Error`: The error type (must be a standard error)
pub trait ResourceStore: Send + Sync {
    /// The domain object type.
    type Item: Serialize + DeserializeOwned;

    /// The identifier type.
    type Id: Clone + Display + FromStr;

    /// The error type for storage operations.
    type Error: std::error::Error + Send + 'static;

    /// Parses an ID string into the store's ID type.
    ///
    /// This is called before `get` to validate ID format. Implementors should
    /// return an error if the ID format is invalid (e.g., not a valid UUID).
    fn parse_id(&self, id_str: &str) -> Result<Self::Id, Self::Error>;

    /// Retrieves an item by ID, returning `None` if not found.
    ///
    /// This is the low-level fetch operation. Use [`resolve`](Self::resolve)
    /// for the convenience method that converts `None` to an error.
    fn get(&self, id: &Self::Id) -> Result<Option<Self::Item>, Self::Error>;

    /// Creates an error for when an item is not found.
    ///
    /// This is used by [`resolve`](Self::resolve) to convert `None` results
    /// into meaningful error messages.
    fn not_found_error(id: &Self::Id) -> Self::Error;

    /// Retrieves an item by ID, returning an error if not found.
    ///
    /// This is a convenience method that combines `get` with `not_found_error`.
    fn resolve(&self, id: &Self::Id) -> Result<Self::Item, Self::Error> {
        self.get(id)?.ok_or_else(|| Self::not_found_error(id))
    }

    /// Lists items, optionally filtered by the given query.
    fn list(&self, query: Option<&ResourceQuery>) -> Result<Vec<Self::Item>, Self::Error>;

    /// Creates a new item from the given data.
    ///
    /// The data is provided as a JSON value containing the field values
    /// extracted from CLI arguments.
    fn create(&self, data: serde_json::Value) -> Result<Self::Item, Self::Error>;

    /// Updates an existing item with the given data.
    ///
    /// The data is provided as a JSON value containing only the fields
    /// that should be updated.
    fn update(&self, id: &Self::Id, data: serde_json::Value) -> Result<Self::Item, Self::Error>;

    /// Deletes an item by ID.
    fn delete(&self, id: &Self::Id) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::RwLock;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Task {
        id: String,
        title: String,
        done: bool,
    }

    #[derive(Debug)]
    struct TestError(String);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestError {}

    struct InMemoryStore {
        tasks: RwLock<HashMap<String, Task>>,
    }

    impl InMemoryStore {
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

    impl ResourceStore for InMemoryStore {
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

        fn update(
            &self,
            id: &Self::Id,
            data: serde_json::Value,
        ) -> Result<Self::Item, Self::Error> {
            let mut tasks = self.tasks.write().unwrap();
            let task = tasks.get_mut(id).ok_or_else(|| Self::not_found_error(id))?;

            if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                task.title = title.to_string();
            }
            if let Some(done) = data.get("done").and_then(|v| v.as_bool()) {
                task.done = done;
            }

            Ok(task.clone())
        }

        fn delete(&self, id: &Self::Id) -> Result<(), Self::Error> {
            let mut tasks = self.tasks.write().unwrap();
            tasks.remove(id).ok_or_else(|| Self::not_found_error(id))?;
            Ok(())
        }
    }

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
        assert!(!query.has_constraints());
    }

    #[test]
    fn test_parse_id_valid() {
        let store = InMemoryStore::new();
        assert_eq!(store.parse_id("task-1").unwrap(), "task-1");
    }

    #[test]
    fn test_parse_id_invalid() {
        let store = InMemoryStore::new();
        assert!(store.parse_id("").is_err());
    }

    #[test]
    fn test_get_existing() {
        let store = InMemoryStore::with_tasks(vec![Task {
            id: "t1".to_string(),
            title: "Test".to_string(),
            done: false,
        }]);

        let task = store.get(&"t1".to_string()).unwrap();
        assert!(task.is_some());
        assert_eq!(task.unwrap().title, "Test");
    }

    #[test]
    fn test_get_missing() {
        let store = InMemoryStore::new();
        let task = store.get(&"nonexistent".to_string()).unwrap();
        assert!(task.is_none());
    }

    #[test]
    fn test_resolve_existing() {
        let store = InMemoryStore::with_tasks(vec![Task {
            id: "t1".to_string(),
            title: "Test".to_string(),
            done: false,
        }]);

        let task = store.resolve(&"t1".to_string()).unwrap();
        assert_eq!(task.title, "Test");
    }

    #[test]
    fn test_resolve_missing() {
        let store = InMemoryStore::new();
        let result = store.resolve(&"nonexistent".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_list_all() {
        let store = InMemoryStore::with_tasks(vec![
            Task {
                id: "t1".to_string(),
                title: "First".to_string(),
                done: false,
            },
            Task {
                id: "t2".to_string(),
                title: "Second".to_string(),
                done: true,
            },
        ]);

        let tasks = store.list(None).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_list_with_limit() {
        let store = InMemoryStore::with_tasks(vec![
            Task {
                id: "t1".to_string(),
                title: "First".to_string(),
                done: false,
            },
            Task {
                id: "t2".to_string(),
                title: "Second".to_string(),
                done: true,
            },
        ]);

        let query = ResourceQuery::new().limit(1);
        let tasks = store.list(Some(&query)).unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_create() {
        let store = InMemoryStore::new();

        let data = serde_json::json!({
            "id": "t1",
            "title": "New Task",
            "done": false
        });

        let task = store.create(data).unwrap();
        assert_eq!(task.id, "t1");
        assert_eq!(task.title, "New Task");
        assert!(!task.done);

        // Verify it was stored
        assert!(store.get(&"t1".to_string()).unwrap().is_some());
    }

    #[test]
    fn test_update() {
        let store = InMemoryStore::with_tasks(vec![Task {
            id: "t1".to_string(),
            title: "Original".to_string(),
            done: false,
        }]);

        let data = serde_json::json!({
            "title": "Updated"
        });

        let task = store.update(&"t1".to_string(), data).unwrap();
        assert_eq!(task.title, "Updated");
        assert!(!task.done); // Unchanged
    }

    #[test]
    fn test_update_missing() {
        let store = InMemoryStore::new();

        let data = serde_json::json!({
            "title": "Updated"
        });

        let result = store.update(&"nonexistent".to_string(), data);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete() {
        let store = InMemoryStore::with_tasks(vec![Task {
            id: "t1".to_string(),
            title: "Test".to_string(),
            done: false,
        }]);

        store.delete(&"t1".to_string()).unwrap();
        assert!(store.get(&"t1".to_string()).unwrap().is_none());
    }

    #[test]
    fn test_delete_missing() {
        let store = InMemoryStore::new();
        let result = store.delete(&"nonexistent".to_string());
        assert!(result.is_err());
    }
}
