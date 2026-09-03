#[test]
fn unbuilt_builder_has_no_execution_or_rendering_entry_points() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/unbuilt_builder_cannot_run.rs");
    cases.compile_fail("tests/ui/unbuilt_builder_cannot_dispatch_parse_or_render.rs");
    cases.pass("tests/ui/built_app_entry_points_compile.rs");
}

#[test]
fn a_handler_cannot_reach_the_run_recorder_or_clone_its_results_channel() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/handler_cannot_reach_the_recorder.rs");
}
