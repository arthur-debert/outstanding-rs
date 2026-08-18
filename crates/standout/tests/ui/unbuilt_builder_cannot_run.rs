use clap::Command;
use standout::cli::App;

fn main() {
    let _handled = App::builder().run(Command::new("app"), ["app"]);
}
