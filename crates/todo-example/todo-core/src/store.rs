use crate::{Todo, TodoFilter};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct StoreData {
    todos: Vec<Todo>,
    next_id: u32,
}

/// A JSON-backed todo store.
///
/// The path is an explicit dependency: deciding where user data belongs is a
/// concern for the caller (the CLI in this example), not for this library.
pub struct TodoStore {
    path: PathBuf,
    data: Mutex<StoreData>,
}

impl TodoStore {
    /// Loads a store from `path`, or creates an empty in-memory store when the
    /// file does not exist yet.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let data = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?
        } else {
            StoreData::default()
        };

        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    /// Returns todos selected by `filter`.
    pub fn list(&self, filter: TodoFilter) -> Vec<Todo> {
        self.lock()
            .todos
            .iter()
            .filter(|todo| filter == TodoFilter::All || !todo.done)
            .cloned()
            .collect()
    }

    /// Validates and persists a new pending todo.
    pub fn add(&self, title: impl Into<String>) -> Result<Todo> {
        let title = title.into();
        if title.trim().is_empty() {
            bail!("title cannot be empty");
        }

        self.update(|next| {
            next.next_id += 1;
            let todo = Todo {
                id: next.next_id,
                title,
                done: false,
            };
            next.todos.push(todo.clone());
            Ok(todo)
        })
    }

    /// Marks todo `id` done and persists the transition.
    pub fn mark_done(&self, id: u32) -> Result<Todo> {
        self.update(|next| {
            let todo = next
                .todos
                .iter_mut()
                .find(|todo| todo.id == id)
                .with_context(|| format!("no todo with id {id}"))?;
            todo.done = true;
            Ok(todo.clone())
        })
    }

    fn update<T>(&self, change: impl FnOnce(&mut StoreData) -> Result<T>) -> Result<T> {
        let mut current = self.lock();
        let mut next = current.clone();
        let result = change(&mut next)?;
        save(&self.path, &next)?;
        *current = next;
        Ok(result)
    }

    fn lock(&self) -> MutexGuard<'_, StoreData> {
        self.data
            .lock()
            .expect("TodoStore mutex poisoned - this is a bug")
    }
}

fn save(path: &Path, data: &StoreData) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
    }

    let json = serde_json::to_string_pretty(data).context("serializing store")?;
    std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (TodoStore, TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/todos.json");
        (TodoStore::load(&path).unwrap(), dir, path)
    }

    #[test]
    fn missing_file_starts_empty() {
        let (store, _dir, _path) = store();
        assert!(store.list(TodoFilter::All).is_empty());
    }

    #[test]
    fn add_validates_titles() {
        let (store, _dir, path) = store();

        let error = store.add("  \n").unwrap_err();

        assert_eq!(error.to_string(), "title cannot be empty");
        assert!(!path.exists(), "invalid input must not create the store");
    }

    #[test]
    fn add_assigns_ids_and_persists() {
        let (store, _dir, path) = store();

        assert_eq!(store.add("first").unwrap().id, 1);
        assert_eq!(store.add("second").unwrap().id, 2);

        let reloaded = TodoStore::load(path).unwrap();
        let todos = reloaded.list(TodoFilter::All);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].title, "first");
        assert_eq!(todos[1].title, "second");
    }

    #[test]
    fn pending_filter_hides_completed_todos() {
        let (store, _dir, _path) = store();
        store.add("first").unwrap();
        store.add("second").unwrap();
        store.mark_done(1).unwrap();

        let pending = store.list(TodoFilter::Pending);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "second");
        assert_eq!(store.list(TodoFilter::All).len(), 2);
    }

    #[test]
    fn mark_done_persists_the_state_transition() {
        let (store, _dir, path) = store();
        store.add("ship it").unwrap();

        let completed = store.mark_done(1).unwrap();

        assert!(completed.done);
        let reloaded = TodoStore::load(path).unwrap();
        assert!(reloaded.list(TodoFilter::All)[0].done);
    }

    #[test]
    fn missing_id_does_not_change_the_store() {
        let (store, _dir, _path) = store();
        store.add("still pending").unwrap();

        let error = store.mark_done(99).unwrap_err();

        assert_eq!(error.to_string(), "no todo with id 99");
        assert!(!store.list(TodoFilter::All)[0].done);
    }

    #[test]
    fn failed_save_does_not_advance_in_memory_state() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("todos.json");
        let store = TodoStore::load(&path).unwrap();
        std::fs::create_dir(&path).unwrap();

        assert!(store.add("cannot persist").is_err());
        assert!(store.list(TodoFilter::All).is_empty());

        std::fs::remove_dir(&path).unwrap();
        assert_eq!(store.add("first real todo").unwrap().id, 1);
    }
}
