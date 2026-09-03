use standout_test::TestHarness;

pub const STORE: &str = r#"{"todos":[{"id":1,"title":"buy milk","done":false}],"next_id":1}"#;

pub fn tdoo() -> TestHarness {
    let harness = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TDOO__STORE", "todos.json");
    let home = harness.tempdir().unwrap().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home = home.to_str().unwrap().to_string();
    // HOME and XDG_CONFIG_HOME move `SearchPath::Platform` under the tempdir.
    harness
        .env("XDG_CONFIG_HOME", format!("{home}/.config"))
        .env("HOME", home)
}
