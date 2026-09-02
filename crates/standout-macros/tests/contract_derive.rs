use serde::Serialize;
use standout_dispatch::ContractSurface;
use standout_macros::ContractSurface;

#[derive(Serialize, ContractSurface)]
#[contract(schema_version = 2)]
struct Listing {
    items: Vec<String>,
}

#[derive(Serialize, ContractSurface)]
#[contract(schema_version = 7)]
struct Wrapped<T: Serialize> {
    inner: T,
}

#[test]
fn the_derive_sets_the_version_and_the_envelope_stamps_it() {
    assert_eq!(Listing::SCHEMA_VERSION, 2);
    let json = serde_json::to_string(
        &Listing {
            items: vec!["a".into()],
        }
        .envelope(),
    )
    .unwrap();
    assert_eq!(json, r#"{"schema_version":2,"data":{"items":["a"]}}"#);
}

#[test]
fn the_derive_keeps_the_types_generics() {
    let json = serde_json::to_string(&Wrapped { inner: 1 }.envelope()).unwrap();
    assert_eq!(json, r#"{"schema_version":7,"data":{"inner":1}}"#);
}

#[test]
fn compile_failures_cover_attribute_misuse() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/contract/*.rs");
}
