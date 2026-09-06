use crate::config::TdooConfig;
use clap::ArgMatches;
use serde::Serialize;
use standout::cli::{
    Artifact, CommandConfig, CommandContext, CommandContextInput, HookError, Output,
};
use standout::handler;
use standout::input::{ArgSource, ConfigSource, FlagSource, InputChain, StdinSource};
use standout::RenderData as JsonValue;
use todo_core::{ExportWarning, Todo, TodoFilter, TodoStore};

/// The title a new todo carries: `--title`, else piped stdin.
pub(crate) fn add_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H> {
    config.input(
        "title",
        InputChain::<String>::new()
            .try_source(ArgSource::new("title"))
            .try_source(StdinSource::new())
            .validate(
                |title: &String| !title.trim().is_empty(),
                "title cannot be empty",
            ),
    )
}

/// Fails the command when the append to `TODO_AUDIT_LOG` fails.
pub(crate) fn audit_hook(
    _matches: &ArgMatches,
    ctx: &CommandContext,
    value: JsonValue,
) -> Result<JsonValue, HookError> {
    if let Ok(path) = std::env::var("TODO_AUDIT_LOG") {
        let line = format!(
            "{}\t{}\n",
            ctx.command_path.join("."),
            value
                .get("todo")
                .and_then(|todo| todo.get("id"))
                .unwrap_or(&JsonValue::Null)
        );
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()))
            .map_err(|error| {
                HookError::post_dispatch(format!("cannot append to the audit log {path}"))
                    .with_source(error)
            })?;
    }
    Ok(value)
}

fn store(ctx: &CommandContext) -> Result<TodoStore, anyhow::Error> {
    let config: &TdooConfig = ctx.config()?;
    TodoStore::load(config.store_path())
}

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

#[derive(Debug, Serialize)]
pub(crate) struct ExportReportView {
    pub(crate) exported: usize,
    pub(crate) warnings: Vec<ExportWarningView>,
}

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
    #[matches] matches: &ArgMatches,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let config: &TdooConfig = ctx.config()?;
    let reverse = InputChain::<bool>::new()
        .try_source(FlagSource::new("reverse"))
        .try_source(ConfigSource::new(Some(config.reverse)))
        .resolve_from(matches, ctx.input_sources())?;
    let store = store(ctx)?;
    let filter = if all {
        TodoFilter::All
    } else {
        TodoFilter::Pending
    };
    let mut todos: Vec<_> = store.list(filter).into_iter().map(TodoView::from).collect();
    if reverse {
        todos.reverse();
    }
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}

#[handler]
pub(crate) fn add(#[ctx] ctx: &CommandContext) -> Result<Output<TodoActionView>, anyhow::Error> {
    let title: &String = ctx.input("title")?;
    let todo = store(ctx)?.add(title.clone())?;
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
    let todo = store(ctx)?.mark_done(id)?;
    Ok(Output::Render(TodoActionView {
        message: format!("Marked #{} done", todo.id),
        todo: todo.into(),
    }))
}

#[handler]
pub(crate) fn export(
    #[flag] all: bool,
    #[flag] stdout: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<ExportReportView>, anyhow::Error> {
    let store = store(ctx)?;
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
        artifact.allow_stdout()
    } else {
        artifact.suggest_destination(export.suggested_filename)
    };

    Ok(Output::Artifact(artifact))
}

#[cfg(test)]
mod tests {
    use super::*;
    use standout::dispatch::Extensions;
    use standout::input::{InputSourceKind, Inputs, ResolvedInput};
    use standout::{InputSources, TermSettings};
    use std::path::PathBuf;
    use std::rc::Rc;
    use tempfile::TempDir;

    fn context(reverse: bool) -> (CommandContext, TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("todos.json");
        let mut ctx = CommandContext::new(Vec::new(), Rc::new(Extensions::new()));
        ctx.extensions.insert(InputSources::from_process());
        ctx.install_config(TdooConfig {
            store: Some(path.to_str().unwrap().to_string()),
            reverse,
            term: TermSettings::default(),
        });
        (ctx, dir, path)
    }

    fn list_matches(args: &[&str]) -> ArgMatches {
        crate::cli::command()
            .try_get_matches_from([&["tdoo", "list"], args].concat())
            .unwrap()
            .subcommand_matches("list")
            .unwrap()
            .clone()
    }

    #[test]
    fn list_maps_the_flag_to_the_core_filter() {
        let (ctx, _dir, path) = context(false);
        let store = TodoStore::load(&path).unwrap();
        store.add("completed").unwrap();
        store.add("pending").unwrap();
        store.mark_done(1).unwrap();

        let Output::Render(pending) = list(false, &list_matches(&[]), &ctx).unwrap() else {
            panic!("expected rendered data");
        };
        let Output::Render(all) = list(true, &list_matches(&[]), &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(pending.total, 1);
        assert_eq!(pending.todos[0].title, "pending");
        assert_eq!(all.total, 2);
    }

    #[test]
    fn list_reads_the_ordering_from_the_resolved_config_in_the_context() {
        let (ctx, _dir, path) = context(true);
        let store = TodoStore::load(&path).unwrap();
        store.add("first").unwrap();
        store.add("second").unwrap();

        let Output::Render(view) = list(false, &list_matches(&[]), &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(view.todos[0].title, "second");
        assert_eq!(view.todos[1].title, "first");
    }

    #[test]
    fn add_maps_resolved_input_to_an_action_view() {
        let (mut ctx, _dir, _path) = context(false);
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
        let (ctx, dir, path) = context(false);
        let store = TodoStore::load(&path).unwrap();
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

        let report = artifact.report().unwrap();
        assert_eq!(report.exported, 1);
        assert_eq!(report.warnings[0].kind, "completed_omitted");
        assert!(report.warnings[0].message.contains("pass --all"));
    }

    #[test]
    fn export_to_stdout_offers_no_destination_suggestion() {
        let (ctx, _dir, path) = context(false);
        TodoStore::load(&path).unwrap().add("buy milk").unwrap();

        let Output::Artifact(artifact) = export(true, true, &ctx).unwrap() else {
            panic!("expected an artifact");
        };

        assert!(artifact.stdout_allowed());
        assert_eq!(artifact.suggested_destination(), None);
        assert!(artifact.report().unwrap().warnings.is_empty());
    }

    #[test]
    fn done_maps_the_core_transition_to_an_action_view() {
        let (ctx, _dir, path) = context(false);
        TodoStore::load(&path).unwrap().add("ship it").unwrap();

        let Output::Render(view) = done(1, &ctx).unwrap() else {
            panic!("expected rendered data");
        };

        assert_eq!(view.message, "Marked #1 done");
        assert!(view.todo.done);
    }
}
