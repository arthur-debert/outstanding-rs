use super::*;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestProjectAnswers {
    pub(super) project_name: String,
    pub(super) executable_name: String,
    pub(super) command_name: String,
    pub(super) command_description: String,
    pub(super) inputs: Vec<CommandInput>,
    pub(super) result_shape: ResultShape,
    pub(super) record_fields: Vec<String>,
}

#[cfg(test)]
impl From<TestProjectAnswers> for NewProjectAnswers {
    fn from(answers: TestProjectAnswers) -> Self {
        Self {
            project: ProjectAnswers {
                name: answers.project_name,
                executable_name: answers.executable_name,
            },
            command: CommandAnswers {
                name: answers.command_name,
                description: answers.command_description,
                inputs: answers
                    .inputs
                    .into_iter()
                    .map(CommandInputAnswers::from)
                    .collect(),
            },
            result: ResultAnswers {
                shape: answers.result_shape,
                fields: match answers.record_fields.is_empty() {
                    true => None,
                    false => Some(answers.record_fields.join(",")),
                },
            },
        }
    }
}

#[cfg(test)]
impl From<CommandInput> for CommandInputAnswers {
    fn from(input: CommandInput) -> Self {
        Self {
            name: input.name,
            value_type: input.value_type,
            cardinality: input.cardinality,
            sources: input
                .sources
                .into_iter()
                .map(InputSource::as_str)
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

#[cfg(test)]
impl InputSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Argument => "argument",
            Self::File => "file",
            Self::Stdin => "stdin",
        }
    }
}

pub(super) fn required_string(name: impl Into<String>) -> CommandInput {
    CommandInput {
        name: name.into(),
        value_type: InputValueType::String,
        cardinality: InputCardinality::Required,
        sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
    }
}

pub(super) fn sample_spec(root: &Path) -> ProjectSpec {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "hello-tool".into(),
        executable_name: "hello-tool".into(),
        command_name: "greet".into(),
        command_description: "Greet one value".into(),
        inputs: vec![required_string("name")],
        result_shape: ResultShape::Record,
        record_fields: vec!["summary".into(), "count".into()],
    })
    .unwrap();
    spec.destination = root.join("hello-tool");
    spec.local_patch_root = Some(workspace_root());
    spec
}

pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("standout crate lives under crates/standout in the repository")
        .to_path_buf()
}

pub(super) fn rich_questionnaire_spec(root: &Path) -> ProjectSpec {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "inspect-tool".into(),
        executable_name: "inspect-tool".into(),
        command_name: "inspect".into(),
        command_description: "Inspect document input".into(),
        inputs: vec![
            CommandInput {
                name: "document".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
            },
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
            CommandInput {
                name: "tag".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Repeated,
                sources: vec![InputSource::Argument],
            },
            CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ],
        result_shape: ResultShape::Record,
        record_fields: vec!["summary".into(), "count".into(), "echo".into()],
    })
    .unwrap();
    spec.destination = root.join("inspect-tool");
    spec.local_patch_root = Some(workspace_root());
    spec
}

pub(super) fn file_only_spec(root: &Path) -> ProjectSpec {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "file-tool".into(),
        executable_name: "file-tool".into(),
        command_name: "inspect".into(),
        command_description: "Inspect file contents".into(),
        inputs: vec![CommandInput {
            name: "document".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::File],
        }],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();
    spec.destination = root.join("file-tool");
    spec.local_patch_root = Some(workspace_root());
    spec
}

pub(super) fn single_input_spec(
    root: &Path,
    project_name: &str,
    input: CommandInput,
) -> ProjectSpec {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: project_name.into(),
        executable_name: project_name.into(),
        command_name: "inspect".into(),
        command_description: "Inspect configured input".into(),
        inputs: vec![input],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();
    spec.destination = root.join(project_name);
    spec.local_patch_root = Some(workspace_root());
    spec
}

pub(super) fn path_first_spec(root: &Path) -> ProjectSpec {
    let mut spec = ProjectSpec::from_answers(TestProjectAnswers {
        project_name: "config-tool".into(),
        executable_name: "config-tool".into(),
        command_name: "inspect".into(),
        command_description: "Inspect a config path".into(),
        inputs: vec![
            CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument],
            },
            CommandInput {
                name: "note".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ],
        result_shape: ResultShape::Message,
        record_fields: Vec::new(),
    })
    .unwrap();
    spec.destination = root.join("config-tool");
    spec.local_patch_root = Some(workspace_root());
    spec
}

pub(super) fn run_cargo<const N: usize>(cwd: &Path, args: [&str; N]) {
    let output = std::process::Command::new("cargo")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
