use standout::cli::Dispatch;

mod handlers {
    use clap::ArgMatches;
    use standout::cli::{CommandContext, HandlerResult, Output};

    pub fn show_all(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    ShowAll,
    #[dispatch(handler = handlers::show_all)]
    ShowALL,
}

fn main() {
    let _ = Commands::dispatch_config();
}
