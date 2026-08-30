#[test]
fn unsupported_attribute_pairs_name_both_attributes() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/pure_with_simple.rs");
    cases.compile_fail("tests/ui/dispatch/pure_with_handler_path.rs");
}

#[test]
fn two_variants_claiming_one_command_name_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/duplicate_command_name.rs");
    cases.compile_fail("tests/ui/dispatch/duplicate_derived_name.rs");
}
