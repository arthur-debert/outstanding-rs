use standout::cli::Dispatch;

mod handlers {
    use clap::ArgMatches;
    use standout::cli::hooks::HookError;
    use standout::cli::{CommandContext, HandlerResult, Output};

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn load_settings(
        _matches: &ArgMatches,
        _ctx: &mut CommandContext,
    ) -> Result<(), HookError> {
        Ok(())
    }

    pub fn load_engine(_matches: &ArgMatches, _ctx: &mut CommandContext) -> Result<(), HookError> {
        Ok(())
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(pre_dispatch = handlers::load_settings)]
    #[dispatch(pre_dispatch = handlers::load_engine)]
    List,
}

fn main() {
    let _ = Commands::dispatch_config();
}
