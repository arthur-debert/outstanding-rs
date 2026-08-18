use clap::{CommandFactory, Parser, Subcommand};
use serde::Serialize;
use standout::cli::{App, CommandContext, Dispatch, Output};
use standout::{embed_styles, embed_templates, handler};

#[derive(Parser)]
#[command(name = "smoketable")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    /// List the star catalog as a table.
    #[dispatch(pure)]
    List,
    /// Describe this tool in one line.
    #[dispatch(pure)]
    About,
}

#[derive(Serialize)]
pub struct Catalog {
    pub stars: Vec<Star>,
}

#[derive(Serialize)]
pub struct Star {
    pub name: String,
    pub constellation: String,
    pub magnitude: f64,
}

#[derive(Serialize)]
pub struct AboutView {
    pub name: String,
    pub purpose: String,
}

mod handlers {
    use super::*;

    #[handler]
    pub fn list(#[ctx] _ctx: &CommandContext) -> Result<Output<Catalog>, anyhow::Error> {
        Ok(Output::Render(Catalog {
            stars: vec![
                Star { name: "Aldebaran".into(), constellation: "Taurus".into(), magnitude: 0.86 },
                Star { name: "Rigel".into(), constellation: "Orion".into(), magnitude: 0.13 },
                Star { name: "Vega".into(), constellation: "Lyra".into(), magnitude: 0.03 },
            ],
        }))
    }

    #[handler]
    pub fn about(#[ctx] _ctx: &CommandContext) -> Result<Output<AboutView>, anyhow::Error> {
        Ok(Output::Render(AboutView {
            name: "smoketable".into(),
            purpose: "a tiny fixed star catalog".into(),
        }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .version(env!("CARGO_PKG_VERSION"))
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("default")
        .commands(Commands::dispatch_config())?
        .build()?;

    app.run(Cli::command(), std::env::args());
    Ok(())
}
