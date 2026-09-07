use super::*;
use crate::new_project::publish::publish_project;
use crate::new_project::test_support::run_cargo;
use crate::new_project::test_support::{required_string, workspace_root, TestProjectAnswers};
use crate::new_project::ResultShape;
use std::fs;
use tempfile::TempDir;

fn spec_with_description(description: &str) -> ProjectSpec {
    ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "demo".into(),
        executable_name: "demo".into(),
        command_name: "inspect".into(),
        command_description: description.into(),
        inputs: vec![required_string("document")],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap()
}

#[test]
fn command_attribute_splits_exactly_at_the_rustfmt_width_boundary() {
    let at_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 25));
    assert_eq!(
        cli_command_attribute(&at_limit),
        format!(
            "#[command(name = \"demo\", about = \"{}\")]",
            "a".repeat(ATTR_FN_LIKE_WIDTH - 25)
        )
    );

    let over_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 24));
    assert_eq!(
        cli_command_attribute(&over_limit),
        format!(
            "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
            "a".repeat(ATTR_FN_LIKE_WIDTH - 24)
        )
    );
}

#[test]
fn command_attribute_measures_display_width_not_char_count() {
    let inline = spec_with_description(&"検".repeat(22));
    assert_eq!(
        cli_command_attribute(&inline),
        format!(
            "#[command(name = \"demo\", about = \"{}\")]",
            "検".repeat(22)
        )
    );

    let split = spec_with_description(&"検".repeat(23));
    assert_eq!(
        cli_command_attribute(&split),
        format!(
            "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
            "検".repeat(23)
        )
    );
}

#[test]
fn long_description_generated_project_is_rustfmt_clean() {
    let dir = TempDir::new().unwrap();
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "provisioning-tool".into(),
        executable_name: "provisioning-tool".into(),
        command_name: "provision".into(),
        command_description: "Provisions pinned env either container or bare metal".into(),
        inputs: vec![required_string("target")],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();
    spec.destination = dir.path().join("provisioning-tool");
    spec.local_patch_root = Some(workspace_root());

    publish_project(&spec).unwrap();

    let cli =
        fs::read_to_string(spec.destination.join("crates/provisioning-tool/src/cli.rs")).unwrap();
    assert!(cli.contains(
        "#[command(\n    name = \"provisioning-tool\",\n    \
             about = \"Provisions pinned env either container or bare metal\"\n)]"
    ));
    run_cargo(&spec.destination, ["fmt", "--check"]);
}
