use super::*;
use crate::new_project::test_support::sample_spec;
use tempfile::TempDir;

#[test]
fn publish_refuses_non_empty_destination_without_partial_staging() {
    let dir = TempDir::new().unwrap();
    let spec = sample_spec(dir.path());
    fs::create_dir_all(&spec.destination).unwrap();
    fs::write(spec.destination.join("keep.txt"), "existing").unwrap();

    let error = publish_project(&spec).unwrap_err();

    assert!(error.to_string().contains("not empty"));
    assert_eq!(
        fs::read_to_string(spec.destination.join("keep.txt")).unwrap(),
        "existing"
    );
    let staged: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
        .collect();
    assert!(staged.is_empty());
}

#[test]
fn publish_refuses_file_destination_with_clear_error() {
    let dir = TempDir::new().unwrap();
    let spec = sample_spec(dir.path());
    fs::write(&spec.destination, "existing").unwrap();

    let error = publish_project(&spec).unwrap_err();

    assert!(error.to_string().contains("not a directory"));
    assert_eq!(fs::read_to_string(&spec.destination).unwrap(), "existing");
    let staged: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
        .collect();
    assert!(staged.is_empty());
}
