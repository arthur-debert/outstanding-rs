#[test]
fn unsupported_attribute_pairs_name_both_attributes() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/pure_with_simple.rs");
    cases.compile_fail("tests/ui/dispatch/pure_with_handler_path.rs");
}
