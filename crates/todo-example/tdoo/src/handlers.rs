//! Thin CLI adapters.
//!
//! Each handler translates parsed shell input into a `todo-core` call and
//! translates the result into a serializable view model. Domain validation,
//! filtering, state transitions, and persistence stay in `todo-core`.

#![allow(non_snake_case)]

use serde::Serialize;
use standout::cli::{Artifact, CommandContext, CommandContextInput, Output};
use standout::handler;
use todo_core::{ExportWarning, Todo, TodoFilter, TodoStore};

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

/// The semantic report that rides along with the export artifact.
///
/// It carries no destination: only Standout knows where the bytes went, and it
/// merges its own receipt in after the write.
#[derive(Debug, Serialize)]
pub(crate) struct ExportReportView {
    pub(crate) exported: usize,
    pub(crate) warnings: Vec<ExportWarningView>,
}

/// A core warning as this CLI presents it.
///
/// The taxonomy (`kind`) is the core's, so `--output json` consumers can match
/// on it; the wording is the CLI's, because prose is presentation.
#[derive(Debug, Serialize)]
pub(crate) struct ExportWarningView {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

impl From<&ExportWarning> for ExportWarningView {
    fn from(warning: &ExportWarning) -> Self {
        match warning {
            ExportWarning::CompletedOmitted { count } => Self {
                kind: "completed_omitted",
                message: format!("{count} completed todo(s) omitted; pass --all to include them"),
            },
            ExportWarning::TitleFlattened { id } => Self {
                kind: "title_flattened",
                message: format!("todo #{id}: newlines in the title were flattened to spaces"),
            },
        }
    }
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

/// Exports todos as CSV.
///
/// This is the whole ownership boundary in one function. The core produces the
/// bytes, the filename suggestion, and the typed warnings. The handler maps
/// those into a view model and states which destination opt-ins apply. It does
/// not open a file, pick a directory, or claim success: Standout selects the
/// destination, performs the write, and renders `export.jinja` afterwards with
/// its own receipt merged in.
#[handler]
pub(crate) fn export(
    #[flag] all: bool,
    #[flag] stdout: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<ExportReportView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all {
        TodoFilter::All
    } else {
        TodoFilter::Pending
    };
    let export = store.export_csv(filter);

    let report = ExportReportView {
        exported: export.exported,
        warnings: export
            .warnings
            .iter()
            .map(ExportWarningView::from)
            .collect(),
    };

    let artifact = Artifact::new(export.csv).with_report(report);
    let artifact = if stdout {
        // `tdoo export --stdout > todos.csv`: the user placed the bytes, so
        // Standout routes the report to stderr instead of corrupting them.
        artifact.allow_stdout()
    } else {
        // A suggestion Standout is authorized to write, absent an override.
        artifact.suggest_destination(export.suggested_filename)
    };

    Ok(Output::Artifact(artifact))
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
    fn export_suggests_a_destination_but_writes_nothing() {
        let (ctx, dir) = context();
        let store = ctx.app_state.get_required::<TodoStore>().unwrap();
        store.add("buy milk").unwrap();
        store.add("ship it").unwrap();
        store.mark_done(2).unwrap();

        let Output::Artifact(artifact) = export(false, false, &ctx).unwrap() else {
            panic!("expected an artifact");
        };

        assert_eq!(
            String::from_utf8(artifact.bytes().to_vec()).unwrap(),
            "id,title,done\n1,buy milk,false\n"
        );
        assert_eq!(
            artifact.suggested_destination(),
            Some(std::path::Path::new("todos.csv"))
        );
        assert!(!artifact.stdout_allowed());
        assert!(!dir.path().join("todos.csv").exists());

        // The core's fact, worded by the CLI, still typed for JSON consumers.
        let report = artifact.report().unwrap();
        assert_eq!(report.exported, 1);
        assert_eq!(report.warnings[0].kind, "completed_omitted");
        assert!(report.warnings[0].message.contains("pass --all"));
    }

    #[test]
    fn export_to_stdout_offers_no_destination_suggestion() {
        let (ctx, _dir) = context();
        ctx.app_state
            .get_required::<TodoStore>()
            .unwrap()
            .add("buy milk")
            .unwrap();

        let Output::Artifact(artifact) = export(true, true, &ctx).unwrap() else {
            panic!("expected an artifact");
        };

        assert!(artifact.stdout_allowed());
        assert_eq!(artifact.suggested_destination(), None);
        assert!(artifact.report().unwrap().warnings.is_empty());
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
