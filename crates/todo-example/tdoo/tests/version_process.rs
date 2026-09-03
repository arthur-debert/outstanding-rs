mod common;

#[test]
fn the_compiled_binary_prints_its_version_and_succeeds() {
    let result = common::tdoo().run_process(env!("CARGO_BIN_EXE_tdoo"), ["--version"]);

    result.assert_success();
    assert_eq!(
        result.stdout().trim(),
        format!("tdoo {}", env!("CARGO_PKG_VERSION"))
    );
    result.assert_stderr_empty();
}
