#[test]
fn unbuilt_builder_has_no_run_entry_point() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/unbuilt_builder_cannot_run.rs");
}
