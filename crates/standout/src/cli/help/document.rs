//! The help document: what `--help`, `-h` and the `help` word answer with
//! under `json` and `yaml` (`docs/topics/standout-help.md`).
//!
//! It is built from the same clap tree as the human page, after `build()`,
//! so clap's own `-h`/`--help` and `-V`/`--version` are among its `args`.
//! `short` and `long` are the tokens as typed, dashes included; `name` is the
//! clap id. `usage` and `path` name the full command path from the root by
//! its canonical names, whichever aliases the caller typed.

use clap::Command;
use serde::{Deserialize, Serialize};

use super::config::HelpLength;
use super::data::{
    about_line, build_root, default_value, possible_values, takes_values, usage_line,
};
use crate::cli::app::find_subcommand;
use crate::ContractSurface;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpDocument {
    pub schema_version: u32,
    pub name: String,
    pub path: Vec<String>,
    pub usage: String,
    pub about: String,
    pub args: Vec<HelpArg>,
    pub subcommands: Vec<HelpSubcommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpArg {
    pub name: String,
    pub short: Option<String>,
    pub long: Option<String>,
    pub value_name: Option<String>,
    pub required: bool,
    pub help: String,
    pub default: Option<String>,
    pub possible_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpSubcommand {
    pub name: String,
    pub about: String,
}

impl ContractSurface for HelpDocument {
    const SCHEMA_VERSION: u32 = 1;
}

impl HelpDocument {
    /// `None` when no command sits at `path` (empty for the root).
    pub fn extract(root: &Command, path: &[&str], length: HelpLength) -> Option<Self> {
        let built = build_root(root);
        let mut cmd = &built;
        let mut full_path = vec![built.get_name().to_string()];
        for word in path {
            cmd = find_subcommand(cmd, word)?;
            full_path.push(cmd.get_name().to_string());
        }

        let mut args: Vec<_> = cmd.get_arguments().filter(|a| !a.is_hide_set()).collect();
        args.sort_by_key(|a| (!a.is_positional(), a.get_display_order()));

        let mut subcommands: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
        subcommands.sort_by_key(|s| s.get_display_order());

        Some(Self {
            schema_version: Self::SCHEMA_VERSION,
            name: cmd.get_name().to_string(),
            path: full_path,
            usage: usage_line(cmd),
            about: about_line(cmd, length),
            args: args.into_iter().map(HelpArg::from).collect(),
            subcommands: subcommands
                .into_iter()
                .map(|sub| HelpSubcommand {
                    name: sub.get_name().to_string(),
                    about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
                })
                .collect(),
        })
    }
}

impl From<&clap::Arg> for HelpArg {
    fn from(arg: &clap::Arg) -> Self {
        let value_name = takes_values(arg).then(|| {
            arg.get_value_names()
                .map(|names| {
                    names
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| arg.get_id().to_string())
        });
        Self {
            name: arg.get_id().to_string(),
            short: arg.get_short().map(|c| format!("-{c}")),
            long: arg.get_long().map(|l| format!("--{l}")),
            value_name,
            required: arg.is_required_set(),
            help: arg.get_help().map(|s| s.to_string()).unwrap_or_default(),
            default: default_value(arg),
            possible_values: possible_values(arg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;

    fn tree() -> Command {
        Command::new("app").about("The root").subcommand(
            Command::new("nest").about("A level").alias("n").subcommand(
                Command::new("leaf")
                    .alias("lf")
                    .about("The leaf")
                    .long_about("The leaf, at length")
                    .arg(Arg::new("formula").required(true).help("Which one"))
                    .arg(
                        Arg::new("tree")
                            .long("tree")
                            .short('t')
                            .action(clap::ArgAction::SetTrue)
                            .help("As a tree"),
                    )
                    .arg(
                        Arg::new("mode")
                            .long("mode")
                            .value_name("MODE")
                            .default_value("set")
                            .value_parser(["set", "tree"]),
                    ),
            ),
        )
    }

    #[test]
    fn a_nested_leaf_names_its_full_path() {
        let document =
            HelpDocument::extract(&tree(), &["nest", "leaf"], HelpLength::Short).unwrap();
        assert_eq!(document.schema_version, 1);
        assert_eq!(document.name, "leaf");
        assert_eq!(document.path, ["app", "nest", "leaf"]);
        assert_eq!(document.usage, "app nest leaf [OPTIONS] <formula>");
        assert_eq!(document.about, "The leaf");
        assert!(document.subcommands.is_empty());
    }

    #[test]
    fn an_aliased_path_names_the_canonical_commands() {
        let document = HelpDocument::extract(&tree(), &["n", "lf"], HelpLength::Short).unwrap();
        assert_eq!(document.name, "leaf");
        assert_eq!(document.path, ["app", "nest", "leaf"]);
        assert_eq!(document.usage, "app nest leaf [OPTIONS] <formula>");
    }

    #[test]
    fn long_help_reads_the_long_about() {
        let document = HelpDocument::extract(&tree(), &["nest", "leaf"], HelpLength::Long).unwrap();
        assert_eq!(document.about, "The leaf, at length");
    }

    #[test]
    fn args_carry_the_typed_tokens_and_the_value_facts() {
        let document =
            HelpDocument::extract(&tree(), &["nest", "leaf"], HelpLength::Short).unwrap();
        let names: Vec<&str> = document.args.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["formula", "tree", "mode", "help"]);

        let formula = &document.args[0];
        assert_eq!(formula.value_name.as_deref(), Some("formula"));
        assert!(formula.required);
        assert_eq!(formula.short, None);
        assert_eq!(formula.long, None);

        let tree = &document.args[1];
        assert_eq!(tree.short.as_deref(), Some("-t"));
        assert_eq!(tree.long.as_deref(), Some("--tree"));
        assert_eq!(tree.value_name, None);
        assert!(!tree.required);
        assert_eq!(tree.help, "As a tree");
        assert_eq!(tree.default, None);
        assert!(tree.possible_values.is_empty());

        let mode = &document.args[2];
        assert_eq!(mode.value_name.as_deref(), Some("MODE"));
        assert_eq!(mode.default.as_deref(), Some("set"));
        assert_eq!(mode.possible_values, ["set", "tree"]);
    }

    #[test]
    fn the_root_lists_its_subcommands() {
        let document = HelpDocument::extract(&tree(), &[], HelpLength::Short).unwrap();
        assert_eq!(document.path, ["app"]);
        assert_eq!(document.usage, "app [COMMAND]");
        assert_eq!(document.subcommands.len(), 2, "{:?}", document.subcommands);
        assert_eq!(document.subcommands[0].name, "nest");
        assert_eq!(document.subcommands[0].about, "A level");
        assert_eq!(document.subcommands[1].name, "help");
    }

    #[test]
    fn an_unknown_path_is_no_document() {
        assert!(HelpDocument::extract(&tree(), &["nest", "twig"], HelpLength::Short).is_none());
    }

    #[test]
    fn the_document_round_trips_through_json() {
        let document =
            HelpDocument::extract(&tree(), &["nest", "leaf"], HelpLength::Short).unwrap();
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.starts_with("{\"schema_version\":1,\"name\":\"leaf\",\"path\":[\"app\",\"nest\",\"leaf\"],\"usage\":"));
        assert_eq!(
            serde_json::from_str::<HelpDocument>(&json).unwrap(),
            document
        );
    }
}
