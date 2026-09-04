//! The in-repo `tflike` archetype (`corpus/archetypes/tflike/spec.md`) the
//! suites run against; the README beside this package says which gates it
//! answers. Nothing here writes to stderr and nothing here draws progress:
//! `apply` reports each resource as a typed event, and how that reaches the
//! user is the representation's decision, not the handler's.

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use standout::cli::{
    App, CommandContext, Diagnostic, EventsFnHandler, ExitStatus, HandlerResult, Output, Results,
};
use standout::EmbeddedTemplates;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;

const TEMPLATES: &[(&str, &str)] = &[
    (
        "plan.event",
        concat!(
            r#"{% if event.type == "version" %}tflike format {{ event.format_version }}"#,
            r#"{% elif event.type == "planned_change" %}{{ event.action }} {{ event.resource }}"#,
            r#"{% elif event.type == "change_summary" %}Plan: {{ event.add }} to add, {{ event.remove }} to remove.{% endif %}"#,
        ),
    ),
    (
        "apply.event",
        concat!(
            r#"{% if event.type == "version" %}tflike format {{ event.format_version }}"#,
            r#"{% elif event.type == "planned_change" %}{{ event.action }} {{ event.resource }}"#,
            r#"{% elif event.type == "apply_start" %}applying {{ event.resource }}"#,
            r#"{% elif event.type == "apply_complete" %}applied {{ event.resource }}"#,
            r#"{% elif event.type == "change_summary" %}Apply complete: {{ event.add }} added, {{ event.remove }} removed.{% endif %}"#,
        ),
    ),
];

fn command() -> Command {
    let config = Arg::new("config")
        .long("config")
        .value_name("PATH")
        .required(true);
    let state = Arg::new("state").long("state").value_name("PATH");
    Command::new("tflike")
        .subcommand(
            Command::new("plan")
                .arg(config.clone())
                .arg(state.clone())
                .arg(
                    Arg::new("detailed-exitcode")
                        .long("detailed-exitcode")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(Command::new("apply").arg(config).arg(state))
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry {
    Version { format_version: u32 },
    PlannedChange { resource: String, action: Action },
    ApplyStart { resource: String },
    ApplyComplete { resource: String },
    ChangeSummary { add: usize, remove: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Create,
    Delete,
}

struct Change {
    resource: String,
    action: Action,
}

struct Plan {
    changes: Vec<Change>,
    add: usize,
    remove: usize,
}

struct Resource {
    name: String,
    present: bool,
}

struct Loaded {
    plan: Plan,
    state: BTreeSet<String>,
    state_path: PathBuf,
}

fn parse_config(path_as_given: &str) -> Result<Vec<Resource>, anyhow::Error> {
    let text = std::fs::read_to_string(path_as_given)?;
    let mut resources = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (name, present) = match fields.as_slice() {
            ["resource", name, "present"] => (*name, true),
            ["resource", name, "absent"] => (*name, false),
            _ => {
                return Err(
                    Diagnostic::error(format!("line {} does not parse", index + 1))
                        .detail("expected `resource <name> <present|absent>`")
                        .range(path_as_given, (index + 1) as u64, 1)
                        .into(),
                )
            }
        };
        resources.push(Resource {
            name: name.to_string(),
            present,
        });
    }
    Ok(resources)
}

fn read_state(path: &Path) -> Result<BTreeSet<String>, anyhow::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error.into()),
    }
}

fn load(matches: &ArgMatches) -> Result<Loaded, anyhow::Error> {
    let config = matches
        .get_one::<String>("config")
        .expect("--config is required");
    let resources = parse_config(config)?;
    let state_path = matches
        .get_one::<String>("state")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{config}.state")));
    let state = read_state(&state_path)?;
    let changes: Vec<Change> = resources
        .iter()
        .filter_map(|resource| {
            let action = match (resource.present, state.contains(&resource.name)) {
                (true, false) => Action::Create,
                (false, true) => Action::Delete,
                _ => return None,
            };
            Some(Change {
                resource: resource.name.clone(),
                action,
            })
        })
        .collect();
    let add = changes
        .iter()
        .filter(|c| c.action == Action::Create)
        .count();
    let remove = changes.len() - add;
    Ok(Loaded {
        plan: Plan {
            changes,
            add,
            remove,
        },
        state,
        state_path,
    })
}

fn plan(
    matches: &ArgMatches,
    _ctx: &CommandContext,
    results: &mut Results<Entry>,
) -> HandlerResult<()> {
    results.emit(Entry::Version {
        format_version: FORMAT_VERSION,
    })?;
    let Loaded { plan, .. } = load(matches)?;
    for change in &plan.changes {
        results.emit(Entry::PlannedChange {
            resource: change.resource.clone(),
            action: change.action,
        })?;
    }
    results.emit(Entry::ChangeSummary {
        add: plan.add,
        remove: plan.remove,
    })?;
    let changed = matches.get_flag("detailed-exitcode") && !plan.changes.is_empty();
    Ok(if changed {
        Output::Silent.with_exit_status(ExitStatus::from(2))
    } else {
        Output::Silent
    })
}

fn apply(
    matches: &ArgMatches,
    _ctx: &CommandContext,
    results: &mut Results<Entry>,
) -> HandlerResult<()> {
    results.emit(Entry::Version {
        format_version: FORMAT_VERSION,
    })?;
    let Loaded {
        plan,
        mut state,
        state_path,
    } = load(matches)?;
    for change in &plan.changes {
        results.emit(Entry::ApplyStart {
            resource: change.resource.clone(),
        })?;
        let verb = match change.action {
            Action::Create => "creation",
            Action::Delete => "deletion",
        };
        if change.resource.starts_with("fail:") {
            return Err(
                Diagnostic::error(format!("{}: {verb} refused", change.resource))
                    .detail("a resource named fail:<name> refuses every apply")
                    .into(),
            );
        }
        match change.action {
            Action::Create => state.insert(change.resource.clone()),
            Action::Delete => state.remove(&change.resource),
        };
        results.emit(Entry::ApplyComplete {
            resource: change.resource.clone(),
        })?;
    }
    let mut recorded = state.into_iter().collect::<Vec<_>>().join("\n");
    if !recorded.is_empty() {
        recorded.push('\n');
    }
    std::fs::write(&state_path, recorded)?;
    results.emit(Entry::ChangeSummary {
        add: plan.add,
        remove: plan.remove,
    })?;
    Ok(Output::Silent)
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("plan", EventsFnHandler::new(plan), |cfg| cfg)
        .unwrap()
        .command_with("apply", EventsFnHandler::new(apply), |cfg| cfg)
        .unwrap()
        .build()
        .unwrap()
}

fn main() {
    // terraform's single-dash long option, which clap cannot declare.
    let args = std::env::args().map(|arg| match arg.as_str() {
        "-detailed-exitcode" => "--detailed-exitcode".to_string(),
        _ => arg,
    });
    app().run(command(), args);
}
