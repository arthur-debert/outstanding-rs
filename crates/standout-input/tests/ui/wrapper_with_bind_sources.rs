use clap::ArgMatches;
use standout_input::{InputCollector, InputError, InputSources, StdinSource};

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

    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<String>>> {
        Some(Box::new(RequestedStdin {
            inner: StdinSource::with_shared_reader(sources.stdin_arc()),
        }))
    }
}

fn main() {}
