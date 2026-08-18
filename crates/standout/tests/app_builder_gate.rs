#[test]
fn unbuilt_builder_has_no_execution_or_rendering_entry_points() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/unbuilt_builder_cannot_run.rs");
    cases.compile_fail("tests/ui/unbuilt_builder_cannot_dispatch_parse_or_render.rs");
    cases.pass("tests/ui/built_app_entry_points_compile.rs");
}
