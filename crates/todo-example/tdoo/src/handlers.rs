//! Thin CLI adapters.
//!
//! Each handler translates parsed shell input into a `todo-core` call and
//! translates the result into a serializable view model. Domain validation,
//! filtering, state transitions, and persistence stay in `todo-core`.

#![allow(non_snake_case)]

use serde::Serialize;
use standout::cli::{CommandContext, CommandContextInput, Output};
use standout::handler;
use todo_core::{Todo, TodoFilter, TodoStore};

#[derive(Debug, Serialize)]
pub(crate) struct TodoView {
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) done: bool,
}

impl From<Todo> for TodoView {
    fn from(todo: Todo) -> Self {
        Self {
            id: todo.id,
            title: todo.title,
            done: todo.done,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct TodoListView {
    pub(crate) todos: Vec<TodoView>,
    pub(crate) total: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct TodoActionView {
    pub(crate) message: String,
    pub(crate) todo: TodoView,
}

#[handler]
pub(crate) fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all {
        TodoFilter::All
    } else {
        TodoFilter::Pending
    };
    let todos: Vec<_> = store.list(filter).into_iter().map(TodoView::from).collect();
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}

#[handler]
pub(crate) fn add(#[ctx] ctx: &CommandContext) -> Result<Output<TodoActionView>, anyhow::Error> {
    let title: &String = ctx.input("title")?;
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let todo = store.add(title.clone())?;
    Ok(Output::Render(TodoActionView {
        message: format!("Added #{}", todo.id),
        todo: todo.into(),
    }))
}

#[handler]
pub(crate) fn done(
    #[arg] id: u32,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoActionView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let todo = store.mark_done(id)?;
    Ok(Output::Render(TodoActionView {
        message: format!("Marked #{} done", todo.id),
        todo: todo.into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use standout_dispatch::Extensions;
    use standout_input::{InputSourceKind, Inputs, ResolvedInput};
    use std::rc::Rc;
    use tempfile::TempDir;

    fn context() -> (CommandContext, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = TodoStore::load(dir.path().join("todos.json")).unwrap();
        let mut state = Extensions::new();
        state.insert(store);
        (CommandContext::new(Vec::new(), Rc::new(state)), dir)
    }

    #[test]
    fn list_maps_the_flag_to_the_core_filter() {
        let (ctx, _dir) = context();
        let store = ctx.app_state.get_required::<TodoStore>().unwrap();
        store.add("completed").unwrap();
        store.add("pending").unwrap();
        store.mark_done(1).unwrap();

        let Output::Render(pending) = list(false, &ctx).unwrap() else {
            panic!("expected rendered data");
        };
        let Output::Render(all) = list(true, &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(pending.total, 1);
        assert_eq!(pending.todos[0].title, "pending");
        assert_eq!(all.total, 2);
    }

    #[test]
    fn add_maps_resolved_input_to_an_action_view() {
        let (mut ctx, _dir) = context();
        let mut inputs = Inputs::new();
        inputs.insert(
            "title",
            ResolvedInput {
                value: "write docs".to_string(),
                source: InputSourceKind::Arg,
            },
        );
        ctx.extensions.insert(inputs);

        let Output::Render(view) = add(&ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(view.message, "Added #1");
        assert_eq!(view.todo.title, "write docs");
    }

    #[test]
    fn done_maps_the_core_transition_to_an_action_view() {
        let (ctx, _dir) = context();
        ctx.app_state
            .get_required::<TodoStore>()
            .unwrap()
            .add("ship it")
            .unwrap();

        let Output::Render(view) = done(1, &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(view.message, "Marked #1 done");
        assert!(view.todo.done);
    }
}
