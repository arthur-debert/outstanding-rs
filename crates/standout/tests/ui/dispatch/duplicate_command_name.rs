use standout::cli::Dispatch;

mod handlers {
    use clap::ArgMatches;
    use standout::cli::{CommandContext, HandlerResult, Output};

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn show(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(name = "show")]
    List,
    Show,
}

fn main() {
    let _ = Commands::dispatch_config();
}
