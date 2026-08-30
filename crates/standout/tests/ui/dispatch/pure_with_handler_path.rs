use standout::cli::Dispatch;

mod handlers {
    use standout::cli::Output;

    #[standout::handler]
    pub fn list(#[flag] all: bool) -> Result<Output<Vec<String>>, anyhow::Error> {
        let _ = all;
        Ok(Output::Render(Vec::new()))
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(pure, handler = handlers::list__handler)]
    List,
}

fn main() {
    let _ = Commands::dispatch_config();
}
