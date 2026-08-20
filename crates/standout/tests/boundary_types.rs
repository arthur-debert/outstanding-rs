//! Data-model checks for ROB04-WS01 public signatures.
//!
//! These tests do not call `todo!()` bodies and do not assert rendering or
//! detection behaviour.

use clap::Command;
use standout::cli::App;
use standout::{InputSources, TargetProperties};

#[test]
fn inner_run_takes_target_properties_and_input_sources() {
    fn assert_fn<I, T>(_: fn(&App, Command, I, TargetProperties, InputSources) -> bool)
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
    }
    assert_fn::<Vec<String>, String>(App::run_with);
}
