//! A downstream that reaches the derive path through `standout` alone.
//!
//! This crate depends on `standout` and on no other Standout crate, and this
//! module compiles under `deny(warnings)`, so the module is what fails when a
//! derive expands to a crate the consumer never named or emits an item whose
//! generated name trips a rustc lint. `command` declares the same subcommands
//! `Commands` registers, spelled the way clap's own derive spells them.
#![deny(warnings)]

use clap::{ArgMatches, Command};
use standout::cli::{App, CommandContext, Dispatch, Output};
use standout::{handler, Questionnaire, QuestionnaireChoices};

pub const NAME: &str = "unitctl";

#[derive(Debug, Clone, PartialEq, Eq, Questionnaire)]
#[question(id = "unitctl.provision")]
pub struct ProvisionAnswers {
    /// Which host?
    #[question(default = "localhost")]
    pub host: String,

    /// Which tier?
    #[question(choice, default = "basic")]
    pub tier: Tier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, QuestionnaireChoices)]
pub enum Tier {
    #[question(rename = "basic")]
    Basic,
    #[question(rename = "pro")]
    Pro,
}

#[derive(serde::Serialize)]
pub struct Units {
    pub names: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct Provisioned {
    pub host: String,
    pub tier: String,
}

pub mod handlers {
    use super::*;
    use standout::cli::CommandContextInput;

    #[handler]
    pub fn list_units(#[flag] all: bool) -> Result<Output<Units>, anyhow::Error> {
        let mut names = vec!["ssh".to_string()];
        if all {
            names.push("cron".to_string());
        }
        Ok(Output::Render(Units { names }))
    }

    #[handler]
    pub fn about(#[ctx] _ctx: &CommandContext) -> Result<Output<Units>, anyhow::Error> {
        Ok(Output::Render(Units {
            names: vec![NAME.to_string()],
        }))
    }

    #[handler]
    pub fn provision(#[ctx] ctx: &CommandContext) -> Result<Output<Provisioned>, anyhow::Error> {
        let answers: &ProvisionAnswers = ctx.questionnaire()?;
        Ok(Output::Render(Provisioned {
            host: answers.host.clone(),
            tier: answers.tier.to_string(),
        }))
    }

    #[handler]
    pub fn reload(#[matches] _matches: &ArgMatches) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    #[dispatch(pure, default, template = "{{ names | join(', ') }}")]
    ListUnits,
    #[dispatch(pure, name = "about-this", template = "{{ names | join(', ') }}")]
    About,
    #[dispatch(pure, questionnaire = ProvisionAnswers, template = "{{ host }}:{{ tier }}")]
    Provision,
    #[dispatch(pure, silent)]
    Reload,
}

pub fn app() -> App {
    App::builder()
        .commands(Commands::dispatch_config())
        .expect("derive-registered commands")
        .build()
        .expect("fixture app builds")
}

pub fn command() -> Command {
    Command::new(NAME)
        .subcommand(
            Command::new("list-units").arg(
                clap::Arg::new("all")
                    .long("all")
                    .action(clap::ArgAction::SetTrue),
            ),
        )
        .subcommand(Command::new("about-this"))
        .subcommand(Command::new("provision"))
        .subcommand(Command::new("reload"))
}
