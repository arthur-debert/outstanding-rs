#[test]
fn unsupported_attribute_pairs_name_both_attributes() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/pure_with_simple.rs");
    cases.compile_fail("tests/ui/dispatch/pure_with_handler_path.rs");
}

#[test]
fn an_unsupported_pair_split_across_two_attributes_is_still_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/pure_with_simple_split_across_attributes.rs");
}

#[test]
fn a_name_carrying_the_path_separator_is_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/name_with_dot.rs");
}

#[test]
fn two_variants_claiming_one_command_name_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/dispatch/duplicate_command_name.rs");
    cases.compile_fail("tests/ui/dispatch/duplicate_derived_name.rs");
}
