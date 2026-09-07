use super::*;
use crate::new_project::publish::write_generated_files;
use std::process::Command;

#[test]
fn generated_manifests_only_depend_on_publishable_workspace_crates() {
    let dir = TempDir::new().unwrap();
    let mut spec = sample_spec(dir.path());
    spec.local_patch_root = None;
    let generated = GeneratedFiles::render(&spec).unwrap();
    write_generated_files(&spec.destination, &generated).unwrap();

    let emitted = generated_family_crates_io_deps(&spec.destination.join("Cargo.toml"));
    assert!(
        !emitted.is_empty(),
        "the generated project is expected to depend on the standout family"
    );

    let publishable = workspace_crates_io_publishable();
    let stranded: Vec<&String> = emitted
        .iter()
        .filter(|name| publishable.get(*name) != Some(&true))
        .collect();
    assert!(
        stranded.is_empty(),
        "the wizard emits crates.io dependencies on workspace crates that are not \
             published to crates.io: {stranded:?}. A generated project cannot resolve \
             them at any version this workspace pins. Publish those crates or stop \
             generating a dependency on them.\nemitted: {emitted:?}\npublishable: {publishable:?}"
    );

    assert!(emitted.contains("clapfig"), "{emitted:?}");
    assert_eq!(
        crates_io_requirement(
            &spec.destination.join("Cargo.toml"),
            "hello-tool",
            "clapfig"
        ),
        crates_io_requirement(&workspace_root().join("Cargo.toml"), "standout", "clapfig"),
        "the generated project must pin clapfig at the requirement standout itself uses"
    );
}

fn cargo_metadata(manifest: &Path) -> serde_json::Value {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo metadata failed for {}\nstderr:\n{}",
        manifest.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

fn generated_family_crates_io_deps(manifest: &Path) -> std::collections::BTreeSet<String> {
    let metadata = cargo_metadata(manifest);
    let mut names = std::collections::BTreeSet::new();
    for package in metadata["packages"].as_array().unwrap() {
        for dependency in package["dependencies"].as_array().unwrap() {
            let name = dependency["name"].as_str().unwrap();
            let from_crates_io = dependency["source"].as_str() == Some(CRATES_IO_SOURCE);
            if from_crates_io && (name.starts_with("standout") || name == "clapfig") {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn workspace_crates_io_publishable() -> std::collections::BTreeMap<String, bool> {
    let metadata = cargo_metadata(&workspace_root().join("Cargo.toml"));
    let mut publishable = std::collections::BTreeMap::new();
    for package in metadata["packages"].as_array().unwrap() {
        let registries = &package["publish"];
        let allowed = match registries.as_array() {
            None => true,
            Some(registries) => registries
                .iter()
                .any(|registry| registry.as_str() == Some("crates-io")),
        };
        publishable.insert(package["name"].as_str().unwrap().to_string(), allowed);
        for dependency in package["dependencies"].as_array().unwrap() {
            if dependency["source"].as_str() == Some(CRATES_IO_SOURCE) {
                publishable
                    .entry(dependency["name"].as_str().unwrap().to_string())
                    .or_insert(true);
            }
        }
    }
    publishable
}

fn crates_io_requirement(manifest: &Path, package: &str, dependency: &str) -> String {
    let metadata = cargo_metadata(manifest);
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["name"].as_str() == Some(package))
        .and_then(|candidate| {
            candidate["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["name"].as_str() == Some(dependency))
        })
        .map(|d| d["req"].as_str().unwrap().to_string())
        .unwrap_or_else(|| panic!("{package} depends on {dependency}"))
}
