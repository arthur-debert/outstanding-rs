//! The in-repo `tflike` archetype (`corpus/archetypes/tflike/spec.md`): the
//! binary `tests/tflike_diagnostic.rs` and `tests/tflike_progress.rs` run
//! against. It carries exactly the capability the framework has, so the
//! assertions still wrapped in `expect_gap` keep failing against it: the plan
//! and its diagnostics ride the `ndjson` stream; `-detailed-exitcode` is
//! accepted but declares no exit status, there being no way for a handler to
//! declare one; `apply` emits no lifecycle events and reports each completed
//! step as stderr prose in every mode, there being no progress seam. The
//! README beside this package maps each of those to the work that closes it.

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use standout::cli::{App, CommandContext, Diagnostic, FnHandler, HandlerResult, Output};
use standout::EmbeddedTemplates;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;

const TEMPLATES: &[(&str, &str)] = &[
    (
        "plan",
        "{% for change in changes %}{{ change.action }} {{ change.resource }}\n{% endfor %}\
         Plan: {{ add }} to add, {{ remove }} to remove.",
    ),
    (
        "apply",
        "Apply complete: {{ add }} added, {{ remove }} removed.",
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
enum Entry<'a> {
    Version { format_version: u32 },
    PlannedChange { resource: &'a str, action: Action },
    ChangeSummary { add: usize, remove: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Create,
    Delete,
}

#[derive(Serialize)]
struct Change {
    resource: String,
    action: Action,
}

#[derive(Serialize)]
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

fn plan(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Plan> {
    let stream = ctx.stream();
    stream.emit(&Entry::Version {
        format_version: FORMAT_VERSION,
    })?;
    let Loaded { plan, .. } = load(matches)?;
    for change in &plan.changes {
        stream.emit(&Entry::PlannedChange {
            resource: &change.resource,
            action: change.action,
        })?;
    }
    stream.emit(&Entry::ChangeSummary {
        add: plan.add,
        remove: plan.remove,
    })?;
    Ok(if stream.is_live() {
        Output::Silent
    } else {
        Output::Render(plan)
    })
}

fn apply(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Plan> {
    let stream = ctx.stream();
    stream.emit(&Entry::Version {
        format_version: FORMAT_VERSION,
    })?;
    let Loaded {
        plan,
        mut state,
        state_path,
    } = load(matches)?;
    for change in &plan.changes {
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
        eprintln!("{}: {verb} complete", change.resource);
    }
    let mut recorded = state.into_iter().collect::<Vec<_>>().join("\n");
    if !recorded.is_empty() {
        recorded.push('\n');
    }
    std::fs::write(&state_path, recorded)?;
    stream.emit(&Entry::ChangeSummary {
        add: plan.add,
        remove: plan.remove,
    })?;
    Ok(if stream.is_live() {
        Output::Silent
    } else {
        Output::Render(plan)
    })
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("plan", FnHandler::new(plan), |cfg| cfg)
        .unwrap()
        .command_with("apply", FnHandler::new(apply), |cfg| cfg)
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
