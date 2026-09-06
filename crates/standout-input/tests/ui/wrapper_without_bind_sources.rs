use clap::ArgMatches;
use standout_input::{InputCollector, InputError, StdinSource};

struct RequestedStdin {
    inner: StdinSource,
}

impl InputCollector<String> for RequestedStdin {
    fn name(&self) -> &'static str {
        "stdin"
    }

    fn is_available(&self, matches: &ArgMatches) -> bool {
        matches.get_flag("stdin") && self.inner.is_available(matches)
    }

    fn collect(&self, matches: &ArgMatches) -> Result<Option<String>, InputError> {
        self.inner.collect(matches)
    }
}

fn main() {}
