#[test]
fn a_collector_that_ignores_the_runs_input_sources_does_not_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/wrapper_without_bind_sources.rs");
    cases.pass("tests/ui/wrapper_with_bind_sources.rs");
}
