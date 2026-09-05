use crate::setup::SetupError;
use clap::{Arg, Command};
use standout_dispatch::verify::{verify_handler_args, ExpectedArg};
use std::collections::HashMap;

pub(crate) fn find_subcommand_recursive<'a>(
    cmd: &'a Command,
    keywords: &[&str],
) -> Option<&'a Command> {
    let mut current = cmd;
    for k in keywords {
        if let Some(sub) = find_subcommand(current, k) {
            current = sub;
        } else {
            return None;
        }
    }
    Some(current)
}

pub(crate) fn find_subcommand<'a>(cmd: &'a Command, name: &str) -> Option<&'a Command> {
    cmd.get_subcommands()
        .find(|s| s.get_name() == name || s.get_aliases().any(|a| a == name))
}

/// Canonical names only: clap resolves an alias before reporting, so an alias registration is dead.
pub(crate) fn find_canonical_subcommand_recursive<'a>(
    cmd: &'a Command,
    keywords: &[&str],
) -> Option<&'a Command> {
    let mut current = cmd;
    for k in keywords {
        current = current.get_subcommands().find(|s| s.get_name() == *k)?;
    }
    Some(current)
}

pub(crate) fn with_globals_propagated(cmd: &Command) -> Command {
    let mut propagated = cmd.clone();
    propagate_globals(&mut propagated);
    propagated
}

fn propagate_globals(cmd: &mut Command) {
    let globals: Vec<Arg> = cmd
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .cloned()
        .collect();

    for sub in cmd.get_subcommands_mut() {
        let inherited: Vec<Arg> = globals
            .iter()
            .filter(|global| {
                !sub.get_arguments()
                    .any(|declared| declared.get_id() == global.get_id())
            })
            .cloned()
            .collect();
        if !inherited.is_empty() {
            let declared = std::mem::take(sub);
            *sub = declared.args(inherited);
        }
        propagate_globals(sub);
    }
}

pub(crate) fn verify_recursive(
    cmd: &Command,
    expected_args: &HashMap<String, Vec<ExpectedArg>>,
    parent_path: &[&str],
    is_root: bool,
) -> Result<(), SetupError> {
    let mut current_path = parent_path.to_vec();
    if !is_root && !cmd.get_name().is_empty() {
        current_path.push(cmd.get_name());
    }

    let path_str = current_path.join(".");
    if let Some(expected) = expected_args.get(&path_str) {
        verify_handler_args(cmd, &path_str, expected)?;
    }

    for sub in cmd.get_subcommands() {
        verify_recursive(sub, expected_args, &current_path, false)?;
    }

    Ok(())
}
