use serde::{Deserialize, Serialize};

/// A persisted todo item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

/// Selects the todos returned by [`crate::TodoStore::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoFilter {
    Pending,
    All,
}
