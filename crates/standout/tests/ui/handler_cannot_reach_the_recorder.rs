// The recorder and the destination are the framework's: a handler reaches its
// run's values only through the borrowed `Results<E>`, which it also cannot
// clone past `handle`, and has no representation left to branch on
// (ADR-0041, "What the handler cannot reach").
use clap::ArgMatches;
use standout::cli::{CommandContext, Results};

fn main() {
    let ctx = CommandContext::default();
    let _recorder = ctx.recorder();
    let _channel = ctx.results::<serde_json::Value>();
    let _stream = ctx.stream();

    fn retain(results: &mut Results<serde_json::Value>) -> Results<serde_json::Value> {
        results.clone()
    }
    let _ = retain;
    let _ = |_: &ArgMatches| ();
}
